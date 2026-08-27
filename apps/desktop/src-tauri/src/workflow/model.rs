#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_statuses_serialize_as_stable_strings() {
        assert_eq!(serde_json::to_value(WorkflowRunStatus::WaitingForApproval).unwrap(), "waiting_for_approval");
        assert_eq!(serde_json::to_value(WorkflowStepStatus::Waiting).unwrap(), "waiting");
    }
}
use crate::canon::model::CanonTbdRecord;
use crate::skills::model::Prerequisite;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Created,
    Running,
    WaitingForApproval,
    ReadyForExecution,
    Completed,
    Rejected,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepStatus {
    Pending,
    Running,
    Waiting,
    Completed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEventType {
    RunCreated,
    RunStarted,
    StepStarted,
    StepCompleted,
    ApprovalRequested,
    ApprovalGranted,
    ApprovalRejected,
    ExecutionStarted,
    ExecutionCompleted,
    RunCompleted,
    RunCancelled,
    RunFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkflowStepDefinition {
    #[serde(rename = "validate_input")]
    ValidateInput { id: String },
    #[serde(rename = "resolve_context")]
    ResolveContext {
        id: String,
        #[serde(rename = "resolverId")]
        resolver_id: String,
    },
    #[serde(rename = "compile_request")]
    CompileRequest {
        id: String,
        #[serde(rename = "compilerId")]
        compiler_id: String,
    },
    #[serde(rename = "approval")]
    Approval {
        id: String,
        title: String,
        description: String,
        #[serde(rename = "approvalArtifactRef")]
        approval_artifact_ref: String,
    },
    #[serde(rename = "execute")]
    Execute {
        id: String,
        #[serde(rename = "executorKind")]
        executor_kind: String,
        #[serde(rename = "requestArtifactRef")]
        request_artifact_ref: String,
    },
    #[serde(rename = "complete")]
    Complete { id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrerequisiteCheck {
    pub id: String,
    pub prerequisite: Prerequisite,
    pub status: String,
    pub message: String,
    pub resolved_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrerequisiteReport {
    pub passed: bool,
    pub checks: Vec<PrerequisiteCheck>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonSnapshotRef {
    pub entity_id: String,
    pub entity_type: String,
    pub section_id: String,
    pub section_key: String,
    pub revision: i64,
    pub status: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSnapshotRef {
    pub asset_id: String,
    pub asset_version_id: String,
    pub asset_type: String,
    pub version_number: i64,
    pub status: String,
    pub path: String,
}

pub type CanonTbdSnapshot = CanonTbdRecord;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowContextSnapshot {
    pub snapshot_version: u8,
    pub project: WorkflowProjectRef,
    pub skill: WorkflowSkillRef,
    pub input: serde_json::Value,
    pub prerequisite_report: PrerequisiteReport,
    pub canon: Vec<CanonSnapshotRef>,
    pub assets: Vec<AssetSnapshotRef>,
    pub protected_tbds: Vec<CanonTbdSnapshot>,
    pub resolved_context: serde_json::Value,
    pub captured_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowProjectRef {
    pub project_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSkillRef {
    pub skill_id: String,
    pub skill_version: String,
    pub operation_id: String,
}
