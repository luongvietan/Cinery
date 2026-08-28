use super::models::{QaReviewStatus, QaRunDetail, QaRunRecord};
use super::service::QaService;
use crate::error::{AppCommandError, AppError};
use std::path::Path;
use std::str::FromStr;

#[tauri::command]
pub fn list_qa_runs(
    project_root_path: String,
    asset_version_id: String,
) -> Result<Vec<QaRunRecord>, AppCommandError> {
    QaService::list_runs(Path::new(&project_root_path), &asset_version_id).map_err(Into::into)
}

#[tauri::command]
pub fn get_qa_run(
    project_root_path: String,
    qa_run_id: String,
) -> Result<QaRunDetail, AppCommandError> {
    QaService::get_run(Path::new(&project_root_path), &qa_run_id).map_err(Into::into)
}

#[tauri::command]
pub fn review_qa_check(
    project_root_path: String,
    qa_run_id: String,
    check_id: String,
    review_status: String,
    note: Option<String>,
) -> Result<QaRunDetail, AppCommandError> {
    let status = QaReviewStatus::from_str(&review_status)
        .map_err(|error| AppCommandError::from(AppError::InvalidQaData(error)))?;
    QaService::review_run_check(
        Path::new(&project_root_path),
        &qa_run_id,
        &check_id,
        status,
        note.as_deref(),
    )
    .map_err(Into::into)
}
