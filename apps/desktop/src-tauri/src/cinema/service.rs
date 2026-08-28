use crate::canon::repository as canon_repository;
use crate::canon::service::CanonService;
use crate::canon::service::VisualLockDto;
use crate::cinema::compiler;
use crate::cinema::export;
use crate::cinema::model;
use crate::cinema::model::{
    validate_shot_duration, BehavioralLocks, CinemaCompilation, CinemaCompileInput,
    SceneCharacterRecord, SceneRecord, ShotRecord, WorldContinuity,
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
    /// Stages a complete scene atomically from exact canonical references.
    /// Any validation or persistence failure rolls back the scene, cast, and
    /// initial shot together so retry cannot leave duplicate partial scenes.
    pub fn stage_scene(
        project_root: &Path,
        title: &str,
        world_asset_version_id: &str,
        character_entity_id: &str,
        look_asset_version_id: &str,
        sheet_asset_version_id: &str,
    ) -> Result<SceneRecord, AppError> {
        let title = validate_title(title)?;
        let intent = validate_intent("Establish the scene")?;
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let tx = conn.transaction().map_err(|e| AppError::Database(e.to_string()))?;
        ensure_canonical_version(&tx, &project.id, world_asset_version_id, &["world_plate"])?;
        let entity = canon_repository::get_entity(&tx, character_entity_id)?;
        if entity.project_id != project.id || entity.entity_type != "character" {
            return Err(AppError::CanonEntityNotFound);
        }
        ensure_canonical_version(&tx, &project.id, look_asset_version_id, &["outfit", "character_sheet"])?;
        ensure_canonical_version(&tx, &project.id, sheet_asset_version_id, &["character_sheet"])?;

        let now = Utc::now().to_rfc3339();
        let scene = SceneRecord {
            id: Ulid::new().to_string(),
            project_id: project.id,
            title,
            world_asset_version_id: Some(world_asset_version_id.to_string()),
            canon_notes: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        repository::create_scene(&tx, &scene)?;
        repository::add_scene_character(&tx, &SceneCharacterRecord {
            scene_id: scene.id.clone(),
            character_entity_id: character_entity_id.to_string(),
            look_asset_version_id: look_asset_version_id.to_string(),
            sheet_asset_version_id: Some(sheet_asset_version_id.to_string()),
            display_order: 0,
        })?;
        repository::create_shot(&tx, &ShotRecord {
            id: Ulid::new().to_string(),
            scene_id: scene.id.clone(),
            ordering: 0,
            duration_seconds: 4.0,
            keyframe_asset_version_id: None,
            intent,
            action: None,
            camera: None,
            generated_video_asset_version_id: None,
            created_at: now.clone(),
            updated_at: now,
        })?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(scene)
    }

    /// Creates a scene with a validated title (1-160 chars after trimming).
    pub fn create_scene(
        project_root: &Path,
        title: &str,
        world_asset_version_id: Option<String>,
        canon_notes: Option<String>,
    ) -> Result<SceneRecord, AppError> {
        let title = validate_title(title)?;
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;

        if let Some(version) = &world_asset_version_id {
            ensure_canonical_version(&conn, &project.id, version, &["world_plate"])?;
        }

        let now = Utc::now().to_rfc3339();
        let record = SceneRecord {
            id: Ulid::new().to_string(),
            project_id: project.id,
            title,
            world_asset_version_id,
            canon_notes: canon_notes
                .map(|notes| notes.trim().to_string())
                .filter(|notes| !notes.is_empty()),
            created_at: now.clone(),
            updated_at: now,
        };
        repository::create_scene(&conn, &record)?;
        Ok(record)
    }

    /// Lists the project's scenes, newest first.
    pub fn list_scenes(project_root: &Path) -> Result<Vec<SceneRecord>, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        repository::list_scenes(&conn, &project.id)
    }

    /// Reads a single scene belonging to the opened project.
    pub fn get_scene(project_root: &Path, scene_id: &str) -> Result<SceneRecord, AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        repository::get_scene(&conn, &project.id, scene_id)
    }
}

impl CinemaService {
    /// Casts a character entity into a scene. The look (and optional sheet)
    /// versions must be the current canonical versions of outfit /
    /// character-sheet assets in the same project.
    pub fn add_character_to_scene(
        project_root: &Path,
        scene_id: &str,
        character_entity_id: &str,
        look_asset_version_id: &str,
        sheet_asset_version_id: Option<String>,
    ) -> Result<(), AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;

        let scene = repository::get_scene(&conn, &project.id, scene_id)?;
        let entity = canon_repository::get_entity(&conn, character_entity_id)?;
        if entity.project_id != project.id || entity.entity_type != "character" {
            return Err(AppError::CanonEntityNotFound);
        }
        ensure_canonical_version(
            &conn,
            &project.id,
            look_asset_version_id,
            &["outfit", "character_sheet"],
        )?;
        if let Some(sheet) = &sheet_asset_version_id {
            ensure_canonical_version(&conn, &project.id, sheet, &["character_sheet"])?;
        }

        let display_order: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scene_characters WHERE scene_id = ?1",
                params![scene.id],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        repository::add_scene_character(
            &conn,
            &SceneCharacterRecord {
                scene_id: scene.id,
                character_entity_id: entity.id,
                look_asset_version_id: look_asset_version_id.to_string(),
                sheet_asset_version_id,
                display_order,
            },
        )?;
        Ok(())
    }

    /// Pins a canonical prop plate version into a scene.
    pub fn add_prop_to_scene(
        project_root: &Path,
        scene_id: &str,
        prop_asset_version_id: &str,
    ) -> Result<(), AppError> {
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let scene = repository::get_scene(&conn, &project.id, scene_id)?;
        ensure_canonical_version(&conn, &project.id, prop_asset_version_id, &["prop_plate"])?;
        let display_order: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scene_props WHERE scene_id = ?1",
                params![scene.id],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        repository::add_scene_prop(
            &conn,
            &crate::cinema::model::ScenePropRecord {
                scene_id: scene.id,
                prop_asset_version_id: prop_asset_version_id.to_string(),
                display_order,
            },
        )?;
        Ok(())
    }

    /// Creates a shot; when `ordering` is omitted the shot is appended after
    /// the scene's existing shots.
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
        let scene = repository::get_scene(&conn, &project.id, scene_id)?;

        let ordering = match ordering {
            Some(ordering) if ordering >= 0 => ordering,
            Some(ordering) => {
                return Err(AppError::InvalidCinemaDuration(format!(
                    "shot ordering must be non-negative, got {ordering}"
                )))
            }
            None => conn
                .query_row(
                    "SELECT COALESCE(MAX(ordering) + 1, 0) FROM shots WHERE scene_id = ?1",
                    params![scene.id],
                    |row| row.get(0),
                )
                .map_err(|e| AppError::Database(e.to_string()))?,
        };

        let now = Utc::now().to_rfc3339();
        let record = ShotRecord {
            id: Ulid::new().to_string(),
            scene_id: scene.id,
            ordering,
            duration_seconds,
            keyframe_asset_version_id: None,
            intent,
            action: action
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            camera: camera
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            generated_video_asset_version_id: None,
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
        repository::get_scene(&conn, &project.id, scene_id)?;
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
    /// (display order wins for duplicate keys).
    pub fn resolve_scene_behavioral_locks(
        conn: &Connection,
        scene_id: &str,
    ) -> Result<BehavioralLocks, AppError> {
        let characters = repository::list_scene_characters(conn, scene_id)?;
        let mut merged = BehavioralLocks::default();
        for character in characters {
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

    /// Resolves world continuity from the scene's canonical world plate
    /// version. `None` yields empty continuity (the plate is optional); a
    /// version that exists but is not the canonical version of a
    /// `world_plate` asset in this project fails compilation prerequisites.
    pub fn resolve_world_continuity(
        project_root: &Path,
        world_asset_version_id: &Option<String>,
    ) -> Result<WorldContinuity, AppError> {
        let Some(version_id) = world_asset_version_id.as_deref() else {
            return Ok(WorldContinuity::default());
        };
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let version =
            ensure_canonical_version(&conn, &project.id, version_id, &["world_plate"])?;
        Ok(WorldContinuity {
            plate_id: Some(version.asset_id),
            plate_asset_version_id: Some(version.version_id),
            description: Some(version.label),
        })
    }
}


impl CinemaService {
    /// Validates a scene is compilable: it exists in this project, has at
    /// least one character with canonical look versions, an optional but
    /// canonical world plate, and at least one shot. The TBD firewall is
    /// invoked separately by the compilation workflow (see `tbd_guard`).
    pub fn validate_scene_for_compilation(
        conn: &Connection,
        project_id: &str,
        scene_id: &str,
    ) -> Result<SceneRecord, AppError> {
        let scene = repository::get_scene(conn, project_id, scene_id)?;

        let characters = repository::list_scene_characters(conn, &scene.id)?;
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
            ensure_canonical_version(conn, project_id, world, &["world_plate"])?;
        }

        let shots = repository::list_shots(conn, &scene.id)?;
        if shots.is_empty() {
            return Err(AppError::WorkflowPrerequisiteFailed(
                "scene has no shots".into(),
            ));
        }

        Ok(scene)
    }

    /// Full compilation workflow: validates the scene, applies the TBD
    /// firewall, resolves behavioral locks / world continuity / visual
    /// locks, compiles the provider-neutral prompt, exports it atomically
    /// under `prompts/cinema/`, and persists the compilation record — all
    /// before the DB insert commits.
    pub fn compile_scene(
        project_root: &Path,
        input: CinemaCompileInput,
    ) -> Result<CinemaCompilation, AppError> {
        model::validate_total_duration(input.total_duration_seconds)?;
        let project = ProjectService::open(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;

        let scene = Self::validate_scene_for_compilation(&conn, &project.id, &input.scene_id)?;
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
                let version = ensure_canonical_version(
                    &conn,
                    &project.id,
                    version_id,
                    &["world_plate"],
                )?;
                WorldContinuity {
                    plate_id: Some(version.asset_id),
                    plate_asset_version_id: Some(version.version_id),
                    description: Some(version.label),
                }
            }
            None => WorldContinuity::default(),
        };

        let characters = repository::list_scene_characters(&conn, &scene.id)?;
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
            &scene,
            &compilation_id,
            input.total_duration_seconds,
            input.shot_count,
            &characters,
            &behavioral_locks,
            &world_continuity,
            &visual_locks,
            &shots,
            &forbidden_topics,
        )?;

        let (export_path, export_sha256) =
            export::export_compilation(project_root, &prompt)?;

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
        repository::get_scene(&conn, &project.id, scene_id)?;
        repository::list_compilations(&conn, scene_id)
    }
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

fn validate_title(title: &str) -> Result<String, AppError> {
    let trimmed = title.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 160 {
        return Err(AppError::InvalidSceneTitle);
    }
    Ok(trimmed.to_string())
}

fn validate_intent(intent: &str) -> Result<String, AppError> {
    let trimmed = intent.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 240 {
        return Err(AppError::InvalidShotIntent);
    }
    Ok(trimmed.to_string())
}

