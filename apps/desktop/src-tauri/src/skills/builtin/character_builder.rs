use crate::skills::model::{
    AssetType, DesiredOutputStatus, ExpectedOutputDefinition, OutputMediaType, Prerequisite,
    SkillDefinition, SkillOperation,
};
use crate::workflow::model::{ExecutorKind, WorkflowStepDefinition};

fn production_workflow(
    resolver_id: &str,
    compiler_id: &str,
    approval_title: &str,
    approval_description: &str,
) -> Vec<WorkflowStepDefinition> {
    vec![
        WorkflowStepDefinition::ValidateInput {
            id: "validate-input".to_string(),
        },
        WorkflowStepDefinition::ResolveContext {
            id: "resolve-context".to_string(),
            resolver_id: resolver_id.to_string(),
        },
        WorkflowStepDefinition::CompileRequest {
            id: "compile-request".to_string(),
            compiler_id: compiler_id.to_string(),
        },
        WorkflowStepDefinition::Approval {
            id: "approve-request".to_string(),
            title: approval_title.to_string(),
            description: approval_description.to_string(),
            approval_artifact_ref: "compiled_request".to_string(),
        },
        WorkflowStepDefinition::Execute {
            id: "execute".to_string(),
            executor_kind: ExecutorKind::DryRun,
            request_artifact_ref: "compiled_request".to_string(),
        },
        WorkflowStepDefinition::Complete {
            id: "complete".to_string(),
        },
    ]
}

pub(crate) fn builtin_character_builder() -> SkillDefinition {
    SkillDefinition {
        id: "character-builder".to_string(),
        name: "Character Builder".to_string(),
        version: "1.1.0".to_string(),
        description: "Build character production assets from locked Canon.".to_string(),
        operations: vec![
            SkillOperation {
                id: "character.create_face_lock".to_string(),
                name: "Create Face Lock".to_string(),
                description: "Compile a provider-neutral face-lock request.".to_string(),
                intent_examples: vec![
                    "Create a face lock for this character".to_string(),
                    "Lock the character's face".to_string(),
                ],
                input_schema_id: "create_face_lock".to_string(),
                prerequisites: vec![Prerequisite::CanonEntityExists {
                    entity_type: crate::canon::model::CanonEntityType::Character,
                    input_ref: "characterEntityId".to_string(),
                }],
                tbd_guards: vec![],
                workflow: production_workflow(
                    "character_face_lock_context",
                    "character_face_lock_v1",
                    "Approve Face Lock Request",
                    "Review canonical context and compiled generation request before execution.",
                ),
                expected_output: Some(ExpectedOutputDefinition {
                    asset_type: AssetType::FaceLock,
                    media_type: OutputMediaType::Image,
                    desired_status: DesiredOutputStatus::Candidate,
                    owner_entity_input_ref: Some("characterEntityId".to_string()),
                }),
            },
            SkillOperation {
                id: "character.create_outfit".to_string(),
                name: "Create Outfit".to_string(),
                description: "Compile a provider-neutral direct-on-character outfit request."
                    .to_string(),
                intent_examples: vec![
                    "Put this character in a new outfit".to_string(),
                    "Dress the character".to_string(),
                ],
                input_schema_id: "create_outfit".to_string(),
                prerequisites: vec![
                    Prerequisite::CanonEntityExists {
                        entity_type: crate::canon::model::CanonEntityType::Character,
                        input_ref: "characterEntityId".to_string(),
                    },
                    Prerequisite::CanonicalAssetExists {
                        owner_entity_input_ref: "characterEntityId".to_string(),
                        asset_type: AssetType::FaceLock,
                    },
                ],
                tbd_guards: vec![],
                workflow: production_workflow(
                    "character_outfit_context",
                    "character_outfit_v1",
                    "Approve Outfit Request",
                    "Review canonical face reference and compiled direct-on-character outfit request before execution.",
                ),
                expected_output: Some(ExpectedOutputDefinition {
                    asset_type: AssetType::Outfit,
                    media_type: OutputMediaType::Image,
                    desired_status: DesiredOutputStatus::Candidate,
                    owner_entity_input_ref: Some("characterEntityId".to_string()),
                }),
            },
            SkillOperation {
                id: "character.create_character_sheet".to_string(),
                name: "Create Character Sheet".to_string(),
                description: "Compile a provider-neutral three-panel character sheet request."
                    .to_string(),
                intent_examples: vec![
                    "Create a character sheet".to_string(),
                    "Build a three-panel character sheet".to_string(),
                ],
                input_schema_id: "create_character_sheet".to_string(),
                prerequisites: vec![
                    Prerequisite::CanonEntityExists {
                        entity_type: crate::canon::model::CanonEntityType::Character,
                        input_ref: "characterEntityId".to_string(),
                    },
                    Prerequisite::CanonicalAssetExists {
                        owner_entity_input_ref: "characterEntityId".to_string(),
                        asset_type: AssetType::Outfit,
                    },
                ],
                tbd_guards: vec![],
                workflow: production_workflow(
                    "character_sheet_context",
                    "character_sheet_v1",
                    "Approve Character Sheet Request",
                    "Review canonical look (outfit + face) and compiled three-panel sheet request before execution.",
                ),
                expected_output: Some(ExpectedOutputDefinition {
                    asset_type: AssetType::CharacterSheet,
                    media_type: OutputMediaType::Image,
                    desired_status: DesiredOutputStatus::Candidate,
                    owner_entity_input_ref: Some("characterEntityId".to_string()),
                }),
            },
        ],
    }
}
