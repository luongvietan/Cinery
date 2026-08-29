#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_statuses_serialize_as_stable_strings() {
        assert_eq!(
            serde_json::to_value(WorkflowRunStatus::WaitingForApproval).unwrap(),
            "waiting_for_approval"
        );
        assert_eq!(
            serde_json::to_value(WorkflowStepStatus::Waiting).unwrap(),
            "waiting"
        );
    }

    #[test]
    fn workflow_tbd_snapshot_rejects_invalid_status() {
        let result = serde_json::from_value::<CanonTbdSnapshot>(serde_json::json!({
            "id": "tbd-1",
            "projectId": "project-1",
            "canonEntityId": null,
            "sectionKey": null,
            "topic": "Unknown",
            "note": null,
            "protected": true,
            "status": "pending",
            "resolutionText": null,
            "createdAt": "2026-08-28T00:00:00.000Z",
            "updatedAt": "2026-08-28T00:00:00.000Z",
            "resolvedAt": null
        }));

        assert!(result.is_err());
    }
}
use crate::canon::model::CanonEntityType;
use crate::skills::model::{AssetType, Prerequisite};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrerequisiteStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonSnapshotStatus {
    Locked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetSnapshotStatus {
    Canonical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorKind {
    DryRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonTbdStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
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
        executor_kind: ExecutorKind,
        #[serde(rename = "requestArtifactRef")]
        request_artifact_ref: String,
    },
    #[serde(rename = "complete")]
    Complete { id: String },
}

impl WorkflowStepDefinition {
    pub fn id(&self) -> &str {
        match self {
            Self::ValidateInput { id } | Self::Approval { id, .. } | Self::Complete { id } => id,
            Self::ResolveContext { id, .. }
            | Self::CompileRequest { id, .. }
            | Self::Execute { id, .. } => id,
        }
    }

    pub fn step_type(&self) -> &'static str {
        match self {
            Self::ValidateInput { .. } => "validate_input",
            Self::ResolveContext { .. } => "resolve_context",
            Self::CompileRequest { .. } => "compile_request",
            Self::Approval { .. } => "approval",
            Self::Execute { .. } => "execute",
            Self::Complete { .. } => "complete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrerequisiteCheck {
    pub id: String,
    pub prerequisite: Prerequisite,
    pub status: PrerequisiteStatus,
    pub message: String,
    pub resolved_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrerequisiteReport {
    pub passed: bool,
    pub checks: Vec<PrerequisiteCheck>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonSnapshotRef {
    pub entity_id: String,
    pub entity_type: CanonEntityType,
    pub section_id: String,
    pub section_key: String,
    pub revision: i64,
    pub status: CanonSnapshotStatus,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetSnapshotRef {
    pub asset_id: String,
    pub asset_version_id: String,
    pub asset_type: AssetType,
    pub version_number: i64,
    pub status: AssetSnapshotStatus,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonTbdSnapshot {
    pub id: String,
    pub project_id: String,
    pub canon_entity_id: Option<String>,
    pub section_key: Option<String>,
    pub topic: String,
    pub note: Option<String>,
    pub protected: bool,
    pub status: CanonTbdStatus,
    pub resolution_text: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowProjectRef {
    pub project_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowSkillRef {
    pub skill_id: String,
    pub skill_version: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCharacterOption {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunRecord {
    pub id: String,
    pub project_id: String,
    pub skill_id: String,
    pub skill_version: String,
    pub operation_id: String,
    pub status: String,
    pub input_json: String,
    pub prerequisite_report_json: Option<String>,
    pub context_snapshot_json: Option<String>,
    pub current_step_index: i64,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepRecord {
    pub id: String,
    pub workflow_run_id: String,
    pub step_definition_id: String,
    pub step_index: i64,
    pub step_type: String,
    pub status: String,
    pub input_json: Option<String>,
    pub output_json: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowEventRecord {
    pub id: String,
    pub workflow_run_id: String,
    pub sequence: i64,
    pub event_type: String,
    pub step_definition_id: Option<String>,
    pub payload_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderExecutionSummary {
    pub id: String,
    pub step_definition_id: String,
    pub attempt_number: i64,
    pub provider_id: String,
    pub model_id: String,
    pub adapter_version: i64,
    pub status: String,
    pub provider_job_id: Option<String>,
    pub normalized_error_json: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunDetail {
    pub run: WorkflowRunRecord,
    pub steps: Vec<WorkflowStepRecord>,
    pub events: Vec<WorkflowEventRecord>,
    pub provider_executions: Vec<ProviderExecutionSummary>,
}
