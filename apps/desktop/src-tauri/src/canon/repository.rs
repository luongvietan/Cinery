use crate::canon::model::{
    CanonEntityRecord, CanonEntityType, CanonSectionRecord, CanonSectionRevisionRecord,
};
use crate::error::AppError;
use rusqlite::{params, Connection, OptionalExtension, Transaction};

pub fn insert_entity(tx: &Transaction<'_>, record: &CanonEntityRecord) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            record.id,
            record.project_id,
            record.entity_type,
            record.name,
            record.slug,
            record.created_at,
            record.updated_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_entities(
    conn: &Connection,
    project_id: &str,
    entity_type: Option<CanonEntityType>,
) -> Result<Vec<CanonEntityRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, type, name, slug, created_at, updated_at
             FROM canon_entities
             WHERE project_id = ?1 AND (?2 IS NULL OR type = ?2)
             ORDER BY type, name COLLATE NOCASE, id",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(
            params![project_id, entity_type.map(CanonEntityType::as_str)],
            |row| {
                Ok(CanonEntityRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    entity_type: row.get(2)?,
                    name: row.get(3)?,
                    slug: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

pub fn get_entity(conn: &Connection, entity_id: &str) -> Result<CanonEntityRecord, AppError> {
    conn.query_row(
        "SELECT id, project_id, type, name, slug, created_at, updated_at
         FROM canon_entities WHERE id = ?1",
        [entity_id],
        |row| {
            Ok(CanonEntityRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                entity_type: row.get(2)?,
                name: row.get(3)?,
                slug: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or(AppError::CanonEntityNotFound)
}

pub fn find_singleton(
    conn: &Connection,
    project_id: &str,
    entity_type: CanonEntityType,
) -> Result<Option<CanonEntityRecord>, AppError> {
    conn.query_row(
        "SELECT id, project_id, type, name, slug, created_at, updated_at
         FROM canon_entities WHERE project_id = ?1 AND type = ?2 LIMIT 1",
        params![project_id, entity_type.as_str()],
        |row| {
            Ok(CanonEntityRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                entity_type: row.get(2)?,
                name: row.get(3)?,
                slug: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))
}

pub fn slug_exists(
    conn: &Connection,
    project_id: &str,
    entity_type: CanonEntityType,
    slug: &str,
) -> Result<bool, AppError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canon_entities WHERE project_id = ?1 AND type = ?2 AND slug = ?3",
            params![project_id, entity_type.as_str(), slug],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(count > 0)
}

pub fn get_section_by_key(
    tx: &Transaction<'_>,
    entity_id: &str,
    section_key: &str,
) -> Result<Option<CanonSectionRecord>, AppError> {
    tx.query_row(
        "SELECT id, canon_entity_id, section_key, value_json, status, revision,
                created_at, updated_at, locked_at
         FROM canon_sections WHERE canon_entity_id = ?1 AND section_key = ?2",
        params![entity_id, section_key],
        section_from_row,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))
}

pub fn get_section(conn: &Connection, section_id: &str) -> Result<CanonSectionRecord, AppError> {
    conn.query_row(
        "SELECT id, canon_entity_id, section_key, value_json, status, revision,
                created_at, updated_at, locked_at
         FROM canon_sections WHERE id = ?1",
        [section_id],
        section_from_row,
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or(AppError::CanonSectionNotFound)
}

pub fn list_sections(
    conn: &Connection,
    entity_id: &str,
) -> Result<Vec<CanonSectionRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, canon_entity_id, section_key, value_json, status, revision,
                    created_at, updated_at, locked_at
             FROM canon_sections WHERE canon_entity_id = ?1",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([entity_id], section_from_row)
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

pub fn insert_section(tx: &Transaction<'_>, record: &CanonSectionRecord) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO canon_sections
         (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            record.id,
            record.entity_id,
            record.key,
            serde_json::to_string(&record.value).map_err(|e| AppError::Database(e.to_string()))?,
            record.status,
            record.revision,
            record.created_at,
            record.updated_at,
            record.locked_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

pub fn update_section(tx: &Transaction<'_>, record: &CanonSectionRecord) -> Result<(), AppError> {
    tx.execute(
        "UPDATE canon_sections SET value_json = ?1, status = ?2, revision = ?3,
         updated_at = ?4, locked_at = ?5 WHERE id = ?6",
        params![
            serde_json::to_string(&record.value).map_err(|e| AppError::Database(e.to_string()))?,
            record.status,
            record.revision,
            record.updated_at,
            record.locked_at,
            record.id,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

pub fn insert_revision(
    tx: &Transaction<'_>,
    record: &CanonSectionRevisionRecord,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO canon_section_revisions
         (id, canon_section_id, revision, value_json, status, change_kind, reason, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            record.id,
            record.section_id,
            record.revision,
            serde_json::to_string(&record.value).map_err(|e| AppError::Database(e.to_string()))?,
            record.status,
            record.change_kind,
            record.reason,
            record.created_at,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_revisions(
    conn: &Connection,
    section_id: &str,
) -> Result<Vec<CanonSectionRevisionRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, canon_section_id, revision, value_json, status, change_kind, reason, created_at
             FROM canon_section_revisions WHERE canon_section_id = ?1 ORDER BY revision DESC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([section_id], revision_from_row)
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

pub fn insert_tbd(
    conn: &Connection,
    record: &crate::canon::model::CanonTbdRecord,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO canon_tbds (id, project_id, canon_entity_id, section_key, topic, note, protected, status, resolution_text, created_at, updated_at, resolved_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![record.id, record.project_id, record.canon_entity_id, record.section_key, record.topic, record.note, record.protected, record.status, record.resolution_text, record.created_at, record.updated_at, record.resolved_at],
    ).map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_tbd(
    conn: &Connection,
    tbd_id: &str,
) -> Result<crate::canon::model::CanonTbdRecord, AppError> {
    conn.query_row(
        "SELECT id, project_id, canon_entity_id, section_key, topic, note, protected, status, resolution_text, created_at, updated_at, resolved_at FROM canon_tbds WHERE id = ?1",
        [tbd_id],
        tbd_from_row,
    ).optional().map_err(|e| AppError::Database(e.to_string()))?.ok_or(AppError::CanonTbdNotFound)
}

pub fn update_tbd(
    conn: &Connection,
    record: &crate::canon::model::CanonTbdRecord,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE canon_tbds SET topic = ?1, note = ?2, protected = ?3, status = ?4, resolution_text = ?5, updated_at = ?6, resolved_at = ?7 WHERE id = ?8",
        params![record.topic, record.note, record.protected, record.status, record.resolution_text, record.updated_at, record.resolved_at, record.id],
    ).map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_tbds(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<crate::canon::model::CanonTbdRecord>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, canon_entity_id, section_key, topic, note, protected, status, resolution_text, created_at, updated_at, resolved_at
         FROM canon_tbds WHERE project_id = ?1
         ORDER BY CASE WHEN status = 'open' AND protected = 1 THEN 0 WHEN status = 'open' THEN 1 ELSE 2 END, created_at, id"
    ).map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], tbd_from_row)
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

fn tbd_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<crate::canon::model::CanonTbdRecord> {
    Ok(crate::canon::model::CanonTbdRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        canon_entity_id: row.get(2)?,
        section_key: row.get(3)?,
        topic: row.get(4)?,
        note: row.get(5)?,
        protected: row.get(6)?,
        status: row.get(7)?,
        resolution_text: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        resolved_at: row.get(11)?,
    })
}

fn section_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CanonSectionRecord> {
    let value_json: String = row.get(3)?;
    Ok(CanonSectionRecord {
        id: row.get(0)?,
        entity_id: row.get(1)?,
        key: row.get(2)?,
        value: serde_json::from_str(&value_json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?,
        status: row.get(4)?,
        revision: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        locked_at: row.get(8)?,
    })
}

fn revision_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CanonSectionRevisionRecord> {
    let value_json: String = row.get(3)?;
    Ok(CanonSectionRevisionRecord {
        id: row.get(0)?,
        section_id: row.get(1)?,
        revision: row.get(2)?,
        value: serde_json::from_str(&value_json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?,
        status: row.get(4)?,
        change_kind: row.get(5)?,
        reason: row.get(6)?,
        created_at: row.get(7)?,
    })
}
