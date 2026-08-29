#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_request_has_no_provider_or_model_fields() {
        let request: ExecutionRequest = serde_json::from_value(serde_json::json!({
            "requestVersion": 1,
            "task": "character_face_lock",
            "mediaType": "image",
            "prompt": "TASK",
            "references": [],
            "constraints": [],
            "expectedOutput": {
                "assetType": "face_lock",
                "mediaType": "image",
                "desiredStatus": "candidate",
                "ownerEntityInputRef": "characterEntityId"
            },
            "provenance": {
                "workflowRunId": "run-1",
                "skillId": "character-builder",
                "skillVersion": "1.0.0",
                "operationId": "character.create_face_lock"
            }
        }))
        .unwrap();

        let value = serde_json::to_value(request).unwrap();
        assert!(value.get("provider").is_none());
        assert!(value.get("model").is_none());
    }

    #[test]
    fn execution_request_rejects_provider_fields_and_invalid_output_values() {
        let with_provider = serde_json::from_value::<ExecutionRequest>(serde_json::json!({
            "requestVersion": 1,
            "task": "character_face_lock",
            "mediaType": "image",
            "prompt": "TASK",
            "references": [],
            "constraints": [],
            "expectedOutput": {
                "assetType": "face_lock",
                "mediaType": "image",
                "desiredStatus": "candidate",
                "ownerEntityInputRef": "characterEntityId"
            },
            "provenance": {
                "workflowRunId": "run-1",
                "skillId": "character-builder",
                "skillVersion": "1.0.0",
                "operationId": "character.create_face_lock"
            },
            "provider": "not-allowed"
        }));
        assert!(with_provider.is_err());

        let invalid_output = serde_json::from_value::<ExecutionRequest>(serde_json::json!({
            "requestVersion": 1,
            "task": "character_face_lock",
            "mediaType": "image",
            "prompt": "TASK",
            "references": [],
            "constraints": [],
            "expectedOutput": {
                "assetType": "not-an-asset",
                "mediaType": "image",
                "desiredStatus": "candidate",
                "ownerEntityInputRef": "characterEntityId"
            },
            "provenance": {
                "workflowRunId": "run-1",
                "skillId": "character-builder",
                "skillVersion": "1.0.0",
                "operationId": "character.create_face_lock"
            }
        }));
        assert!(invalid_output.is_err());
    }
}
use crate::skills::model::ExpectedOutputDefinition;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use rusqlite::OptionalExtension;

fn deserialize_true<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = bool::deserialize(deserializer)?;
    if value {
        Ok(true)
    } else {
        Err(serde::de::Error::custom("value must be true"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTask {
    CharacterFaceLock,
    CharacterOutfit,
    CharacterSheet,
    WorldPlate,
    ShotKeyframe,
    VisualRepair,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionReferenceType {
    AssetVersion,
    CanonSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMediaType {
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceBackground {
    #[serde(rename = "18_percent_neutral_gray")]
    NeutralGray,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionReference {
    #[serde(rename = "type")]
    pub reference_type: ExecutionReferenceType,
    pub reference: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum ExecutionConstraint {
    #[serde(rename = "flat_reference_background")]
    FlatReferenceBackground { value: ReferenceBackground },
    #[serde(rename = "shadowless_lighting")]
    ShadowlessLighting { #[serde(deserialize_with = "deserialize_true")] value: bool },
    #[serde(rename = "no_cast_shadow")]
    NoCastShadow { #[serde(deserialize_with = "deserialize_true")] value: bool },
    #[serde(rename = "no_contact_shadow")]
    NoContactShadow { #[serde(deserialize_with = "deserialize_true")] value: bool },
    #[serde(rename = "no_cinematic_dof")]
    NoCinematicDof { #[serde(deserialize_with = "deserialize_true")] value: bool },
    #[serde(rename = "preserve_visual_lock")]
    PreserveVisualLock { key: String, description: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionRequest {
    pub request_version: u8,
    pub task: ExecutionTask,
    pub media_type: ExecutionMediaType,
    pub prompt: String,
    pub references: Vec<ExecutionReference>,
    pub constraints: Vec<ExecutionConstraint>,
    pub expected_output: ExpectedOutputDefinition,
    pub provenance: ExecutionProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionProvenance {
    pub workflow_run_id: String,
    pub skill_id: String,
    pub skill_version: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionResult {
    pub kind: String,
    pub artifact_path: PathBuf,
    pub result_set_id: Option<String>,
    pub artifact_ids: Vec<String>,
    pub request: ExecutionRequest,
}

/// Maximum accepted reference attachment size (25 MiB).
const MAX_REFERENCE_BYTES: usize = 25 * 1024 * 1024;

/// Resolves every `AssetVersion` reference of `request` into a verified,
/// ordered, ephemeral attachment. Verification re-hashes the stored bytes
/// against the version metadata and rejects foreign, missing, oversized, or
/// corrupted references before any provider submission. The execution layer
/// owns resolution; adapters receive ready bytes only.
pub fn resolve_reference_attachments(
    project_root: &std::path::Path,
    request: &ExecutionRequest,
) -> Result<Vec<crate::providers::model::ProviderReferenceAttachment>, AppError> {
    let mut attachments = Vec::with_capacity(request.references.len());
    for reference in &request.references {
        let version_id = match reference.reference_type {
            crate::workflow::execution::ExecutionReferenceType::AssetVersion => &reference.reference,
            // Canon-snapshot references are compiled prompt context, not media.
            crate::workflow::execution::ExecutionReferenceType::CanonSnapshot => continue,
        };
        attachments.push(resolve_one_attachment(project_root, version_id)?);
    }
    Ok(attachments)
}

fn resolve_one_attachment(
    project_root: &std::path::Path,
    version_id: &str,
) -> Result<crate::providers::model::ProviderReferenceAttachment, AppError> {
    use sha2::{Digest, Sha256};

    let conn = crate::db::open_existing_connection(&project_root.join("project.db"))?;
    let project = crate::project::repository::read_project(&conn)?;
    let row = conn
        .query_row(
            "SELECT av.file_path, av.sha256, av.mime_type, av.byte_size, av.original_filename \
             FROM asset_versions av JOIN assets a ON a.id = av.asset_id \
             WHERE av.id = ?1 AND a.project_id = ?2",
            rusqlite::params![version_id, project.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::Database(error.to_string()))?
        .ok_or_else(|| {
            AppError::WorkflowPrerequisiteFailed(format!(
                "reference asset version {version_id} does not exist in this project"
            ))
        })?;
    let (file_path, expected_sha256, media_type, byte_size, original_filename) = row;

    if !matches!(media_type.as_str(), "image/png" | "image/jpeg" | "image/webp") {
        return Err(AppError::WorkflowPrerequisiteFailed(format!(
            "reference asset version {version_id} has unsupported media type {media_type}"
        )));
    }
    if byte_size as usize > MAX_REFERENCE_BYTES {
        return Err(AppError::WorkflowPrerequisiteFailed(format!(
            "reference asset version {version_id} exceeds the {MAX_REFERENCE_BYTES} byte attachment limit"
        )));
    }

    let absolute = project_root.join(&file_path);
    let bytes = std::fs::read(&absolute).map_err(|error| {
        AppError::WorkflowPrerequisiteFailed(format!(
            "reference asset version {version_id} could not be read: {error}"
        ))
    })?;
    if bytes.len() > MAX_REFERENCE_BYTES {
        return Err(AppError::WorkflowPrerequisiteFailed(format!(
            "reference asset version {version_id} exceeds the attachment size limit"
        )));
    }

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual_sha256 = format!("{:x}", hasher.finalize());
    if actual_sha256 != expected_sha256 {
        return Err(AppError::GenerationArtifactIntegrityMismatch(format!(
            "reference asset version {version_id} failed its integrity check before submission"
        )));
    }

    let file_name = if original_filename.trim().is_empty() {
        std::path::Path::new(&file_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("reference")
            .to_string()
    } else {
        original_filename
    };

    Ok(crate::providers::model::ProviderReferenceAttachment {
        asset_version_id: version_id.to_string(),
        file_name,
        media_type,
        bytes,
        sha256: actual_sha256,
    })
}
