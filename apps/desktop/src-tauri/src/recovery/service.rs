use super::models::{ProjectRecoveryState, RecoveryClassification, RecoveryDisposition};
use crate::error::{AppCommandError, AppError};
use crate::project::{paths, repository as project_repository};
use crate::workflow::repository::WorkflowRepository;
use rusqlite::Connection;
use std::path::Path;

/// Recovery service for classifying incomplete jobs and determining recovery actions.
pub struct RecoveryService;

impl RecoveryService {
    /// Scan all incomplete jobs in a project and return their recovery classifications.
    pub fn scan_incomplete_jobs(
        project_root_path: &Path,
    ) -> Result<ProjectRecoveryState, AppCommandError> {
        // Validate path and read manifest
        let manifest = paths::read_manifest(project_root_path).map_err(AppCommandError::from)?;
        let conn =
            crate::db::open_existing_connection(&project_root_path.join("project.db")).map_err(AppCommandError::from)?;
        let project = project_repository::read_project(&conn).map_err(AppCommandError::from)?;

        if project.id != manifest.project_id {
            return Err(AppCommandError::from(AppError::ProjectIdentityMismatch));
        }

        let mut classifications = Vec::new();

        // Scan workflow runs
        let workflow_classifications =
            Self::scan_workflow_runs(&conn, &project.id).unwrap_or_default();
        classifications.extend(workflow_classifications);

        // Scan provider executions
        let provider_classifications =
            Self::scan_provider_executions(&conn, &project.id).unwrap_or_default();
        classifications.extend(provider_classifications);

        let has_incomplete_jobs = !classifications.is_empty();

        Ok(ProjectRecoveryState {
            project_id: project.id,
            classifications,
            has_incomplete_jobs,
        })
    }

    /// Scan all workflow runs and classify incomplete ones.
    fn scan_workflow_runs(
        conn: &Connection,
        project_id: &str,
    ) -> Result<Vec<RecoveryClassification>, AppError> {
        let mut classifications = Vec::new();

        // Get all workflow runs
        let runs = WorkflowRepository::list_runs(conn, project_id)?;

        for run in runs {
            // Only classify incomplete runs
            if !is_workflow_complete(&run.status) {
                let classification = Self::classify_workflow_run(&run);
                classifications.push(classification);
            }
        }

        Ok(classifications)
    }

    /// Scan all provider executions and classify incomplete ones.
    fn scan_provider_executions(
        conn: &Connection,
        project_id: &str,
    ) -> Result<Vec<RecoveryClassification>, AppError> {
        let mut classifications = Vec::new();

        // Query all provider executions for this project
        let mut statement = conn
            .prepare(
                "SELECT id, step_definition_id, attempt_number, provider_id, model_id, adapter_version,
                        status, provider_job_id, normalized_error_json, started_at, completed_at
                 FROM provider_executions WHERE project_id = ?1",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let executions = statement
            .query_map([project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,  // id
                    row.get::<_, String>(6)?,  // status
                    row.get::<_, Option<String>>(8)?,  // normalized_error_json
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))?;

        for (exec_id, status, error_json) in executions {
            // Only classify incomplete executions
            if !is_provider_complete(&status) {
                // Check if asset version was created for this execution
                let asset_created = check_asset_for_provider_execution(conn, &exec_id)
                    .unwrap_or(false);

                let classification = Self::classify_provider_execution(&exec_id, &status, asset_created, error_json.as_deref());
                classifications.push(classification);
            }
        }

        Ok(classifications)
    }

    /// Classify a workflow run.
    fn classify_workflow_run(
        run: &crate::workflow::model::WorkflowRunRecord,
    ) -> RecoveryClassification {
        let disposition = match run.status.as_str() {
            "completed" | "cancelled" | "rejected" | "failed" | "waiting_for_approval"
            | "created" | "running" | "ready_for_execution" => RecoveryDisposition::NothingRequired,
            _ => RecoveryDisposition::NothingRequired,
        };

        let explanation = match run.status.as_str() {
            "completed" => "Workflow completed successfully.".to_string(),
            "cancelled" => "Workflow was cancelled and remains in that state. It cannot be accidentally resumed.".to_string(),
            "rejected" => "Workflow execution was rejected during validation.".to_string(),
            "failed" => format!(
                "Workflow failed: {}. The project state is safe. You can start a new workflow run.",
                run.failure_message.as_deref().unwrap_or("Unknown error")
            ),
            "waiting_for_approval" => {
                format!(
                    "Workflow is awaiting approval at step {}. Your canonical data was not changed. Provide approval to continue or start a new run.",
                    run.current_step_index
                )
            }
            "created" => "Workflow was created but never started.".to_string(),
            "running" | "ready_for_execution" => {
                "Workflow execution was interrupted. The canonical data was not mutated. You can inspect the incomplete run or start a new one.".to_string()
            }
            _ => "Workflow is in an unknown state.".to_string(),
        };

        RecoveryClassification {
            job_type: "workflow".to_string(),
            job_id: run.id.clone(),
            disposition: disposition.to_string(),
            explanation,
            preserved_failure_info: None,
            parent_version_id: None,
            user_action: None,
        }
    }

    /// Classify a provider execution.
    fn classify_provider_execution(
        exec_id: &str,
        status: &str,
        asset_version_created: bool,
        error_json: Option<&str>,
    ) -> RecoveryClassification {
        let (disposition, user_action) = if status == "succeeded" {
            (RecoveryDisposition::NothingRequired, None)
        } else if status == "cancelled" || status == "cancellation_requested" {
            (RecoveryDisposition::NothingRequired, None)
        } else if status == "failed" {
            if asset_version_created {
                // CRITICAL: phantom asset created on provider failure
                (RecoveryDisposition::ManualResolutionRequired, None)
            } else {
                // Safe: no asset created, user must explicitly retry
                (RecoveryDisposition::AwaitUserRetry, Some("explicit_retry".to_string()))
            }
        } else if status == "queued" || status == "submitted" || status == "running" {
            // Provider call in progress: check remote state
            (RecoveryDisposition::InspectRemoteResult, None)
        } else {
            (RecoveryDisposition::InspectRemoteResult, None)
        };

        let explanation = match status {
            status if status == "succeeded" => "Provider call succeeded.".to_string(),
            status if status == "cancelled" || status == "cancellation_requested" => {
                "Provider call was cancelled and cannot be resumed.".to_string()
            }
            status if status == "failed" => {
                if asset_version_created {
                    format!(
                        "Provider failed, but an AssetVersion was created (phantom asset). This should not happen. {}",
                        error_json.unwrap_or("No error details available.")
                    )
                } else {
                    format!(
                        "Provider failed: {}. no output asset was created. Retry only via explicit user action.",
                        error_json.unwrap_or("Unknown error")
                    )
                }
            }
            _ => format!(
                "Provider call is in progress ({}). Check remote provider status and fetch result.",
                status
            ),
        };

        RecoveryClassification {
            job_type: "provider".to_string(),
            job_id: exec_id.to_string(),
            disposition: disposition.to_string(),
            explanation,
            preserved_failure_info: None,
            parent_version_id: None,
            user_action,
        }
    }
}

/// Check if a workflow status is terminal/complete.
fn is_workflow_complete(status: &str) -> bool {
    matches!(status, "completed" | "cancelled" | "rejected" | "failed")
}

/// Check if a provider status is terminal/complete.
fn is_provider_complete(status: &str) -> bool {
    matches!(
        status,
        "succeeded" | "failed" | "cancelled" | "cancellation_requested"
    )
}

/// Check if an asset version was created for a provider execution.
fn check_asset_for_provider_execution(conn: &Connection, exec_id: &str) -> Result<bool, AppError> {
    let mut statement = conn
        .prepare(
            "SELECT 1 FROM asset_versions WHERE generation_artifact_id IN (
               SELECT artifact_id FROM generation_artifacts WHERE provider_execution_id = ?1
             ) LIMIT 1",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let exists = statement
        .exists([exec_id])
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(exists)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_workflow_waiting_approval() {
        let run = crate::workflow::model::WorkflowRunRecord {
            id: "run-1".to_string(),
            project_id: "proj-1".to_string(),
            skill_id: "face-lock".to_string(),
            skill_version: "1.0.0".to_string(),
            operation_id: "op-1".to_string(),
            status: "waiting_for_approval".to_string(),
            input_json: "{}".to_string(),
            prerequisite_report_json: None,
            context_snapshot_json: Some("{}".to_string()),
            current_step_index: 1,
            failure_code: None,
            failure_message: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            completed_at: None,
        };

        let classification = RecoveryService::classify_workflow_run(&run);
        assert_eq!(
            classification.disposition,
            RecoveryDisposition::NothingRequired.to_string()
        );
        assert!(classification.explanation.contains("awaiting approval"));
    }

    #[test]
    fn test_classify_workflow_cancelled() {
        let run = crate::workflow::model::WorkflowRunRecord {
            id: "run-2".to_string(),
            project_id: "proj-1".to_string(),
            skill_id: "face-lock".to_string(),
            skill_version: "1.0.0".to_string(),
            operation_id: "op-1".to_string(),
            status: "cancelled".to_string(),
            input_json: "{}".to_string(),
            prerequisite_report_json: None,
            context_snapshot_json: Some("{}".to_string()),
            current_step_index: 0,
            failure_code: None,
            failure_message: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            completed_at: Some("2024-01-01T00:01:00Z".to_string()),
        };

        let classification = RecoveryService::classify_workflow_run(&run);
        assert_eq!(
            classification.disposition,
            RecoveryDisposition::NothingRequired.to_string()
        );
        assert!(classification.explanation.contains("cancelled"));
    }

    #[test]
    fn test_classify_provider_failure_no_asset() {
        let classification = RecoveryService::classify_provider_execution(
            "exec-1",
            "failed",
            false,
            Some("{\"code\":\"rate_limit_exceeded\"}"),
        );

        assert_eq!(
            classification.disposition,
            RecoveryDisposition::AwaitUserRetry.to_string()
        );
        assert_eq!(
            classification.user_action,
            Some("explicit_retry".to_string())
        );
        assert!(classification.explanation.contains("no output asset"));
    }

    #[test]
    fn test_classify_provider_failure_with_phantom_asset() {
        let classification = RecoveryService::classify_provider_execution(
            "exec-2",
            "failed",
            true,
            Some("{\"code\":\"server_error\"}"),
        );

        assert_eq!(
            classification.disposition,
            RecoveryDisposition::ManualResolutionRequired.to_string()
        );
        assert!(classification.explanation.contains("phantom"));
    }

    #[test]
    fn test_classify_provider_in_progress() {
        let classification =
            RecoveryService::classify_provider_execution("exec-3", "running", false, None);

        assert_eq!(
            classification.disposition,
            RecoveryDisposition::InspectRemoteResult.to_string()
        );
    }
}
