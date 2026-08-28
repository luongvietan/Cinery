use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Recovery disposition for a job.
/// Determines what action to take when a project reopens with incomplete jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDisposition {
    NothingRequired,          // Job completed or safe state; no action needed
    ResumeLocal,              // Local operation can safely resume
    AwaitUserRetry,           // User must explicitly retry (provider/cloud failure)
    InspectRemoteResult,      // Fetch remote state before deciding
    ManualResolutionRequired, // Broken state requiring user intervention
}

impl fmt::Display for RecoveryDisposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::NothingRequired => "nothing_required",
            Self::ResumeLocal => "resume_local",
            Self::AwaitUserRetry => "await_user_retry",
            Self::InspectRemoteResult => "inspect_remote_result",
            Self::ManualResolutionRequired => "manual_resolution_required",
        };
        f.write_str(s)
    }
}

impl FromStr for RecoveryDisposition {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "nothing_required" => Ok(Self::NothingRequired),
            "resume_local" => Ok(Self::ResumeLocal),
            "await_user_retry" => Ok(Self::AwaitUserRetry),
            "inspect_remote_result" => Ok(Self::InspectRemoteResult),
            "manual_resolution_required" => Ok(Self::ManualResolutionRequired),
            other => Err(format!("invalid recovery disposition: {other}")),
        }
    }
}

/// User action guidance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserAction {
    ExplicitRetry,
    InspectAndRepair,
    CompleteRepair,
}

/// Preserved QA failure information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreservedQaFailure {
    pub checks: Vec<QaCheckSummary>,
}

/// Summary of a single QA check result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaCheckSummary {
    pub id: String,
    pub check_type: String,
    pub status: String,
    pub label: String,
}

/// Classification result for a single incomplete job.
/// Explains what happened, why, what state is safe, and what user can do.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryClassification {
    pub job_type: String,
    pub job_id: String,
    pub disposition: String,
    pub explanation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserved_failure_info: Option<PreservedQaFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_version_id: Option<String>,
    pub user_action: Option<String>,
}

/// Payload for get_project_recovery_state command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecoveryState {
    pub project_id: String,
    pub classifications: Vec<RecoveryClassification>,
    pub has_incomplete_jobs: bool,
}
