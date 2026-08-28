use crate::error::AppCommandError;
use crate::integration::readiness::{
    get_project_overview as derive_project_overview, ProjectOverview,
};
use crate::project::service::validate_root_path;
use std::path::Path;

#[tauri::command]
pub fn get_project_health(project_root_path: String) -> Result<Vec<crate::integration::health::ProjectHealthIssue>, AppCommandError> {
    validate_root_path(&project_root_path)?;
    crate::integration::health::scan_project(Path::new(&project_root_path)).map_err(AppCommandError::from)
}

#[tauri::command]
pub fn get_provenance_graph(project_root_path: String, target_kind: String, target_id: String) -> Result<crate::integration::provenance::ProvenanceGraph, AppCommandError> {
    validate_root_path(&project_root_path)?;
    crate::integration::provenance::get_provenance_graph(Path::new(&project_root_path), &target_kind, &target_id).map_err(AppCommandError::from)
}

/// Returns a derived, read-only project production overview. The project root
/// remains the public desktop scope boundary; the backend reads its own
/// durable records and never accepts a caller-supplied project id.
#[tauri::command]
pub fn get_project_overview(project_root_path: String) -> Result<ProjectOverview, AppCommandError> {
    validate_root_path(&project_root_path)?;
    derive_project_overview(Path::new(&project_root_path)).map_err(AppCommandError::from)
}
