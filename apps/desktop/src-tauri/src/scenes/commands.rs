use crate::error::AppCommandError;
use crate::project::service as project_service;
use crate::assets::model::AssetRecord;
use crate::scenes::model::{
    ResolvedSceneReference, ResolvedSceneReferences, Scene, SceneCharacterAssignment,
    ScenePropAssignment, SceneReadiness, SceneTbdBinding, TbdDecisionKind,
};
use crate::scenes::service::SceneService;
use std::path::PathBuf;

#[tauri::command]
pub fn create_scene(
    project_root_path: String,
    title: String,
    summary: String,
) -> Result<Scene, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::create_scene(&root, &title, &summary).map_err(Into::into)
}

#[tauri::command]
pub fn list_scenes(project_root_path: String) -> Result<Vec<Scene>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::list_scenes(&root).map_err(Into::into)
}

#[tauri::command]
pub fn get_scene(
    project_root_path: String,
    scene_id: String,
) -> Result<Scene, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::get_scene(&root, &scene_id).map_err(Into::into)
}

#[tauri::command]
pub fn update_scene_details(
    project_root_path: String,
    scene_id: String,
    title: String,
    summary: String,
) -> Result<Scene, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::update_scene_details(&root, &scene_id, &title, &summary).map_err(Into::into)
}

#[tauri::command]
pub fn assign_scene_world(
    project_root_path: String,
    scene_id: String,
    world_id: String,
) -> Result<Scene, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::assign_scene_world(&root, &scene_id, &world_id).map_err(Into::into)
}

#[tauri::command]
pub fn clear_scene_world(
    project_root_path: String,
    scene_id: String,
) -> Result<Scene, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::clear_scene_world(&root, &scene_id).map_err(Into::into)
}

#[tauri::command]
pub fn add_scene_character(
    project_root_path: String,
    scene_id: String,
    character_entity_id: String,
    look_asset_version_id: String,
    sheet_asset_version_id: Option<String>,
    notes: Option<String>,
) -> Result<SceneCharacterAssignment, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::add_scene_character(
        &root,
        &scene_id,
        &character_entity_id,
        &look_asset_version_id,
        sheet_asset_version_id.as_deref(),
        notes.as_deref(),
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn remove_scene_character(
    project_root_path: String,
    scene_id: String,
    character_entity_id: String,
) -> Result<(), AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::remove_scene_character(&root, &scene_id, &character_entity_id).map_err(Into::into)
}

#[tauri::command]
pub fn list_scene_characters(
    project_root_path: String,
    scene_id: String,
) -> Result<Vec<SceneCharacterAssignment>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::list_scene_characters(&root, &scene_id).map_err(Into::into)
}

#[tauri::command]
pub fn add_scene_prop(
    project_root_path: String,
    scene_id: String,
    prop_asset_version_id: String,
    label: Option<String>,
    notes: Option<String>,
) -> Result<ScenePropAssignment, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::add_scene_prop(
        &root,
        &scene_id,
        &prop_asset_version_id,
        label.as_deref(),
        notes.as_deref(),
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn remove_scene_prop(
    project_root_path: String,
    scene_id: String,
    prop_asset_version_id: String,
) -> Result<(), AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::remove_scene_prop(&root, &scene_id, &prop_asset_version_id).map_err(Into::into)
}

#[tauri::command]
pub fn list_scene_props(
    project_root_path: String,
    scene_id: String,
) -> Result<Vec<ScenePropAssignment>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::list_scene_props(&root, &scene_id).map_err(Into::into)
}

#[tauri::command]
pub fn resolve_scene_references(
    project_root_path: String,
    scene_id: String,
) -> Result<ResolvedSceneReferences, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::resolve_scene_references(&root, &scene_id).map_err(Into::into)
}

#[tauri::command]
pub fn upgrade_scene_world_reference(
    project_root_path: String,
    scene_id: String,
) -> Result<ResolvedSceneReference, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::upgrade_scene_world_reference(&root, &scene_id).map_err(Into::into)
}

#[tauri::command]
pub fn upgrade_scene_character_look_reference(
    project_root_path: String,
    scene_id: String,
    assignment_id: String,
) -> Result<ResolvedSceneReference, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::upgrade_scene_character_look_reference(&root, &scene_id, &assignment_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn upgrade_scene_character_sheet_reference(
    project_root_path: String,
    scene_id: String,
    assignment_id: String,
) -> Result<ResolvedSceneReference, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::upgrade_scene_character_sheet_reference(&root, &scene_id, &assignment_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn upgrade_scene_prop_reference(
    project_root_path: String,
    scene_id: String,
    assignment_id: String,
) -> Result<ResolvedSceneReference, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::upgrade_scene_prop_reference(&root, &scene_id, &assignment_id).map_err(Into::into)
}

#[tauri::command]
pub fn get_scene_readiness(
    project_root_path: String,
    scene_id: String,
) -> Result<SceneReadiness, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::get_scene_readiness(&root, &scene_id).map_err(Into::into)
}

#[tauri::command]
pub fn ensure_scene_keyframe_asset(
    project_root_path: String,
    scene_id: String,
) -> Result<AssetRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::ensure_scene_keyframe_asset(&root, &scene_id).map_err(Into::into)
}

#[tauri::command]
pub fn set_scene_tbd_binding(
    project_root_path: String,
    scene_id: String,
    tbd_id: String,
    decision: String,
    justification: Option<String>,
) -> Result<SceneTbdBinding, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    let kind: TbdDecisionKind = decision.parse().map_err(|e: String| {
        AppCommandError {
            code: "INVALID_TBD_DECISION".into(),
            message: e,
        }
    })?;
    SceneService::set_scene_tbd_binding(&root, &scene_id, &tbd_id, kind, justification).map_err(Into::into)
}

#[tauri::command]
pub fn remove_scene_tbd_binding(
    project_root_path: String,
    scene_id: String,
    tbd_id: String,
) -> Result<(), AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::remove_scene_tbd_binding(&root, &scene_id, &tbd_id).map_err(Into::into)
}

#[tauri::command]
pub fn list_scene_tbd_bindings(
    project_root_path: String,
    scene_id: String,
) -> Result<Vec<SceneTbdBinding>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    SceneService::list_scene_tbd_bindings(&root, &scene_id).map_err(Into::into)
}
