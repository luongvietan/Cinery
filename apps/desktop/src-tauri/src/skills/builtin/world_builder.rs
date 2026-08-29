use crate::skills::model::{
    AssetType, DesiredOutputStatus, ExpectedOutputDefinition, OutputMediaType, Prerequisite,
    SkillDefinition, SkillOperation,
};
use crate::workflow::model::WorkflowStepDefinition;

pub(crate) fn builtin_world_builder() -> SkillDefinition {
    SkillDefinition {
        id: "world-builder".to_string(),
        name: "World Builder".to_string(),
        version: "1.0.0".to_string(),
        description: "Build world production plates from locked Location Canon.".to_string(),
        operations: vec![SkillOperation {
            id: "world.create_plate".to_string(),
            name: "Create World Plate".to_string(),
            description: "Compile a provider-neutral world plate request.".to_string(),
            intent_examples: vec![
                "Create a world plate for this location".to_string(),
                "Generate environment plate".to_string(),
            ],
            input_schema_id: "create_world_plate".to_string(),
            prerequisites: vec![
                Prerequisite::CanonSectionLocked {
                    entity_input_ref: "worldId".to_string(),
                    section_key: "description".to_string(),
                },
                Prerequisite::CanonSectionLocked {
                    entity_input_ref: "worldId".to_string(),
                    section_key: "geography".to_string(),
                },
            ],
            tbd_guards: vec![],
            workflow: vec![
                WorkflowStepDefinition::ValidateInput {
                    id: "validate-input".to_string(),
                },
                WorkflowStepDefinition::ResolveContext {
                    id: "resolve-context".to_string(),
                    resolver_id: "world_plate_context".to_string(),
                },
                WorkflowStepDefinition::CompileRequest {
                    id: "compile-request".to_string(),
                    compiler_id: "world_plate_v1".to_string(),
                },
                WorkflowStepDefinition::Approval {
                    id: "approve-request".to_string(),
                    title: "Approve World Plate Request".to_string(),
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
                asset_type: AssetType::WorldPlate,
                media_type: OutputMediaType::Image,
                desired_status: DesiredOutputStatus::Candidate,
                owner_entity_input_ref: Some("worldId".to_string()),
            }),
        }],
    }
}
