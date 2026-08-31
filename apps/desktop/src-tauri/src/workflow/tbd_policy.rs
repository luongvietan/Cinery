use crate::canon::model::CanonTbdRecord;
use crate::error::AppError;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Deterministic TBD handling decision kind for P7 Worlds/Scenes.
///
/// There is no `resolve_for_generation` in P7. To reveal protected
/// information, resolve it in Canon first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TbdDecisionKind {
    PreserveUnknown,
    NotApplicable,
}

impl TbdDecisionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreserveUnknown => "preserve_unknown",
            Self::NotApplicable => "not_applicable",
        }
    }
}

/// Immutable handling decision for a single protected TBD.
///
/// `topic_snapshot` and `note_snapshot` are copied at decision time
/// so that later Canon edits do not retroactively rewrite the workflow
/// context. This satisfies the "immutable workflow snapshot representation"
/// requirement: the snapshot is stored with the decision, not re-queried
/// from live Canon at generation time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TbdDecision {
    pub tbd_id: String,
    pub topic_snapshot: String,
    pub note_snapshot: Option<String>,
    pub decision: TbdDecisionKind,
    pub justification: Option<String>,
}

impl TbdDecision {
    /// Create a snapshot decision from a live `CanonTbdRecord`.
    ///
    /// Copies `topic` and `note` into the snapshot fields so the workflow
    /// snapshot is immutable even if Canon is later edited.
    pub fn snapshot_from_tbd(
        tbd: &CanonTbdRecord,
        decision: TbdDecisionKind,
        justification: Option<String>,
    ) -> Self {
        Self {
            tbd_id: tbd.id.clone(),
            topic_snapshot: tbd.topic.clone(),
            note_snapshot: tbd.note.clone(),
            decision,
            justification: justification
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        }
    }
}

/// Returns true if the TBD is project-scoped (applies globally, not to a
/// specific Canon entity). Project-scoped TBDs are the only ones that may
/// be classified as `not_applicable` (with justification).
pub fn is_project_scoped(tbd: &CanonTbdRecord) -> bool {
    tbd.canon_entity_id.is_none()
}

/// Core deterministic validator — pure, no DB I/O.
///
/// Rules (spec 11.1-11.3):
/// - 11.1 Entity/section-scoped protected TBD must be `preserve_unknown`,
///   not `not_applicable` → `PROTECTED_TBD_MUST_BE_PRESERVED`.
/// - 11.2 Project-scoped may be `preserve_unknown` or `not_applicable`
///   (requires non-empty justification) → `TBD_NOT_APPLICABLE_REASON_REQUIRED`.
/// - 11.3 Missing decision blocks generation → `TBD_DECISION_REQUIRED`.
///
/// Only `protected == true && status == "open"` TBDs are considered.
/// Resolved or unprotected TBDs never block.
///
/// No operation mutates `canon_tbds` — validation is read-only.
pub fn validate_tbd_decisions(
    applicable_tbds: &[CanonTbdRecord],
    decisions: &[TbdDecision],
) -> Result<(), AppError> {
    let mut decision_map: HashMap<&str, &TbdDecision> = HashMap::new();
    for decision in decisions {
        // First entry wins; duplicates are tolerated but first is authoritative.
        decision_map
            .entry(decision.tbd_id.as_str())
            .or_insert(decision);
    }

    for tbd in applicable_tbds {
        if !tbd.protected || tbd.status != "open" {
            continue;
        }

        let decision = match decision_map.get(tbd.id.as_str()) {
            Some(d) => d,
            None => {
                return Err(AppError::TbdDecisionRequired(format!(
                    "TBD {} '{}' requires an explicit handling decision",
                    tbd.id, tbd.topic
                )))
            }
        };

        match decision.decision {
            TbdDecisionKind::PreserveUnknown => {
                // Always allowed; justification optional.
            }
            TbdDecisionKind::NotApplicable => {
                if !is_project_scoped(tbd) {
                    return Err(AppError::ProtectedTbdMustBePreserved(format!(
                        "TBD {} '{}' is directly scoped (entity/section) and must be preserve_unknown, not not_applicable",
                        tbd.id, tbd.topic
                    )));
                }
                let has_reason = decision
                    .justification
                    .as_ref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                if !has_reason {
                    return Err(AppError::TbdNotApplicableReasonRequired(format!(
                        "TBD {} '{}' not_applicable requires a non-empty justification",
                        tbd.id, tbd.topic
                    )));
                }
            }
        }
    }

    Ok(())
}

/// Loads open protected TBDs applicable to a set of Canon entity ids.
///
/// Applicable = project-scoped (canon_entity_id IS NULL) OR entity/section-scoped
/// where canon_entity_id IN (entity_ids). This captures:
/// - location-scoped (`canon_entity_id = location_id`, section_key IS NULL)
/// - section-scoped (`canon_entity_id = location_id`, section_key = 'description' etc)
/// - project-scoped (global)
///
/// Resolved or unprotected TBDs are not returned (they do not block).
pub fn load_applicable_tbds(
    conn: &Connection,
    project_id: &str,
    entity_ids: &[String],
) -> Result<Vec<CanonTbdRecord>, AppError> {
    if entity_ids.is_empty() {
        // Only project-scoped globals.
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, canon_entity_id, section_key, topic, note, protected, status, resolution_text, created_at, updated_at, resolved_at \
                 FROM canon_tbds \
                 WHERE project_id = ?1 AND protected = 1 AND status = 'open' AND canon_entity_id IS NULL \
                 ORDER BY id",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![project_id], tbd_from_row)
            .map_err(|e| AppError::Database(e.to_string()))?;
        return rows
            .map(|r| r.map_err(|e| AppError::Database(e.to_string())))
            .collect();
    }

    let placeholders = entity_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 2))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, project_id, canon_entity_id, section_key, topic, note, protected, status, resolution_text, created_at, updated_at, resolved_at \
         FROM canon_tbds \
         WHERE project_id = ?1 AND protected = 1 AND status = 'open' \
           AND (canon_entity_id IS NULL OR canon_entity_id IN ({placeholders})) \
         ORDER BY id"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut query_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
    query_params.push(&project_id);
    for id in entity_ids {
        query_params.push(id);
    }
    let rows = stmt
        .query_map(query_params.as_slice(), tbd_from_row)
        .map_err(|e| AppError::Database(e.to_string()))?;
    rows.map(|r| r.map_err(|e| AppError::Database(e.to_string())))
        .collect()
}

/// Validates the TBD firewall for a World (location) generation.
///
/// Loads applicable protected TBDs for `location_entity_id` (entity + section + project)
/// and runs `validate_tbd_decisions`.
pub fn validate_world_tbd_firewall(
    conn: &Connection,
    project_id: &str,
    location_entity_id: &str,
    decisions: &[TbdDecision],
) -> Result<(), AppError> {
    let applicable = load_applicable_tbds(conn, project_id, &[location_entity_id.to_string()])?;
    validate_tbd_decisions(&applicable, decisions)
}

/// Validates the TBD firewall for a Scene generation.
///
/// `entity_ids` should contain the Scene's World location entity id plus all
/// explicitly assigned character entity ids (and any other canon entities
/// pinned to the scene). Project-scoped globals are always included.
pub fn validate_scene_tbd_firewall(
    conn: &Connection,
    project_id: &str,
    entity_ids: &[String],
    decisions: &[TbdDecision],
) -> Result<(), AppError> {
    let applicable = load_applicable_tbds(conn, project_id, entity_ids)?;
    validate_tbd_decisions(&applicable, decisions)
}

fn tbd_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CanonTbdRecord> {
    Ok(CanonTbdRecord {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use rusqlite::Connection;

    /// Where a TBD hangs: optionally scoped to an entity section.
    struct TbdLocator<'a> {
        entity_id: Option<&'a str>,
        section_key: Option<&'a str>,
    }

    fn test_tbd(
        id: &str,
        project_id: &str,
        locator: TbdLocator<'_>,
        topic: &str,
        note: Option<&str>,
        protected: bool,
        status: &str,
    ) -> CanonTbdRecord {
        let TbdLocator {
            entity_id,
            section_key,
        } = locator;
        CanonTbdRecord {
            id: id.to_string(),
            project_id: project_id.to_string(),
            canon_entity_id: entity_id.map(str::to_string),
            section_key: section_key.map(str::to_string),
            topic: topic.to_string(),
            note: note.map(str::to_string),
            protected,
            status: status.to_string(),
            resolution_text: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            resolved_at: None,
        }
    }

    fn decision(
        tbd_id: &str,
        topic_snapshot: &str,
        note_snapshot: Option<&str>,
        kind: TbdDecisionKind,
        justification: Option<&str>,
    ) -> TbdDecision {
        TbdDecision {
            tbd_id: tbd_id.to_string(),
            topic_snapshot: topic_snapshot.to_string(),
            note_snapshot: note_snapshot.map(str::to_string),
            decision: kind,
            justification: justification.map(str::to_string),
        }
    }

    #[test]
    fn location_scoped_protected_tbd_must_be_preserve_unknown() {
        let tbd = test_tbd(
            "tbd-1",
            "proj-1",
            TbdLocator {
                entity_id: Some("loc-1"),
                section_key: None,
            },
            "What is behind the red door?",
            Some("Do not reveal"),
            true,
            "open",
        );
        // preserve_unknown allowed
        let preserve = decision(
            "tbd-1",
            &tbd.topic,
            tbd.note.as_deref(),
            TbdDecisionKind::PreserveUnknown,
            None,
        );
        assert!(validate_tbd_decisions(std::slice::from_ref(&tbd), &[preserve]).is_ok());

        // not_applicable must be rejected with PROTECTED_TBD_MUST_BE_PRESERVED
        let not_applicable = decision(
            "tbd-1",
            &tbd.topic,
            tbd.note.as_deref(),
            TbdDecisionKind::NotApplicable,
            Some("not relevant"),
        );
        let err = validate_tbd_decisions(&[tbd], &[not_applicable]).unwrap_err();
        assert_eq!(err.code(), "PROTECTED_TBD_MUST_BE_PRESERVED");
    }

    #[test]
    fn section_scoped_protected_tbd_must_be_preserve_unknown() {
        let tbd = test_tbd(
            "tbd-2",
            "proj-1",
            TbdLocator {
                entity_id: Some("loc-1"),
                section_key: Some("description"),
            },
            "Unknown geography detail",
            None,
            true,
            "open",
        );
        let preserve = decision(
            "tbd-2",
            &tbd.topic,
            None,
            TbdDecisionKind::PreserveUnknown,
            None,
        );
        assert!(validate_tbd_decisions(std::slice::from_ref(&tbd), &[preserve]).is_ok());

        let not_applicable = decision(
            "tbd-2",
            &tbd.topic,
            None,
            TbdDecisionKind::NotApplicable,
            Some("not relevant to this world"),
        );
        let err = validate_tbd_decisions(&[tbd], &[not_applicable]).unwrap_err();
        assert_eq!(err.code(), "PROTECTED_TBD_MUST_BE_PRESERVED");
    }

    #[test]
    fn project_scoped_tbd_missing_decision_blocks_with_tbd_decision_required() {
        let tbd = test_tbd(
            "tbd-3",
            "proj-1",
            TbdLocator {
                entity_id: None,
                section_key: None,
            },
            "Global unknown",
            None,
            true,
            "open",
        );
        let err = validate_tbd_decisions(&[tbd], &[]).unwrap_err();
        assert_eq!(err.code(), "TBD_DECISION_REQUIRED");
    }

    #[test]
    fn project_scoped_not_applicable_without_reason_blocks() {
        let tbd = test_tbd(
            "tbd-4",
            "proj-1",
            TbdLocator {
                entity_id: None,
                section_key: None,
            },
            "Global unknown",
            None,
            true,
            "open",
        );
        let no_reason = decision(
            "tbd-4",
            &tbd.topic,
            None,
            TbdDecisionKind::NotApplicable,
            None,
        );
        let err = validate_tbd_decisions(std::slice::from_ref(&tbd), &[no_reason]).unwrap_err();
        assert_eq!(err.code(), "TBD_NOT_APPLICABLE_REASON_REQUIRED");

        let empty_reason = decision(
            "tbd-4",
            &tbd.topic,
            None,
            TbdDecisionKind::NotApplicable,
            Some("   "),
        );
        let err2 = validate_tbd_decisions(&[tbd], &[empty_reason]).unwrap_err();
        assert_eq!(err2.code(), "TBD_NOT_APPLICABLE_REASON_REQUIRED");
    }

    #[test]
    fn project_scoped_not_applicable_with_reason_allowed() {
        let tbd = test_tbd(
            "tbd-5",
            "proj-1",
            TbdLocator {
                entity_id: None,
                section_key: None,
            },
            "Global unknown",
            Some("note"),
            true,
            "open",
        );
        let with_reason = decision(
            "tbd-5",
            &tbd.topic,
            tbd.note.as_deref(),
            TbdDecisionKind::NotApplicable,
            Some("Not relevant to this scene; exterior only"),
        );
        assert!(validate_tbd_decisions(&[tbd], &[with_reason]).is_ok());
    }

    #[test]
    fn project_scoped_preserve_unknown_without_reason_allowed() {
        let tbd = test_tbd(
            "tbd-6",
            "proj-1",
            TbdLocator {
                entity_id: None,
                section_key: None,
            },
            "Global unknown",
            None,
            true,
            "open",
        );
        let preserve = decision(
            "tbd-6",
            &tbd.topic,
            None,
            TbdDecisionKind::PreserveUnknown,
            None,
        );
        assert!(validate_tbd_decisions(&[tbd], &[preserve]).is_ok());
    }

    #[test]
    fn resolved_tbd_does_not_require_decision() {
        let open_tbd = test_tbd(
            "tbd-7",
            "proj-1",
            TbdLocator {
                entity_id: None,
                section_key: None,
            },
            "Open unknown",
            None,
            true,
            "open",
        );
        let resolved_tbd = test_tbd(
            "tbd-8",
            "proj-1",
            TbdLocator {
                entity_id: None,
                section_key: None,
            },
            "Resolved unknown",
            None,
            true,
            "resolved",
        );
        // Only open TBD needs decision; resolved does not.
        let preserve = decision(
            "tbd-7",
            &open_tbd.topic,
            None,
            TbdDecisionKind::PreserveUnknown,
            None,
        );
        // Validate with both TBDS but only decision for open one -> should pass (resolved ignored)
        assert!(validate_tbd_decisions(&[open_tbd, resolved_tbd.clone()], &[preserve]).is_ok());

        // Validate with resolved alone and no decisions -> should pass
        assert!(validate_tbd_decisions(&[resolved_tbd], &[]).is_ok());
    }

    #[test]
    fn unprotected_tbd_does_not_block() {
        let unprotected = test_tbd(
            "tbd-9",
            "proj-1",
            TbdLocator {
                entity_id: Some("loc-1"),
                section_key: None,
            },
            "Unprotected detail",
            None,
            false,
            "open",
        );
        assert!(validate_tbd_decisions(&[unprotected], &[]).is_ok());
    }

    #[test]
    fn no_operation_mutates_canon_status() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('proj-1', 'P', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO canon_tbds (id, project_id, topic, protected, status, created_at, updated_at) VALUES ('tbd-10', 'proj-1', 'Global', 1, 'open', 'now', 'now')",
            [],
        )
        .unwrap();
        let applicable = load_applicable_tbds(&conn, "proj-1", &[]).unwrap();
        assert_eq!(applicable.len(), 1);
        assert_eq!(applicable[0].status, "open");
        let decisions = vec![TbdDecision::snapshot_from_tbd(
            &applicable[0],
            TbdDecisionKind::PreserveUnknown,
            None,
        )];
        validate_tbd_decisions(&applicable, &decisions).unwrap();
        // Verify DB still open, not resolved
        let status: String = conn
            .query_row(
                "SELECT status FROM canon_tbds WHERE id = 'tbd-10'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "open");
    }

    #[test]
    fn missing_decision_error_code_is_tbd_decision_required() {
        let tbd = test_tbd(
            "tbd-11",
            "proj-1",
            TbdLocator {
                entity_id: None,
                section_key: None,
            },
            "Missing",
            None,
            true,
            "open",
        );
        let err = validate_tbd_decisions(&[tbd], &[]).unwrap_err();
        assert_eq!(err.code(), "TBD_DECISION_REQUIRED");
        assert!(err.to_string().contains("tbd-11"));
    }

    #[test]
    fn protected_must_be_preserved_error_code() {
        let tbd = test_tbd(
            "tbd-12",
            "proj-1",
            TbdLocator {
                entity_id: Some("loc-1"),
                section_key: None,
            },
            "Location secret",
            None,
            true,
            "open",
        );
        let d = decision(
            "tbd-12",
            &tbd.topic,
            None,
            TbdDecisionKind::NotApplicable,
            Some("reason"),
        );
        let err = validate_tbd_decisions(&[tbd], &[d]).unwrap_err();
        assert_eq!(err.code(), "PROTECTED_TBD_MUST_BE_PRESERVED");
    }

    #[test]
    fn not_applicable_reason_required_error_code() {
        let tbd = test_tbd(
            "tbd-13",
            "proj-1",
            TbdLocator {
                entity_id: None,
                section_key: None,
            },
            "Project unknown",
            None,
            true,
            "open",
        );
        let d = decision(
            "tbd-13",
            &tbd.topic,
            None,
            TbdDecisionKind::NotApplicable,
            Some(""),
        );
        let err = validate_tbd_decisions(&[tbd], &[d]).unwrap_err();
        assert_eq!(err.code(), "TBD_NOT_APPLICABLE_REASON_REQUIRED");
    }

    #[test]
    fn tbd_topic_and_note_copied_into_snapshot() {
        let tbd = test_tbd(
            "tbd-14",
            "proj-1",
            TbdLocator {
                entity_id: Some("loc-1"),
                section_key: None,
            },
            "Exact topic",
            Some("Exact note for preservation"),
            true,
            "open",
        );
        let snapshot = TbdDecision::snapshot_from_tbd(&tbd, TbdDecisionKind::PreserveUnknown, None);
        assert_eq!(snapshot.tbd_id, "tbd-14");
        assert_eq!(snapshot.topic_snapshot, "Exact topic");
        assert_eq!(
            snapshot.note_snapshot.as_deref(),
            Some("Exact note for preservation")
        );
        assert_eq!(snapshot.decision, TbdDecisionKind::PreserveUnknown);

        // Snapshot must be immutable: mutating live TBD does not affect snapshot
        let mut mutated_tbd = tbd.clone();
        mutated_tbd.topic = "Mutated topic".into();
        mutated_tbd.note = Some("Mutated note".into());
        // Snapshot still holds original values
        assert_eq!(snapshot.topic_snapshot, "Exact topic");
        assert_ne!(snapshot.topic_snapshot, mutated_tbd.topic);
    }

    #[test]
    fn tbd_decision_serializes_with_camel_case() {
        let d = TbdDecision {
            tbd_id: "tbd-15".into(),
            topic_snapshot: "Topic".into(),
            note_snapshot: Some("Note".into()),
            decision: TbdDecisionKind::PreserveUnknown,
            justification: None,
        };
        let value = serde_json::to_value(&d).unwrap();
        assert_eq!(value["tbdId"], "tbd-15");
        assert_eq!(value["topicSnapshot"], "Topic");
        assert_eq!(value["decision"], "preserve_unknown");
        // Ensure no extra fields like resolve_for_generation
        assert!(value.get("resolveForGeneration").is_none());
    }

    #[test]
    fn no_resolve_for_generation_variant_exists() {
        // Ensure TbdDecisionKind only has two variants and no resolve_for_generation
        let preserve = serde_json::to_value(TbdDecisionKind::PreserveUnknown).unwrap();
        let not_applicable = serde_json::to_value(TbdDecisionKind::NotApplicable).unwrap();
        assert_eq!(preserve, "preserve_unknown");
        assert_eq!(not_applicable, "not_applicable");
        // Deserializing an invalid variant must fail
        let invalid =
            serde_json::from_value::<TbdDecisionKind>(serde_json::json!("resolve_for_generation"));
        assert!(invalid.is_err());
    }

    #[test]
    fn world_firewall_blocks_without_decision_and_allows_with_preserve() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('proj-1', 'P', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('loc-1', 'proj-1', 'location', 'Station', 'station', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO canon_tbds (id, project_id, canon_entity_id, topic, protected, status, created_at, updated_at) VALUES ('tbd-loc', 'proj-1', 'loc-1', 'Location secret', 1, 'open', 'now', 'now')",
            [],
        )
        .unwrap();
        // Without decision -> block
        let err = validate_world_tbd_firewall(&conn, "proj-1", "loc-1", &[]).unwrap_err();
        assert_eq!(err.code(), "TBD_DECISION_REQUIRED");

        // With preserve_unknown -> allowed
        let applicable = load_applicable_tbds(&conn, "proj-1", &["loc-1".into()]).unwrap();
        let decisions = vec![TbdDecision::snapshot_from_tbd(
            &applicable[0],
            TbdDecisionKind::PreserveUnknown,
            None,
        )];
        assert!(validate_world_tbd_firewall(&conn, "proj-1", "loc-1", &decisions).is_ok());

        // With not_applicable -> must be rejected
        let bad = vec![TbdDecision {
            tbd_id: applicable[0].id.clone(),
            topic_snapshot: applicable[0].topic.clone(),
            note_snapshot: applicable[0].note.clone(),
            decision: TbdDecisionKind::NotApplicable,
            justification: Some("reason".into()),
        }];
        let err2 = validate_world_tbd_firewall(&conn, "proj-1", "loc-1", &bad).unwrap_err();
        assert_eq!(err2.code(), "PROTECTED_TBD_MUST_BE_PRESERVED");
    }

    #[test]
    fn scene_firewall_includes_multiple_entities_and_project_global() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('proj-1', 'P', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        for (id, typ) in [("loc-1", "location"), ("char-1", "character")] {
            conn.execute(
                "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES (?1, 'proj-1', ?2, 'Name', 'slug', 'now', 'now')",
                params![id, typ],
            )
            .unwrap();
        }
        // Location TBD
        conn.execute(
            "INSERT INTO canon_tbds (id, project_id, canon_entity_id, topic, protected, status, created_at, updated_at) VALUES ('tbd-loc', 'proj-1', 'loc-1', 'Loc TBD', 1, 'open', 'now', 'now')",
            [],
        )
        .unwrap();
        // Character TBD
        conn.execute(
            "INSERT INTO canon_tbds (id, project_id, canon_entity_id, topic, protected, status, created_at, updated_at) VALUES ('tbd-char', 'proj-1', 'char-1', 'Char TBD', 1, 'open', 'now', 'now')",
            [],
        )
        .unwrap();
        // Project global TBD
        conn.execute(
            "INSERT INTO canon_tbds (id, project_id, topic, protected, status, created_at, updated_at) VALUES ('tbd-proj', 'proj-1', 'Global TBD', 1, 'open', 'now', 'now')",
            [],
        )
        .unwrap();
        // Scene covers loc-1 and char-1 -> should see 3 TBDs
        let applicable =
            load_applicable_tbds(&conn, "proj-1", &["loc-1".into(), "char-1".into()]).unwrap();
        assert_eq!(applicable.len(), 3);

        // Missing any one blocks
        let partial = vec![
            TbdDecision::snapshot_from_tbd(&applicable[0], TbdDecisionKind::PreserveUnknown, None),
            TbdDecision::snapshot_from_tbd(&applicable[1], TbdDecisionKind::PreserveUnknown, None),
        ];
        let err = validate_scene_tbd_firewall(
            &conn,
            "proj-1",
            &["loc-1".into(), "char-1".into()],
            &partial,
        )
        .unwrap_err();
        assert_eq!(err.code(), "TBD_DECISION_REQUIRED");

        // All decisions with correct kinds -> ok (project global can be not_applicable with reason)
        let full = vec![
            TbdDecision::snapshot_from_tbd(&applicable[0], TbdDecisionKind::PreserveUnknown, None),
            TbdDecision::snapshot_from_tbd(&applicable[1], TbdDecisionKind::PreserveUnknown, None),
            TbdDecision {
                tbd_id: applicable[2].id.clone(),
                topic_snapshot: applicable[2].topic.clone(),
                note_snapshot: applicable[2].note.clone(),
                decision: TbdDecisionKind::NotApplicable,
                justification: Some("Exterior scene, global interior unknown not relevant".into()),
            },
        ];
        assert!(validate_scene_tbd_firewall(
            &conn,
            "proj-1",
            &["loc-1".into(), "char-1".into()],
            &full
        )
        .is_ok());
    }

    #[test]
    fn unrelated_entity_tbd_does_not_block_world() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('proj-1', 'P', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('loc-1', 'proj-1', 'location', 'Station', 'station', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('loc-2', 'proj-1', 'location', 'Other', 'other', 'now', 'now')",
            [],
        )
        .unwrap();
        // TBD for loc-2 should not block world for loc-1
        conn.execute(
            "INSERT INTO canon_tbds (id, project_id, canon_entity_id, topic, protected, status, created_at, updated_at) VALUES ('tbd-other', 'proj-1', 'loc-2', 'Other loc secret', 1, 'open', 'now', 'now')",
            [],
        )
        .unwrap();
        assert!(validate_world_tbd_firewall(&conn, "proj-1", "loc-1", &[]).is_ok());
    }
}
