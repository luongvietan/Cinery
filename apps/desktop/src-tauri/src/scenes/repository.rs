use crate::error::AppError;
use crate::scenes::model::{
    Scene, SceneCharacterAssignment, ScenePropAssignment, SceneReferenceEvent,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

fn row_to_scene(row: &rusqlite::Row) -> rusqlite::Result<Scene> {
    Ok(Scene {
        id: row.get(0)?,
        project_id: row.get(1)?,
        ordinal: row.get(2)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        world_id: row.get(5)?,
        world_asset_version_id: row.get(6)?,
        keyframe_asset_id: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn row_to_character(row: &rusqlite::Row) -> rusqlite::Result<SceneCharacterAssignment> {
    Ok(SceneCharacterAssignment {
        id: row.get(0)?,
        scene_id: row.get(1)?,
        character_entity_id: row.get(2)?,
        look_asset_version_id: row.get(3)?,
        sheet_asset_version_id: row.get(4)?,
        notes: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn row_to_prop(row: &rusqlite::Row) -> rusqlite::Result<ScenePropAssignment> {
    Ok(ScenePropAssignment {
        id: row.get(0)?,
        scene_id: row.get(1)?,
        prop_asset_version_id: row.get(2)?,
        label: row.get(3)?,
        notes: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<SceneReferenceEvent> {
    let kind_str: String = row.get(2)?;
    let action_str: String = row.get(4)?;
    Ok(SceneReferenceEvent {
        id: row.get(0)?,
        scene_id: row.get(1)?,
        reference_kind: kind_str.parse().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        assignment_id: row.get(3)?,
        action: action_str.parse().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        from_version_id: row.get(5)?,
        to_version_id: row.get(6)?,
        created_at: row.get(7)?,
    })
}

// ---------------------------------------------------------------------------
// Scene CRUD
// ---------------------------------------------------------------------------

pub fn insert_scene(tx: &Transaction<'_>, scene: &Scene) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO world_scenes (id, project_id, ordinal, title, summary, world_id, world_asset_version_id, keyframe_asset_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            scene.id,
            scene.project_id,
            scene.ordinal,
            scene.title,
            scene.summary,
            scene.world_id,
            scene.world_asset_version_id,
            scene.keyframe_asset_id,
            scene.created_at,
            scene.updated_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_scene(conn: &Connection, scene_id: &str) -> Result<Scene, AppError> {
    conn.query_row(
        "SELECT id, project_id, ordinal, title, summary, world_id, world_asset_version_id, keyframe_asset_id, created_at, updated_at FROM world_scenes WHERE id = ?1",
        params![scene_id],
        row_to_scene,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or(AppError::SceneNotFound)
}

pub fn list_scenes(conn: &Connection, project_id: &str) -> Result<Vec<Scene>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, ordinal, title, summary, world_id, world_asset_version_id, keyframe_asset_id, created_at, updated_at FROM world_scenes WHERE project_id = ?1 ORDER BY ordinal ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![project_id], row_to_scene)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut scenes = Vec::new();
    for row in rows {
        scenes.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(scenes)
}

pub fn next_ordinal(tx: &Transaction<'_>, project_id: &str) -> Result<i64, AppError> {
    tx.query_row(
        "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM world_scenes WHERE project_id = ?1",
        params![project_id],
        |row| row.get(0),
    )
    .map_err(|e| AppError::Database(e.to_string()))
}

pub fn update_scene_details(
    tx: &Transaction<'_>,
    scene_id: &str,
    title: &str,
    summary: &str,
    updated_at: &str,
) -> Result<(), AppError> {
    let changed = tx
        .execute(
            "UPDATE world_scenes SET title = ?1, summary = ?2, updated_at = ?3 WHERE id = ?4",
            params![title, summary, updated_at, scene_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if changed == 0 {
        return Err(AppError::SceneNotFound);
    }
    Ok(())
}

pub fn update_scene_world(
    tx: &Transaction<'_>,
    scene_id: &str,
    world_id: Option<&str>,
    world_asset_version_id: Option<&str>,
    updated_at: &str,
) -> Result<(), AppError> {
    let changed = tx
        .execute(
            "UPDATE world_scenes SET world_id = ?1, world_asset_version_id = ?2, updated_at = ?3 WHERE id = ?4",
            params![world_id, world_asset_version_id, updated_at, scene_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if changed == 0 {
        return Err(AppError::SceneNotFound);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Scene Characters
// ---------------------------------------------------------------------------

pub fn insert_scene_character(
    tx: &Transaction<'_>,
    assignment: &SceneCharacterAssignment,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO world_scene_characters (id, scene_id, character_entity_id, look_asset_version_id, sheet_asset_version_id, notes, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            assignment.id,
            assignment.scene_id,
            assignment.character_entity_id,
            assignment.look_asset_version_id,
            assignment.sheet_asset_version_id,
            assignment.notes,
            assignment.created_at,
            assignment.updated_at,
        ],
    )
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("UNIQUE constraint failed") && msg.contains("world_scene_characters") {
            AppError::SceneCharacterAlreadyExists
        } else {
            AppError::Database(msg)
        }
    })?;
    Ok(())
}

pub fn list_scene_characters(
    conn: &Connection,
    scene_id: &str,
) -> Result<Vec<SceneCharacterAssignment>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scene_id, character_entity_id, look_asset_version_id, sheet_asset_version_id, notes, created_at, updated_at FROM world_scene_characters WHERE scene_id = ?1 ORDER BY created_at ASC, id ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![scene_id], row_to_character)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(out)
}

pub fn get_scene_character(
    conn: &Connection,
    assignment_id: &str,
) -> Result<SceneCharacterAssignment, AppError> {
    conn.query_row(
        "SELECT id, scene_id, character_entity_id, look_asset_version_id, sheet_asset_version_id, notes, created_at, updated_at FROM world_scene_characters WHERE id = ?1",
        params![assignment_id],
        row_to_character,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or(AppError::SceneCharacterNotFound)
}

pub fn find_scene_character_by_scene_and_character(
    conn: &Connection,
    scene_id: &str,
    character_entity_id: &str,
) -> Result<Option<SceneCharacterAssignment>, AppError> {
    conn.query_row(
        "SELECT id, scene_id, character_entity_id, look_asset_version_id, sheet_asset_version_id, notes, created_at, updated_at FROM world_scene_characters WHERE scene_id = ?1 AND character_entity_id = ?2",
        params![scene_id, character_entity_id],
        row_to_character,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))
}

pub fn delete_scene_character(tx: &Transaction<'_>, assignment_id: &str) -> Result<(), AppError> {
    let changed = tx
        .execute(
            "DELETE FROM world_scene_characters WHERE id = ?1",
            params![assignment_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if changed == 0 {
        return Err(AppError::SceneCharacterNotFound);
    }
    Ok(())
}

pub fn delete_scene_character_by_scene_and_character(
    tx: &Transaction<'_>,
    scene_id: &str,
    character_entity_id: &str,
) -> Result<SceneCharacterAssignment, AppError> {
    let existing = {
        // Need to query inside transaction connection
        tx.query_row(
            "SELECT id, scene_id, character_entity_id, look_asset_version_id, sheet_asset_version_id, notes, created_at, updated_at FROM world_scene_characters WHERE scene_id = ?1 AND character_entity_id = ?2",
            params![scene_id, character_entity_id],
            row_to_character,
        )
        .optional()
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::SceneCharacterNotFound)?
    };
    tx.execute(
        "DELETE FROM world_scene_characters WHERE id = ?1",
        params![existing.id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(existing)
}

pub fn update_scene_character_look(
    tx: &Transaction<'_>,
    assignment_id: &str,
    new_version_id: &str,
    updated_at: &str,
) -> Result<(), AppError> {
    let changed = tx
        .execute(
            "UPDATE world_scene_characters SET look_asset_version_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_version_id, updated_at, assignment_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if changed == 0 {
        return Err(AppError::SceneCharacterNotFound);
    }
    Ok(())
}

pub fn update_scene_character_sheet(
    tx: &Transaction<'_>,
    assignment_id: &str,
    new_version_id: Option<&str>,
    updated_at: &str,
) -> Result<(), AppError> {
    let changed = tx
        .execute(
            "UPDATE world_scene_characters SET sheet_asset_version_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_version_id, updated_at, assignment_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if changed == 0 {
        return Err(AppError::SceneCharacterNotFound);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Scene Props
// ---------------------------------------------------------------------------

pub fn insert_scene_prop(
    tx: &Transaction<'_>,
    assignment: &ScenePropAssignment,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO world_scene_props (id, scene_id, prop_asset_version_id, label, notes, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            assignment.id,
            assignment.scene_id,
            assignment.prop_asset_version_id,
            assignment.label,
            assignment.notes,
            assignment.created_at,
        ],
    )
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("UNIQUE constraint failed") && msg.contains("world_scene_props") {
            AppError::ScenePropAlreadyExists
        } else {
            AppError::Database(msg)
        }
    })?;
    Ok(())
}

pub fn list_scene_props(
    conn: &Connection,
    scene_id: &str,
) -> Result<Vec<ScenePropAssignment>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scene_id, prop_asset_version_id, label, notes, created_at FROM world_scene_props WHERE scene_id = ?1 ORDER BY created_at ASC, id ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![scene_id], row_to_prop)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(out)
}

pub fn find_scene_prop_by_version(
    conn: &Connection,
    scene_id: &str,
    prop_asset_version_id: &str,
) -> Result<Option<ScenePropAssignment>, AppError> {
    conn.query_row(
        "SELECT id, scene_id, prop_asset_version_id, label, notes, created_at FROM world_scene_props WHERE scene_id = ?1 AND prop_asset_version_id = ?2",
        params![scene_id, prop_asset_version_id],
        row_to_prop,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))
}

pub fn delete_scene_prop(tx: &Transaction<'_>, assignment_id: &str) -> Result<(), AppError> {
    let changed = tx
        .execute("DELETE FROM world_scene_props WHERE id = ?1", params![assignment_id])
        .map_err(|e| AppError::Database(e.to_string()))?;
    if changed == 0 {
        return Err(AppError::ScenePropNotFound);
    }
    Ok(())
}

pub fn delete_scene_prop_by_version(
    tx: &Transaction<'_>,
    scene_id: &str,
    prop_asset_version_id: &str,
) -> Result<ScenePropAssignment, AppError> {
    let existing = tx
        .query_row(
            "SELECT id, scene_id, prop_asset_version_id, label, notes, created_at FROM world_scene_props WHERE scene_id = ?1 AND prop_asset_version_id = ?2",
            params![scene_id, prop_asset_version_id],
            row_to_prop,
        )
        .optional()
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::ScenePropNotFound)?;
    tx.execute("DELETE FROM world_scene_props WHERE id = ?1", params![existing.id])
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(existing)
}

pub fn get_scene_prop(
    conn: &Connection,
    assignment_id: &str,
) -> Result<ScenePropAssignment, AppError> {
    conn.query_row(
        "SELECT id, scene_id, prop_asset_version_id, label, notes, created_at FROM world_scene_props WHERE id = ?1",
        params![assignment_id],
        row_to_prop,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or(AppError::ScenePropNotFound)
}

pub fn update_scene_prop_version(
    tx: &Transaction<'_>,
    assignment_id: &str,
    new_version_id: &str,
) -> Result<(), AppError> {
    let changed = tx
        .execute(
            "UPDATE world_scene_props SET prop_asset_version_id = ?1 WHERE id = ?2",
            params![new_version_id, assignment_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if changed == 0 {
        return Err(AppError::ScenePropNotFound);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Reference Events
// ---------------------------------------------------------------------------

pub fn insert_reference_event(
    tx: &Transaction<'_>,
    event: &SceneReferenceEvent,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO scene_reference_events (id, scene_id, reference_kind, assignment_id, action, from_version_id, to_version_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            event.id,
            event.scene_id,
            event.reference_kind.as_str(),
            event.assignment_id,
            event.action.as_str(),
            event.from_version_id,
            event.to_version_id,
            event.created_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_reference_events(
    conn: &Connection,
    scene_id: &str,
) -> Result<Vec<SceneReferenceEvent>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scene_id, reference_kind, assignment_id, action, from_version_id, to_version_id, created_at FROM scene_reference_events WHERE scene_id = ?1 ORDER BY created_at ASC, id ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![scene_id], row_to_event)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Scene Keyframe Asset
// ---------------------------------------------------------------------------

pub fn update_scene_keyframe_asset(
    tx: &Transaction<'_>,
    scene_id: &str,
    keyframe_asset_id: Option<&str>,
    updated_at: &str,
) -> Result<(), AppError> {
    let changed = tx
        .execute(
            "UPDATE world_scenes SET keyframe_asset_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![keyframe_asset_id, updated_at, scene_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if changed == 0 {
        return Err(AppError::SceneNotFound);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Scene TBD Bindings
// ---------------------------------------------------------------------------

fn row_to_tbd_binding(row: &rusqlite::Row) -> rusqlite::Result<crate::scenes::model::SceneTbdBinding> {
    let decision_str: String = row.get(5)?;
    Ok(crate::scenes::model::SceneTbdBinding {
        id: row.get(0)?,
        scene_id: row.get(1)?,
        canon_tbd_id: row.get(2)?,
        topic_snapshot: row.get(3)?,
        note_snapshot: row.get(4)?,
        decision: decision_str.parse().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        justification: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn list_scene_tbd_bindings(
    conn: &Connection,
    scene_id: &str,
) -> Result<Vec<crate::scenes::model::SceneTbdBinding>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scene_id, canon_tbd_id, topic_snapshot, note_snapshot, decision, justification, created_at, updated_at FROM scene_tbd_bindings WHERE scene_id = ?1 ORDER BY created_at ASC, id ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![scene_id], row_to_tbd_binding)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(out)
}

pub fn get_scene_tbd_binding(
    conn: &Connection,
    scene_id: &str,
    canon_tbd_id: &str,
) -> Result<Option<crate::scenes::model::SceneTbdBinding>, AppError> {
    conn.query_row(
        "SELECT id, scene_id, canon_tbd_id, topic_snapshot, note_snapshot, decision, justification, created_at, updated_at FROM scene_tbd_bindings WHERE scene_id = ?1 AND canon_tbd_id = ?2",
        params![scene_id, canon_tbd_id],
        row_to_tbd_binding,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))
}

pub fn upsert_scene_tbd_binding(
    tx: &Transaction<'_>,
    binding: &crate::scenes::model::SceneTbdBinding,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO scene_tbd_bindings (id, scene_id, canon_tbd_id, topic_snapshot, note_snapshot, decision, justification, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT(scene_id, canon_tbd_id) DO UPDATE SET topic_snapshot = excluded.topic_snapshot, note_snapshot = excluded.note_snapshot, decision = excluded.decision, justification = excluded.justification, updated_at = excluded.updated_at",
        params![
            binding.id,
            binding.scene_id,
            binding.canon_tbd_id,
            binding.topic_snapshot,
            binding.note_snapshot,
            binding.decision.as_str(),
            binding.justification,
            binding.created_at,
            binding.updated_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

pub fn delete_scene_tbd_binding(
    tx: &Transaction<'_>,
    scene_id: &str,
    canon_tbd_id: &str,
) -> Result<(), AppError> {
    let changed = tx
        .execute(
            "DELETE FROM scene_tbd_bindings WHERE scene_id = ?1 AND canon_tbd_id = ?2",
            params![scene_id, canon_tbd_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if changed == 0 {
        return Err(AppError::CanonTbdNotFound);
    }
    Ok(())
}
