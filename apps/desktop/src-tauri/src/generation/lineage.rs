use super::model::ArtifactLineage;
use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineageInput {
    pub artifact_id: String,
    pub workflow_run_id: String,
    pub workflow_step_key: String,
    pub workflow_definition_id: String,
    pub workflow_version: String,
    pub skill_id: String,
    pub skill_version: String,
    pub compiled_execution_artifact_id: String,
    pub compiled_request_sha256: String,
    pub canon_snapshot_id: Option<String>,
    pub canon_snapshot_sha256: Option<String>,
    pub provider_attempt_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub source_asset_version_ids: Vec<String>,
    pub created_at: String,
}

pub fn build_lineage(input: LineageInput) -> Result<ArtifactLineage, AppError> {
    let required = [
        &input.artifact_id,
        &input.workflow_run_id,
        &input.workflow_step_key,
        &input.workflow_definition_id,
        &input.workflow_version,
        &input.skill_id,
        &input.skill_version,
        &input.compiled_execution_artifact_id,
        &input.provider_attempt_id,
        &input.provider_id,
        &input.model_id,
        &input.created_at,
    ];
    // Note: `source_asset_version_ids` may legitimately be empty — a
    // character's first Face Lock has no pinned source reference (the
    // provider runs reference-free). Every other provenance field remains
    // mandatory so lineage stays inspectable end-to-end.
    if required.iter().any(|value| value.trim().is_empty())
        || input
            .source_asset_version_ids
            .iter()
            .any(|value| value.trim().is_empty())
        || !is_hash(&input.compiled_request_sha256)
        || input
            .canon_snapshot_sha256
            .as_deref()
            .is_some_and(|hash| !is_hash(hash))
        || contains_secret_marker(&input)
    {
        return Err(AppError::GenerationLineageIncomplete);
    }

    Ok(ArtifactLineage {
        artifact_id: input.artifact_id,
        workflow_run_id: input.workflow_run_id,
        workflow_step_key: input.workflow_step_key,
        workflow_definition_id: input.workflow_definition_id,
        workflow_version: input.workflow_version,
        skill_id: input.skill_id,
        skill_version: input.skill_version,
        compiled_execution_artifact_id: input.compiled_execution_artifact_id,
        compiled_request_sha256: input.compiled_request_sha256,
        canon_snapshot_id: input.canon_snapshot_id,
        canon_snapshot_sha256: input.canon_snapshot_sha256,
        provider_attempt_id: input.provider_attempt_id,
        provider_id: input.provider_id,
        model_id: input.model_id,
        source_asset_version_ids: input.source_asset_version_ids,
        created_at: input.created_at,
    })
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn contains_secret_marker(input: &LineageInput) -> bool {
    let values = [
        input.provider_id.as_str(),
        input.model_id.as_str(),
        input.compiled_execution_artifact_id.as_str(),
    ];
    values.iter().any(|value| {
        let lower = value.to_ascii_lowercase();
        lower.contains("api_key")
            || lower.contains("authorization")
            || lower.contains("bearer ")
            || lower.contains("secret")
    })
}
