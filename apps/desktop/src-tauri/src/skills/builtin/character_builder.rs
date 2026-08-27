use crate::skills::model::{
    AssetType, DesiredOutputStatus, ExpectedOutputDefinition, OutputMediaType, Prerequisite,
    SkillDefinition, SkillOperation,
};
use crate::workflow::model::WorkflowStepDefinition;

pub(crate) fn builtin_character_builder() -> SkillDefinition {
    SkillDefinition {
        id: "character-builder".to_string(),
        name: "Character Builder".to_string(),
        version: "1.0.0".to_string(),
        description: "Build character production assets from locked Canon.".to_string(),
        operations: vec![SkillOperation {
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
            workflow: vec![
                WorkflowStepDefinition::ValidateInput {
                    id: "validate-input".to_string(),
                },
                WorkflowStepDefinition::ResolveContext {
                    id: "resolve-context".to_string(),
                    resolver_id: "character_face_lock_context".to_string(),
                },
                WorkflowStepDefinition::CompileRequest {
                    id: "compile-request".to_string(),
                    compiler_id: "character_face_lock_v1".to_string(),
                },
                WorkflowStepDefinition::Approval {
                    id: "approve-request".to_string(),
                    title: "Approve Face Lock Request".to_string(),
                    description:
                        "Review canonical context and compiled generation request before execution."
                            .to_string(),
                    approval_artifact_ref: "compiled_request".to_string(),
                },
                WorkflowStepDefinition::Execute {
                    id: "execute".to_string(),
                    executor_kind: crate::workflow::model::ExecutorKind::DryRun,
                    request_artifact_ref: "compiled_request".to_string(),
                },
                WorkflowStepDefinition::Complete {
                    id: "complete".to_string(),
                },
            ],
            expected_output: Some(ExpectedOutputDefinition {
                asset_type: AssetType::FaceLock,
                media_type: OutputMediaType::Image,
                desired_status: DesiredOutputStatus::Candidate,
                owner_entity_input_ref: Some("characterEntityId".to_string()),
            }),
        }],
    }
}
