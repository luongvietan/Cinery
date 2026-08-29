use crate::canon::model::CanonEntityType;
use crate::error::AppError;
use crate::skills::model::AssetType;
use crate::workflow::model::{
    CanonSnapshotRef, CanonSnapshotStatus, CanonTbdSnapshot, PrerequisiteReport,
    WorkflowContextSnapshot, WorkflowProjectRef, WorkflowSkillRef,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub fn resolve_character_face_lock_context(
    conn: &Connection,
    project_id: &str,
    skill_id: &str,
    skill_version: &str,
    operation_id: &str,
    input: &Value,
    prerequisite_report: PrerequisiteReport,
) -> Result<WorkflowContextSnapshot, AppError> {
    let character_id = input
        .get("characterEntityId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::WorkflowInputInvalid("characterEntityId must be a non-empty string".into())
        })?;
    let (story_name, entity_type): (String, String) = conn
        .query_row(
            "SELECT name, type FROM canon_entities WHERE project_id = ?1 AND id = ?2",
            params![project_id, character_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::CanonEntityNotFound,
            other => AppError::Database(other.to_string()),
        })?;
    if entity_type != CanonEntityType::Character.as_str() {
        return Err(AppError::WorkflowPrerequisiteFailed(
            "selected entity is not a character".into(),
        ));
    }

    let mut canon = Vec::new();
    let mut role_tag = None;
    let mut visual_summary = None;
    let mut permanent_visual_locks = Vec::new();
    let mut statement = conn
        .prepare("SELECT id, section_key, value_json, revision, status FROM canon_sections WHERE canon_entity_id = ?1 AND status = 'locked' ORDER BY section_key")
        .map_err(db_error)?;
    let rows = statement
        .query_map([character_id], |row| {
            let value_json: String = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                value_json,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(db_error)?;
    for row in rows {
        let (section_id, section_key, value_json, revision, status) = row.map_err(db_error)?;
        let value: Value = serde_json::from_str(&value_json)
            .map_err(|error| AppError::Database(error.to_string()))?;
        canon.push(CanonSnapshotRef {
            entity_id: character_id.to_string(),
            entity_type: CanonEntityType::Character,
            section_id,
            section_key: section_key.clone(),
            revision,
            status: CanonSnapshotStatus::Locked,
            value: value.clone(),
        });
        match section_key.as_str() {
            "role_tag" => {
                role_tag = value
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }
            "visual_summary" => {
                visual_summary = value
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }
            "visual_locks" => {
                if let Some(values) = value.get("locks").and_then(Value::as_array) {
                    permanent_visual_locks = values.clone();
                }
            }
            _ => {}
        }
        let _ = status;
    }

    let protected_tbds = load_protected_tbds(conn, project_id)?;
    let detailed_visual_spec = input.get("visualSpec").cloned().unwrap_or(Value::Null);
    let baseline_wardrobe = input
        .get("baselineWardrobe")
        .cloned()
        .unwrap_or(Value::String(String::new()));
    let resolved_context = json!({
        "character": {
            "entityId": character_id,
            "storyName": story_name,
            "roleTag": role_tag,
            "visualSummary": visual_summary,
            "permanentVisualLocks": permanent_visual_locks,
        },
        "detailedVisualSpec": detailed_visual_spec,
        "baselineWardrobe": baseline_wardrobe,
        "referencePlateRules": {
            "background": "flat 18% neutral gray field",
            "lighting": "flat shadowless neutral illumination",
            "castShadow": false,
            "contactShadow": false,
            "cinematicDepthOfField": false,
            "biologicalRealism": true,
        }
    });
    let assets = if let Some(asset_version_id) = input
        .get("sourceAssetVersionId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        vec![load_selected_asset_snapshot(conn, project_id, asset_version_id)?]
    } else {
        Vec::new()
    };

    Ok(WorkflowContextSnapshot {
        snapshot_version: 1,
        project: WorkflowProjectRef {
            project_id: project_id.to_string(),
        },
        skill: WorkflowSkillRef {
            skill_id: skill_id.to_string(),
            skill_version: skill_version.to_string(),
            operation_id: operation_id.to_string(),
        },
        input: input.clone(),
        prerequisite_report,
        canon,
        assets,
        protected_tbds,
        resolved_context,
        captured_at: Utc::now().to_rfc3339(),
    })
}

pub fn resolve_character_outfit_context(
    conn: &Connection,
    project_id: &str,
    _skill_id: &str,
    _skill_version: &str,
    _operation_id: &str,
    input: &Value,
    prerequisite_report: PrerequisiteReport,
) -> Result<WorkflowContextSnapshot, AppError> {
    let character_id = input
        .get("characterEntityId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::WorkflowInputInvalid("characterEntityId must be a non-empty string".into())
        })?;
    let (story_name, entity_type): (String, String) = conn
        .query_row(
            "SELECT name, type FROM canon_entities WHERE project_id = ?1 AND id = ?2",
            params![project_id, character_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::CanonEntityNotFound,
            other => AppError::Database(other.to_string()),
        })?;
    if entity_type != CanonEntityType::Character.as_str() {
        return Err(AppError::WorkflowPrerequisiteFailed(
            "selected entity is not a character".into(),
        ));
    }

    let mut canon = Vec::new();
    let mut role_tag = None;
    let mut visual_summary = None;
    let mut permanent_visual_locks = Vec::new();
    let mut statement = conn
        .prepare("SELECT id, section_key, value_json, revision, status FROM canon_sections WHERE canon_entity_id = ?1 AND status = 'locked' ORDER BY section_key")
        .map_err(db_error)?;
    let rows = statement
        .query_map([character_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(db_error)?;
    for row in rows {
        let (section_id, section_key, value_json, revision, status) = row.map_err(db_error)?;
        let value: Value = serde_json::from_str(&value_json)
            .map_err(|error| AppError::Database(error.to_string()))?;
        canon.push(CanonSnapshotRef {
            entity_id: character_id.to_string(),
            entity_type: CanonEntityType::Character,
            section_id,
            section_key: section_key.clone(),
            revision,
            status: CanonSnapshotStatus::Locked,
            value: value.clone(),
        });
        match section_key.as_str() {
            "role_tag" => {
                role_tag = value
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }
            "visual_summary" => {
                visual_summary = value
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }
            "visual_locks" => {
                if let Some(values) = value.get("locks").and_then(Value::as_array) {
                    permanent_visual_locks = values.clone();
                }
            }
            _ => {}
        }
        let _ = status;
    }

    let protected_tbds = load_protected_tbds(conn, project_id)?;
    let wardrobe_proposal = input.get("wardrobeProposal").cloned().unwrap_or(Value::Null);
    let canonical_face_version = load_canonical_asset_version(
        conn,
        project_id,
        character_id,
        "face_lock",
    )?;
    let assets = if let Some(version_id) = &canonical_face_version {
        vec![load_selected_asset_snapshot(conn, project_id, version_id)?]
    } else {
        Vec::new()
    };
    let resolved_context = json!({
        "character": {
            "entityId": character_id,
            "storyName": story_name,
            "roleTag": role_tag,
            "visualSummary": visual_summary,
            "permanentVisualLocks": permanent_visual_locks,
        },
        "wardrobeProposal": wardrobe_proposal,
        "canonicalFaceAssetVersionId": canonical_face_version,
    });

    Ok(WorkflowContextSnapshot {
        snapshot_version: 1,
        project: WorkflowProjectRef {
            project_id: project_id.to_string(),
        },
        skill: WorkflowSkillRef {
            skill_id: _skill_id.to_string(),
            skill_version: _skill_version.to_string(),
            operation_id: _operation_id.to_string(),
        },
        input: input.clone(),
        prerequisite_report,
        canon,
        assets,
        protected_tbds,
        resolved_context,
        captured_at: Utc::now().to_rfc3339(),
    })
}

pub fn resolve_character_sheet_context(
    conn: &Connection,
    project_id: &str,
    _skill_id: &str,
    _skill_version: &str,
    _operation_id: &str,
    input: &Value,
    prerequisite_report: PrerequisiteReport,
) -> Result<WorkflowContextSnapshot, AppError> {
    let character_id = input
        .get("characterEntityId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::WorkflowInputInvalid("characterEntityId must be a non-empty string".into())
        })?;
    let (story_name, entity_type): (String, String) = conn
        .query_row(
            "SELECT name, type FROM canon_entities WHERE project_id = ?1 AND id = ?2",
            params![project_id, character_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::CanonEntityNotFound,
            other => AppError::Database(other.to_string()),
        })?;
    if entity_type != CanonEntityType::Character.as_str() {
        return Err(AppError::WorkflowPrerequisiteFailed(
            "selected entity is not a character".into(),
        ));
    }

    let mut canon = Vec::new();
    let mut role_tag = None;
    let mut visual_summary = None;
    let mut statement = conn
        .prepare("SELECT id, section_key, value_json, revision, status FROM canon_sections WHERE canon_entity_id = ?1 AND status = 'locked' ORDER BY section_key")
        .map_err(db_error)?;
    let rows = statement
        .query_map([character_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(db_error)?;
    for row in rows {
        let (section_id, section_key, value_json, revision, status) = row.map_err(db_error)?;
        let value: Value = serde_json::from_str(&value_json)
            .map_err(|error| AppError::Database(error.to_string()))?;
        canon.push(CanonSnapshotRef {
            entity_id: character_id.to_string(),
            entity_type: CanonEntityType::Character,
            section_id,
            section_key: section_key.clone(),
            revision,
            status: CanonSnapshotStatus::Locked,
            value: value.clone(),
        });
        match section_key.as_str() {
            "role_tag" => {
                role_tag = value
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }
            "visual_summary" => {
                visual_summary = value
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }
            _ => {}
        }
        let _ = status;
    }

    let protected_tbds = load_protected_tbds(conn, project_id)?;
    let canonical_outfit_version = load_canonical_asset_version(
        conn,
        project_id,
        character_id,
        "outfit",
    )?;
    let assets = if let Some(version_id) = &canonical_outfit_version {
        vec![load_selected_asset_snapshot(conn, project_id, version_id)?]
    } else {
        Vec::new()
    };
    let resolved_context = json!({
        "character": {
            "entityId": character_id,
            "storyName": story_name,
            "roleTag": role_tag,
            "visualSummary": visual_summary,
        },
        "canonicalOutfitAssetVersionId": canonical_outfit_version,
    });

    Ok(WorkflowContextSnapshot {
        snapshot_version: 1,
        project: WorkflowProjectRef {
            project_id: project_id.to_string(),
        },
        skill: WorkflowSkillRef {
            skill_id: _skill_id.to_string(),
            skill_version: _skill_version.to_string(),
            operation_id: _operation_id.to_string(),
        },
        input: input.clone(),
        prerequisite_report,
        canon,
        assets,
        protected_tbds,
        resolved_context,
        captured_at: Utc::now().to_rfc3339(),
    })
}

fn load_canonical_asset_version(
    conn: &Connection,
    project_id: &str,
    owner_entity_id: &str,
    asset_type: &str,
) -> Result<Option<String>, AppError> {
    conn.query_row(
        "SELECT a.canonical_version_id FROM assets a JOIN asset_versions v ON v.id = a.canonical_version_id WHERE a.project_id = ?1 AND a.owner_entity_id = ?2 AND a.type = ?3 AND v.status = 'canonical'",
        params![project_id, owner_entity_id, asset_type],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(db_error)
    .map(|value| value.flatten())
}

fn load_protected_tbds(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<CanonTbdSnapshot>, AppError> {
    let mut statement = conn.prepare("SELECT id, canon_entity_id, section_key, topic, note, protected, status, resolution_text, created_at, updated_at, resolved_at FROM canon_tbds WHERE project_id = ?1 AND protected = 1 AND status = 'open' ORDER BY id").map_err(db_error)?;
    let result = statement
        .query_map([project_id], |row| {
            Ok(CanonTbdSnapshot {
                id: row.get(0)?,
                project_id: project_id.to_string(),
                canon_entity_id: row.get(1)?,
                section_key: row.get(2)?,
                topic: row.get(3)?,
                note: row.get(4)?,
                protected: row.get::<_, i64>(5)? != 0,
                status: crate::workflow::model::CanonTbdStatus::Open,
                resolution_text: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                resolved_at: row.get(10)?,
            })
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error);
    result
}

pub fn resolve_world_plate_context(
    conn: &Connection,
    project_id: &str,
    skill_id: &str,
    skill_version: &str,
    operation_id: &str,
    input: &Value,
    prerequisite_report: PrerequisiteReport,
) -> Result<WorkflowContextSnapshot, AppError> {
    let world_id = input
        .get("worldId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::WorkflowInputInvalid("worldId must be a non-empty string".into()))?;
    let world_row: (String, String, String, String) = conn
        .query_row(
            "SELECT id, project_id, canon_location_entity_id, world_plate_asset_id FROM worlds WHERE id = ?1 AND project_id = ?2",
            params![world_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::WorldNotFound,
            other => AppError::Database(other.to_string()),
        })?;
    let location_entity_id = world_row.2.clone();
    let world_plate_asset_id = world_row.3.clone();
    let (location_name, location_type): (String, String) = conn
        .query_row(
            "SELECT name, type FROM canon_entities WHERE project_id = ?1 AND id = ?2",
            params![project_id, location_entity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::CanonEntityNotFound,
            other => AppError::Database(other.to_string()),
        })?;
    if location_type != CanonEntityType::Location.as_str() {
        return Err(AppError::WorldLocationInvalidType);
    }
    // Load all locked sections for location
    let mut canon = Vec::new();
    let mut description_text: Option<String> = None;
    let mut geography_text: Option<String> = None;
    let mut visual_tags: Option<Vec<String>> = None;
    let mut location_rules: Option<Vec<String>> = None;
    let mut location_revision_refs: Vec<Value> = Vec::new();
    {
        let mut statement = conn
            .prepare(
                "SELECT id, section_key, value_json, revision, status FROM canon_sections WHERE canon_entity_id = ?1 AND status = 'locked' ORDER BY section_key",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([&location_entity_id], |row| {
                let value_json: String = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    value_json,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(db_error)?;
        for row in rows {
            let (section_id, section_key, value_json, revision, status) = row.map_err(db_error)?;
            let value: Value = serde_json::from_str(&value_json)
                .map_err(|error| AppError::Database(error.to_string()))?;
            // Only include relevant location keys: description, geography, visual_tags, rules
            if !matches!(section_key.as_str(), "description" | "geography" | "visual_tags" | "rules") {
                continue;
            }
            canon.push(CanonSnapshotRef {
                entity_id: location_entity_id.clone(),
                entity_type: CanonEntityType::Location,
                section_id: section_id.clone(),
                section_key: section_key.clone(),
                revision,
                status: CanonSnapshotStatus::Locked,
                value: value.clone(),
            });
            location_revision_refs.push(json!({
                "sectionId": section_id,
                "sectionKey": section_key,
                "revision": revision
            }));
            match section_key.as_str() {
                "description" => {
                    description_text = value.get("text").and_then(Value::as_str).map(str::to_string);
                }
                "geography" => {
                    geography_text = value.get("text").and_then(Value::as_str).map(str::to_string);
                }
                "visual_tags" => {
                    if let Some(tags) = value.get("tags").and_then(Value::as_array) {
                        visual_tags = Some(
                            tags.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect(),
                        );
                    }
                }
                "rules" => {
                    if let Some(rules) = value.get("rules").and_then(Value::as_array) {
                        location_rules = Some(
                            rules.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect(),
                        );
                    }
                }
                _ => {}
            }
            let _ = status;
        }
    }
    if description_text.is_none() {
        return Err(AppError::WorkflowPrerequisiteFailed(
            "Location description is not locked".into(),
        ));
    }
    if geography_text.is_none() {
        return Err(AppError::WorkflowPrerequisiteFailed(
            "Location geography is not locked".into(),
        ));
    }
    // Load locked Story Aesthetic if available
    let mut aesthetic_value: Option<Value> = None;
    let mut aesthetic_revision: Option<Value> = None;
    if let Ok(story_entity_id) = conn.query_row(
        "SELECT id FROM canon_entities WHERE project_id = ?1 AND type = 'story' LIMIT 1",
        params![project_id],
        |row| row.get::<_, String>(0),
    ) {
        let mut stmt = conn
            .prepare(
                "SELECT id, value_json, revision FROM canon_sections WHERE canon_entity_id = ?1 AND section_key = 'aesthetic' AND status = 'locked' LIMIT 1",
            )
            .map_err(db_error)?;
        if let Ok((section_id, value_json, revision)) = stmt.query_row([&story_entity_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
        }) {
            let value: Value = serde_json::from_str(&value_json)
                .map_err(|error| AppError::Database(error.to_string()))?;
            aesthetic_value = Some(value.clone());
            aesthetic_revision = Some(json!({
                "sectionId": section_id,
                "sectionKey": "aesthetic",
                "revision": revision
            }));
            canon.push(CanonSnapshotRef {
                entity_id: story_entity_id.clone(),
                entity_type: CanonEntityType::Story,
                section_id: section_id.clone(),
                section_key: "aesthetic".to_string(),
                revision,
                status: CanonSnapshotStatus::Locked,
                value: value.clone(),
            });
        }
    }
    // Load locked World Rules
    let mut world_rules: Vec<Value> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.name, s.id, s.value_json, s.revision FROM canon_entities e JOIN canon_sections s ON s.canon_entity_id = e.id WHERE e.project_id = ?1 AND e.type = 'world_rule' AND s.section_key = 'rule' AND s.status = 'locked' ORDER BY e.id",
            )
            .map_err(db_error)?;
        let rows = stmt
            .query_map(params![project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(db_error)?;
        for row in rows {
            let (entity_id, name, section_id, value_json, revision) = row.map_err(db_error)?;
            let value: Value = serde_json::from_str(&value_json)
                .map_err(|error| AppError::Database(error.to_string()))?;
            let rule_text = value.get("text").and_then(Value::as_str).unwrap_or("").to_string();
            canon.push(CanonSnapshotRef {
                entity_id: entity_id.clone(),
                entity_type: CanonEntityType::WorldRule,
                section_id: section_id.clone(),
                section_key: "rule".to_string(),
                revision,
                status: CanonSnapshotStatus::Locked,
                value: value.clone(),
            });
            world_rules.push(json!({
                "entityId": entity_id,
                "name": name,
                "rule": rule_text,
                "revision": revision,
                "sectionId": section_id
            }));
        }
    }
    // Load locked Production Rules
    let mut production_rules: Vec<Value> = Vec::new();
    if let Ok(prod_entity_id) = conn.query_row(
        "SELECT id FROM canon_entities WHERE project_id = ?1 AND type = 'production_rules' LIMIT 1",
        params![project_id],
        |row| row.get::<_, String>(0),
    ) {
        let mut stmt = conn
            .prepare(
                "SELECT id, value_json, revision FROM canon_sections WHERE canon_entity_id = ?1 AND section_key = 'rules' AND status = 'locked' LIMIT 1",
            )
            .map_err(db_error)?;
        if let Ok((section_id, value_json, revision)) = stmt.query_row([&prod_entity_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
        }) {
            let value: Value = serde_json::from_str(&value_json)
                .map_err(|error| AppError::Database(error.to_string()))?;
            if let Some(rules) = value.get("rules").and_then(Value::as_array) {
                for rule in rules {
                    production_rules.push(rule.clone());
                }
            }
            canon.push(CanonSnapshotRef {
                entity_id: prod_entity_id.clone(),
                entity_type: CanonEntityType::ProductionRules,
                section_id: section_id.clone(),
                section_key: "rules".to_string(),
                revision,
                status: CanonSnapshotStatus::Locked,
                value: value.clone(),
            });
        }
    }
    // Parse TBD decisions from input
    let tbd_decisions: Vec<crate::workflow::tbd_policy::TbdDecision> = input
        .get("tbdDecisions")
        .or_else(|| input.get("tbd_decisions"))
        .map(|value| serde_json::from_value(value.clone()).unwrap_or_default())
        .unwrap_or_default();
    // Validate TBD firewall (ensures decisions cover applicable TBDs)
    crate::workflow::tbd_policy::validate_world_tbd_firewall(
        conn,
        project_id,
        &location_entity_id,
        &tbd_decisions,
    )?;
    // Load protected TBD snapshots for inclusion (applicable)
    let applicable_tbds = crate::workflow::tbd_policy::load_applicable_tbds(
        conn,
        project_id,
        &[location_entity_id.clone()],
    )?;
    let protected_tbds: Vec<CanonTbdSnapshot> = applicable_tbds
        .iter()
        .filter(|tbd| tbd.protected && tbd.status == "open")
        .map(|tbd| CanonTbdSnapshot {
            id: tbd.id.clone(),
            project_id: tbd.project_id.clone(),
            canon_entity_id: tbd.canon_entity_id.clone(),
            section_key: tbd.section_key.clone(),
            topic: tbd.topic.clone(),
            note: tbd.note.clone(),
            protected: tbd.protected,
            status: crate::workflow::model::CanonTbdStatus::Open,
            resolution_text: tbd.resolution_text.clone(),
            created_at: tbd.created_at.clone(),
            updated_at: tbd.updated_at.clone(),
            resolved_at: tbd.resolved_at.clone(),
        })
        .collect();
    let resolved_context = json!({
        "world": {
            "id": world_id,
            "plateAssetId": world_plate_asset_id,
            "locationEntityId": location_entity_id,
            "locationName": location_name
        },
        "worldId": world_id,
        "location": {
            "entityId": location_entity_id,
            "name": location_name,
            "description": description_text.clone().unwrap_or_default(),
            "geography": geography_text.clone().unwrap_or_default(),
            "visualTags": visual_tags,
            "rules": location_rules,
            "canonRevisionRefs": location_revision_refs
        },
        "aesthetic": aesthetic_value.map(|value| json!({
            "value": value,
            "revisionRef": aesthetic_revision
        })),
        "worldRules": world_rules,
        "productionRules": production_rules,
        "tbdDecisions": tbd_decisions
    });
    Ok(WorkflowContextSnapshot {
        snapshot_version: 1,
        project: WorkflowProjectRef {
            project_id: project_id.to_string(),
        },
        skill: WorkflowSkillRef {
            skill_id: skill_id.to_string(),
            skill_version: skill_version.to_string(),
            operation_id: operation_id.to_string(),
        },
        input: input.clone(),
        prerequisite_report,
        canon,
        assets: Vec::new(),
        protected_tbds,
        resolved_context,
        captured_at: Utc::now().to_rfc3339(),
    })
}

pub fn resolve_scene_keyframe_context(
    conn: &Connection,
    project_id: &str,
    skill_id: &str,
    skill_version: &str,
    operation_id: &str,
    input: &Value,
    prerequisite_report: PrerequisiteReport,
) -> Result<WorkflowContextSnapshot, AppError> {
    let scene_id = input
        .get("sceneId")
        .or_else(|| input.get("scene_id"))
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::WorkflowInputInvalid("sceneId must be a non-empty string".into()))?;

    // Load scene
    let scene_row: (String, String, i64, String, String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT id, project_id, ordinal, title, summary, world_id, world_asset_version_id FROM world_scenes WHERE id = ?1 AND project_id = ?2",
            params![scene_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::SceneNotFound,
            other => AppError::Database(other.to_string()),
        })?;
    let (scene_db_id, scene_project_id, scene_ordinal, scene_title, scene_summary, scene_world_id, scene_world_version_id) = scene_row;
    if scene_project_id != project_id {
        return Err(AppError::SceneNotFound);
    }

    // Readiness validation (derived, not stored)
    if scene_title.trim().is_empty() {
        return Err(AppError::SceneNotReady("title is empty".into()));
    }
    if scene_summary.trim().is_empty() {
        return Err(AppError::SceneNotReady("summary is empty".into()));
    }
    let world_id = scene_world_id.clone().ok_or_else(|| AppError::SceneNotReady("world reference missing".into()))?;
    let world_version_id = scene_world_version_id.clone().ok_or_else(|| AppError::SceneNotReady("world reference missing".into()))?;

    // Validate world reference is not broken (allow historical/superseded)
    let world_version: (String, String, i64, String, String) = conn
        .query_row(
            "SELECT av.id, av.asset_id, av.version_number, av.status, av.file_path FROM asset_versions av WHERE av.id = ?1",
            params![world_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|_| AppError::SceneReferenceBroken(format!("world version {} not found", world_version_id)))?;
    let world_asset: (String, String, String, Option<String>) = conn
        .query_row(
            "SELECT id, project_id, type, owner_entity_id FROM assets WHERE id = ?1",
            params![world_version.1],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| AppError::SceneReferenceBroken(format!("world asset {} not found", world_version.1)))?;
    if world_asset.1 != project_id {
        return Err(AppError::SceneReferenceBroken("world asset project mismatch".into()));
    }
    if world_asset.2 != "world_plate" {
        return Err(AppError::SceneReferenceBroken("world asset type mismatch".into()));
    }
    // Verify world exists and owns asset
    let world_location_check: String = conn
        .query_row(
            "SELECT canon_location_entity_id FROM worlds WHERE id = ?1 AND project_id = ?2",
            params![world_id, project_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::SceneReferenceBroken("world not found".into()))?;
    if world_asset.3.as_deref() != Some(world_id.as_str()) {
        return Err(AppError::SceneReferenceBroken("world asset owner mismatch".into()));
    }
    let _ = world_location_check;
    // Check that world asset still exists, but canonical may have changed; that's okay for historical
    // No failure for historical: we already have exact version, so it's valid even if superseded

    // Validate character references not broken
    let mut char_stmt = conn
        .prepare("SELECT id, character_entity_id, look_asset_version_id, sheet_asset_version_id FROM world_scene_characters WHERE scene_id = ?1")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let char_rows = char_stmt
        .query_map(params![scene_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut characters: Vec<Value> = Vec::new();
    let mut entity_ids_for_tbd: Vec<String> = Vec::new();
    // world location entity id for TBD
    let world_loc_entity: String = conn
        .query_row(
            "SELECT canon_location_entity_id FROM worlds WHERE id = ?1",
            params![world_id],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    entity_ids_for_tbd.push(world_loc_entity.clone());

    let mut asset_snapshots: Vec<crate::workflow::model::AssetSnapshotRef> = Vec::new();

    // Helper to create asset snapshot
    let load_asset_snapshot = |version_id: &str| -> Result<crate::workflow::model::AssetSnapshotRef, AppError> {
        let (asset_id, asset_type_str, version_number, _status, file_path): (String, String, i64, String, String) = conn
            .query_row(
                "SELECT av.asset_id, a.type, av.version_number, av.status, av.file_path FROM asset_versions av JOIN assets a ON a.id = av.asset_id WHERE av.id = ?1",
                params![version_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .map_err(|_| AppError::SceneReferenceBroken(format!("asset version {} not found", version_id)))?;
        let asset_type = match asset_type_str.as_str() {
            "face_lock" => AssetType::FaceLock,
            "outfit" => AssetType::Outfit,
            "character_sheet" => AssetType::CharacterSheet,
            "world_plate" => AssetType::WorldPlate,
            "shot_keyframe" => AssetType::ShotKeyframe,
            "prop_plate" => AssetType::PropPlate,
            "image" => AssetType::Image,
            "video" => AssetType::Video,
            "audio" => AssetType::Audio,
            _ => return Err(AppError::SceneReferenceBroken(format!("unknown asset type {}", asset_type_str))),
        };
        // For historical, status may be superseded but we store as Canonical for snapshot compatibility
        Ok(crate::workflow::model::AssetSnapshotRef {
            asset_id,
            asset_version_id: version_id.to_string(),
            asset_type,
            version_number,
            status: crate::workflow::model::AssetSnapshotStatus::Canonical,
            path: file_path,
        })
    };

    // World asset snapshot (exact pinned)
    let world_asset_snapshot = load_asset_snapshot(&world_version_id)?;
    let world_asset_id = world_asset_snapshot.asset_id.clone();
    asset_snapshots.push(world_asset_snapshot);

    let mut canon_revision_refs: Vec<Value> = Vec::new();

    for row in char_rows {
        let (assignment_id, char_entity_id, look_version_id, sheet_version_id) = row.map_err(|e| AppError::Database(e.to_string()))?;
        entity_ids_for_tbd.push(char_entity_id.clone());

        // Validate look version
        let look_version: (String, String, String) = conn
            .query_row(
                "SELECT av.asset_id, a.type, a.owner_entity_id FROM asset_versions av JOIN assets a ON a.id = av.asset_id WHERE av.id = ?1",
                params![look_version_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, Option<String>>(2)?.unwrap_or_default())),
            )
            .map_err(|_| AppError::SceneReferenceBroken(format!("look version {} not found", look_version_id)))?;
        if look_version.1 == "world_plate" || look_version.1 == "prop_plate" || look_version.1 == "shot_keyframe" {
            return Err(AppError::SceneReferenceBroken("look asset type invalid".into()));
        }
        if look_version.2 != char_entity_id {
            return Err(AppError::SceneReferenceBroken("look owner mismatch".into()));
        }
        // Validate character entity exists
        let char_type: String = conn
            .query_row(
                "SELECT type FROM canon_entities WHERE id = ?1 AND project_id = ?2",
                params![char_entity_id, project_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::SceneReferenceBroken(format!("character {} not found", char_entity_id)))?;
        if char_type != "character" {
            return Err(AppError::SceneReferenceBroken("character type mismatch".into()));
        }

        let look_asset_id = look_version.0.clone();
        let look_snapshot = load_asset_snapshot(&look_version_id)?;
        asset_snapshots.push(look_snapshot);

        let mut char_entry = json!({
            "characterEntityId": char_entity_id,
            "look": {
                "assetId": look_asset_id,
                "assetVersionId": look_version_id
            }
        });

        if let Some(sheet_id) = sheet_version_id {
            let sheet_version: (String, String, String) = conn
                .query_row(
                    "SELECT av.asset_id, a.type, a.owner_entity_id FROM asset_versions av JOIN assets a ON a.id = av.asset_id WHERE av.id = ?1",
                    params![sheet_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, Option<String>>(2)?.unwrap_or_default())),
                )
                .map_err(|_| AppError::SceneReferenceBroken(format!("sheet version {} not found", sheet_id)))?;
            if sheet_version.1 != "character_sheet" && sheet_version.1 != "outfit" {
                return Err(AppError::SceneReferenceBroken("sheet asset type invalid".into()));
            }
            if sheet_version.2 != char_entity_id {
                return Err(AppError::SceneReferenceBroken("sheet owner mismatch".into()));
            }
            let sheet_asset_id = sheet_version.0.clone();
            let sheet_snapshot = load_asset_snapshot(&sheet_id)?;
            asset_snapshots.push(sheet_snapshot);
            char_entry["sheet"] = json!({
                "assetId": sheet_asset_id,
                "assetVersionId": sheet_id
            });
        }
        char_entry["assignmentId"] = Value::String(assignment_id);
        characters.push(char_entry);
    }

    // Props
    let mut prop_stmt = conn
        .prepare("SELECT id, prop_asset_version_id FROM world_scene_props WHERE scene_id = ?1")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let prop_rows = prop_stmt
        .query_map(params![scene_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut props: Vec<Value> = Vec::new();
    for row in prop_rows {
        let (assignment_id, prop_version_id) = row.map_err(|e| AppError::Database(e.to_string()))?;
        let prop_asset: (String, String) = conn
            .query_row(
                "SELECT av.asset_id, a.type FROM asset_versions av JOIN assets a ON a.id = av.asset_id WHERE av.id = ?1",
                params![prop_version_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| AppError::SceneReferenceBroken(format!("prop version {} not found", prop_version_id)))?;
        if prop_asset.1 != "prop_plate" {
            return Err(AppError::SceneReferenceBroken("prop asset type invalid".into()));
        }
        let prop_asset_id = prop_asset.0.clone();
        let prop_snapshot = load_asset_snapshot(&prop_version_id)?;
        asset_snapshots.push(prop_snapshot);
        props.push(json!({
            "assignmentId": assignment_id,
            "assetId": prop_asset_id,
            "assetVersionId": prop_version_id
        }));
    }

    // TBD handling: load applicable protected open TBDs and require bindings
    let applicable = crate::workflow::tbd_policy::load_applicable_tbds(conn, project_id, &entity_ids_for_tbd)?;
    // Load scene bindings
    let mut tbd_decisions: Vec<Value> = Vec::new();
    let mut protected_tbds_for_snapshot: Vec<CanonTbdSnapshot> = Vec::new();
    for tbd in &applicable {
        if !tbd.protected || tbd.status != "open" {
            continue;
        }
        // Find binding
        let binding: Option<(String, Option<String>, String, Option<String>)> = conn
            .query_row(
                "SELECT topic_snapshot, note_snapshot, decision, justification FROM scene_tbd_bindings WHERE scene_id = ?1 AND canon_tbd_id = ?2",
                params![scene_id, tbd.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let (topic_snapshot, note_snapshot, decision_str, justification) = binding.ok_or_else(|| {
            AppError::TbdDecisionRequired(format!(
                "TBD {} requires explicit handling decision for scene {}",
                tbd.id, scene_id
            ))
        })?;
        // Validate decision via policy
        let decision_kind = match decision_str.as_str() {
            "preserve_unknown" => crate::workflow::tbd_policy::TbdDecisionKind::PreserveUnknown,
            "not_applicable" => crate::workflow::tbd_policy::TbdDecisionKind::NotApplicable,
            _ => return Err(AppError::TbdDecisionRequired(format!("TBD {} has invalid decision", tbd.id))),
        };
        let tbd_record = crate::canon::model::CanonTbdRecord {
            id: tbd.id.clone(),
            project_id: tbd.project_id.clone(),
            canon_entity_id: tbd.canon_entity_id.clone(),
            section_key: tbd.section_key.clone(),
            topic: tbd.topic.clone(),
            note: tbd.note.clone(),
            protected: tbd.protected,
            status: tbd.status.clone(),
            resolution_text: tbd.resolution_text.clone(),
            created_at: tbd.created_at.clone(),
            updated_at: tbd.updated_at.clone(),
            resolved_at: tbd.resolved_at.clone(),
        };
        let temp_decision = crate::workflow::tbd_policy::TbdDecision {
            tbd_id: tbd.id.clone(),
            topic_snapshot: topic_snapshot.clone(),
            note_snapshot: note_snapshot.clone(),
            decision: decision_kind,
            justification: justification.clone(),
        };
        crate::workflow::tbd_policy::validate_tbd_decisions(&[tbd_record], &[temp_decision])
            .map_err(|e| AppError::TbdDecisionRequired(e.to_string()))?;

        tbd_decisions.push(json!({
            "tbdId": tbd.id,
            "topicSnapshot": topic_snapshot,
            "noteSnapshot": note_snapshot,
            "decision": decision_str,
            "justification": justification
        }));
        protected_tbds_for_snapshot.push(CanonTbdSnapshot {
            id: tbd.id.clone(),
            project_id: tbd.project_id.clone(),
            canon_entity_id: tbd.canon_entity_id.clone(),
            section_key: tbd.section_key.clone(),
            topic: tbd.topic.clone(),
            note: tbd.note.clone(),
            protected: tbd.protected,
            status: crate::workflow::model::CanonTbdStatus::Open,
            resolution_text: tbd.resolution_text.clone(),
            created_at: tbd.created_at.clone(),
            updated_at: tbd.updated_at.clone(),
            resolved_at: tbd.resolved_at.clone(),
        });
        canon_revision_refs.push(json!({
            "tbdId": tbd.id,
            "topicSnapshot": topic_snapshot,
            "decision": decision_str
        }));
    }

    // Load locked Production Rules
    let mut production_rules: Vec<Value> = Vec::new();
    let mut canon_snapshots: Vec<CanonSnapshotRef> = Vec::new();
    if let Ok(prod_entity_id) = conn.query_row(
        "SELECT id FROM canon_entities WHERE project_id = ?1 AND type = 'production_rules' LIMIT 1",
        params![project_id],
        |row| row.get::<_, String>(0),
    ) {
        let mut stmt = conn
            .prepare(
                "SELECT id, value_json, revision FROM canon_sections WHERE canon_entity_id = ?1 AND section_key = 'rules' AND status = 'locked' LIMIT 1",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        if let Ok((section_id, value_json, revision)) = stmt.query_row(params![prod_entity_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
        }) {
            let value: Value = serde_json::from_str(&value_json).map_err(|e| AppError::Database(e.to_string()))?;
            if let Some(rules) = value.get("rules").and_then(Value::as_array) {
                for rule in rules {
                    production_rules.push(rule.clone());
                }
            }
            canon_snapshots.push(CanonSnapshotRef {
                entity_id: prod_entity_id.clone(),
                entity_type: CanonEntityType::ProductionRules,
                section_id: section_id.clone(),
                section_key: "rules".to_string(),
                revision,
                status: CanonSnapshotStatus::Locked,
                value: value.clone(),
            });
            canon_revision_refs.push(json!({
                "sectionId": section_id,
                "sectionKey": "rules",
                "revision": revision
            }));
        }
    }

    // Also collect world location revision refs (description, geography)
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, section_key, value_json, revision FROM canon_sections WHERE canon_entity_id = ?1 AND status = 'locked' AND section_key IN ('description','geography','visual_tags','rules') ORDER BY section_key",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![world_loc_entity], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        for row in rows {
            let (section_id, section_key, value_json, revision) = row.map_err(|e| AppError::Database(e.to_string()))?;
            let value: Value = serde_json::from_str(&value_json).map_err(|e| AppError::Database(e.to_string()))?;
            canon_snapshots.push(CanonSnapshotRef {
                entity_id: world_loc_entity.clone(),
                entity_type: CanonEntityType::Location,
                section_id: section_id.clone(),
                section_key: section_key.clone(),
                revision,
                status: CanonSnapshotStatus::Locked,
                value,
            });
            canon_revision_refs.push(json!({
                "sectionId": section_id,
                "sectionKey": section_key,
                "revision": revision
            }));
        }
    }

    let resolved_context = json!({
        "scene": {
            "id": scene_db_id,
            "ordinal": scene_ordinal,
            "title": scene_title,
            "summary": scene_summary
        },
        "world": {
            "worldId": world_id,
            "assetId": world_asset_id,
            "assetVersionId": world_version_id
        },
        "characters": characters,
        "props": props,
        "tbdDecisions": tbd_decisions,
        "productionRules": production_rules,
        "canonRevisionRefs": canon_revision_refs
    });

    // Ensure protected_tbds contains the relevant ones
    // Use the collected protected_tbds_for_snapshot
    // If none, load via tbd_policy for snapshot? Already have

    Ok(WorkflowContextSnapshot {
        snapshot_version: 1,
        project: WorkflowProjectRef {
            project_id: project_id.to_string(),
        },
        skill: WorkflowSkillRef {
            skill_id: skill_id.to_string(),
            skill_version: skill_version.to_string(),
            operation_id: operation_id.to_string(),
        },
        input: input.clone(),
        prerequisite_report: prerequisite_report,
        canon: canon_snapshots,
        assets: asset_snapshots,
        protected_tbds: protected_tbds_for_snapshot,
        resolved_context,
        captured_at: Utc::now().to_rfc3339(),
    })
}

pub fn write_snapshot_atomically(
    path: &Path,
    snapshot: &WorkflowContextSnapshot,
) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| {
        AppError::WorkflowArtifactWriteFailed("snapshot has no parent directory".into())
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(snapshot)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;
    fs::write(&temp, bytes)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;
    fs::rename(&temp, path)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use crate::workflow::model::PrerequisiteReport;

    fn fixture() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute("INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('p', 'Red Door', 'now', 'now', 1)", []).unwrap();
        conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('mara', 'p', 'character', 'Mara', 'mara', 'now', 'now')", []).unwrap();
        for (id, key, value, status, revision) in [
            (
                "s1",
                "role_tag",
                serde_json::json!({"text":"Protagonist"}),
                "locked",
                2,
            ),
            (
                "s2",
                "visual_summary",
                serde_json::json!({"text":"Angular face, dark hair."}),
                "locked",
                3,
            ),
            (
                "s3",
                "visual_locks",
                serde_json::json!({"locks":[{"id":"scar","key":"right_eyebrow_scar","description":"Small healed scar.","severity":"required","validatorHint":null}]}),
                "locked",
                4,
            ),
            (
                "s4",
                "psychology",
                serde_json::json!({"text":"Draft psychology"}),
                "draft",
                5,
            ),
        ] {
            conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at) VALUES (?1, 'mara', ?2, ?3, ?4, ?5, 'now', 'now')", params![id, key, value.to_string(), status, revision]).unwrap();
        }
        conn
    }

    fn input() -> Value {
        json!({
            "projectRootPath":"C:/projects/red-door",
            "characterEntityId":"mara",
            "visualSpec":{"head":"oval","eyes":"brown","brows":"straight","nose":"narrow","lips":"neutral","skin":"olive","hair":"black","build":"athletic","expression":"neutral"},
            "baselineWardrobe":"charcoal crew neck"
        })
    }

    #[test]
    fn snapshots_only_locked_sections_and_preserves_visual_locks() {
        let conn = fixture();
        let snapshot = resolve_character_face_lock_context(
            &conn,
            "p",
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            &input(),
            PrerequisiteReport {
                passed: true,
                checks: vec![],
            },
        )
        .unwrap();

        assert_eq!(snapshot.canon.len(), 3);
        assert!(snapshot
            .canon
            .iter()
            .all(|section| section.section_key != "psychology"));
        assert_eq!(
            snapshot
                .canon
                .iter()
                .find(|section| section.section_key == "visual_locks")
                .unwrap()
                .revision,
            4
        );
        assert_eq!(
            snapshot.resolved_context["character"]["roleTag"],
            "Protagonist"
        );
        assert_eq!(
            snapshot.resolved_context["character"]["visualSummary"],
            "Angular face, dark hair."
        );
        assert_eq!(
            snapshot.resolved_context["character"]["permanentVisualLocks"][0]["key"],
            "right_eyebrow_scar"
        );
    }

    #[test]
    fn snapshot_value_does_not_change_after_current_canon_mutates() {
        let conn = fixture();
        let snapshot = resolve_character_face_lock_context(
            &conn,
            "p",
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            &input(),
            PrerequisiteReport {
                passed: true,
                checks: vec![],
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE canon_sections SET value_json = '{\"locks\":[]}', revision = 5 WHERE id = 's3'",
            [],
        )
        .unwrap();

        assert_eq!(
            snapshot
                .canon
                .iter()
                .find(|section| section.section_key == "visual_locks")
                .unwrap()
                .revision,
            4
        );
        assert_eq!(
            snapshot.resolved_context["character"]["permanentVisualLocks"][0]["key"],
            "right_eyebrow_scar"
        );
    }

    #[test]
    fn resolves_the_selected_asset_version_into_the_immutable_context_snapshot() {
        let conn = fixture();
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, created_at, updated_at)
             VALUES ('face-asset', 'p', 'face_lock', 'MARA-FACE', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_versions
             (id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
              original_filename, mime_type, byte_size, created_at)
             VALUES ('face-v002', 'face-asset', 2, 'canonical', 'assets/face.png',
                     'thumbnails/face.webp', 'd', 'face.png', 'image/png', 1, 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE assets SET canonical_version_id = 'face-v002' WHERE id = 'face-asset'",
            [],
        )
        .unwrap();
        let mut selected = input();
        selected["sourceAssetVersionId"] = serde_json::json!("face-v002");

        let snapshot = resolve_character_face_lock_context(
            &conn,
            "p",
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            &selected,
            PrerequisiteReport { passed: true, checks: vec![] },
        )
        .unwrap();

        assert_eq!(snapshot.assets.len(), 1);
        assert_eq!(snapshot.assets[0].asset_version_id, "face-v002");
        assert_eq!(snapshot.assets[0].version_number, 2);
    }

    #[test]
    fn rejects_a_canonical_source_that_is_no_longer_the_asset_canonical_pointer() {
        let conn = fixture();
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, created_at, updated_at)
              VALUES ('face-asset', 'p', 'face_lock', 'MARA-FACE', 'now', 'now')",
            [],
        ).unwrap();
        for (id, number) in [("face-v002", 2), ("face-v003", 3)] {
            let hash = format!("hash-{number}");
            conn.execute(
                "INSERT INTO asset_versions
                  (id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
                   original_filename, mime_type, byte_size, created_at)
                  VALUES (?1, 'face-asset', ?2, 'canonical', 'assets/face.png',
                          'thumbnails/face.webp', ?3, 'face.png', 'image/png', 1, 'now')",
                params![id, number, hash],
            ).unwrap();
        }
        conn.execute(
            "UPDATE assets SET canonical_version_id = 'face-v003' WHERE id = 'face-asset'",
            [],
        ).unwrap();
        let mut selected = input();
        selected["sourceAssetVersionId"] = serde_json::json!("face-v002");

        let error = resolve_character_face_lock_context(
            &conn,
            "p",
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            &selected,
            PrerequisiteReport { passed: true, checks: vec![] },
        ).unwrap_err();

        assert!(matches!(error, AppError::WorkflowPrerequisiteFailed(_)));
    }

    fn world_fixture() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute("INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('p', 'Red Door', 'now', 'now', 1)", []).unwrap();
        conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('loc-1', 'p', 'location', 'Station', 'station', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at) VALUES ('s-desc', 'loc-1', 'description', '{\"text\":\"A derelict station\"}', 'locked', 1, 'now', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at) VALUES ('s-geo', 'loc-1', 'geography', '{\"text\":\"Rust belt\"}', 'locked', 2, 'now', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO assets (id, project_id, type, label, owner_entity_id, created_at, updated_at) VALUES ('asset-loc', 'p', 'world_plate', 'STATION-WORLD', 'world-1', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO worlds (id, project_id, canon_location_entity_id, world_plate_asset_id, created_at, updated_at) VALUES ('world-1', 'p', 'loc-1', 'asset-loc', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('story-1', 'p', 'story', 'Story', 'story', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('prod-1', 'p', 'production_rules', 'Production Rules', 'production-rules', 'now', 'now')", []).unwrap();
        conn
    }

    fn world_input() -> Value {
        json!({
            "worldId": "world-1",
            "tbdDecisions": []
        })
    }

    #[test]
    fn world_plate_context_requires_locked_description_and_geography() {
        let conn = world_fixture();
        // Remove description -> should fail
        conn.execute("UPDATE canon_sections SET status = 'draft' WHERE id = 's-desc'", []).unwrap();
        let err = resolve_world_plate_context(
            &conn,
            "p",
            "world-builder",
            "1.0.0",
            "world.create_plate",
            &world_input(),
            PrerequisiteReport { passed: true, checks: vec![] },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::WorkflowPrerequisiteFailed(_)));
        // Restore description, remove geography
        conn.execute("UPDATE canon_sections SET status = 'locked' WHERE id = 's-desc'", []).unwrap();
        conn.execute("UPDATE canon_sections SET status = 'draft' WHERE id = 's-geo'", []).unwrap();
        let err2 = resolve_world_plate_context(
            &conn,
            "p",
            "world-builder",
            "1.0.0",
            "world.create_plate",
            &world_input(),
            PrerequisiteReport { passed: true, checks: vec![] },
        )
        .unwrap_err();
        assert!(matches!(err2, AppError::WorkflowPrerequisiteFailed(_)));
        // Restore both -> should pass
        conn.execute("UPDATE canon_sections SET status = 'locked' WHERE id = 's-geo'", []).unwrap();
        let ok = resolve_world_plate_context(
            &conn,
            "p",
            "world-builder",
            "1.0.0",
            "world.create_plate",
            &world_input(),
            PrerequisiteReport { passed: true, checks: vec![] },
        )
        .unwrap();
        assert_eq!(ok.resolved_context["worldId"], "world-1");
    }

    #[test]
    fn world_plate_context_includes_world_id_and_locked_fields_and_excludes_draft() {
        let conn = world_fixture();
        conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at) VALUES ('s-tags', 'loc-1', 'visual_tags', '{\"tags\":[\"neon\",\"rain\"]}', 'locked', 4, 'now', 'now', 'now')", []).unwrap();
        // Insert a draft section for a different key to ensure draft is excluded from canon
        conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('char-1', 'p', 'character', 'Mara', 'mara', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at) VALUES ('s-char-draft', 'char-1', 'psychology', '{\"text\":\"draft\"}', 'draft', 1, 'now', 'now')", []).unwrap();
        let snapshot = resolve_world_plate_context(
            &conn,
            "p",
            "world-builder",
            "1.0.0",
            "world.create_plate",
            &world_input(),
            PrerequisiteReport { passed: true, checks: vec![] },
        )
        .unwrap();
        assert_eq!(snapshot.resolved_context["world"]["id"], "world-1");
        assert_eq!(snapshot.resolved_context["worldId"], "world-1");
        assert_eq!(snapshot.resolved_context["location"]["description"], "A derelict station");
        assert_eq!(snapshot.resolved_context["location"]["geography"], "Rust belt");
        assert_eq!(snapshot.resolved_context["location"]["visualTags"][0], "neon");
        // Draft excluded
        assert!(snapshot.canon.iter().all(|c| c.section_key != "psychology"));
        assert!(snapshot.canon.iter().any(|c| c.section_key == "description" && c.revision == 1));
        assert!(snapshot.canon.iter().any(|c| c.section_key == "geography" && c.revision == 2));
        // Ensure no character canon
        assert!(snapshot.canon.iter().all(|c| c.entity_type != crate::canon::model::CanonEntityType::Character));
        assert!(snapshot.assets.is_empty());
    }

    #[test]
    fn world_plate_context_includes_world_rules_and_production_rules_and_tbd_decisions() {
        let conn = world_fixture();
        conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('wr-1', 'p', 'world_rule', 'Gravity', 'gravity', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at) VALUES ('wr-sec', 'wr-1', 'rule', '{\"text\":\"Low gravity\"}', 'locked', 1, 'now', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at) VALUES ('prod-sec', 'prod-1', 'rules', '{\"rules\":[{\"id\":\"r1\",\"title\":\"Rule\",\"body\":\"Do not reveal\"}]}', 'locked', 1, 'now', 'now', 'now')", []).unwrap();
        conn.execute("INSERT INTO canon_tbds (id, project_id, canon_entity_id, section_key, topic, note, protected, status, created_at, updated_at) VALUES ('tbd-1', 'p', 'loc-1', 'description', 'Secret', 'Do not reveal', 1, 'open', 'now', 'now')", []).unwrap();
        let mut input = world_input();
        input["tbdDecisions"] = json!([{
            "tbdId": "tbd-1",
            "topicSnapshot": "Secret",
            "noteSnapshot": "Do not reveal",
            "decision": "preserve_unknown",
            "justification": null
        }]);
        let snapshot = resolve_world_plate_context(
            &conn,
            "p",
            "world-builder",
            "1.0.0",
            "world.create_plate",
            &input,
            PrerequisiteReport { passed: true, checks: vec![] },
        )
        .unwrap();
        assert_eq!(snapshot.resolved_context["worldRules"][0]["rule"], "Low gravity");
        assert_eq!(snapshot.resolved_context["productionRules"][0]["id"], "r1");
        assert_eq!(snapshot.resolved_context["tbdDecisions"][0]["tbdId"], "tbd-1");
        assert_eq!(snapshot.protected_tbds.len(), 1);
        assert_eq!(snapshot.protected_tbds[0].topic, "Secret");
        assert!(snapshot.canon.iter().any(|c| c.entity_type == crate::canon::model::CanonEntityType::WorldRule));
    }

    #[test]
    fn world_plate_context_blocks_without_tbd_decision() {
        let conn = world_fixture();
        conn.execute("INSERT INTO canon_tbds (id, project_id, canon_entity_id, topic, protected, status, created_at, updated_at) VALUES ('tbd-1', 'p', 'loc-1', 'Secret', 1, 'open', 'now', 'now')", []).unwrap();
        let input = world_input(); // empty decisions
        let err = resolve_world_plate_context(
            &conn,
            "p",
            "world-builder",
            "1.0.0",
            "world.create_plate",
            &input,
            PrerequisiteReport { passed: true, checks: vec![] },
        )
        .unwrap_err();
        assert_eq!(err.code(), "TBD_DECISION_REQUIRED");
    }
}

fn load_selected_asset_snapshot(
    conn: &Connection,
    project_id: &str,
    asset_version_id: &str,
) -> Result<crate::workflow::model::AssetSnapshotRef, AppError> {
    let (asset_id, asset_type, version_number, status, canonical_version_id, path): (String, String, i64, String, Option<String>, String) = conn
        .query_row(
            "SELECT av.asset_id, a.type, av.version_number, av.status, a.canonical_version_id, av.file_path
             FROM asset_versions av JOIN assets a ON a.id = av.asset_id
             WHERE av.id = ?1 AND a.project_id = ?2",
            params![asset_version_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::AssetVersionNotFound,
            other => AppError::Database(other.to_string()),
        })?;
    if status != "canonical" {
        return Err(AppError::WorkflowPrerequisiteFailed(
            "source asset version must be canonical".into(),
        ));
    }
    if canonical_version_id.as_deref() != Some(asset_version_id) {
        return Err(AppError::WorkflowPrerequisiteFailed(
            "source asset version is not the current canonical pointer".into(),
        ));
    }
    let asset_type = match asset_type.as_str() {
        "face_lock" => AssetType::FaceLock,
        "outfit" => AssetType::Outfit,
        "character_sheet" => AssetType::CharacterSheet,
        "world_plate" => AssetType::WorldPlate,
        "shot_keyframe" => AssetType::ShotKeyframe,
        "prop_plate" => AssetType::PropPlate,
        "image" => AssetType::Image,
        "video" => AssetType::Video,
        "audio" => AssetType::Audio,
        _ => return Err(AppError::InvalidAssetType),
    };
    Ok(crate::workflow::model::AssetSnapshotRef {
        asset_id,
        asset_version_id: asset_version_id.into(),
        asset_type,
        version_number,
        status: crate::workflow::model::AssetSnapshotStatus::Canonical,
        path,
    })
}
