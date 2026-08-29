use crate::error::AppError;
use crate::worlds::model::World;
use rusqlite::{params, Connection, OptionalExtension, Transaction};

fn row_to_world(row: &rusqlite::Row) -> rusqlite::Result<World> {
    Ok(World {
        id: row.get(0)?,
        project_id: row.get(1)?,
        canon_location_entity_id: row.get(2)?,
        world_plate_asset_id: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

/// Inserts a new World row inside the caller's transaction.
pub fn insert_world(tx: &Transaction<'_>, world: &World) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO worlds (id, project_id, canon_location_entity_id, world_plate_asset_id, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            world.id,
            world.project_id,
            world.canon_location_entity_id,
            world.world_plate_asset_id,
            world.created_at,
            world.updated_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// Fetches a World by its primary id. Returns `WorldNotFound` if missing.
pub fn get_world(conn: &Connection, world_id: &str) -> Result<World, AppError> {
    conn.query_row(
        "SELECT id, project_id, canon_location_entity_id, world_plate_asset_id, created_at, updated_at \
         FROM worlds WHERE id = ?1",
        params![world_id],
        row_to_world,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or(AppError::WorldNotFound)
}

/// Looks up a World by its unique `(project_id, canon_location_entity_id)` key.
pub fn find_world_by_location(
    conn: &Connection,
    project_id: &str,
    canon_location_entity_id: &str,
) -> Result<Option<World>, AppError> {
    conn.query_row(
        "SELECT id, project_id, canon_location_entity_id, world_plate_asset_id, created_at, updated_at \
         FROM worlds WHERE project_id = ?1 AND canon_location_entity_id = ?2",
        params![project_id, canon_location_entity_id],
        row_to_world,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))
}

/// Lists every World belonging to `project_id`, ordered by creation time.
pub fn list_worlds(conn: &Connection, project_id: &str) -> Result<Vec<World>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, canon_location_entity_id, world_plate_asset_id, created_at, updated_at \
             FROM worlds WHERE project_id = ?1 ORDER BY created_at ASC, id ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![project_id], row_to_world)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut worlds = Vec::new();
    for row in rows {
        worlds.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(worlds)
}
