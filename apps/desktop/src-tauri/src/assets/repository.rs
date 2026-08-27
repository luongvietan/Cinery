use crate::assets::model::{
    AssetRecord, AssetSummaryRecord, AssetVersionRecord, CanonicalPromotionResult,
};
use crate::error::AppError;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

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

/// Lists every asset belonging to `project_id` with sidebar summary fields.
pub fn list_asset_summaries(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<AssetSummaryRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.project_id, a.type, a.label, a.owner_entity_id, \
             a.canonical_version_id, a.created_at, a.updated_at, \
             (SELECT COUNT(*) FROM asset_versions WHERE asset_id = a.id) AS version_count, \
             cv.version_number AS canonical_version_number, \
             COALESCE(cv.thumbnail_path, \
               (SELECT thumbnail_path FROM asset_versions \
                WHERE asset_id = a.id ORDER BY version_number DESC LIMIT 1)) \
             AS preview_thumbnail_path \
             FROM assets a \
             LEFT JOIN asset_versions cv ON cv.id = a.canonical_version_id \
             WHERE a.project_id = ?1 ORDER BY a.created_at ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id], row_to_asset_summary_record)
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
             original_filename, mime_type, byte_size, width, height, parent_version_id, created_at \
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

/// Looks up a single asset version by its own id, regardless of which asset
/// it belongs to. Returns `None` rather than an error when it doesn't
/// exist, since "not found" is a valid outcome callers (e.g. parent-version
/// validation) need to distinguish from a database failure.
pub fn get_asset_version_by_id(
    conn: &Connection,
    version_id: &str,
) -> Result<Option<AssetVersionRecord>, AppError> {
    conn.query_row(
        "SELECT id, asset_id, version_number, status, file_path, thumbnail_path, sha256, \
         original_filename, mime_type, byte_size, width, height, parent_version_id, created_at \
         FROM asset_versions WHERE id = ?1",
        params![version_id],
        row_to_asset_version_record,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))
}

/// Looks up the version of `asset_id` whose content hash is `sha256`, used
/// to reject duplicate content on the same asset before any file is
/// written. Runs inside the caller's transaction so the check and the
/// eventual insert observe a consistent snapshot.
pub fn find_version_by_hash(
    tx: &Transaction<'_>,
    asset_id: &str,
    sha256: &str,
) -> Result<Option<AssetVersionRecord>, AppError> {
    tx.query_row(
        "SELECT id, asset_id, version_number, status, file_path, thumbnail_path, sha256, \
         original_filename, mime_type, byte_size, width, height, parent_version_id, created_at \
         FROM asset_versions WHERE asset_id = ?1 AND sha256 = ?2",
        params![asset_id, sha256],
        row_to_asset_version_record,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))
}

/// Computes the next `version_number` to allocate for `asset_id` (1 if the
/// asset has no versions yet). Must be called inside the same transaction
/// that will insert the new version, so the allocation and the insert are
/// atomic with respect to concurrent importers.
pub fn next_version_number(tx: &Transaction<'_>, asset_id: &str) -> Result<i64, AppError> {
    tx.query_row(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM asset_versions WHERE asset_id = ?1",
        params![asset_id],
        |row| row.get(0),
    )
    .map_err(|e| AppError::Database(e.to_string()))
}

/// Inserts a new asset version row.
pub fn insert_asset_version(
    tx: &Transaction<'_>,
    record: &AssetVersionRecord,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO asset_versions \
         (id, asset_id, version_number, status, file_path, thumbnail_path, sha256, \
          original_filename, mime_type, byte_size, width, height, parent_version_id, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            record.id,
            record.asset_id,
            record.version_number,
            record.status,
            record.file_path,
            record.thumbnail_path,
            record.sha256,
            record.original_filename,
            record.mime_type,
            record.byte_size,
            record.width,
            record.height,
            record.parent_version_id,
            record.created_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(())
}

pub fn promote_canonical_version(
    conn: &mut Connection,
    target_version_id: &str,
) -> Result<CanonicalPromotionResult, AppError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| AppError::Database(e.to_string()))?;

    let target_version =
        get_asset_version_by_id(&tx, target_version_id)?.ok_or(AppError::AssetVersionNotFound)?;
    let asset_id = target_version.asset_id.clone();
    let asset = get_asset(&tx, &asset_id)?;

    let superseded_version_id = if asset.canonical_version_id.as_deref() == Some(target_version_id)
    {
        None
    } else {
        let current_canonical_id = asset.canonical_version_id.clone();
        if let Some(current_id) = &current_canonical_id {
            tx.execute(
                "UPDATE asset_versions SET status = 'superseded' WHERE id = ?1",
                params![current_id],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        tx.execute(
            "UPDATE asset_versions SET status = 'canonical' WHERE id = ?1",
            params![target_version_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE assets SET canonical_version_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![target_version_id, now, asset_id],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        current_canonical_id
    };

    let canonical_count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM asset_versions WHERE asset_id = ?1 AND status = 'canonical'",
            params![asset_id],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    if canonical_count != 1 {
        return Err(AppError::Database(format!(
            "canonical-promotion invariant violated for asset {asset_id}: expected exactly 1 canonical version, found {canonical_count}"
        )));
    }

    let refreshed_asset = get_asset(&tx, &asset_id)?;
    let promoted_version =
        get_asset_version_by_id(&tx, target_version_id)?.ok_or(AppError::AssetVersionNotFound)?;
    if promoted_version.status != "canonical"
        || refreshed_asset.canonical_version_id.as_deref() != Some(target_version_id)
    {
        return Err(AppError::Database(format!(
            "canonical-promotion invariant violated for asset {asset_id}: canonical pointer and target status disagree"
        )));
    }

    tx.commit().map_err(|e| AppError::Database(e.to_string()))?;

    Ok(CanonicalPromotionResult {
        asset: refreshed_asset,
        promoted_version,
        superseded_version_id,
    })
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
        width: row.get(10)?,
        height: row.get(11)?,
        parent_version_id: row.get(12)?,
        created_at: row.get(13)?,
    })
}

fn row_to_asset_summary_record(row: &rusqlite::Row) -> rusqlite::Result<AssetSummaryRecord> {
    Ok(AssetSummaryRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        asset_type: row.get(2)?,
        label: row.get(3)?,
        owner_entity_id: row.get(4)?,
        canonical_version_id: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        version_count: row.get(8)?,
        canonical_version_number: row.get(9)?,
        preview_thumbnail_path: row.get(10)?,
    })
}
