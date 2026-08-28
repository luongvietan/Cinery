use crate::error::AppError;
use crate::skills::model::{Prerequisite, TbdGuard};
use crate::workflow::model::{PrerequisiteCheck, PrerequisiteReport, PrerequisiteStatus};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

pub fn evaluate_prerequisites(
    conn: &Connection,
    project_id: &str,
    input: &Value,
    prerequisites: &[Prerequisite],
) -> Result<PrerequisiteReport, AppError> {
    let mut checks = Vec::with_capacity(prerequisites.len());
    for (index, prerequisite) in prerequisites.iter().enumerate() {
        let (passed, message, resolved_ref) = evaluate_one(conn, project_id, input, prerequisite)?;
        checks.push(PrerequisiteCheck {
            id: format!("prerequisite-{}", index + 1),
            prerequisite: prerequisite.clone(),
            status: if passed {
                PrerequisiteStatus::Pass
            } else {
                PrerequisiteStatus::Fail
            },
            message,
            resolved_ref,
        });
    }
    Ok(PrerequisiteReport {
        passed: checks
            .iter()
            .all(|check| check.status == PrerequisiteStatus::Pass),
        checks,
    })
}

pub fn evaluate_tbd_guards(
    conn: &Connection,
    project_id: &str,
    input: &Value,
    guards: &[TbdGuard],
) -> Result<Vec<String>, AppError> {
    let mut blocking_topics = Vec::new();
    for guard in guards {
        match guard {
            TbdGuard::EntityScope { entity_input_ref } => {
                let entity_id = input_string(input, entity_input_ref)?;
                let mut statement = conn
                    .prepare("SELECT topic FROM canon_tbds WHERE project_id = ?1 AND protected = 1 AND status = 'open' AND canon_entity_id = ?2 ORDER BY id")
                    .map_err(db_error)?;
                let rows = statement
                    .query_map(params![project_id, entity_id], |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(db_error)?;
                for row in rows {
                    blocking_topics.push(row.map_err(db_error)?);
                }
            }
            TbdGuard::SectionScope {
                entity_input_ref,
                section_key,
            } => {
                let entity_id = input_string(input, entity_input_ref)?;
                let mut statement = conn
                    .prepare("SELECT topic FROM canon_tbds WHERE project_id = ?1 AND protected = 1 AND status = 'open' AND canon_entity_id = ?2 AND section_key = ?3 ORDER BY id")
                    .map_err(db_error)?;
                let rows = statement
                    .query_map(params![project_id, entity_id, section_key], |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(db_error)?;
                for row in rows {
                    blocking_topics.push(row.map_err(db_error)?);
                }
            }
            TbdGuard::ProjectScope => {
                let mut statement = conn
                    .prepare("SELECT topic FROM canon_tbds WHERE project_id = ?1 AND protected = 1 AND status = 'open' ORDER BY id")
                    .map_err(db_error)?;
                let rows = statement
                    .query_map(params![project_id], |row| row.get::<_, String>(0))
                    .map_err(db_error)?;
                for row in rows {
                    blocking_topics.push(row.map_err(db_error)?);
                }
            }
        }
    }
    Ok(blocking_topics)
}

fn evaluate_one(
    conn: &Connection,
    project_id: &str,
    input: &Value,
    prerequisite: &Prerequisite,
) -> Result<(bool, String, Option<String>), AppError> {
    match prerequisite {
        Prerequisite::CanonEntityExists {
            entity_type,
            input_ref,
        } => {
            let entity_id = input_string(input, input_ref)?;
            let found: Option<String> = conn
                .query_row(
                    "SELECT id FROM canon_entities WHERE project_id = ?1 AND id = ?2 AND type = ?3",
                    params![project_id, entity_id, entity_type.as_str()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_error)?;
            Ok((
                found.is_some(),
                if found.is_some() {
                    "Canon entity exists with the required type"
                } else {
                    "Canon entity is missing or has the wrong type"
                }
                .to_string(),
                found,
            ))
        }
        Prerequisite::CanonSectionLocked {
            entity_input_ref,
            section_key,
        } => {
            let entity_id = input_string(input, entity_input_ref)?;
            let found: Option<String> = conn
                .query_row(
                    "SELECT s.id FROM canon_sections s JOIN canon_entities e ON e.id = s.canon_entity_id WHERE e.project_id = ?1 AND e.id = ?2 AND s.section_key = ?3 AND s.status = 'locked'",
                    params![project_id, entity_id, section_key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_error)?;
            Ok((
                found.is_some(),
                if found.is_some() {
                    "Canon section is locked"
                } else {
                    "Canon section is not locked"
                }
                .to_string(),
                found,
            ))
        }
        Prerequisite::CanonicalAssetExists {
            owner_entity_input_ref,
            asset_type,
        } => {
            let owner_id = input_string(input, owner_entity_input_ref)?;
            let found: Option<String> = conn
                .query_row(
                    "SELECT a.canonical_version_id FROM assets a JOIN asset_versions v ON v.id = a.canonical_version_id WHERE a.project_id = ?1 AND a.owner_entity_id = ?2 AND a.type = ?3 AND v.status = 'canonical'",
                    params![project_id, owner_id, asset_type.as_str()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_error)?;
            Ok((
                found.is_some(),
                if found.is_some() {
                    "Canonical asset version exists"
                } else {
                    "No canonical asset version exists"
                }
                .to_string(),
                found,
            ))
        }
        Prerequisite::AssetVersionStatus {
            asset_version_input_ref,
            status,
        } => {
            let version_id = input_string(input, asset_version_input_ref)?;
            let found: Option<String> = conn
                .query_row(
                    "SELECT v.id FROM asset_versions v JOIN assets a ON a.id = v.asset_id WHERE a.project_id = ?1 AND v.id = ?2 AND v.status = ?3",
                    params![project_id, version_id, status.as_str()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_error)?;
            Ok((
                found.is_some(),
                if found.is_some() {
                    "Asset version has the required status"
                } else {
                    "Asset version is missing or has a different status"
                }
                .to_string(),
                found,
            ))
        }
    }
}

fn input_string(input: &Value, reference: &str) -> Result<String, AppError> {
    input
        .get(reference)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            AppError::WorkflowInputInvalid(format!(
                "input reference '{reference}' must be a non-empty string"
            ))
        })
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;

    fn connection() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute("INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('p', 'Project', 'now', 'now', 1)", []).unwrap();
        conn
    }

    #[test]
    fn entity_prerequisite_checks_project_and_type() {
        let conn = connection();
        conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('c', 'p', 'character', 'Mara', 'mara', 'now', 'now')", []).unwrap();
        let prerequisite = Prerequisite::CanonEntityExists {
            entity_type: crate::canon::model::CanonEntityType::Character,
            input_ref: "characterEntityId".into(),
        };
        let report = evaluate_prerequisites(
            &conn,
            "p",
            &serde_json::json!({"characterEntityId":"c"}),
            &[prerequisite],
        )
        .unwrap();
        assert!(report.passed);
    }

    #[test]
    fn newest_candidate_does_not_satisfy_canonical_asset_requirement() {
        let conn = connection();
        conn.execute("INSERT INTO assets (id, project_id, type, label, owner_entity_id, canonical_version_id, created_at, updated_at) VALUES ('a', 'p', 'face_lock', 'Face', 'c', NULL, 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) VALUES ('v1', 'a', 1, 'candidate', 'f', 't', 'h1', 'f.png', 'image/png', 1, 'now')", []).unwrap();
        let prerequisite = Prerequisite::CanonicalAssetExists {
            owner_entity_input_ref: "characterEntityId".into(),
            asset_type: crate::skills::model::AssetType::FaceLock,
        };
        let report = evaluate_prerequisites(
            &conn,
            "p",
            &serde_json::json!({"characterEntityId":"c"}),
            &[prerequisite],
        )
        .unwrap();
        assert!(!report.passed);
    }

    #[test]
    fn relevant_protected_tbd_blocks_but_resolved_tbd_does_not() {
        let conn = connection();
        conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('c', 'p', 'character', 'Mara', 'mara', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO canon_tbds (id, project_id, canon_entity_id, section_key, topic, protected, status, created_at, updated_at) VALUES ('t1', 'p', 'c', 'visual_locks', 'scar placement', 1, 'open', 'now', 'now')", []).unwrap();
        let guard = TbdGuard::SectionScope {
            entity_input_ref: "characterEntityId".into(),
            section_key: "visual_locks".into(),
        };
        let topics = evaluate_tbd_guards(
            &conn,
            "p",
            &serde_json::json!({"characterEntityId":"c"}),
            &[guard.clone()],
        )
        .unwrap();
        assert_eq!(topics, vec!["scar placement"]);
        conn.execute(
            "UPDATE canon_tbds SET status = 'resolved' WHERE id = 't1'",
            [],
        )
        .unwrap();
        assert!(evaluate_tbd_guards(
            &conn,
            "p",
            &serde_json::json!({"characterEntityId":"c"}),
            &[guard]
        )
        .unwrap()
        .is_empty());
    }
}
