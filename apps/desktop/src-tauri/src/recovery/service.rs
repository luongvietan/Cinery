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

        // TODO: Scan provider executions (requires provider execution repository methods)
        // TODO: Scan QA runs (requires QA repository methods)

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

    /// Classify a workflow run.
    fn classify_workflow_run(
        run: &crate::workflow::model::WorkflowRunRecord,
    ) -> RecoveryClassification {
        let disposition = match run.status.as_str() {
            "completed" => RecoveryDisposition::NothingRequired,
            "cancelled" => RecoveryDisposition::NothingRequired,
            "rejected" => RecoveryDisposition::NothingRequired,
            "failed" => RecoveryDisposition::NothingRequired,
            "waiting_for_approval" => RecoveryDisposition::NothingRequired,
            "created" => RecoveryDisposition::NothingRequired,
            "running" | "ready_for_execution" => RecoveryDisposition::NothingRequired,
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
}

/// Check if a workflow status is terminal/complete.
fn is_workflow_complete(status: &str) -> bool {
    matches!(status, "completed" | "cancelled" | "rejected" | "failed")
}
