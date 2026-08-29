use crate::cinema::model::{
    CinemaCompilation, SceneCharacterRecord, ScenePropRecord, SceneRecord, ShotRecord, ShotUpdate,
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

/// Lists the characters cast into `scene_id`, in display order.
pub fn list_scene_characters(
    conn: &Connection,
    scene_id: &str,
) -> Result<Vec<SceneCharacterRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT scene_id, character_entity_id, look_asset_version_id, \
             sheet_asset_version_id, display_order FROM scene_characters \
             WHERE scene_id = ?1 ORDER BY display_order ASC, character_entity_id",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![scene_id], |row| {
            Ok(SceneCharacterRecord {
                scene_id: row.get(0)?,
                character_entity_id: row.get(1)?,
                look_asset_version_id: row.get(2)?,
                sheet_asset_version_id: row.get(3)?,
                display_order: row.get(4)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

/// Lists the props pinned into `scene_id`, in display order.
pub fn list_scene_props(
    conn: &Connection,
    scene_id: &str,
) -> Result<Vec<ScenePropRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT scene_id, prop_asset_version_id, display_order FROM scene_props \
             WHERE scene_id = ?1 ORDER BY display_order ASC, prop_asset_version_id",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![scene_id], |row| {
            Ok(ScenePropRecord {
                scene_id: row.get(0)?,
                prop_asset_version_id: row.get(1)?,
                display_order: row.get(2)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}


/// Renames a scene within `project_id`, returning the updated record.
pub fn rename_scene(conn: &Connection, project_id: &str, scene_id: &str, title: &str) -> Result<SceneRecord, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let updated = conn
        .execute(
            "UPDATE scenes SET title = ?1, updated_at = ?2 WHERE id = ?3 AND project_id = ?4",
            params![title, now, scene_id, project_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if updated == 0 {
        return Err(AppError::SceneNotFound);
    }
    get_scene(conn, project_id, scene_id)
}

/// Pins or clears the scene's world plate version reference.
pub fn set_scene_world(conn: &Connection, project_id: &str, scene_id: &str, version_id: Option<&str>) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let updated = conn
        .execute(
            "UPDATE scenes SET world_asset_version_id = ?1, updated_at = ?2 WHERE id = ?3 AND project_id = ?4",
            params![version_id, now, scene_id, project_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if updated == 0 {
        return Err(AppError::SceneNotFound);
    }
    Ok(())
}

/// Updates the exact look/sheet pins of one cast record.
pub fn update_scene_character(
    conn: &Connection,
    project_id: &str,
    scene_id: &str,
    character_id: &str,
    look_id: Option<&str>,
    sheet_id: Option<&str>,
) -> Result<(), AppError> {
    get_scene(conn, project_id, scene_id)?;
    let updated = conn
        .execute(
            "UPDATE scene_characters SET look_asset_version_id = ?1, sheet_asset_version_id = ?2 \
             WHERE scene_id = ?3 AND character_entity_id = ?4",
            params![look_id, sheet_id, scene_id, character_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if updated == 0 {
        return Err(AppError::SceneNotFound);
    }
    Ok(())
}

/// Removes one cast record. Only the relationship row is deleted.
pub fn remove_scene_character(conn: &Connection, project_id: &str, scene_id: &str, character_id: &str) -> Result<(), AppError> {
    get_scene(conn, project_id, scene_id)?;
    conn.execute(
        "DELETE FROM scene_characters WHERE scene_id = ?1 AND character_entity_id = ?2",
        params![scene_id, character_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Removes one prop relationship identified by its exact version id.
pub fn remove_scene_prop(conn: &Connection, project_id: &str, scene_id: &str, prop_version_id: &str) -> Result<(), AppError> {
    get_scene(conn, project_id, scene_id)?;
    conn.execute(
        "DELETE FROM scene_props WHERE scene_id = ?1 AND prop_asset_version_id = ?2",
        params![scene_id, prop_version_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Applies a field update to one shot within `project_id`.
pub fn update_shot(conn: &Connection, project_id: &str, update: &ShotUpdate) -> Result<ShotRecord, AppError> {
    let existing = conn
        .query_row(
            "SELECT s.id FROM shots s JOIN scenes sc ON sc.id = s.scene_id \
             WHERE s.id = ?1 AND sc.project_id = ?2",
            params![update.shot_id, project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::ShotNotFound)?;
    let _ = existing;
    let now = chrono::Utc::now().to_rfc3339();
    let updated = conn
        .execute(
            "UPDATE shots SET \
               duration_seconds = COALESCE(?1, duration_seconds), \
               intent = COALESCE(?2, intent), \
               action = ?3, \
               camera = ?4, \
               updated_at = ?5 \
             WHERE id = ?6",
            params![
                update.duration_seconds,
                update.intent,
                update.action,
                update.camera,
                now,
                update.shot_id,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if updated == 0 {
        return Err(AppError::ShotNotFound);
    }
    let scene_id: String = conn
        .query_row("SELECT scene_id FROM shots WHERE id = ?1", params![update.shot_id], |row| row.get(0))
        .map_err(|e| AppError::Database(e.to_string()))?;
    let shots = list_shots(conn, &scene_id)?;
    shots.into_iter().find(|shot| shot.id == update.shot_id).ok_or(AppError::ShotNotFound)
}

/// Deletes one shot. Only the shot row is removed — never canon, assets, or
/// versions.
pub fn delete_shot(conn: &Connection, project_id: &str, scene_id: &str, shot_id: &str) -> Result<(), AppError> {
    get_scene(conn, project_id, scene_id)?;
    conn.execute(
        "DELETE FROM shots WHERE id = ?1 AND scene_id = ?2",
        params![shot_id, scene_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Reorders the scene's shots into the exact given order, transactionally
/// and contiguously. Rejects duplicates, foreign ids, and incomplete sets
/// without changing any state.
pub fn reorder_shots(
    conn: &mut Connection,
    project_id: &str,
    scene_id: &str,
    ordered_ids: &[String],
) -> Result<Vec<ShotRecord>, AppError> {
    use std::collections::HashSet;
    get_scene(conn, project_id, scene_id)?;
    let existing = list_shots(conn, scene_id)?;
    let existing_ids: HashSet<&str> = existing.iter().map(|shot| shot.id.as_str()).collect();
    if ordered_ids.len() != existing.len() {
        return Err(AppError::WorkflowInputInvalid(
            "reorder must list every shot of the scene exactly once".into(),
        ));
    }
    let mut seen = HashSet::new();
    for id in ordered_ids {
        if !existing_ids.contains(id.as_str()) || !seen.insert(id.as_str()) {
            return Err(AppError::WorkflowInputInvalid(
                "reorder must not duplicate or reference foreign shots".into(),
            ));
        }
    }

    let tx = conn
        .transaction()
        .map_err(|e| AppError::Database(e.to_string()))?;
    // Two-phase update so a UNIQUE(scene_id, ordering) constraint cannot
    // collide mid-reorder: shift everything far up first, then write the
    // final contiguous positions.
    let offset = existing.len() as i64 + 10_000;
    for (position, shot_id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE shots SET ordering = ?1 WHERE id = ?2 AND scene_id = ?3",
            params![offset + position as i64, shot_id, scene_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }
    for (position, shot_id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE shots SET ordering = ?1, updated_at = ?2 WHERE id = ?3 AND scene_id = ?4",
            params![position as i64, chrono::Utc::now().to_rfc3339(), shot_id, scene_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }
    tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
    list_shots(conn, scene_id)
}

/// Pins or clears one shot's canonical keyframe version reference.
pub fn set_shot_keyframe(conn: &Connection, project_id: &str, shot_id: &str, version_id: Option<&str>) -> Result<(), AppError> {
    let updated = conn
        .execute(
            "UPDATE shots SET keyframe_asset_version_id = ?1, updated_at = ?2 \
             WHERE id = ?3 AND scene_id IN (SELECT id FROM scenes WHERE project_id = ?4)",
            params![version_id, chrono::Utc::now().to_rfc3339(), shot_id, project_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if updated == 0 {
        return Err(AppError::ShotNotFound);
    }
    Ok(())
}
