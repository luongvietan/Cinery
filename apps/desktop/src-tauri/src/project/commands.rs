use crate::error::{AppCommandError, AppError};
use crate::project::model::ProjectSummary;
use crate::project::recent::{self, RecentProject};
use crate::project::service::{self, ProjectService};
use std::path::PathBuf;
use tauri::Manager;

/// Creates a new project on disk and registers it as the most recent.
#[tauri::command]
pub fn create_project(
    app: tauri::AppHandle,
    root_path: String,
    name: String,
) -> Result<ProjectSummary, AppCommandError> {
    service::validate_root_path(&root_path)?;
    let root = PathBuf::from(root_path);
    let summary = ProjectService::create(&root, &name)?;
    record_recent(&app, &summary)?;
    Ok(summary)
}

/// Opens an existing project and registers it as the most recent.
#[tauri::command]
pub fn open_project(
    app: tauri::AppHandle,
    root_path: String,
) -> Result<ProjectSummary, AppCommandError> {
    service::validate_root_path(&root_path)?;
    let root = PathBuf::from(root_path);
    let summary = ProjectService::open(&root)?;
    record_recent(&app, &summary)?;
    Ok(summary)
}

/// Lists the global recent-projects registry, most recently opened first.
#[tauri::command]
pub fn list_recent_projects(app: tauri::AppHandle) -> Result<Vec<RecentProject>, AppCommandError> {
    let config_dir = app_config_dir(&app)?;
    recent::list_recent_projects(&config_dir).map_err(Into::into)
}

fn record_recent(app: &tauri::AppHandle, summary: &ProjectSummary) -> Result<(), AppCommandError> {
    let config_dir = app_config_dir(app)?;
    recent::record_recent_project(&config_dir, summary)?;
    Ok(())
}

fn app_config_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppCommandError> {
    app.path()
        .app_config_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()).into())
}
