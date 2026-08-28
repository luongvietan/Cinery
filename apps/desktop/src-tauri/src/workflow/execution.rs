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
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceRole {
    World,
    CharacterLook,
    CharacterSheet,
    Prop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionReference {
    #[serde(rename = "type")]
    pub reference_type: ExecutionReferenceType,
    pub reference: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ReferenceRole>,
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
