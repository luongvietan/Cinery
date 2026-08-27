use crate::assets::model::{AssetRecord, AssetVersionRecord};
use crate::error::AppError;
use rusqlite::{params, Connection};

/// Inserts a new asset row.
pub fn insert_asset(conn: &Connection, record: &AssetRecord) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO assets \
         (id, project_id, type, label, owner_entity_id, canonical_version_id, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            record.id,
            record.project_id,
            record.asset_type,
            record.label,
            record.owner_entity_id,
            record.canonical_version_id,
            record.created_at,
            record.updated_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(())
}

/// Lists every asset belonging to `project_id`.
pub fn list_assets(conn: &Connection, project_id: &str) -> Result<Vec<AssetRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, type, label, owner_entity_id, canonical_version_id, \
             created_at, updated_at \
             FROM assets WHERE project_id = ?1 ORDER BY created_at ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id], row_to_asset_record)
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut assets = Vec::new();
    for row in rows {
        assets.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(assets)
}

/// Reads a single asset by id, returning `AppError::AssetNotFound` if it
/// does not exist.
pub fn get_asset(conn: &Connection, asset_id: &str) -> Result<AssetRecord, AppError> {
    conn.query_row(
        "SELECT id, project_id, type, label, owner_entity_id, canonical_version_id, \
         created_at, updated_at \
         FROM assets WHERE id = ?1",
        params![asset_id],
        row_to_asset_record,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::AssetNotFound,
        other => AppError::Database(other.to_string()),
    })
}

/// Lists every version of `asset_id`, newest version first.
pub fn list_asset_versions(
    conn: &Connection,
    asset_id: &str,
) -> Result<Vec<AssetVersionRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, asset_id, version_number, status, file_path, thumbnail_path, sha256, \
             original_filename, mime_type, byte_size, parent_version_id, created_at \
             FROM asset_versions WHERE asset_id = ?1 ORDER BY version_number DESC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![asset_id], row_to_asset_version_record)
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut versions = Vec::new();
    for row in rows {
        versions.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(versions)
}

fn row_to_asset_record(row: &rusqlite::Row) -> rusqlite::Result<AssetRecord> {
    Ok(AssetRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        asset_type: row.get(2)?,
        label: row.get(3)?,
        owner_entity_id: row.get(4)?,
        canonical_version_id: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn row_to_asset_version_record(row: &rusqlite::Row) -> rusqlite::Result<AssetVersionRecord> {
    Ok(AssetVersionRecord {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        version_number: row.get(2)?,
        status: row.get(3)?,
        file_path: row.get(4)?,
        thumbnail_path: row.get(5)?,
        sha256: row.get(6)?,
        original_filename: row.get(7)?,
        mime_type: row.get(8)?,
        byte_size: row.get(9)?,
        parent_version_id: row.get(10)?,
        created_at: row.get(11)?,
    })
}
