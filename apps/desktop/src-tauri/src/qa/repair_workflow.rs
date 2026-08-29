use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    assets::{model::AssetVersionRecord, repository as asset_repository},
    error::AppError,
    skills::model::{AssetType, DesiredOutputStatus, ExpectedOutputDefinition, OutputMediaType},
    workflow::execution::{
        ExecutionConstraint, ExecutionMediaType, ExecutionProvenance, ExecutionReference,
        ExecutionReferenceType, ExecutionRequest, ExecutionTask,
    },
};

use super::{repair::CompiledRepair, repair::RepairCompiler, repository as qa_repository};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairWorkflowContext {
    pub compiled: CompiledRepair,
    pub source_asset_type: String,
    pub source_owner_entity_id: Option<String>,
    pub source: AssetVersionRecord,
    pub references: Vec<AssetVersionRecord>,
}

pub fn resolve(
    conn: &Connection,
    project_root: &Path,
    project_id: &str,
    input: &Value,
) -> Result<RepairWorkflowContext, AppError> {
    let qa_run_id = required_input(input, "qaRunId")?;
    let detail =
        qa_repository::get_run(conn, project_id, &qa_run_id)?.ok_or(AppError::QaRunNotFound)?;
    let compiled = RepairCompiler::compile(&detail)?;
    let source =
        asset_repository::get_asset_version_by_id(conn, &compiled.plan.source_asset_version_id)?
            .ok_or(AppError::AssetVersionNotFound)?;
    if source.asset_id != compiled.plan.source_asset_id {
        return Err(invalid(
            "repair source Asset Version belongs to another Asset",
        ));
    }
    require_image_file(project_root, &source)?;
    let source_asset = asset_repository::get_asset(conn, &source.asset_id)?;
    if source_asset.project_id != project_id {
        return Err(invalid("repair source Asset belongs to another project"));
    }

    let mut references = Vec::with_capacity(compiled.plan.reference_asset_version_ids.len());
    for version_id in &compiled.plan.reference_asset_version_ids {
        let version = asset_repository::get_asset_version_by_id(conn, version_id)?
            .ok_or(AppError::AssetVersionNotFound)?;
        require_image_file(project_root, &version)?;
        references.push(version);
    }

    Ok(RepairWorkflowContext {
        compiled,
        source_asset_type: source_asset.asset_type,
        source_owner_entity_id: source_asset.owner_entity_id,
        source,
        references,
    })
}

pub fn compile_request(
    workflow_run_id: &str,
    context: &RepairWorkflowContext,
) -> Result<ExecutionRequest, AppError> {
    let asset_type: AssetType =
        serde_json::from_value(Value::String(context.source_asset_type.clone()))
            .map_err(|_| invalid("repair source has an unsupported Asset type"))?;
    let mut references = vec![ExecutionReference {
        reference_type: ExecutionReferenceType::AssetVersion,
        reference: context.source.id.clone(),
        description: "Exact source candidate to edit; never overwrite this version.".into(),
        role: None,
    }];
    references.extend(context.references.iter().map(|version| ExecutionReference {
        reference_type: ExecutionReferenceType::AssetVersion,
        reference: version.id.clone(),
        description: format!(
            "Exact immutable repair reference Asset Version {}",
            version.id
        ),
        role: None,
    }));
    let constraints = context
        .compiled
        .plan
        .preserve
        .iter()
        .map(|item| ExecutionConstraint::PreserveVisualLock {
            key: item.key.clone(),
            description: item.instruction.clone(),
        })
        .collect();

    Ok(ExecutionRequest {
        request_version: 1,
        task: ExecutionTask::VisualRepair,
        media_type: ExecutionMediaType::Image,
        prompt: context.compiled.request.prompt.clone(),
        references,
        constraints,
        expected_output: ExpectedOutputDefinition {
            asset_type,
            media_type: OutputMediaType::Image,
            desired_status: DesiredOutputStatus::Candidate,
            owner_entity_input_ref: None,
        },
        provenance: ExecutionProvenance {
            workflow_run_id: workflow_run_id.into(),
            skill_id: "visual-qa".into(),
            skill_version: "1.0.0".into(),
            operation_id: "asset.repair_failed_qa".into(),
        },
    })
}

pub struct RepairProvenanceInput<'a> {
    pub project_id: &'a str,
    pub workflow_run_id: &'a str,
    pub child_asset_version_id: &'a str,
    pub provider_id: &'a str,
    pub adapter_version: u32,
    pub model_id: &'a str,
    pub provider_job_id: &'a str,
    pub compiled_request: &'a ExecutionRequest,
    pub context: &'a RepairWorkflowContext,
    pub created_at: &'a str,
}

pub fn record_repair(conn: &Connection, input: &RepairProvenanceInput<'_>) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO qa_repairs
         (id, project_id, source_asset_id, source_asset_version_id, child_asset_version_id,
          source_qa_run_id, workflow_run_id, failed_check_ids_json, repair_plan_json,
          compiled_request_json, provider_id, adapter_version, model_id, provider_job_id,
          reference_asset_version_ids_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            ulid::Ulid::new().to_string(),
            input.project_id,
            input.context.compiled.plan.source_asset_id,
            input.context.compiled.plan.source_asset_version_id,
            input.child_asset_version_id,
            input.context.compiled.plan.source_qa_run_id,
            input.workflow_run_id,
            serde_json::to_string(&input.context.compiled.plan.failed_check_ids)
                .map_err(|error| invalid(error.to_string()))?,
            serde_json::to_string(&input.context.compiled.plan)
                .map_err(|error| invalid(error.to_string()))?,
            serde_json::to_string(input.compiled_request)
                .map_err(|error| invalid(error.to_string()))?,
            input.provider_id,
            input.adapter_version,
            input.model_id,
            input.provider_job_id,
            serde_json::to_string(&input.context.compiled.plan.reference_asset_version_ids)
                .map_err(|error| invalid(error.to_string()))?,
            input.created_at,
        ],
    )
    .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(())
}

pub fn link_follow_up_qa(
    conn: &Connection,
    workflow_run_id: &str,
    child_qa_run_id: &str,
    auto_qa_workflow_run_id: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE qa_repairs
         SET child_qa_run_id = ?1, auto_qa_workflow_run_id = ?2
         WHERE workflow_run_id = ?3",
        params![child_qa_run_id, auto_qa_workflow_run_id, workflow_run_id],
    )
    .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(())
}

fn required_input(input: &Value, key: &str) -> Result<String, AppError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| AppError::WorkflowInputInvalid(format!("{key} is required")))
}

fn require_image_file(project_root: &Path, version: &AssetVersionRecord) -> Result<(), AppError> {
    if !version.mime_type.starts_with("image/") {
        return Err(invalid(format!(
            "Asset Version {} is not an image",
            version.id
        )));
    }
    if !project_root.join(&version.file_path).is_file() {
        return Err(invalid(format!(
            "Asset Version {} file is missing",
            version.id
        )));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidQaData(message.into())
}
