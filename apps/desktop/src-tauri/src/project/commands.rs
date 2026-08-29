use crate::error::{AppCommandError, AppError};
use crate::project::model::ProjectSummary;
use crate::project::recent::{self, RecentProject};
use crate::project::service::{self, ProjectService};
use std::path::{Path, PathBuf};
use tauri::Manager;

/// Creates a new project on disk and registers it as the most recent.
///
/// The user never picks a folder: when `root_path` is omitted the app
/// derives the project location from the project name itself
/// (`<Documents>/Cinery/<name-slug>`), so creating a project is a
/// name-only flow. An explicit `root_path` stays supported for
/// power users and tests.
#[tauri::command]
pub fn create_project(
    app: tauri::AppHandle,
    root_path: Option<String>,
    name: String,
) -> Result<ProjectSummary, AppCommandError> {
    let root = match root_path.filter(|value| !value.trim().is_empty()) {
        Some(path) => PathBuf::from(path),
        None => default_project_root(&app, &name)?,
    };
    let summary = ProjectService::create(&root, &name)?;
    record_recent(&app, &summary);
    allow_asset_protocol_access(&app, &root);
    Ok(summary)
}

/// Derives a fresh, non-colliding project directory from the project name:
/// `<Documents>/Cinery/<slug>`, then `<slug>-2`, `<slug>-3`, …
fn default_project_root(
    app: &tauri::AppHandle,
    name: &str,
) -> Result<PathBuf, AppCommandError> {
    let documents = app
        .path()
        .document_dir()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;
    let base = documents.join("Cinery");
    let slug = slugify_project_name(name);
    let mut candidate = base.join(&slug);
    let mut counter = 2;
    while candidate.exists() {
        candidate = base.join(format!("{slug}-{counter}"));
        counter += 1;
    }
    Ok(candidate)
}

/// Turns a project name into a filesystem-friendly slug. Non-ASCII names
/// keep their characters where the filesystem allows them; only path-hostile
/// characters are replaced. Falls back to "project" when nothing survives.
fn slugify_project_name(name: &str) -> String {
    let slug: String = name
        .trim()
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character.to_ascii_lowercase(),
            ' ' | '.' => '-',
            other if !other.is_control() && other != '/' && other != '\\' => other,
            _ => '-',
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "project".into()
    } else {
        slug
    }
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
pub fn create_project_standalone(
    root_path: String,
    name: String,
) -> Result<ProjectSummary, AppCommandError> {
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

#[cfg(test)]
mod tests {
    use super::slugify_project_name;

    #[test]
    fn slugifies_names_into_safe_unique_friendly_folders() {
        assert_eq!(slugify_project_name("Night Harbor"), "night-harbor");
        assert_eq!(slugify_project_name("  Mixed_Case-99 "), "mixed_case-99");
        // Path-hostile characters never survive into the folder name.
        assert_eq!(slugify_project_name("a/b\\c"), "a-b-c");
        assert_eq!(slugify_project_name("///"), "project");
        assert_eq!(slugify_project_name(""), "project");
        // Non-ASCII names keep their letters where the filesystem allows.
        assert_eq!(slugify_project_name("Bến cảng"), "bến-cảng");
    }
}
