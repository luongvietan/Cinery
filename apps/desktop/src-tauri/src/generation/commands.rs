use super::model::{GeneratedArtifactDetail, GenerationResultSetDetail};
use super::service::GenerationService;
use crate::assets::model::AssetVersionRecord;
use crate::error::AppCommandError;
use crate::project::service as project_service;
use std::path::PathBuf;

#[tauri::command]
pub fn list_generation_results(
    project_root_path: String,
    workflow_run_id: Option<String>,
) -> Result<Vec<GenerationResultSetDetail>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    GenerationService::list_results(&PathBuf::from(project_root_path), workflow_run_id.as_deref())
        .map_err(Into::into)
}

#[tauri::command]
pub fn get_generated_artifact(
    project_root_path: String,
    artifact_id: String,
) -> Result<GeneratedArtifactDetail, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    GenerationService::get_artifact_detail(&PathBuf::from(project_root_path), &artifact_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn promote_generated_artifact(
    project_root_path: String,
    artifact_id: String,
    target_asset_id: String,
    set_canonical: bool,
) -> Result<AssetVersionRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    GenerationService::promote_generated_artifact(
        &PathBuf::from(project_root_path),
        &artifact_id,
        &target_asset_id,
        set_canonical,
    )
    .map_err(Into::into)
}
