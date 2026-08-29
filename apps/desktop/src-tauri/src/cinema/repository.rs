use crate::cinema::model::{CinemaCompilation, ShotRecord, ShotUpdate};
use crate::error::AppError;
use rusqlite::{params, Connection, OptionalExtension};

fn shot_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ShotRecord> {
    Ok(ShotRecord {
        id: row.get(0)?,
        scene_id: row.get(1)?,
        ordering: row.get(2)?,
        duration_seconds: row.get(3)?,
        keyframe_asset_version_id: row.get(4)?,
        intent: row.get(5)?,
        action: row.get(6)?,
        camera: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

const SHOT_COLUMNS: &str = "id, scene_id, ordering, duration_seconds, keyframe_asset_version_id, \
     intent, action, camera, created_at, updated_at";

/// Verifies the scene exists and belongs to `project_id` (authoritative
/// `world_scenes` aggregate). Scenes from other projects are reported as
/// [`AppError::SceneNotFound`] rather than leaked.
pub fn ensure_scene_in_project(
    conn: &Connection,
    project_id: &str,
    scene_id: &str,
) -> Result<(), AppError> {
    let owned: Option<String> = conn
        .query_row(
            "SELECT id FROM world_scenes WHERE id = ?1 AND project_id = ?2",
            params![scene_id, project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if owned.is_none() {
        return Err(AppError::SceneNotFound);
    }
    Ok(())
}

/// Inserts a new shot row. The schema's UNIQUE(scene_id, ordering) constraint
/// rejects duplicate orderings and the CHECK constraints bound durations.
pub fn create_shot(conn: &Connection, record: &ShotRecord) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO scene_shots (id, scene_id, ordering, duration_seconds, \
         keyframe_asset_version_id, intent, action, camera, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            record.id,
            record.scene_id,
            record.ordering,
            record.duration_seconds,
            record.keyframe_asset_version_id,
            record.intent,
            record.action,
            record.camera,
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
        .prepare(&format!(
            "SELECT {SHOT_COLUMNS} FROM scene_shots WHERE scene_id = ?1 ORDER BY ordering ASC"
        ))
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![scene_id], shot_from_row)
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

/// Inserts a compilation record (input snapshot, compiled JSON, export
/// artifact path and content hash).
pub fn insert_compilation(conn: &Connection, record: &CinemaCompilation) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO scene_compilations (id, project_id, scene_id, input_json, \
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
         export_sha256, created_at FROM scene_compilations WHERE id = ?1",
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
             export_sha256, created_at FROM scene_compilations WHERE scene_id = ?1 \
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

/// Loads the authoritative scene's cast (exact look/sheet versions) in cast
/// order for compilation input.
pub fn list_scene_cast(
    conn: &Connection,
    scene_id: &str,
) -> Result<Vec<crate::scenes::model::SceneCharacterAssignment>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scene_id, character_entity_id, look_asset_version_id, \
             sheet_asset_version_id, notes, created_at, updated_at \
             FROM world_scene_characters WHERE scene_id = ?1 \
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![scene_id], |row| {
            Ok(crate::scenes::model::SceneCharacterAssignment {
                id: row.get(0)?,
                scene_id: row.get(1)?,
                character_entity_id: row.get(2)?,
                look_asset_version_id: row.get(3)?,
                sheet_asset_version_id: row.get(4)?,
                notes: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

/// Applies a field update to one shot within `project_id`.
pub fn update_shot(
    conn: &Connection,
    project_id: &str,
    update: &ShotUpdate,
) -> Result<ShotRecord, AppError> {
    let scene_id: String = conn
        .query_row(
            "SELECT ss.scene_id FROM scene_shots ss \
             JOIN world_scenes ws ON ws.id = ss.scene_id \
             WHERE ss.id = ?1 AND ws.project_id = ?2",
            params![update.shot_id, project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::ShotNotFound)?;
    let now = chrono::Utc::now().to_rfc3339();
    let updated = conn
        .execute(
            "UPDATE scene_shots SET \
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
    list_shots(conn, &scene_id)?
        .into_iter()
        .find(|shot| shot.id == update.shot_id)
        .ok_or(AppError::ShotNotFound)
}

/// Deletes one shot. Only the shot row is removed — never canon, assets, or
/// versions.
pub fn delete_shot(
    conn: &Connection,
    project_id: &str,
    scene_id: &str,
    shot_id: &str,
) -> Result<(), AppError> {
    ensure_scene_in_project(conn, project_id, scene_id)?;
    conn.execute(
        "DELETE FROM scene_shots WHERE id = ?1 AND scene_id = ?2",
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
    ensure_scene_in_project(conn, project_id, scene_id)?;
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
            "UPDATE scene_shots SET ordering = ?1 WHERE id = ?2 AND scene_id = ?3",
            params![offset + position as i64, shot_id, scene_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }
    for (position, shot_id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE scene_shots SET ordering = ?1, updated_at = ?2 WHERE id = ?3 AND scene_id = ?4",
            params![
                position as i64,
                chrono::Utc::now().to_rfc3339(),
                shot_id,
                scene_id
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }
    tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
    list_shots(conn, scene_id)
}

/// Pins or clears one shot's canonical keyframe version reference.
pub fn set_shot_keyframe(
    conn: &Connection,
    project_id: &str,
    shot_id: &str,
    version_id: Option<&str>,
) -> Result<(), AppError> {
    let updated = conn
        .execute(
            "UPDATE scene_shots SET keyframe_asset_version_id = ?1, updated_at = ?2 \
             WHERE id = ?3 AND scene_id IN (SELECT id FROM world_scenes WHERE project_id = ?4)",
            params![
                version_id,
                chrono::Utc::now().to_rfc3339(),
                shot_id,
                project_id
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if updated == 0 {
        return Err(AppError::ShotNotFound);
    }
    Ok(())
}
