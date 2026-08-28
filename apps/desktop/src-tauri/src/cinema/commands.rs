use crate::cinema::model::{
    CinemaCompilation, SceneCharacterRecord, ScenePropRecord, SceneRecord, ShotRecord,
};
use crate::cinema::service::CinemaService;
use crate::error::AppCommandError;
use crate::project::service::validate_root_path;
use serde::Serialize;
use std::path::Path;

/// Full scene payload returned by `get_scene`: the scene plus its cast,
/// props, and shots in deterministic order.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneDetail {
    pub scene: SceneRecord,
    pub characters: Vec<SceneCharacterRecord>,
    pub props: Vec<ScenePropRecord>,
    pub shots: Vec<ShotRecord>,
}

fn root_path(project_root_path: &str) -> Result<&Path, AppCommandError> {
    validate_root_path(project_root_path)?;
    Ok(Path::new(project_root_path))
}

#[tauri::command]
pub fn create_scene(
    project_root_path: String,
    title: String,
    world_asset_version_id: Option<String>,
    canon_notes: Option<String>,
) -> Result<SceneRecord, AppCommandError> {
    CinemaService::create_scene(
        root_path(&project_root_path)?,
        &title,
        world_asset_version_id,
        canon_notes,
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn stage_scene(
    project_root_path: String,
    title: String,
    world_asset_version_id: String,
    character_entity_id: String,
    look_asset_version_id: String,
    sheet_asset_version_id: String,
) -> Result<SceneRecord, AppCommandError> {
    CinemaService::stage_scene(
        root_path(&project_root_path)?,
        &title,
        &world_asset_version_id,
        &character_entity_id,
        &look_asset_version_id,
        &sheet_asset_version_id,
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn list_scenes(project_root_path: String) -> Result<Vec<SceneRecord>, AppCommandError> {
    CinemaService::list_scenes(root_path(&project_root_path)?).map_err(AppCommandError::from)
}

#[tauri::command]
pub fn get_scene(
    project_root_path: String,
    scene_id: String,
) -> Result<SceneDetail, AppCommandError> {
    scene_detail(root_path(&project_root_path)?, &scene_id)
}

#[tauri::command]
pub fn add_scene_character(
    project_root_path: String,
    scene_id: String,
    character_entity_id: String,
    look_asset_version_id: String,
    sheet_asset_version_id: Option<String>,
) -> Result<SceneDetail, AppCommandError> {
    let root = root_path(&project_root_path)?;
    CinemaService::add_character_to_scene(
        root,
        &scene_id,
        &character_entity_id,
        &look_asset_version_id,
        sheet_asset_version_id,
    )
    .map_err(AppCommandError::from)?;
    scene_detail(root, &scene_id)
}

#[tauri::command]
pub fn add_scene_prop(
    project_root_path: String,
    scene_id: String,
    prop_asset_version_id: String,
) -> Result<SceneDetail, AppCommandError> {
    let root = root_path(&project_root_path)?;
    CinemaService::add_prop_to_scene(root, &scene_id, &prop_asset_version_id)
        .map_err(AppCommandError::from)?;
    scene_detail(root, &scene_id)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_shot(
    project_root_path: String,
    scene_id: String,
    ordering: Option<i64>,
    duration_seconds: f64,
    intent: String,
    action: Option<String>,
    camera: Option<String>,
) -> Result<ShotRecord, AppCommandError> {
    CinemaService::create_shot(
        root_path(&project_root_path)?,
        &scene_id,
        ordering,
        duration_seconds,
        &intent,
        action,
        camera,
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn list_shots(
    project_root_path: String,
    scene_id: String,
) -> Result<Vec<ShotRecord>, AppCommandError> {
    CinemaService::list_shots(root_path(&project_root_path)?, &scene_id).map_err(AppCommandError::from)
}

#[tauri::command]
pub fn compile_cinema(
    project_root_path: String,
    scene_id: String,
    total_duration_seconds: f64,
    shot_count: Option<usize>,
) -> Result<CinemaCompilation, AppCommandError> {
    CinemaService::compile_scene(
        root_path(&project_root_path)?,
        crate::cinema::model::CinemaCompileInput {
            scene_id,
            total_duration_seconds,
            shot_count,
        },
    )
    .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn get_cinema_compilation(
    project_root_path: String,
    compilation_id: String,
) -> Result<CinemaCompilation, AppCommandError> {
    CinemaService::get_compilation(root_path(&project_root_path)?, &compilation_id)
        .map_err(AppCommandError::from)
}

#[tauri::command]
pub fn list_cinema_compilations(
    project_root_path: String,
    scene_id: String,
) -> Result<Vec<CinemaCompilation>, AppCommandError> {
    CinemaService::list_compilations(root_path(&project_root_path)?, &scene_id)
        .map_err(AppCommandError::from)
}

fn scene_detail(root: &Path, scene_id: &str) -> Result<SceneDetail, AppCommandError> {
    let scene = CinemaService::get_scene(root, scene_id).map_err(AppCommandError::from)?;
    let conn = crate::db::open_existing_connection(&root.join("project.db"))?;
    let characters = crate::cinema::repository::list_scene_characters(&conn, &scene.id)
        .map_err(AppCommandError::from)?;
    let props =
        crate::cinema::repository::list_scene_props(&conn, &scene.id).map_err(AppCommandError::from)?;
    let shots = CinemaService::list_shots(root, &scene.id).map_err(AppCommandError::from)?;
    Ok(SceneDetail {
        scene,
        characters,
        props,
        shots,
    })
}
