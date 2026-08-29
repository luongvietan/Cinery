use crate::skills::model::{
    AssetType, DesiredOutputStatus, ExpectedOutputDefinition, OutputMediaType, SkillDefinition,
    SkillOperation,
};
use crate::workflow::model::WorkflowStepDefinition;

pub(crate) fn builtin_scene_builder() -> SkillDefinition {
    SkillDefinition {
        id: "scene-builder".to_string(),
        name: "Scene Builder".to_string(),
        version: "1.0.0".to_string(),
        description: "Build scene shot keyframes from exact pinned references.".to_string(),
        operations: vec![SkillOperation {
            id: "scene.create_keyframe".to_string(),
            name: "Create Scene Keyframe".to_string(),
            description: "Compile a provider-neutral scene keyframe request.".to_string(),
            intent_examples: vec![
                "Create a shot keyframe for this scene".to_string(),
                "Generate scene keyframe".to_string(),
            ],
            input_schema_id: "create_scene_keyframe".to_string(),
            prerequisites: vec![],
            tbd_guards: vec![],
            workflow: vec![
                WorkflowStepDefinition::ValidateInput {
                    id: "validate-input".to_string(),
                },
                WorkflowStepDefinition::ResolveContext {
                    id: "resolve-context".to_string(),
                    resolver_id: "scene_keyframe_context".to_string(),
                },
                WorkflowStepDefinition::CompileRequest {
                    id: "compile-request".to_string(),
                    compiler_id: "scene_keyframe_v1".to_string(),
                },
                WorkflowStepDefinition::Approval {
                    id: "approve-request".to_string(),
                    title: "Approve Scene Keyframe Request".to_string(),
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
                asset_type: AssetType::ShotKeyframe,
                media_type: OutputMediaType::Image,
                desired_status: DesiredOutputStatus::Candidate,
                owner_entity_input_ref: Some("sceneId".to_string()),
            }),
        }, SkillOperation {
            id: "scene.generate_video".to_string(),
            name: "Generate Scene Video".to_string(),
            description: "Animate the compiled cinema prompt into a real video shot.".to_string(),
            intent_examples: vec![
                "Turn this scene into a video".to_string(),
                "Generate video from the compiled scene".to_string(),
            ],
            input_schema_id: "generate_scene_video".to_string(),
            prerequisites: vec![],
            tbd_guards: vec![],
            workflow: vec![
                WorkflowStepDefinition::ValidateInput {
                    id: "validate-input".to_string(),
                },
                WorkflowStepDefinition::ResolveContext {
                    id: "resolve-context".to_string(),
                    resolver_id: "scene_video_context".to_string(),
                },
                WorkflowStepDefinition::CompileRequest {
                    id: "compile-request".to_string(),
                    compiler_id: "scene_video_v1".to_string(),
                },
                WorkflowStepDefinition::Approval {
                    id: "approve-request".to_string(),
                    title: "Approve Scene Video Request".to_string(),
                    description:
                        "Review the compiled video request before spending a generation."
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
                asset_type: AssetType::Video,
                media_type: OutputMediaType::Video,
                desired_status: DesiredOutputStatus::Candidate,
                owner_entity_input_ref: Some("sceneId".to_string()),
            }),
        }],
    }
}
