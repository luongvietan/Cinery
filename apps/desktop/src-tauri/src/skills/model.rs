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
            "operations": []
        }))
        .unwrap();

        assert_eq!(definition.id, "character-builder");
        assert_eq!(serde_json::to_value(definition).unwrap()["version"], "1.0.0");
    }
}
use crate::workflow::model::WorkflowStepDefinition;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Prerequisite {
    #[serde(rename = "canon_entity_exists")]
    CanonEntityExists {
        #[serde(rename = "entityType")]
        entity_type: String,
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
        asset_type: String,
    },
    #[serde(rename = "asset_version_status")]
    AssetVersionStatus {
        #[serde(rename = "assetVersionInputRef")]
        asset_version_input_ref: String,
        status: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
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
#[serde(rename_all = "camelCase")]
pub struct ExpectedOutputDefinition {
    pub asset_type: String,
    pub media_type: String,
    pub desired_status: String,
    pub owner_entity_input_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillOperation {
    pub id: String,
    pub name: String,
    pub description: String,
    pub intent_examples: Vec<String>,
    pub input_schema_id: String,
    pub prerequisites: Vec<Prerequisite>,
    pub tbd_guards: Vec<TbdGuard>,
    pub workflow: Vec<WorkflowStepDefinition>,
    pub expected_output: Option<ExpectedOutputDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub operations: Vec<SkillOperation>,
}
