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
