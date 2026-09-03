use super::adapters::{MockVisualQaAdapter, OpenAiCompatibleVisualQaAdapter, VisualQaAdapter};
use super::check_planner::QaCheckPlanner;
use super::context::{resolve_qa_context, QaPlanningRequest, ResolvedQaContext};
use super::models::{
    QaCheckPlan, QaCheckRecord, QaMediaKind, QaReviewStatus, QaRunRecord, QaRunStatus,
    VisualExpectation, VisualQaMedia, VisualQaReference, VisualQaRequest,
};
use super::normalizer::{NormalizedQaResult, QaResponseNormalizer};
use super::repository;
use crate::error::AppError;
use crate::providers;
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaWorkflowContext {
    pub qa_run_id: String,
    pub resolved: ResolvedQaContext,
    pub plan: QaCheckPlan,
    pub adapter_id: String,
    pub adapter_version: String,
    pub model_id: String,
    pub execution_location: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledVisualQaRequest {
    pub qa_run_id: String,
    pub plan: QaCheckPlan,
    pub request: VisualQaRequest,
    pub adapter_id: String,
    pub adapter_version: String,
    pub model_id: String,
    pub execution_location: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualQaExecutionResult {
    pub qa_run_id: String,
    pub overall_status: super::models::QaOverallStatus,
    pub check_count: usize,
}

pub fn resolve_and_persist(
    conn: &Connection,
    project_id: &str,
    workflow_run_id: &str,
    input: &Value,
) -> Result<QaWorkflowContext, AppError> {
    let asset_version_id = required_input(input, "assetVersionId")?;
    let adapter_id = required_input(input, "adapterId")?;
    let expectations = input
        .get("expectations")
        .cloned()
        .map(serde_json::from_value::<Vec<VisualExpectation>>)
        .transpose()
        .map_err(|error| AppError::WorkflowInputInvalid(format!("invalid expectations: {error}")))?
        .unwrap_or_default();
    let created_at = Utc::now().to_rfc3339();
    let resolved = resolve_qa_context(
        conn,
        &QaPlanningRequest {
            project_id: project_id.into(),
            asset_version_id,
            created_at: created_at.clone(),
            expectations,
        },
    )?;
    let plan = QaCheckPlanner::compile(&resolved)?;
    let (model_id, execution_location) = adapter_metadata(conn, input, &adapter_id)?;
    let qa_run_id = ulid::Ulid::new().to_string();
    repository::insert_run(
        conn,
        &QaRunRecord {
            id: qa_run_id.clone(),
            project_id: project_id.into(),
            asset_id: resolved.target.asset_id.clone(),
            asset_version_id: resolved.target.asset_version_id.clone(),
            media_kind: QaMediaKind::Image,
            workflow_run_id: Some(workflow_run_id.into()),
            status: QaRunStatus::Queued,
            overall_status: None,
            adapter_id: Some(adapter_id.clone()),
            adapter_version: Some("1".into()),
            model_id: Some(model_id.clone()),
            execution_location: execution_location.clone(),
            check_plan: serde_json::to_value(&plan)
                .map_err(|error| AppError::Database(error.to_string()))?,
            context_snapshot: serde_json::to_value(&resolved)
                .map_err(|error| AppError::Database(error.to_string()))?,
            raw_response_metadata: None,
            error_code: None,
            error_message: None,
            created_at,
            started_at: None,
            completed_at: None,
        },
    )?;
    Ok(QaWorkflowContext {
        qa_run_id,
        resolved,
        plan,
        adapter_id,
        adapter_version: "1".into(),
        model_id,
        execution_location,
    })
}

pub fn compile_request(
    project_root: &Path,
    context: &QaWorkflowContext,
) -> CompiledVisualQaRequest {
    let mut references = Vec::new();
    if let Some(reference) = &context.resolved.canonical_face {
        references.push(VisualQaReference {
            asset_version_id: reference.asset_version_id.clone(),
            local_path: project_root
                .join(&reference.file_path)
                .to_string_lossy()
                .into_owned(),
            purpose: reference.purpose.clone(),
        });
    }
    if let Some(reference) = &context.resolved.canonical_look {
        references.push(VisualQaReference {
            asset_version_id: reference.asset_version_id.clone(),
            local_path: project_root
                .join(&reference.file_path)
                .to_string_lossy()
                .into_owned(),
            purpose: reference.purpose.clone(),
        });
    }
    CompiledVisualQaRequest {
        qa_run_id: context.qa_run_id.clone(),
        plan: context.plan.clone(),
        request: VisualQaRequest {
            request_id: format!("qa:{}", context.qa_run_id),
            target: VisualQaMedia {
                asset_version_id: context.resolved.target.asset_version_id.clone(),
                local_path: project_root
                    .join(&context.resolved.target.file_path)
                    .to_string_lossy()
                    .into_owned(),
                media_type: "image".into(),
            },
            references,
            checks: context.plan.checks.clone(),
            response_schema_version: 1,
        },
        adapter_id: context.adapter_id.clone(),
        adapter_version: context.adapter_version.clone(),
        model_id: context.model_id.clone(),
        execution_location: context.execution_location.clone(),
    }
}

pub fn execute(
    conn: &mut Connection,
    input: &Value,
    compiled: &CompiledVisualQaRequest,
) -> Result<VisualQaExecutionResult, AppError> {
    let started_at = Utc::now().to_rfc3339();
    repository::mark_run_running(conn, &compiled.qa_run_id, &started_at)?;
    let adapter = build_adapter(conn, input, compiled).inspect_err(|error| {
        let _ = repository::mark_run_failed(
            conn,
            &compiled.qa_run_id,
            "QA_ADAPTER_CONFIGURATION",
            &error.to_string(),
            None,
            &Utc::now().to_rfc3339(),
        );
    })?;
    let raw = adapter.analyze(&compiled.request).map_err(|error| {
        let message = error.to_string();
        let _ = repository::mark_run_failed(
            conn,
            &compiled.qa_run_id,
            "QA_ADAPTER_FAILED",
            &message,
            error
                .diagnostic
                .as_ref()
                .map(|diagnostic| serde_json::json!({"diagnostic": diagnostic}))
                .as_ref(),
            &Utc::now().to_rfc3339(),
        );
        AppError::ProviderExecution(message)
    })?;
    let normalized = QaResponseNormalizer::normalize(&compiled.plan, &raw.response_text)
        .inspect_err(|error| {
            let message = error.to_string();
            let _ = repository::mark_run_failed(
                conn,
                &compiled.qa_run_id,
                "INVALID_VLM_RESPONSE",
                &message,
                Some(&raw.metadata),
                &Utc::now().to_rfc3339(),
            );
        })?;
    let completed_at = Utc::now().to_rfc3339();
    let checks = normalized_check_records(
        &compiled.qa_run_id,
        &compiled.plan,
        &normalized,
        &completed_at,
    );
    repository::complete_run(
        conn,
        &compiled.qa_run_id,
        normalized.overall,
        &raw.metadata,
        &checks,
        &completed_at,
    )?;
    Ok(VisualQaExecutionResult {
        qa_run_id: compiled.qa_run_id.clone(),
        overall_status: normalized.overall,
        check_count: checks.len(),
    })
}

pub(crate) fn normalized_check_records(
    qa_run_id: &str,
    plan: &QaCheckPlan,
    normalized: &NormalizedQaResult,
    created_at: &str,
) -> Vec<QaCheckRecord> {
    plan.checks
        .iter()
        .zip(&normalized.checks)
        .map(|(definition, result)| QaCheckRecord {
            id: ulid::Ulid::new().to_string(),
            qa_run_id: qa_run_id.to_string(),
            check_id: result.check_id.clone(),
            check_type: definition.check_type,
            source: definition.source,
            requirement: serde_json::to_value(definition).unwrap_or(Value::Null),
            status: result.status,
            confidence: result.confidence,
            observed: result.observed.clone(),
            reason: result.reason.clone(),
            repair_hint: result.repair_hint.clone(),
            review_status: QaReviewStatus::Unreviewed,
            review_note: None,
            reviewed_at: None,
            created_at: created_at.to_string(),
        })
        .collect()
}

fn adapter_metadata(
    conn: &Connection,
    input: &Value,
    adapter_id: &str,
) -> Result<(String, String), AppError> {
    match adapter_id {
        "mock" => Ok(("mock-vlm".into(), "local".into())),
        "openai" => {
            let provider_id = input
                .get("providerId")
                .and_then(Value::as_str)
                .unwrap_or("openai");
            let config = providers::repository::get_provider_config(conn, provider_id)?
                .filter(|config| config.enabled)
                .ok_or_else(|| {
                    AppError::ProviderConfiguration(
                        "OpenAI-compatible visual QA provider is not configured".into(),
                    )
                })?;
            let endpoint = config.endpoint.ok_or_else(|| {
                AppError::ProviderConfiguration("Visual QA endpoint is required".into())
            })?;
            let model_id = input
                .get("modelId")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or(config.default_model)
                .ok_or_else(|| {
                    AppError::ProviderConfiguration("Visual QA model is required".into())
                })?;
            let adapter = OpenAiCompatibleVisualQaAdapter::new(endpoint, "", model_id.clone())
                .map_err(|error| AppError::ProviderConfiguration(error.to_string()))?;
            Ok((model_id, adapter.execution_location()))
        }
        other => Err(AppError::ProviderConfiguration(format!(
            "Unknown visual QA adapter: {other}"
        ))),
    }
}

fn build_adapter(
    conn: &Connection,
    input: &Value,
    compiled: &CompiledVisualQaRequest,
) -> Result<Box<dyn VisualQaAdapter>, AppError> {
    match compiled.adapter_id.as_str() {
        "mock" => {
            let response = input.get("mockResponse").cloned().ok_or_else(|| {
                AppError::WorkflowInputInvalid("mockResponse is required for mock QA".into())
            })?;
            Ok(Box::new(MockVisualQaAdapter::new(response)))
        }
        "openai" => {
            let provider_id = input
                .get("providerId")
                .and_then(Value::as_str)
                .unwrap_or("openai");
            let config = providers::repository::get_provider_config(conn, provider_id)?
                .filter(|config| config.enabled)
                .ok_or_else(|| {
                    AppError::ProviderConfiguration("OpenAI provider is disabled".into())
                })?;
            let endpoint = config.endpoint.ok_or_else(|| {
                AppError::ProviderConfiguration("Visual QA endpoint is required".into())
            })?;
            let credential = config
                .credential_reference
                .as_deref()
                .and_then(|reference| std::env::var(reference).ok())
                .unwrap_or_default();
            Ok(Box::new(
                OpenAiCompatibleVisualQaAdapter::new(
                    endpoint,
                    credential,
                    compiled.model_id.clone(),
                )
                .map_err(|error| AppError::ProviderConfiguration(error.to_string()))?,
            ))
        }
        other => Err(AppError::ProviderConfiguration(format!(
            "Unknown visual QA adapter: {other}"
        ))),
    }
}

fn required_input(input: &Value, key: &str) -> Result<String, AppError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::WorkflowInputInvalid(format!("{key} must be a non-empty string")))
}
