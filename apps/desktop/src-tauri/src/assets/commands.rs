use crate::assets::model::{
    AssetRecord, AssetSummaryRecord, AssetVersionRecord, AssetWithVersions,
    CanonicalPromotionResult,
};
use crate::assets::service::AssetService;
use crate::error::AppCommandError;
use crate::project::service as project_service;
use std::path::PathBuf;

/// Creates a new asset in the project rooted at `project_root_path`.
#[tauri::command]
pub fn create_asset(
    project_root_path: String,
    asset_type: String,
    label: String,
    owner_entity_id: Option<String>,
) -> Result<AssetRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    AssetService::create_asset(&root, &asset_type, &label, owner_entity_id).map_err(Into::into)
}

/// Lists every asset in the project rooted at `project_root_path`.
#[tauri::command]
pub fn list_assets(project_root_path: String) -> Result<Vec<AssetSummaryRecord>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    AssetService::list_assets(&root).map_err(Into::into)
}

/// Fetches a single asset and all of its versions.
#[tauri::command]
pub fn get_asset_with_versions(
    project_root_path: String,
    asset_id: String,
) -> Result<AssetWithVersions, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    AssetService::get_asset_with_versions(&root, &asset_id).map_err(Into::into)
}

/// Promotes one existing asset version to canonical. Any different current
/// canonical remains on disk and is marked superseded by the backend
/// transaction.
#[tauri::command]
pub fn promote_asset_version(
    project_root_path: String,
    asset_version_id: String,
) -> Result<CanonicalPromotionResult, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    AssetService::promote_asset_version(&root, &asset_version_id).map_err(Into::into)
}

/// Imports `source_path` as a brand-new, immutable version of `asset_id` in
/// the project rooted at `project_root_path`.
#[tauri::command]
pub fn import_asset_version(
    project_root_path: String,
    asset_id: String,
    source_path: String,
    parent_version_id: Option<String>,
) -> Result<AssetVersionRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    let source = PathBuf::from(source_path);
    AssetService::import_asset_version(&root, &asset_id, &source, parent_version_id)
        .map_err(Into::into)
}
