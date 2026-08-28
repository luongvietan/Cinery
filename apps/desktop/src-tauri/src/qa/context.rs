use super::errors::QaError;
use super::models::VisualExpectation;
use crate::error::AppError;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaPlanningRequest {
    pub project_id: String,
    pub asset_version_id: String,
    pub created_at: String,
    pub expectations: Vec<VisualExpectation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaTargetContext {
    pub asset_id: String,
    pub asset_version_id: String,
    pub asset_type: String,
    pub owner_entity_id: Option<String>,
    pub file_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaReferenceContext {
    pub asset_id: String,
    pub asset_version_id: String,
    pub asset_type: String,
    pub file_path: String,
    pub purpose: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedVisualLock {
    pub section_id: String,
    pub section_revision: i64,
    pub key: String,
    pub description: String,
    pub severity: String,
    pub validator_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedQaContext {
    pub project_id: String,
    pub target: QaTargetContext,
    pub visual_locks: Vec<ResolvedVisualLock>,
    pub canonical_face: Option<QaReferenceContext>,
    pub canonical_look: Option<QaReferenceContext>,
    pub expectations: Vec<VisualExpectation>,
    pub created_at: String,
}

pub fn resolve_qa_context(
    conn: &Connection,
    request: &QaPlanningRequest,
) -> Result<ResolvedQaContext, AppError> {
    let target = conn
        .query_row(
            "SELECT a.id, av.id, a.type, a.owner_entity_id, av.file_path
             FROM asset_versions av
             JOIN assets a ON a.id = av.asset_id
             WHERE av.id = ?1 AND a.project_id = ?2",
            params![request.asset_version_id, request.project_id],
            |row| {
                Ok(QaTargetContext {
                    asset_id: row.get(0)?,
                    asset_version_id: row.get(1)?,
                    asset_type: row.get(2)?,
                    owner_entity_id: row.get(3)?,
                    file_path: row.get(4)?,
                })
            },
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::AssetVersionNotFound,
            other => AppError::Database(other.to_string()),
        })?;

    let (visual_locks, canonical_face, canonical_look) = if let Some(owner_id) = &target.owner_entity_id {
        ensure_character_owner(conn, &request.project_id, owner_id)?;
        let visual_locks = load_locked_visual_locks(conn, owner_id)?;
        let canonical_face = load_canonical_reference(
            conn,
            &request.project_id,
            owner_id,
            &["face_lock"],
            "identity_reference",
        )?;
        if canonical_face.is_none() {
            return Err(QaError::InvalidData(
                "QA blocked: character has no exact canonical Face version".into(),
            )
            .into());
        }
        let canonical_look = load_canonical_reference(
            conn,
            &request.project_id,
            owner_id,
            &["character_sheet", "outfit"],
            "look_reference",
        )?;
        (visual_locks, canonical_face, canonical_look)
    } else {
        (Vec::new(), None, None)
    };

    Ok(ResolvedQaContext {
        project_id: request.project_id.clone(),
        target,
        visual_locks,
        canonical_face,
        canonical_look,
        expectations: request.expectations.clone(),
        created_at: request.created_at.clone(),
    })
}

fn ensure_character_owner(
    conn: &Connection,
    project_id: &str,
    owner_id: &str,
) -> Result<(), AppError> {
    let entity_type = conn
        .query_row(
            "SELECT type FROM canon_entities WHERE id = ?1 AND project_id = ?2",
            params![owner_id, project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(db_error)?;
    match entity_type.as_deref() {
        Some("character") => Ok(()),
        Some(_) => Err(QaError::InvalidData("asset owner is not a character".into()).into()),
        None => Err(AppError::CanonEntityNotFound),
    }
}

fn load_locked_visual_locks(
    conn: &Connection,
    owner_id: &str,
) -> Result<Vec<ResolvedVisualLock>, AppError> {
    let row = conn
        .query_row(
            "SELECT id, revision, value_json
             FROM canon_sections
             WHERE canon_entity_id = ?1 AND section_key = 'visual_locks' AND status = 'locked'",
            [owner_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(db_error)?;
    let Some((section_id, section_revision, value_json)) = row else {
        return Ok(Vec::new());
    };
    let value: Value = serde_json::from_str(&value_json)
        .map_err(|_| AppError::InvalidCanonSectionValue)?;
    let locks = value
        .get("locks")
        .and_then(Value::as_array)
        .ok_or(AppError::InvalidCanonSectionValue)?;
    let mut resolved = locks
        .iter()
        .map(|lock| {
            Ok(ResolvedVisualLock {
                section_id: section_id.clone(),
                section_revision,
                key: required_string(lock, "key")?,
                description: required_string(lock, "description")?,
                severity: required_string(lock, "severity")?,
                validator_hint: lock
                    .get("validatorHint")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    resolved.sort_by(|left, right| left.key.cmp(&right.key));
    resolved.dedup_by(|left, right| left.key == right.key);
    Ok(resolved)
}

fn load_canonical_reference(
    conn: &Connection,
    project_id: &str,
    owner_id: &str,
    asset_types: &[&str],
    purpose: &str,
) -> Result<Option<QaReferenceContext>, AppError> {
    let preferred_type = asset_types.first().copied().unwrap_or("");
    conn.query_row(
        "SELECT a.id, av.id, a.type, av.file_path
         FROM assets a
         JOIN asset_versions av ON av.id = a.canonical_version_id
         WHERE a.project_id = ?1 AND a.owner_entity_id = ?2
           AND (a.type = ?3 OR a.type = ?4)
           AND av.asset_id = a.id
         ORDER BY CASE WHEN a.type = ?3 THEN 0 ELSE 1 END, a.id
         LIMIT 1",
        params![
            project_id,
            owner_id,
            preferred_type,
            asset_types.get(1).copied().unwrap_or(preferred_type)
        ],
        |row| {
            Ok(QaReferenceContext {
                asset_id: row.get(0)?,
                asset_version_id: row.get(1)?,
                asset_type: row.get(2)?,
                file_path: row.get(3)?,
                purpose: purpose.to_string(),
            })
        },
    )
    .optional()
    .map_err(db_error)
}

fn required_string(value: &Value, key: &str) -> Result<String, AppError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or(AppError::InvalidCanonSectionValue)
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}
