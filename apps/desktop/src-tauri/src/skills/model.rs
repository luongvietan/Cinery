#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_definition_round_trips_camel_case_contracts() {
        let definition: SkillDefinition = serde_json::from_value(serde_json::json!({
            "id": "character-builder",
            "name": "Character Builder",
            "version": "1.0.0",
            "description": "Build character production assets.",
            "operations": [{
                "id": "character.create_face_lock",
                "name": "Create Face Lock",
                "description": "Create a face-lock request.",
                "intentExamples": [],
                "inputSchemaId": "create_face_lock",
                "prerequisites": [],
                "tbdGuards": [],
                "workflow": [{"id": "validate-input", "type": "validate_input"}],
                "expectedOutput": null
            }]
        }))
        .unwrap();

        assert_eq!(definition.id, "character-builder");
        assert_eq!(serde_json::to_value(definition).unwrap()["version"], "1.0.0");
    }

    #[test]
    fn skill_definition_rejects_empty_operations_and_unknown_fields() {
        let empty = serde_json::from_value::<SkillDefinition>(serde_json::json!({
            "id": "character-builder",
            "name": "Character Builder",
            "version": "1.0.0",
            "description": "Build character production assets.",
            "operations": []
        }));
        assert!(empty.is_err());

        let unknown = serde_json::from_value::<SkillDefinition>(serde_json::json!({
            "id": "character-builder",
            "name": "Character Builder",
            "version": "1.0.0",
            "description": "Build character production assets.",
            "operations": [],
            "provider": "not-allowed"
        }));
        assert!(unknown.is_err());
    }
}
use crate::workflow::model::WorkflowStepDefinition;
use crate::canon::model::CanonEntityType;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;

fn deserialize_non_empty_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned,
{
    let values = Vec::<T>::deserialize(deserializer)?;
    if values.is_empty() {
        return Err(serde::de::Error::custom("must contain at least one item"));
    }
    Ok(values)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetType {
    FaceLock,
    Outfit,
    CharacterSheet,
    WorldPlate,
    ShotKeyframe,
    PropPlate,
    Image,
    Video,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetVersionStatus {
    Draft,
    Generated,
    Candidate,
    QaFailed,
    Repairing,
    Approved,
    Canonical,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMediaType {
    Image,
    Video,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesiredOutputStatus {
    Candidate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Prerequisite {
    #[serde(rename = "canon_entity_exists")]
    CanonEntityExists {
        #[serde(rename = "entityType")]
        entity_type: CanonEntityType,
        #[serde(rename = "inputRef")]
        input_ref: String,
    },
    #[serde(rename = "canon_section_locked")]
    CanonSectionLocked {
        #[serde(rename = "entityInputRef")]
        entity_input_ref: String,
        #[serde(rename = "sectionKey")]
        section_key: String,
    },
    #[serde(rename = "canonical_asset_exists")]
    CanonicalAssetExists {
        #[serde(rename = "ownerEntityInputRef")]
        owner_entity_input_ref: String,
        #[serde(rename = "assetType")]
        asset_type: AssetType,
    },
    #[serde(rename = "asset_version_status")]
    AssetVersionStatus {
        #[serde(rename = "assetVersionInputRef")]
        asset_version_input_ref: String,
        status: AssetVersionStatus,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum TbdGuard {
    #[serde(rename = "entity_scope")]
    EntityScope {
        #[serde(rename = "entityInputRef")]
        entity_input_ref: String,
    },
    #[serde(rename = "section_scope")]
    SectionScope {
        #[serde(rename = "entityInputRef")]
        entity_input_ref: String,
        #[serde(rename = "sectionKey")]
        section_key: String,
    },
    #[serde(rename = "project_scope")]
    ProjectScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpectedOutputDefinition {
    pub asset_type: AssetType,
    pub media_type: OutputMediaType,
    pub desired_status: DesiredOutputStatus,
    pub owner_entity_input_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillOperation {
    pub id: String,
    pub name: String,
    pub description: String,
    pub intent_examples: Vec<String>,
    pub input_schema_id: String,
    pub prerequisites: Vec<Prerequisite>,
    pub tbd_guards: Vec<TbdGuard>,
    #[serde(deserialize_with = "deserialize_non_empty_vec")]
    pub workflow: Vec<WorkflowStepDefinition>,
    pub expected_output: Option<ExpectedOutputDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(deserialize_with = "deserialize_non_empty_vec")]
    pub operations: Vec<SkillOperation>,
}
