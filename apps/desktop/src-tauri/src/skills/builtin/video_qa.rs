use crate::skills::model::{SkillDefinition, SkillOperation};
use crate::workflow::model::{ExecutorKind, WorkflowStepDefinition};

/// A separate, versioned contract for evaluating one immutable video Asset
/// Version. Image Visual QA remains unchanged under `visual-qa@1.0.0`.
pub(crate) fn builtin_video_qa() -> SkillDefinition {
    SkillDefinition {
        id: "video-qa".into(),
        name: "Video QA".into(),
        version: "1.0.0".into(),
        description: "Evaluate one exact video Asset Version against immutable temporal evidence."
            .into(),
        operations: vec![SkillOperation {
            id: "asset.run_video_qa".into(),
            name: "Run Video QA".into(),
            description: "Inspect one exact video Asset Version without changing promotion or generation state."
                .into(),
            intent_examples: vec!["Run video QA on this shot candidate".into()],
            input_schema_id: "run_video_qa".into(),
            prerequisites: vec![],
            tbd_guards: vec![],
            workflow: vec![
                WorkflowStepDefinition::ValidateInput {
                    id: "validate-input".into(),
                },
                WorkflowStepDefinition::ResolveContext {
                    id: "resolve-context".into(),
                    resolver_id: "video_qa_context".into(),
                },
                WorkflowStepDefinition::CompileRequest {
                    id: "compile-request".into(),
                    compiler_id: "video_qa_v1".into(),
                },
                WorkflowStepDefinition::Approval {
                    id: "approve-video-qa".into(),
                    title: "Approve Video QA".into(),
                    description: "Review the exact video, evidence mode, provider/model, and LOCAL/CLOUD disclosure before execution."
                        .into(),
                    approval_artifact_ref: "compiled_request".into(),
                },
                WorkflowStepDefinition::Execute {
                    id: "execute".into(),
                    executor_kind: ExecutorKind::DryRun,
                    request_artifact_ref: "compiled_request".into(),
                },
                WorkflowStepDefinition::Complete {
                    id: "complete".into(),
                },
            ],
            expected_output: None,
        }],
    }
}
