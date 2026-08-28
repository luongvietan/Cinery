use super::models::ProjectRecoveryState;
use super::service::RecoveryService;
use crate::error::AppCommandError;
use crate::project::service::validate_root_path;
use std::path::Path;

/// Get recovery state for a project: all incomplete jobs and their classifications.
/// Called when a project opens to determine what recovery actions are needed.
#[tauri::command]
pub fn get_project_recovery_state(
    project_root_path: String,
) -> Result<ProjectRecoveryState, AppCommandError> {
    validate_root_path(&project_root_path)?;
    RecoveryService::scan_incomplete_jobs(Path::new(&project_root_path))
}
