use crate::error::AppCommandError;
use crate::workflow::model::{WorkflowCharacterOption, WorkflowRunDetail, WorkflowRunRecord};
use crate::workflow::runtime::WorkflowRuntime;
use serde_json::Value;

#[tauri::command]
pub fn create_workflow_run(
    project_root_path: String,
    skill_id: String,
    skill_version: String,
    operation_id: String,
    input: Value,
) -> Result<WorkflowRunDetail, AppCommandError> {
    WorkflowRuntime::create_run(
        std::path::Path::new(&project_root_path),
        &skill_id,
        &skill_version,
        &operation_id,
        input,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn advance_workflow_run(
    project_root_path: String,
    workflow_run_id: String,
) -> Result<WorkflowRunDetail, AppCommandError> {
    let root = std::path::PathBuf::from(&project_root_path);
    let detail =
        WorkflowRuntime::advance_run(std::path::Path::new(&project_root_path), &workflow_run_id)
            .map_err(AppCommandError::from)?;
    // P10.1: if this advance handed a provider job to the background
    // runner, wake it so polling starts immediately.
    crate::workflow::background::wake_runner(&root);
    Ok(detail)
}

#[tauri::command]
pub fn approve_workflow_step(
    project_root_path: String,
    workflow_run_id: String,
    step_definition_id: String,
    note: Option<String>,
) -> Result<WorkflowRunDetail, AppCommandError> {
    WorkflowRuntime::approve_run_step(
        std::path::Path::new(&project_root_path),
        &workflow_run_id,
        &step_definition_id,
        note,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn reject_workflow_step(
    project_root_path: String,
    workflow_run_id: String,
    step_definition_id: String,
    note: Option<String>,
) -> Result<WorkflowRunDetail, AppCommandError> {
    WorkflowRuntime::reject_run_step(
        std::path::Path::new(&project_root_path),
        &workflow_run_id,
        &step_definition_id,
        note,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn cancel_workflow_run(
    project_root_path: String,
    workflow_run_id: String,
) -> Result<WorkflowRunDetail, AppCommandError> {
    WorkflowRuntime::cancel_run(std::path::Path::new(&project_root_path), &workflow_run_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn get_workflow_run(
    project_root_path: String,
    workflow_run_id: String,
) -> Result<WorkflowRunDetail, AppCommandError> {
    WorkflowRuntime::get_run(std::path::Path::new(&project_root_path), &workflow_run_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_workflow_runs(
    project_root_path: String,
) -> Result<Vec<WorkflowRunRecord>, AppCommandError> {
    WorkflowRuntime::list_runs(std::path::Path::new(&project_root_path)).map_err(Into::into)
}

#[tauri::command]
pub fn list_workflow_characters(
    project_root_path: String,
) -> Result<Vec<WorkflowCharacterOption>, AppCommandError> {
    WorkflowRuntime::list_characters(std::path::Path::new(&project_root_path)).map_err(Into::into)
}
