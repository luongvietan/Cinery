use crate::assets::model::{AssetRecord, AssetWithVersions};
use crate::assets::service::AssetService;
use crate::error::AppCommandError;
use std::path::PathBuf;

/// Creates a new asset in the project rooted at `project_root_path`.
#[tauri::command]
pub fn create_asset(
    project_root_path: String,
    asset_type: String,
    label: String,
    owner_entity_id: Option<String>,
) -> Result<AssetRecord, AppCommandError> {
    let root = PathBuf::from(project_root_path);
    AssetService::create_asset(&root, &asset_type, &label, owner_entity_id).map_err(Into::into)
}

/// Lists every asset in the project rooted at `project_root_path`.
#[tauri::command]
pub fn list_assets(project_root_path: String) -> Result<Vec<AssetRecord>, AppCommandError> {
    let root = PathBuf::from(project_root_path);
    AssetService::list_assets(&root).map_err(Into::into)
}

/// Fetches a single asset and all of its versions.
#[tauri::command]
pub fn get_asset_with_versions(
    project_root_path: String,
    asset_id: String,
) -> Result<AssetWithVersions, AppCommandError> {
    let root = PathBuf::from(project_root_path);
    AssetService::get_asset_with_versions(&root, &asset_id).map_err(Into::into)
}
