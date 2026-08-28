use crate::skills::model::{SkillDefinition, SkillOperation};
use crate::workflow::model::{ExecutorKind, WorkflowStepDefinition};

pub(crate) fn builtin_visual_qa() -> SkillDefinition {
    SkillDefinition {
        id: "visual-qa".into(),
        name: "Visual QA".into(),
        version: "1.0.0".into(),
        description: "Evaluate exact image Asset Versions against immutable QA context.".into(),
        operations: vec![
            SkillOperation {
                id: "asset.run_visual_qa".into(),
                name: "Run Visual QA".into(),
                description:
                    "Inspect one exact image Asset Version against a deterministic check plan."
                        .into(),
                intent_examples: vec!["Run visual QA on this candidate".into()],
                input_schema_id: "run_visual_qa".into(),
                prerequisites: vec![],
                tbd_guards: vec![],
                workflow: vec![
                    WorkflowStepDefinition::ValidateInput {
                        id: "validate-input".into(),
                    },
                    WorkflowStepDefinition::ResolveContext {
                        id: "resolve-context".into(),
                        resolver_id: "visual_qa_context".into(),
                    },
                    WorkflowStepDefinition::CompileRequest {
                        id: "compile-request".into(),
                        compiler_id: "visual_qa_v1".into(),
                    },
                    WorkflowStepDefinition::Approval {
                        id: "approve-qa".into(),
                        title: "Approve Visual QA".into(),
                        description:
                            "Review exact media inputs and LOCAL/CLOUD disclosure before execution."
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
            },
            SkillOperation {
                id: "asset.repair_failed_qa".into(),
                name: "Repair Failed Visual QA".into(),
                description: "Apply a minimal image edit driven only by reviewed QA failures."
                    .into(),
                intent_examples: vec!["Repair only the failed visual QA checks".into()],
                input_schema_id: "repair_failed_qa".into(),
                prerequisites: vec![],
                tbd_guards: vec![],
                workflow: vec![
                    WorkflowStepDefinition::ValidateInput {
                        id: "validate-input".into(),
                    },
                    WorkflowStepDefinition::ResolveContext {
                        id: "resolve-context".into(),
                        resolver_id: "visual_qa_repair_context".into(),
                    },
                    WorkflowStepDefinition::CompileRequest {
                        id: "compile-request".into(),
                        compiler_id: "visual_qa_repair_v1".into(),
                    },
                    WorkflowStepDefinition::Approval {
                        id: "approve-repair".into(),
                        title: "Approve Targeted Repair".into(),
                        description:
                            "Review the failed checks, preserve set, exact source and reference images before editing."
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
            },
        ],
    }
}
