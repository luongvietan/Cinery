use crate::error::AppCommandError;
use crate::integration::readiness::{
    get_project_overview as derive_project_overview, ProjectOverview,
};
use crate::project::service::validate_root_path;
use std::path::Path;

/// Returns a derived, read-only project production overview. The project root
/// remains the public desktop scope boundary; the backend reads its own
/// durable records and never accepts a caller-supplied project id.
#[tauri::command]
pub fn get_project_overview(project_root_path: String) -> Result<ProjectOverview, AppCommandError> {
    validate_root_path(&project_root_path)?;
    derive_project_overview(Path::new(&project_root_path)).map_err(AppCommandError::from)
}
