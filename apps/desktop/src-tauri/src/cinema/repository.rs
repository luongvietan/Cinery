use crate::cinema::model::{
    CinemaCompilation, SceneCharacterRecord, ScenePropRecord, SceneRecord, ShotRecord,
};
use crate::error::AppError;
use rusqlite::{params, Connection, OptionalExtension};

fn scene_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SceneRecord> {
    Ok(SceneRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        world_asset_version_id: row.get(3)?,
        canon_notes: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

/// Inserts a new scene row.
pub fn create_scene(conn: &Connection, record: &SceneRecord) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO scenes (id, project_id, title, world_asset_version_id, canon_notes, \
         created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            record.id,
            record.project_id,
            record.title,
            record.world_asset_version_id,
            record.canon_notes,
            record.created_at,
            record.updated_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Reads a single scene scoped to `project_id`; scenes from other projects
/// are reported as [`AppError::SceneNotFound`] rather than leaked.
pub fn get_scene(
    conn: &Connection,
    project_id: &str,
    scene_id: &str,
) -> Result<SceneRecord, AppError> {
    conn.query_row(
        "SELECT id, project_id, title, world_asset_version_id, canon_notes, created_at, \
         updated_at FROM scenes WHERE id = ?1 AND project_id = ?2",
        params![scene_id, project_id],
        scene_from_row,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or(AppError::SceneNotFound)
}

/// Lists every scene of `project_id`, newest first.
pub fn list_scenes(conn: &Connection, project_id: &str) -> Result<Vec<SceneRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, title, world_asset_version_id, canon_notes, created_at, \
             updated_at FROM scenes WHERE project_id = ?1 ORDER BY created_at DESC, id",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![project_id], scene_from_row)
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

/// Pins a character entity into a scene with canonical look (and optional
/// sheet) versions.
pub fn add_scene_character(
    conn: &Connection,
    record: &SceneCharacterRecord,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO scene_characters (scene_id, character_entity_id, look_asset_version_id, \
         sheet_asset_version_id, display_order) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            record.scene_id,
            record.character_entity_id,
            record.look_asset_version_id,
            record.sheet_asset_version_id,
            record.display_order,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Pins a prop plate version into a scene.
pub fn add_scene_prop(conn: &Connection, record: &ScenePropRecord) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO scene_props (scene_id, prop_asset_version_id, display_order) \
         VALUES (?1, ?2, ?3)",
        params![record.scene_id, record.prop_asset_version_id, record.display_order],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}
/// Inserts a new shot row. The schema's UNIQUE(scene_id, ordering) constraint
/// rejects duplicate orderings and the CHECK constraints bound durations.
pub fn create_shot(conn: &Connection, record: &ShotRecord) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO shots (id, scene_id, ordering, duration_seconds, keyframe_asset_version_id, \
         intent, action, camera, generated_video_asset_version_id, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            record.id,
            record.scene_id,
            record.ordering,
            record.duration_seconds,
            record.keyframe_asset_version_id,
            record.intent,
            record.action,
            record.camera,
            record.generated_video_asset_version_id,
            record.created_at,
            record.updated_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Lists every shot of `scene_id` ordered by its `ordering`.
pub fn list_shots(conn: &Connection, scene_id: &str) -> Result<Vec<ShotRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scene_id, ordering, duration_seconds, keyframe_asset_version_id, intent, \
             action, camera, generated_video_asset_version_id, created_at, updated_at \
             FROM shots WHERE scene_id = ?1 ORDER BY ordering ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![scene_id], |row| {
            Ok(ShotRecord {
                id: row.get(0)?,
                scene_id: row.get(1)?,
                ordering: row.get(2)?,
                duration_seconds: row.get(3)?,
                keyframe_asset_version_id: row.get(4)?,
                intent: row.get(5)?,
                action: row.get(6)?,
                camera: row.get(7)?,
                generated_video_asset_version_id: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

/// Persists one compilation record (input snapshot, compiled JSON, export
/// artifact path and content hash).
pub fn insert_compilation(
    conn: &Connection,
    record: &CinemaCompilation,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO cinema_compilations (id, project_id, scene_id, input_json, \
         compilation_json, export_path, export_sha256, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            record.id,
            record.project_id,
            record.scene_id,
            record.input_json,
            record.compilation_json,
            record.export_path,
            record.export_sha256,
            record.created_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Reads a single compilation record by id.
pub fn get_compilation(conn: &Connection, id: &str) -> Result<CinemaCompilation, AppError> {
    conn.query_row(
        "SELECT id, project_id, scene_id, input_json, compilation_json, export_path, \
         export_sha256, created_at FROM cinema_compilations WHERE id = ?1",
        params![id],
        compilation_from_row,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or(AppError::CinemaCompilationNotFound)
}

/// Lists every compilation recorded for `scene_id`, newest first.
pub fn list_compilations(
    conn: &Connection,
    scene_id: &str,
) -> Result<Vec<CinemaCompilation>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, scene_id, input_json, compilation_json, export_path, \
             export_sha256, created_at FROM cinema_compilations WHERE scene_id = ?1 \
             ORDER BY created_at DESC, id",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![scene_id], compilation_from_row)
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

fn compilation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CinemaCompilation> {
    Ok(CinemaCompilation {
        id: row.get(0)?,
        project_id: row.get(1)?,
        scene_id: row.get(2)?,
        input_json: row.get(3)?,
        compilation_json: row.get(4)?,
        export_path: row.get(5)?,
        export_sha256: row.get(6)?,
        created_at: row.get(7)?,
    })
}

