use serde::Serialize;

/// Application-level error type shared across all backend modules.
///
/// Add new variants here as later tasks need them; keep each variant's
/// `#[error(...)]` message user-presentable, since it becomes the
/// `AppCommandError.message` seen by the frontend.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Project name must contain 1 to 120 characters")]
    InvalidProjectName,

    #[error("Project path is empty")]
    InvalidProjectPath,

    #[error("Project directory is not empty")]
    ProjectDirectoryNotEmpty,

    #[error("Directory is not an AI Cinematic Production OS project")]
    InvalidProjectDirectory,

    #[error("Project manifest does not match project database")]
    ProjectIdentityMismatch,

    #[error("Filesystem operation failed: {0}")]
    FileSystem(String),

    #[error("Database operation failed: {0}")]
    Database(String),
}

/// Serializable error shape sent across the Tauri IPC boundary.
///
/// `code` is a stable SCREAMING_SNAKE_CASE identifier derived from the
/// `AppError` variant name, e.g. `PROJECT_DIRECTORY_NOT_EMPTY`.
#[derive(Debug, Serialize)]
pub struct AppCommandError {
    pub code: String,
    pub message: String,
}

impl AppError {
    /// Stable SCREAMING_SNAKE_CASE identifier for this error variant.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::InvalidProjectName => "INVALID_PROJECT_NAME",
            AppError::InvalidProjectPath => "INVALID_PROJECT_PATH",
            AppError::ProjectDirectoryNotEmpty => "PROJECT_DIRECTORY_NOT_EMPTY",
            AppError::InvalidProjectDirectory => "INVALID_PROJECT_DIRECTORY",
            AppError::ProjectIdentityMismatch => "PROJECT_IDENTITY_MISMATCH",
            AppError::FileSystem(_) => "FILE_SYSTEM_ERROR",
            AppError::Database(_) => "DATABASE_ERROR",
        }
    }
}

impl From<AppError> for AppCommandError {
    fn from(error: AppError) -> Self {
        AppCommandError {
            code: error.code().to_string(),
            message: error.to_string(),
        }
    }
}
