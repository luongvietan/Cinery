use crate::error::{AppCommandError, AppError};
use crate::project::model::ProjectSummary;
use crate::project::recent::{self, RecentProject};
use crate::project::service::{self, ProjectService};
use std::path::{Path, PathBuf};
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
    record_recent(&app, &summary);
    allow_asset_protocol_access(&app, &root);
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
    record_recent(&app, &summary);
    allow_asset_protocol_access(&app, &root);
    Ok(summary)
}

/// Extends the asset protocol's filesystem scope to cover `root`, best-effort.
///
/// Project roots are arbitrary user-chosen directories (picked via the native
/// file dialog), so they can't be known ahead of time in `tauri.conf.json`'s
/// static `security.assetProtocol.scope`. Instead the scope starts empty and
/// each project directory the user actually opens or creates is added here at
/// runtime, so the webview can load thumbnails from it (via `convertFileSrc`)
/// without granting access to the rest of the filesystem.
///
/// Like `record_recent`, a failure here must never fail the create/open the
/// user is waiting on: it only means thumbnails won't render for this
/// session, not that the project itself is broken.
fn allow_asset_protocol_access(app: &tauri::AppHandle, root: &Path) {
    if let Err(e) = app.asset_protocol_scope().allow_directory(root, true) {
        eprintln!("failed to extend asset protocol scope for project root (non-fatal): {e}");
    }
}

/// Lists the global recent-projects registry, most recently opened first.
#[tauri::command]
pub fn list_recent_projects(app: tauri::AppHandle) -> Result<Vec<RecentProject>, AppCommandError> {
    let config_dir = app_config_dir(&app)?;
    recent::list_recent_projects(&config_dir).map_err(Into::into)
}

/// Records `summary` in the recent-projects registry, best-effort.
///
/// The registry is a purely cosmetic convenience cache: a failure here
/// (e.g. an unwritable config directory, or a corrupted registry file
/// that still can't be repaired) must never fail the create/open the user
/// is actually waiting on, so errors are logged and swallowed rather than
/// propagated to the caller.
fn record_recent(app: &tauri::AppHandle, summary: &ProjectSummary) {
    let config_dir = match app_config_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("could not resolve app config dir for recent-projects registry: {e:?}");
            return;
        }
    };

    if let Err(e) = recent::record_recent_project(&config_dir, summary) {
        eprintln!("failed to record recent project (non-fatal): {e}");
    }
}

fn app_config_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppCommandError> {
    app.path()
        .app_config_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()).into())
}

/// Creates a project without Tauri app-handle side effects (recent registry
/// and asset-protocol scope). Used by tests that exercise the public
/// command boundary; production always uses `create_project`.
#[tauri::command]
pub fn create_project_standalone(root_path: String, name: String) -> Result<ProjectSummary, AppCommandError> {
    service::validate_root_path(&root_path)?;
    ProjectService::create(&PathBuf::from(root_path), &name).map_err(Into::into)
}

/// Opens a project without Tauri app-handle side effects. See
/// [`create_project_standalone`].
#[tauri::command]
pub fn open_project_standalone(root_path: String) -> Result<ProjectSummary, AppCommandError> {
    service::validate_root_path(&root_path)?;
    ProjectService::open(&PathBuf::from(root_path)).map_err(Into::into)
}
