use super::adapters::{
    MockVideoQaAdapter, OpenAiCompatibleVideoQaAdapter, RawVideoQaResponse, VideoQaAdapter,
    VideoQaAdapterError, VideoQaAdapterErrorKind, VideoQaCapabilities,
};
use super::models::{
    QaCheckPlan, QaCheckRecord, QaMediaKind, QaRunRecord, QaRunStatus, ResolvedVideoQaContext,
    RunVideoQaInput, VideoQaMedia, VideoQaReference, VideoQaRequest,
};
use super::normalizer::QaResponseNormalizer;
use super::repository;
use super::video_check_planner::VideoQaCheckPlanner;
use super::video_context::resolve_video_qa_context;
use crate::error::AppError;
use crate::providers;
use crate::video_qa::evidence::{EvidenceMode, TemporalDecoderAvailability};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoQaWorkflowContext {
    pub qa_run_id: String,
    pub resolved: ResolvedVideoQaContext,
    pub plan: QaCheckPlan,
    pub adapter_id: String,
    pub provider_id: Option<String>,
    pub adapter_version: String,
    pub model_id: String,
    pub evidence_mode: String,
    pub execution_location: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledVideoQaRequest {
    pub qa_run_id: String,
    pub plan: QaCheckPlan,
    pub request: VideoQaRequest,
    pub adapter_id: String,
    pub provider_id: Option<String>,
    pub adapter_version: String,
    pub model_id: String,
    pub evidence_mode: String,
    pub execution_location: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoQaExecutionResult {
    pub qa_run_id: String,
    pub overall_status: super::models::QaOverallStatus,
    pub check_count: usize,
}

pub(crate) struct PreparedVideoQaCompletion {
    pub result: VideoQaExecutionResult,
    pub raw_response_metadata: Value,
    pub checks: Vec<QaCheckRecord>,
    pub completed_at: String,
}

pub fn resolve_and_persist(
    conn: &Connection,
    project_root: &Path,
    project_id: &str,
    workflow_run_id: &str,
    input: &Value,
) -> Result<VideoQaWorkflowContext, AppError> {
    let input = parse_input(input)?;
    let created_at = Utc::now().to_rfc3339();
    let resolved = resolve_video_qa_context(
        conn,
        project_root,
        &super::models::VideoQaContextRequest {
            project_id: project_id.to_string(),
            asset_version_id: input.asset_version_id.clone(),
            created_at: created_at.clone(),
        },
    )?;
    let plan = VideoQaCheckPlanner::compile(&resolved)?;
    let metadata = adapter_metadata(conn, &input)?;
    let qa_run_id = ulid::Ulid::new().to_string();
    repository::insert_run(
        conn,
        &QaRunRecord {
            id: qa_run_id.clone(),
            project_id: project_id.to_string(),
            asset_id: resolved.target.asset_id.clone(),
            asset_version_id: resolved.target.asset_version_id.clone(),
            media_kind: QaMediaKind::Video,
            workflow_run_id: Some(workflow_run_id.to_string()),
            status: QaRunStatus::Queued,
            overall_status: None,
            adapter_id: Some(input.adapter_id.clone()),
            adapter_version: Some(metadata.adapter_version.clone()),
            model_id: Some(metadata.model_id.clone()),
            execution_location: metadata.execution_location.clone(),
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
    Ok(VideoQaWorkflowContext {
        qa_run_id,
        resolved,
        plan,
        adapter_id: input.adapter_id,
        provider_id: input.provider_id,
        adapter_version: metadata.adapter_version,
        model_id: metadata.model_id,
        evidence_mode: metadata.evidence_mode,
        execution_location: metadata.execution_location,
    })
}

pub fn compile_request(
    project_root: &Path,
    context: &VideoQaWorkflowContext,
) -> CompiledVideoQaRequest {
    CompiledVideoQaRequest {
        qa_run_id: context.qa_run_id.clone(),
        plan: context.plan.clone(),
        request: VideoQaRequest {
            request_id: format!("video-qa:{}", context.qa_run_id),
            target: VideoQaMedia {
                asset_version_id: context.resolved.target.asset_version_id.clone(),
                local_path: project_root
                    .join(&context.resolved.target.file_path)
                    .to_string_lossy()
                    .into_owned(),
                mime_type: context.resolved.target.mime_type.clone(),
                content_sha256: context.resolved.target.content_sha256.clone(),
                size_bytes: context.resolved.target.size_bytes,
            },
            references: context
                .resolved
                .references
                .iter()
                .map(|reference| VideoQaReference {
                    asset_version_id: reference.asset_version_id.clone(),
                    local_path: project_root
                        .join(&reference.file_path)
                        .to_string_lossy()
                        .into_owned(),
                    mime_type: reference.mime_type.clone(),
                    content_sha256: reference.content_sha256.clone(),
                    size_bytes: reference.size_bytes,
                    purpose: reference.purpose.clone(),
                })
                .collect(),
            checks: context.plan.checks.clone(),
            response_schema_version: 1,
        },
        adapter_id: context.adapter_id.clone(),
        provider_id: context.provider_id.clone(),
        adapter_version: context.adapter_version.clone(),
        model_id: context.model_id.clone(),
        evidence_mode: context.evidence_mode.clone(),
        execution_location: context.execution_location.clone(),
    }
}

/// Executes the non-durable evaluator call, but intentionally leaves terminal
/// persistence to the owning WorkflowRuntime transaction. A process restart
/// while this call is in flight is therefore recovered as an interrupted run;
/// it is never submitted again automatically.
pub(crate) fn execute(
    conn: &Connection,
    compiled: &CompiledVideoQaRequest,
) -> Result<PreparedVideoQaCompletion, AppError> {
    let started_at = Utc::now().to_rfc3339();
    repository::mark_run_running(conn, &compiled.qa_run_id, &started_at)?;
    let adapter = build_adapter(conn, compiled).inspect_err(|error| {
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
        let safe_metadata = adapter_failure_metadata(&error);
        let message = error.to_string();
        let _ = repository::mark_run_failed(
            conn,
            &compiled.qa_run_id,
            "QA_ADAPTER_FAILED",
            &message,
            Some(&safe_metadata),
            &Utc::now().to_rfc3339(),
        );
        AppError::ProviderExecution(message)
    })?;
    let normalized = QaResponseNormalizer::normalize(&compiled.plan, &raw.response_text)
        .inspect_err(|error| {
            let _ = repository::mark_run_failed(
                conn,
                &compiled.qa_run_id,
                "INVALID_VLM_RESPONSE",
                &error.to_string(),
                Some(&raw.metadata),
                &Utc::now().to_rfc3339(),
            );
        })?;
    let completed_at = Utc::now().to_rfc3339();
    let checks = super::workflow::normalized_check_records(
        &compiled.qa_run_id,
        &compiled.plan,
        &normalized,
        &completed_at,
    );
    Ok(PreparedVideoQaCompletion {
        result: VideoQaExecutionResult {
            qa_run_id: compiled.qa_run_id.clone(),
            overall_status: normalized.overall,
            check_count: checks.len(),
        },
        raw_response_metadata: raw.metadata,
        checks,
        completed_at,
    })
}

fn parse_input(input: &Value) -> Result<RunVideoQaInput, AppError> {
    serde_json::from_value(input.clone())
        .map_err(|error| AppError::WorkflowInputInvalid(format!("invalid video QA input: {error}")))
}

struct AdapterMetadata {
    adapter_version: String,
    model_id: String,
    evidence_mode: String,
    execution_location: String,
}

fn adapter_metadata(
    conn: &Connection,
    input: &RunVideoQaInput,
) -> Result<AdapterMetadata, AppError> {
    match input.adapter_id.as_str() {
        "mock" | "mock_adapter_failure" | "mock_invalid_response" => Ok(AdapterMetadata {
            adapter_version: "1".into(),
            model_id: input
                .model_id
                .clone()
                .unwrap_or_else(|| "mock-video-qa".into()),
            evidence_mode: "direct_video".into(),
            execution_location: "local".into(),
        }),
        "openai" => {
            let provider_id = input.provider_id.as_deref().unwrap_or("openai");
            let config = providers::repository::get_provider_config(conn, provider_id)?
                .filter(|config| config.enabled)
                .ok_or_else(|| {
                    AppError::ProviderConfiguration(
                        "Video QA provider is not configured or is disabled".into(),
                    )
                })?;
            let endpoint = config.endpoint.ok_or_else(|| {
                AppError::ProviderConfiguration("Video QA endpoint is required".into())
            })?;
            let model_id = input
                .model_id
                .clone()
                .or(config.default_model)
                .ok_or_else(|| {
                    AppError::ProviderConfiguration("Video QA model is required".into())
                })?;
            let adapter = OpenAiCompatibleVideoQaAdapter::new(
                endpoint,
                "",
                model_id.clone(),
                EvidenceMode::DirectVideo,
                TemporalDecoderAvailability::Unavailable,
            )
            .map_err(|error| AppError::ProviderConfiguration(error.to_string()))?;
            Ok(AdapterMetadata {
                adapter_version: adapter.adapter_version().to_string(),
                model_id,
                evidence_mode: evidence_mode_name(adapter.evidence_mode()).into(),
                execution_location: adapter.execution_location(),
            })
        }
        other => Err(AppError::ProviderConfiguration(format!(
            "Unknown Video QA adapter: {other}"
        ))),
    }
}

fn build_adapter(
    conn: &Connection,
    compiled: &CompiledVideoQaRequest,
) -> Result<Box<dyn VideoQaAdapter>, AppError> {
    match compiled.adapter_id.as_str() {
        "mock" => Ok(Box::new(MockVideoQaAdapter::new(success_response(
            &compiled.plan,
        )))),
        "mock_invalid_response" => Ok(Box::new(MockVideoQaAdapter::new(
            serde_json::json!({"schemaVersion": 1, "checks": []}),
        ))),
        "mock_adapter_failure" => Ok(Box::new(FailingVideoQaAdapter {
            model_id: compiled.model_id.clone(),
        })),
        "openai" => {
            let provider_id = compiled.provider_id.as_deref().unwrap_or("openai");
            let config = providers::repository::get_provider_config(conn, provider_id)?
                .filter(|config| config.enabled)
                .ok_or_else(|| {
                    AppError::ProviderConfiguration("Video QA provider is disabled".into())
                })?;
            let endpoint = config.endpoint.ok_or_else(|| {
                AppError::ProviderConfiguration("Video QA endpoint is required".into())
            })?;
            let credential = config
                .credential_reference
                .as_deref()
                .and_then(|reference| std::env::var(reference).ok())
                .unwrap_or_default();
            Ok(Box::new(
                OpenAiCompatibleVideoQaAdapter::new(
                    endpoint,
                    credential,
                    compiled.model_id.clone(),
                    parse_evidence_mode(&compiled.evidence_mode)?,
                    TemporalDecoderAvailability::Unavailable,
                )
                .map_err(|error| AppError::ProviderConfiguration(error.to_string()))?,
            ))
        }
        other => Err(AppError::ProviderConfiguration(format!(
            "Unknown Video QA adapter: {other}"
        ))),
    }
}

fn success_response(plan: &QaCheckPlan) -> Value {
    serde_json::json!({
        "schemaVersion": 1,
        "checks": plan.checks.iter().map(|check| serde_json::json!({
            "checkId": check.id,
            "status": "pass",
            "confidence": 1.0,
            "observed": "Deterministic mock evidence satisfies the planned check.",
            "reason": "Mock Video QA fixture returned a passing result.",
            "repairHint": null
        })).collect::<Vec<_>>(),
        "modelSummary": "All planned checks passed in the deterministic mock evaluator."
    })
}

fn adapter_failure_metadata(error: &VideoQaAdapterError) -> Value {
    serde_json::json!({
        "adapterErrorKind": error.kind,
        "diagnostic": error.diagnostic,
    })
}

fn evidence_mode_name(mode: EvidenceMode) -> &'static str {
    match mode {
        EvidenceMode::DirectVideo => "direct_video",
        EvidenceMode::SampledFrames => "sampled_frames",
    }
}

fn parse_evidence_mode(value: &str) -> Result<EvidenceMode, AppError> {
    match value {
        "direct_video" => Ok(EvidenceMode::DirectVideo),
        "sampled_frames" => Ok(EvidenceMode::SampledFrames),
        other => Err(AppError::WorkflowRunInconsistent(format!(
            "unknown Video QA evidence mode: {other}"
        ))),
    }
}

struct FailingVideoQaAdapter {
    model_id: String,
}

impl VideoQaAdapter for FailingVideoQaAdapter {
    fn id(&self) -> &'static str {
        "mock_video_qa_failure"
    }

    fn adapter_version(&self) -> u32 {
        1
    }

    fn capabilities(&self) -> VideoQaCapabilities {
        VideoQaCapabilities {
            supports_direct_video: true,
            supports_sampled_frames: false,
            supports_multiple_references: true,
            max_media_inputs: 32,
        }
    }

    fn evidence_mode(&self) -> EvidenceMode {
        EvidenceMode::DirectVideo
    }

    fn execution_location(&self) -> String {
        "local".into()
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn analyze(
        &self,
        _request: &VideoQaRequest,
    ) -> Result<RawVideoQaResponse, VideoQaAdapterError> {
        Err(VideoQaAdapterError::new(
            VideoQaAdapterErrorKind::Network,
            "Video QA mock adapter failed",
        )
        .with_diagnostic("deterministic safe diagnostic"))
    }
}
