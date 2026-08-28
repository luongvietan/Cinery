use crate::error::AppError;
use crate::workflow::context::write_snapshot_atomically;
use crate::workflow::execution::{ExecutionRequest, ExecutionResult};
use crate::workflow::model::WorkflowContextSnapshot;
use std::fs;
use std::path::{Path, PathBuf};

pub fn workflow_artifact_dir(project_root: &Path, run_id: &str) -> PathBuf {
    project_root.join("workflows").join(run_id)
}

pub fn write_run_artifacts(
    project_root: &Path,
    run_id: &str,
    context: &WorkflowContextSnapshot,
    request: &ExecutionRequest,
) -> Result<PathBuf, AppError> {
    let dir = workflow_artifact_dir(project_root, run_id);
    fs::create_dir_all(&dir)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;
    write_snapshot_atomically(&dir.join("context-snapshot.json"), context)?;
    write_json(&dir.join("compiled-request.json"), request)?;
    write_text_atomically(&dir.join("compiled-prompt.txt"), &request.prompt)?;
    Ok(dir)
}

pub fn write_dry_run_result(
    project_root: &Path,
    run_id: &str,
    request: &ExecutionRequest,
) -> Result<ExecutionResult, AppError> {
    let dir = workflow_artifact_dir(project_root, run_id);
    let artifact_path = dir.join("dry-run-result.json");
    let result = ExecutionResult {
        kind: "dry_run".into(),
        artifact_path: artifact_path.clone(),
        request: request.clone(),
    };
    write_json(&artifact_path, &result)?;
    Ok(result)
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;
    fs::rename(&temp, path)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))
}

fn write_text_atomically(path: &Path, value: &str) -> Result<(), AppError> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, value)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;
    fs::rename(&temp, path)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))
}
