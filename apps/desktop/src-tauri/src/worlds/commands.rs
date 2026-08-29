use crate::error::AppCommandError;
use crate::project::service as project_service;
use crate::worlds::model::{World, WorldDetail};
use crate::worlds::service::WorldService;
use std::path::PathBuf;

/// Creates a production World for an existing Canon Location.
#[tauri::command]
pub fn create_world(
    project_root_path: String,
    canon_location_entity_id: String,
) -> Result<World, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    WorldService::create_world(&root, &canon_location_entity_id).map_err(Into::into)
}

/// Lists Worlds for the project (persistence rows). Use detailed variants in UI if needed.
#[tauri::command]
pub fn list_worlds(project_root_path: String) -> Result<Vec<World>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    WorldService::list_worlds(&root).map_err(Into::into)
}

/// Gets a single World by id.
#[tauri::command]
pub fn get_world(project_root_path: String, world_id: String) -> Result<World, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    WorldService::get_world(&root, &world_id).map_err(Into::into)
}

/// Lists Worlds enriched with Location display data and World Plate Asset, without copying narrative.
#[tauri::command]
pub fn list_worlds_detailed(
    project_root_path: String,
) -> Result<Vec<WorldDetail>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    WorldService::list_worlds_detailed(&root).map_err(Into::into)
}

/// Gets a single World enriched with Location display data and World Plate Asset.
#[tauri::command]
pub fn get_world_detailed(
    project_root_path: String,
    world_id: String,
) -> Result<WorldDetail, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    WorldService::get_world_detailed(&root, &world_id).map_err(Into::into)
}
