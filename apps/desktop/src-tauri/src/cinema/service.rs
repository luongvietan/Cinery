use crate::canon::service::CanonService;
use crate::canon::service::VisualLockDto;
use crate::cinema::compiler;
use crate::cinema::export;
use crate::cinema::model;
use crate::cinema::model::{
    validate_shot_duration, BehavioralLocks, CinemaCompilation, CinemaCompileInput, SceneRef,
    ShotRecord, ShotUpdate, WorldContinuity,
};
use crate::cinema::repository;
use crate::cinema::tbd_guard;
use crate::db;
use crate::error::AppError;
use crate::project::service::ProjectService;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;
use std::path::Path;
use ulid::Ulid;

/// A version that has been verified as the current canonical version of a
/// known asset type within the owning project.
#[derive(Debug, Clone)]
pub struct CanonicalVersion {
    pub asset_id: String,
    pub version_id: String,
    pub asset_type: String,
    pub label: String,
}

pub struct CinemaService;

impl CinemaService {
    /// Creates a shot on the authoritative Scene (`world_scenes`); when
    /// `ordering` is omitted the shot is appended after the existing shots.
    pub fn create_shot(
        project_root: &Path,
        scene_id: &str,
        ordering: Option<i64>,
        duration_seconds: f64,
        intent: &str,
        action: Option<String>,
        camera: Option<String>,
    ) -> Result<ShotRecord, AppError> {
        validate_shot_duration(duration_seconds)?;
        let intent = validate_intent(intent)?;
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        repository::ensure_scene_in_project(&conn, &project.id, scene_id)?;

        let ordering = match ordering {
            Some(ordering) if ordering >= 0 => ordering,
            Some(ordering) => {
                return Err(AppError::InvalidCinemaDuration(format!(
                    "shot ordering must be non-negative, got {ordering}"
                )))
            }
            None => conn
                .query_row(
                    "SELECT COALESCE(MAX(ordering) + 1, 0) FROM scene_shots WHERE scene_id = ?1",
                    params![scene_id],
                    |row| row.get(0),
                )
                .map_err(|e| AppError::Database(e.to_string()))?,
        };

        let now = Utc::now().to_rfc3339();
        let record = ShotRecord {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            ordering,
            duration_seconds,
            keyframe_asset_version_id: None,
            generated_video_asset_version_id: None,
            intent,
            action: action
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            camera: camera
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            created_at: now.clone(),
            updated_at: now,
        };
        repository::create_shot(&conn, &record)?;
        Ok(record)
    }

    /// Lists the scene's shots ordered by `ordering`.
    pub fn list_shots(project_root: &Path, scene_id: &str) -> Result<Vec<ShotRecord>, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        repository::ensure_scene_in_project(&conn, &project.id, scene_id)?;
        repository::list_shots(&conn, scene_id)
    }
}

impl CinemaService {
    /// Resolves the locked `speech` / `movement` / `stillness` canon values
    /// for one character. Strict: every one of the three sections must exist
    /// and be locked, otherwise [`AppError::WorkflowPrerequisiteFailed`] is
    /// returned naming the missing keys.
    pub fn resolve_behavioral_locks(
        conn: &Connection,
        character_entity_id: &str,
    ) -> Result<BehavioralLocks, AppError> {
        let mut stmt = conn
            .prepare(
                "SELECT section_key, value_json FROM canon_sections \
                 WHERE canon_entity_id = ?1 AND status = 'locked' \
                 AND section_key IN ('speech', 'movement', 'stillness')",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![character_entity_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut values: BTreeMap<String, String> = BTreeMap::new();
        for row in rows {
            let (key, value_json) = row.map_err(|e| AppError::Database(e.to_string()))?;
            let text = serde_json::from_str::<serde_json::Value>(&value_json)
                .ok()
                .and_then(|value| {
                    value
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty());
            if let Some(text) = text {
                values.insert(key, text);
            }
        }

        let missing: Vec<&str> = ["speech", "movement", "stillness"]
            .into_iter()
            .filter(|key| !values.contains_key(*key))
            .collect();
        if !missing.is_empty() {
            return Err(AppError::WorkflowPrerequisiteFailed(format!(
                "character behavioral canon not locked: {}",
                missing.join("/")
            )));
        }

        Ok(BehavioralLocks {
            speech: values.get("speech").cloned(),
            movement: values.get("movement").cloned(),
            stillness: values.get("stillness").cloned(),
        })
    }

    /// Merged behavioral locks across every character cast into the scene
    /// (cast order wins for duplicate keys).
    pub fn resolve_scene_behavioral_locks(
        conn: &Connection,
        scene_id: &str,
    ) -> Result<BehavioralLocks, AppError> {
        let characters = repository::list_scene_cast(conn, scene_id)?;
        let mut merged = BehavioralLocks::default();
        for character in &characters {
            let locks = Self::resolve_behavioral_locks(conn, &character.character_entity_id)?;
            if merged.speech.is_none() {
                merged.speech = locks.speech;
            }
            if merged.movement.is_none() {
                merged.movement = locks.movement;
            }
            if merged.stillness.is_none() {
                merged.stillness = locks.stillness;
            }
        }
        Ok(merged)
    }
}

impl CinemaService {
    /// Validates the authoritative scene is compilable: it exists in this
    /// project, has at least one character with canonical look versions, an
    /// optional but canonical world plate, and at least one shot. The TBD
    /// firewall is invoked separately (see `tbd_guard`).
    fn validate_scene_for_compilation(
        conn: &Connection,
        project_root: &Path,
        project_id: &str,
        scene_id: &str,
    ) -> Result<(crate::scenes::model::Scene, Vec<crate::scenes::model::SceneCharacterAssignment>), AppError> {
        let scene = load_scene(conn, project_id, scene_id)?;

        let characters = repository::list_scene_cast(conn, &scene.id)?;
        if characters.is_empty() {
            return Err(AppError::WorkflowPrerequisiteFailed(
                "scene has no characters cast".into(),
            ));
        }
        for character in &characters {
            ensure_canonical_version(
                conn,
                project_id,
                &character.look_asset_version_id,
                &["outfit", "character_sheet"],
            )?;
            if let Some(sheet) = &character.sheet_asset_version_id {
                ensure_canonical_version(conn, project_id, sheet, &["character_sheet"])?;
            }
        }

        if let Some(world) = &scene.world_asset_version_id {
            let conn = db::open_existing_connection(&project_root.join("project.db"))?;
            ensure_canonical_version(&conn, project_id, world, &["world_plate"])?;
        }

        let shots = repository::list_shots(conn, &scene.id)?;
        if shots.is_empty() {
            return Err(AppError::WorkflowPrerequisiteFailed(
                "scene has no shots".into(),
            ));
        }

        Ok((scene, characters))
    }

    /// Full compilation workflow: validates the scene, applies the TBD
    /// firewall, resolves behavioral locks / world continuity / visual
    /// locks, compiles the provider-neutral prompt, exports it atomically
    /// under `prompts/cinema/`, and persists the compilation record.
    pub fn compile_scene(
        project_root: &Path,
        input: CinemaCompileInput,
    ) -> Result<CinemaCompilation, AppError> {
        model::validate_total_duration(input.total_duration_seconds)?;
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;

        let (scene, characters) =
            Self::validate_scene_for_compilation(&conn, project_root, &project.id, &input.scene_id)?;
        tbd_guard::check_tbd_firewall(&conn, &project.id, &scene.id)?;

        // Collect the topics of every open TBD so their text is scrubbed
        // from the prompt even when they do not block compilation
        // (e.g. an unprotected question, or one scoped to an unrelated arc).
        let forbidden_topics: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT topic FROM canon_tbds WHERE project_id = ?1 AND status = 'open' \
                     ORDER BY created_at ASC, id ASC",
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![project.id], |row| row.get::<_, String>(0))
                .map_err(|e| AppError::Database(e.to_string()))?;
            rows.map(|row| row.map_err(|e| AppError::Database(e.to_string())))
                .collect::<Result<Vec<String>, AppError>>()?
        };

        let behavioral_locks = Self::resolve_scene_behavioral_locks(&conn, &scene.id)?;
        let world_continuity = match &scene.world_asset_version_id {
            Some(version_id) => {
                let version =
                    ensure_canonical_version(&conn, &project.id, version_id, &["world_plate"])?;
                WorldContinuity {
                    plate_id: Some(version.asset_id),
                    plate_asset_version_id: Some(version.version_id),
                    description: Some(version.label),
                }
            }
            None => WorldContinuity::default(),
        };

        let shots = repository::list_shots(&conn, &scene.id)?;

        // Aggregate visual locks across every cast character (deterministic
        // ordering is handled by the compiler).
        let mut visual_locks: Vec<VisualLockDto> = Vec::new();
        for character in &characters {
            visual_locks.extend(CanonService::get_locked_character_visual_locks(
                project_root,
                &character.character_entity_id,
            )?);
        }

        let compilation_id = Ulid::new().to_string();
        let prompt = compiler::compile(
            &SceneRef {
                id: scene.id.clone(),
                project_id: scene.project_id.clone(),
                title: scene.title.clone(),
                summary: scene.summary.clone(),
            },
            &compilation_id,
            input.total_duration_seconds,
            input.shot_count,
            &behavioral_locks,
            &world_continuity,
            &visual_locks,
            &shots,
            &forbidden_topics,
        )?;

        let (export_path, export_sha256) = export::export_compilation(project_root, &prompt)?;

        let now = Utc::now().to_rfc3339();
        let record = CinemaCompilation {
            id: compilation_id,
            project_id: project.id,
            scene_id: scene.id,
            input_json: serde_json::to_string(&input)
                .map_err(|e| AppError::Database(e.to_string()))?,
            compilation_json: serde_json::to_string(&prompt)
                .map_err(|e| AppError::Database(e.to_string()))?,
            export_path,
            export_sha256,
            created_at: now,
        };
        repository::insert_compilation(&conn, &record)?;
        Ok(record)
    }

    /// Reads a single compilation record for the opened project.
    pub fn get_compilation(
        project_root: &Path,
        compilation_id: &str,
    ) -> Result<CinemaCompilation, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let record = repository::get_compilation(&conn, compilation_id)?;
        if record.project_id != project.id {
            return Err(AppError::CinemaCompilationNotFound);
        }
        Ok(record)
    }

    /// Lists every compilation recorded for `scene_id`, newest first.
    pub fn list_compilations(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<Vec<CinemaCompilation>, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        repository::ensure_scene_in_project(&conn, &project.id, scene_id)?;
        repository::list_compilations(&conn, scene_id)
    }

    /// Applies a field update to one shot with validation.
    pub fn update_shot(project_root: &Path, update: &ShotUpdate) -> Result<ShotRecord, AppError> {
        if let Some(duration) = update.duration_seconds {
            model::validate_shot_duration(duration)?;
        }
        if let Some(intent) = &update.intent {
            validate_intent(intent)?;
        }
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        repository::update_shot(&conn, &project.id, update)
    }

    /// Deletes one shot. Deleting the last shot is allowed and makes the
    /// scene not ready.
    pub fn delete_shot(project_root: &Path, scene_id: &str, shot_id: &str) -> Result<(), AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        repository::delete_shot(&conn, &project.id, scene_id, shot_id)
    }

    /// Reorders the scene's shots transactionally into a contiguous,
    /// deterministic order.
    pub fn reorder_shots(
        project_root: &Path,
        scene_id: &str,
        ordered_ids: &[String],
    ) -> Result<Vec<ShotRecord>, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        repository::reorder_shots(&mut conn, &project.id, scene_id, ordered_ids)
    }

    /// Pins or clears one shot's canonical keyframe version.
    pub fn set_shot_keyframe(
        project_root: &Path,
        shot_id: &str,
        version_id: Option<&str>,
    ) -> Result<(), AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        if let Some(version) = version_id {
            ensure_canonical_version(&conn, &project.id, version, &["shot_keyframe"])?;
        }
        repository::set_shot_keyframe(&conn, &project.id, shot_id, version_id)
    }

    /// Pins (or clears) one shot's exact generated-video version reference.
    /// Mirrors [`set_shot_keyframe`]: the pinned id is an exact immutable
    /// AssetVersion of a `video` asset -- promoting a newer video later
    /// never rewrites this pin.
    pub fn set_shot_video(
        project_root: &Path,
        shot_id: &str,
        version_id: Option<&str>,
    ) -> Result<(), AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        if let Some(version) = version_id {
            ensure_canonical_version(&conn, &project.id, version, &["video"])?;
        }
        repository::set_shot_video(&conn, &project.id, shot_id, version_id)
    }

    /// Computes structured compile readiness for the authoritative scene.
    pub fn scene_readiness(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<CinemaReadiness, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let scene = load_scene(&conn, &project.id, scene_id)?;
        let mut blockers: Vec<CinemaReadinessBlocker> = Vec::new();

        if scene.world_asset_version_id.is_none() {
            blockers.push(CinemaReadinessBlocker {
                code: "missing_world".into(),
                scene_id: scene.id.clone(),
                entity_id: None,
                shot_id: None,
                message: "No World is assigned to this scene.".into(),
                action_target: "world".into(),
            });
        }

        let cast = repository::list_scene_cast(&conn, &scene.id)?;
        if cast.is_empty() {
            blockers.push(CinemaReadinessBlocker {
                code: "missing_cast_look".into(),
                scene_id: scene.id.clone(),
                entity_id: None,
                shot_id: None,
                message: "No character is cast in this scene.".into(),
                action_target: "cast".into(),
            });
        }
        for member in &cast {
            let look_canonical = ensure_canonical_version(
                &conn,
                &project.id,
                &member.look_asset_version_id,
                &["outfit", "character_sheet"],
            )
            .is_ok();
            if !look_canonical {
                blockers.push(CinemaReadinessBlocker {
                    code: "missing_cast_look".into(),
                    scene_id: scene.id.clone(),
                    entity_id: Some(member.character_entity_id.clone()),
                    shot_id: None,
                    message: "The pinned cast look is not the current canonical version.".into(),
                    action_target: "cast".into(),
                });
            }
            if let Some(sheet) = &member.sheet_asset_version_id {
                if ensure_canonical_version(&conn, &project.id, sheet, &["character_sheet"])
                    .is_err()
                {
                    blockers.push(CinemaReadinessBlocker {
                        code: "missing_cast_sheet".into(),
                        scene_id: scene.id.clone(),
                        entity_id: Some(member.character_entity_id.clone()),
                        shot_id: None,
                        message: "The pinned character sheet is not canonical.".into(),
                        action_target: "cast".into(),
                    });
                }
            }
        }

        let shots = repository::list_shots(&conn, &scene.id)?;
        if shots.is_empty() {
            blockers.push(CinemaReadinessBlocker {
                code: "missing_shot".into(),
                scene_id: scene.id.clone(),
                entity_id: None,
                shot_id: None,
                message: "This scene has no shots.".into(),
                action_target: "shot".into(),
            });
        }
        for shot in &shots {
            if let Some(keyframe) = &shot.keyframe_asset_version_id {
                if ensure_canonical_version(&conn, &project.id, keyframe, &["shot_keyframe"])
                    .is_err()
                {
                    blockers.push(CinemaReadinessBlocker {
                        code: "missing_shot_keyframe".into(),
                        scene_id: scene.id.clone(),
                        entity_id: None,
                        shot_id: Some(shot.id.clone()),
                        message: "The pinned shot keyframe is not the current canonical version."
                            .into(),
                        action_target: "shot".into(),
                    });
                }
            }
        }

        Ok(CinemaReadiness {
            scene_id: scene.id,
            ready: blockers.is_empty(),
            blockers,
        })
    }
}

/// Loads the authoritative scene row, scoped to the project.
fn load_scene(
    conn: &Connection,
    project_id: &str,
    scene_id: &str,
) -> Result<crate::scenes::model::Scene, AppError> {
    conn.query_row(
        "SELECT id, project_id, ordinal, title, summary, world_id, world_asset_version_id, \
         keyframe_asset_id, created_at, updated_at \
         FROM world_scenes WHERE id = ?1 AND project_id = ?2",
        params![scene_id, project_id],
        |row| {
            Ok(crate::scenes::model::Scene {
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
        },
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or(AppError::SceneNotFound)
}

/// Verifies `version_id` is the current canonical version of an asset of one
/// of `allowed_types` inside `project_id`.
pub fn ensure_canonical_version(
    conn: &Connection,
    project_id: &str,
    version_id: &str,
    allowed_types: &[&str],
) -> Result<CanonicalVersion, AppError> {
    let row = conn
        .query_row(
            "SELECT a.id, a.project_id, a.type, a.canonical_version_id, av.status, a.label \
             FROM asset_versions av JOIN assets a ON a.id = av.asset_id WHERE av.id = ?1",
            params![version_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let (asset_id, version_project, asset_type, canonical_version_id, status, label) = row
        .ok_or_else(|| {
            AppError::WorkflowPrerequisiteFailed(format!(
                "asset version {version_id} does not exist"
            ))
        })?;

    if version_project != project_id
        || !allowed_types.contains(&asset_type.as_str())
        || status != "canonical"
        || canonical_version_id.as_deref() != Some(version_id)
    {
        return Err(AppError::WorkflowPrerequisiteFailed(format!(
            "asset version {version_id} must be the current canonical version of a {} asset",
            allowed_types.join(" or ")
        )));
    }

    Ok(CanonicalVersion {
        asset_id,
        version_id: version_id.to_string(),
        asset_type,
        label,
    })
}

fn validate_intent(intent: &str) -> Result<String, AppError> {
    let trimmed = intent.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 240 {
        return Err(AppError::InvalidShotIntent);
    }
    Ok(trimmed.to_string())
}

/// A structured readiness blocker for one scene. The UI renders these
/// verbatim and offers the matching section control.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CinemaReadinessBlocker {
    pub code: String,
    pub scene_id: String,
    pub entity_id: Option<String>,
    pub shot_id: Option<String>,
    pub message: String,
    pub action_target: String,
}

/// Structured readiness for one scene: `ready` is true only when no
/// blocker remains.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CinemaReadiness {
    pub scene_id: String,
    pub ready: bool,
    pub blockers: Vec<CinemaReadinessBlocker>,
}
