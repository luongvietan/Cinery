use crate::diagnostics::bundle::{export_bundle, DiagnosticsBundle};
use crate::error::AppCommandError;
use crate::project::service::validate_root_path;
use std::path::Path;

/// Exports a redacted, media-free diagnostics bundle for the project. The
/// bundle is returned to the caller; the backend never writes media or
/// credentials into it.
#[tauri::command]
pub fn export_diagnostics(project_root_path: String) -> Result<DiagnosticsBundle, AppCommandError> {
    validate_root_path(&project_root_path)?;
    export_bundle(Path::new(&project_root_path)).map_err(AppCommandError::from)
}

/// Returns the project's diagnostics folder path so the UI can offer an
/// "Open diagnostics folder" action. The path always points inside the
/// project root the user already chose, so it is platform-safe by
/// construction.
#[tauri::command]
pub fn get_diagnostics_folder(project_root_path: String) -> Result<String, AppCommandError> {
    validate_root_path(&project_root_path)?;
    let folder = Path::new(&project_root_path).join("diagnostics");
    Ok(folder.to_string_lossy().into_owned())
}

/// Appends one structured, redacted event to the project diagnostics log.
#[tauri::command]
pub fn append_diagnostics_log(
    project_root_path: String,
    subsystem: String,
    event: String,
    correlation_id: Option<String>,
    message: String,
) -> Result<(), AppCommandError> {
    validate_root_path(&project_root_path)?;
    crate::diagnostics::bundle::log_event(
        Path::new(&project_root_path),
        &subsystem,
        &event,
        correlation_id.as_deref(),
        &message,
    )
    .map_err(AppCommandError::from)
}
