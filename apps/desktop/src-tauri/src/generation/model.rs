use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationResultSet {
    pub id: String,
    pub project_id: String,
    pub workflow_run_id: String,
    pub workflow_step_key: String,
    pub provider_attempt_id: String,
    pub media_kind: String,
    pub requested_output_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedArtifact {
    pub id: String,
    pub result_set_id: String,
    pub ordinal: i64,
    pub media_kind: String,
    pub mime_type: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub byte_size: i64,
    pub sha256: String,
    pub storage_path: String,
    pub capture_status: String,
    pub capture_error_code: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedArtifactSource {
    pub artifact_id: String,
    pub asset_version_id: String,
    pub role: String,
    pub ordinal: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactLineage {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactPromotion {
    pub id: String,
    pub artifact_id: String,
    pub asset_id: String,
    pub asset_version_id: String,
    pub set_canonical: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedArtifactDetail {
    pub artifact: GeneratedArtifact,
    pub lineage: Option<ArtifactLineage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationResultSetDetail {
    pub result_set: GenerationResultSet,
    pub artifacts: Vec<GeneratedArtifactDetail>,
}
