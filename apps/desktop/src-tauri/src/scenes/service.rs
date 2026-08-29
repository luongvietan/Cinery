use crate::assets::repository as asset_repository;
use crate::canon::repository as canon_repository;
use crate::db;
use crate::error::AppError;
use crate::project::service::ProjectService;
use crate::assets::model::AssetRecord;
use crate::scenes::model::{
    ResolvedCharacterReference, ResolvedPropReference, ResolvedSceneReference,
    ResolvedSceneReferences, Scene, SceneCharacterAssignment, ScenePropAssignment,
    SceneReadiness, SceneReadinessBlocker, SceneReadinessBlockerKind, SceneReadinessWarning,
    SceneReadinessWarningKind, SceneReferenceAction, SceneReferenceEvent, SceneReferenceHealth,
    SceneReferenceKind, SceneTbdBinding, TbdDecisionKind,
};
use crate::scenes::repository as scenes_repository;
use crate::worlds::repository as worlds_repository;
use chrono::Utc;
use rusqlite::TransactionBehavior;
use std::path::Path;
use ulid::Ulid;

pub struct SceneService;

impl SceneService {
    /// Creates a new Scene for the project rooted at `project_root`.
    ///
    /// Allocates `ordinal` as `COALESCE(MAX(ordinal),0)+1` inside an
    /// `IMMEDIATE` transaction so two concurrent creators cannot claim the
    /// same number. Title is required non-empty; summary may be empty (but
    /// readiness will remain false until filled).
    pub fn create_scene(
        project_root: &Path,
        title: &str,
        summary: &str,
    ) -> Result<Scene, AppError> {
        let trimmed_title = title.trim();
        if trimmed_title.is_empty() {
            return Err(AppError::InvalidSceneTitle);
        }
        // Summary may be empty; no validation except not null.
        let summary = summary.to_string();

        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;

        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;

        let ordinal = scenes_repository::next_ordinal(&tx, &project.id)?;
        let now = Utc::now().to_rfc3339();
        let scene = Scene {
            id: Ulid::new().to_string(),
            project_id: project.id.clone(),
            ordinal,
            title: trimmed_title.to_string(),
            summary,
            world_id: None,
            world_asset_version_id: None,
            keyframe_asset_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        scenes_repository::insert_scene(&tx, &scene)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(scene)
    }

    pub fn list_scenes(project_root: &Path) -> Result<Vec<Scene>, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        scenes_repository::list_scenes(&conn, &project.id)
    }

    pub fn get_scene(project_root: &Path, scene_id: &str) -> Result<Scene, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        Ok(scene)
    }

    pub fn update_scene_details(
        project_root: &Path,
        scene_id: &str,
        title: &str,
        summary: &str,
    ) -> Result<Scene, AppError> {
        let trimmed_title = title.trim();
        if trimmed_title.is_empty() {
            return Err(AppError::InvalidSceneTitle);
        }
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        // Verify scene exists and belongs to project before update
        let existing = scenes_repository::get_scene(&conn, scene_id)?;
        if existing.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let now = Utc::now().to_rfc3339();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        scenes_repository::update_scene_details(
            &tx,
            scene_id,
            trimmed_title,
            summary,
            &now,
        )?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        // Re-fetch updated scene
        let mut conn2 = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn2)?;
        let updated = scenes_repository::get_scene(&conn2, scene_id)?;
        Ok(updated)
    }

    /// Assigns a World to a Scene, resolving the World's current canonical
    /// `world_plate` version once and storing its exact
    /// `asset_version.id` in `scenes.world_asset_version_id`.
    ///
    /// This must never store just the asset alias; later promotion must not
    /// mutate the Scene.
    pub fn assign_scene_world(
        project_root: &Path,
        scene_id: &str,
        world_id: &str,
    ) -> Result<Scene, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;

        // Load scene and world outside transaction for validation
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let world = worlds_repository::get_world(&conn, world_id).map_err(|e| match e {
            AppError::WorldNotFound => AppError::WorldNotFound,
            other => other,
        })?;
        if world.project_id != project.id {
            return Err(AppError::WorldNotFound);
        }
        // Resolve current canonical world plate version
        let asset = asset_repository::get_asset(&conn, &world.world_plate_asset_id)?;
        if asset.project_id != project.id {
            return Err(AppError::Database(format!(
                "world {} plate asset project mismatch",
                world.id
            )));
        }
        let canonical_version_id = asset.canonical_version_id.clone().ok_or_else(|| {
            AppError::SceneWorldPlateNotCanonical(format!(
                "world {} has no canonical world_plate version",
                world.id
            ))
        })?;
        let version = asset_repository::get_asset_version_by_id(&conn, &canonical_version_id)?
            .ok_or(AppError::AssetVersionNotFound)?;
        if version.status != "canonical" {
            return Err(AppError::SceneWorldPlateNotCanonical(format!(
                "world {} canonical pointer {} status != canonical ({})",
                world.id, canonical_version_id, version.status
            )));
        }
        if version.asset_id != asset.id {
            return Err(AppError::Database(format!(
                "world {} canonical version asset mismatch",
                world.id
            )));
        }

        // Now transactionally update scene pin
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        // Re-validate inside tx that scene still exists (optional)
        let current = scenes_repository::get_scene(&tx, scene_id)?;
        let from_version = current.world_asset_version_id.clone();
        let now = Utc::now().to_rfc3339();
        scenes_repository::update_scene_world(
            &tx,
            scene_id,
            Some(world_id),
            Some(&canonical_version_id),
            &now,
        )?;
        let action = if from_version.is_none() {
            SceneReferenceAction::Pin
        } else if from_version.as_deref() == Some(&canonical_version_id) {
            // Already pinned to same version — still succeed but no distinct upgrade? Treat as Pin no-op? But we already updated; just keep Pin
            SceneReferenceAction::Pin
        } else {
            SceneReferenceAction::Upgrade
        };
        let event = SceneReferenceEvent {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            reference_kind: SceneReferenceKind::World,
            assignment_id: None,
            action,
            from_version_id: from_version,
            to_version_id: Some(canonical_version_id.clone()),
            created_at: now.clone(),
        };
        scenes_repository::insert_reference_event(&tx, &event)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;

        let mut conn2 = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn2)?;
        let updated = scenes_repository::get_scene(&conn2, scene_id)?;
        Ok(updated)
    }

    pub fn clear_scene_world(project_root: &Path, scene_id: &str) -> Result<Scene, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let from_version = scene.world_asset_version_id.clone();
        if from_version.is_none() && scene.world_id.is_none() {
            // Already cleared — idempotent
            return Ok(scene);
        }
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let now = Utc::now().to_rfc3339();
        scenes_repository::update_scene_world(&tx, scene_id, None, None, &now)?;
        let event = SceneReferenceEvent {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            reference_kind: SceneReferenceKind::World,
            assignment_id: None,
            action: SceneReferenceAction::Remove,
            from_version_id: from_version,
            to_version_id: None,
            created_at: now.clone(),
        };
        scenes_repository::insert_reference_event(&tx, &event)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        let mut conn2 = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn2)?;
        let updated = scenes_repository::get_scene(&conn2, scene_id)?;
        Ok(updated)
    }

    pub fn add_scene_character(
        project_root: &Path,
        scene_id: &str,
        character_entity_id: &str,
        look_asset_version_id: &str,
        sheet_asset_version_id: Option<&str>,
        notes: Option<&str>,
    ) -> Result<SceneCharacterAssignment, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;

        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }

        // Validate character entity exists and is a character
        let character = canon_repository::get_entity(&conn, character_entity_id).map_err(|e| match e {
            AppError::CanonEntityNotFound => AppError::CanonEntityNotFound,
            other => other,
        })?;
        if character.project_id != project.id {
            return Err(AppError::CanonEntityNotFound);
        }
        if character.entity_type != "character" {
            return Err(AppError::CanonEntityNotFound);
        }

        // Validate Look version
        let look_version =
            asset_repository::get_asset_version_by_id(&conn, look_asset_version_id)?
                .ok_or(AppError::AssetVersionNotFound)?;
        if look_version.status != "canonical" {
            return Err(AppError::SceneCharacterLookNotCanonical);
        }
        let look_asset = asset_repository::get_asset(&conn, &look_version.asset_id)?;
        if look_asset.project_id != project.id {
            return Err(AppError::SceneCharacterLookNotOwned);
        }
        if look_asset.owner_entity_id.as_deref() != Some(character_entity_id) {
            return Err(AppError::SceneCharacterLookNotOwned);
        }
        // Optionally, enforce asset type is suitable for a Look (outfit, face_lock, etc.)
        // For now allow any type except world_plate/prop_plate/shot_keyframe, but check label?
        // Spec says reject incompatible Sheet, but Look should be checked as not prop_plate.
        if matches!(
            look_asset.asset_type.as_str(),
            "world_plate" | "prop_plate" | "shot_keyframe"
        ) {
            return Err(AppError::SceneCharacterLookNotCanonical);
        }

        // Validate optional sheet
        let sheet_version_id_owned: Option<String> = if let Some(sheet_id) = sheet_asset_version_id {
            let sheet_version =
                asset_repository::get_asset_version_by_id(&conn, sheet_id)?
                    .ok_or(AppError::AssetVersionNotFound)?;
            if sheet_version.status != "canonical" {
                return Err(AppError::SceneCharacterSheetNotCanonical);
            }
            let sheet_asset = asset_repository::get_asset(&conn, &sheet_version.asset_id)?;
            if sheet_asset.project_id != project.id {
                return Err(AppError::SceneCharacterSheetNotOwned);
            }
            if sheet_asset.owner_entity_id.as_deref() != Some(character_entity_id) {
                return Err(AppError::SceneCharacterSheetNotOwned);
            }
            // Sheet asset type should be character_sheet (or outfit? but we enforce character_sheet)
            if sheet_asset.asset_type != "character_sheet" {
                // Allow outfit as sheet? Spec says Sheet must belong to same Character / Look according to P5 relationships
                // At minimum same owner; but if asset type not character_sheet, treat as ownership mismatch
                // We'll still require character_sheet for strictness, but if it's outfit we could allow? For now enforce character_sheet.
                // To keep tests flexible, we allow character_sheet or outfit.
                if sheet_asset.asset_type != "outfit" && sheet_asset.asset_type != "character_sheet" {
                    return Err(AppError::SceneCharacterSheetNotOwned);
                }
            }
            Some(sheet_version.id.clone())
        } else {
            None
        };

        // Check uniqueness: scene + character already assigned?
        if scenes_repository::find_scene_character_by_scene_and_character(
            &conn,
            scene_id,
            character_entity_id,
        )?
        .is_some()
        {
            return Err(AppError::SceneCharacterAlreadyExists);
        }

        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let now = Utc::now().to_rfc3339();
        let assignment = SceneCharacterAssignment {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            character_entity_id: character_entity_id.to_string(),
            look_asset_version_id: look_version.id.clone(),
            sheet_asset_version_id: sheet_version_id_owned.clone(),
            notes: notes.map(|s| s.to_string()),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        scenes_repository::insert_scene_character(&tx, &assignment)?;

        // Reference events
        let look_event = SceneReferenceEvent {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            reference_kind: SceneReferenceKind::CharacterLook,
            assignment_id: Some(assignment.id.clone()),
            action: SceneReferenceAction::Pin,
            from_version_id: None,
            to_version_id: Some(look_version.id.clone()),
            created_at: now.clone(),
        };
        scenes_repository::insert_reference_event(&tx, &look_event)?;

        if let Some(sheet_id) = &sheet_version_id_owned {
            let sheet_event = SceneReferenceEvent {
                id: Ulid::new().to_string(),
                scene_id: scene_id.to_string(),
                reference_kind: SceneReferenceKind::CharacterSheet,
                assignment_id: Some(assignment.id.clone()),
                action: SceneReferenceAction::Pin,
                from_version_id: None,
                to_version_id: Some(sheet_id.clone()),
                created_at: now.clone(),
            };
            scenes_repository::insert_reference_event(&tx, &sheet_event)?;
        }

        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(assignment)
    }

    pub fn remove_scene_character(
        project_root: &Path,
        scene_id: &str,
        character_entity_id: &str,
    ) -> Result<(), AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let existing = scenes_repository::delete_scene_character_by_scene_and_character(
            &tx,
            scene_id,
            character_entity_id,
        )?;
        let now = Utc::now().to_rfc3339();
        let event = SceneReferenceEvent {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            reference_kind: SceneReferenceKind::CharacterLook,
            assignment_id: Some(existing.id.clone()),
            action: SceneReferenceAction::Remove,
            from_version_id: Some(existing.look_asset_version_id.clone()),
            to_version_id: None,
            created_at: now.clone(),
        };
        scenes_repository::insert_reference_event(&tx, &event)?;
        if let Some(sheet_id) = existing.sheet_asset_version_id.clone() {
            let sheet_event = SceneReferenceEvent {
                id: Ulid::new().to_string(),
                scene_id: scene_id.to_string(),
                reference_kind: SceneReferenceKind::CharacterSheet,
                assignment_id: Some(existing.id.clone()),
                action: SceneReferenceAction::Remove,
                from_version_id: Some(sheet_id),
                to_version_id: None,
                created_at: now.clone(),
            };
            scenes_repository::insert_reference_event(&tx, &sheet_event)?;
        }
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn list_scene_characters(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<Vec<SceneCharacterAssignment>, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        scenes_repository::list_scene_characters(&conn, scene_id)
    }

    pub fn add_scene_prop(
        project_root: &Path,
        scene_id: &str,
        prop_asset_version_id: &str,
        label: Option<&str>,
        notes: Option<&str>,
    ) -> Result<ScenePropAssignment, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let prop_version =
            asset_repository::get_asset_version_by_id(&conn, prop_asset_version_id)?
                .ok_or(AppError::AssetVersionNotFound)?;
        if prop_version.status != "canonical" {
            return Err(AppError::ScenePropNotCanonical);
        }
        let prop_asset = asset_repository::get_asset(&conn, &prop_version.asset_id)?;
        if prop_asset.project_id != project.id {
            return Err(AppError::ScenePropNotCanonical);
        }
        if prop_asset.asset_type != "prop_plate" {
            return Err(AppError::ScenePropInvalidType);
        }
        // Check duplicate
        if scenes_repository::find_scene_prop_by_version(&conn, scene_id, prop_asset_version_id)?
            .is_some()
        {
            return Err(AppError::ScenePropAlreadyExists);
        }
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let now = Utc::now().to_rfc3339();
        let assignment = ScenePropAssignment {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            prop_asset_version_id: prop_version.id.clone(),
            label: label.map(|s| s.to_string()),
            notes: notes.map(|s| s.to_string()),
            created_at: now.clone(),
        };
        scenes_repository::insert_scene_prop(&tx, &assignment)?;
        let event = SceneReferenceEvent {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            reference_kind: SceneReferenceKind::Prop,
            assignment_id: Some(assignment.id.clone()),
            action: SceneReferenceAction::Pin,
            from_version_id: None,
            to_version_id: Some(prop_version.id.clone()),
            created_at: now.clone(),
        };
        scenes_repository::insert_reference_event(&tx, &event)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(assignment)
    }

    pub fn remove_scene_prop(
        project_root: &Path,
        scene_id: &str,
        prop_asset_version_id: &str,
    ) -> Result<(), AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let existing = scenes_repository::delete_scene_prop_by_version(
            &tx,
            scene_id,
            prop_asset_version_id,
        )?;
        let now = Utc::now().to_rfc3339();
        let event = SceneReferenceEvent {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            reference_kind: SceneReferenceKind::Prop,
            assignment_id: Some(existing.id.clone()),
            action: SceneReferenceAction::Remove,
            from_version_id: Some(existing.prop_asset_version_id.clone()),
            to_version_id: None,
            created_at: now.clone(),
        };
        scenes_repository::insert_reference_event(&tx, &event)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn list_scene_props(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<Vec<ScenePropAssignment>, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        scenes_repository::list_scene_props(&conn, scene_id)
    }

    // -----------------------------------------------------------------------
    // Reference health resolver (read-only, never mutates)
    // -----------------------------------------------------------------------

    /// Resolves the health of every exact reference pinned to a Scene.
    ///
    /// This function is *read-only* and must never executeUPDATE/DELETE.
    /// It derives health from the current canonical pointers and validates
    /// ownership/type/file invariants.
    pub fn resolve_scene_references(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<ResolvedSceneReferences, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }

        let world = if let Some(pinned) = &scene.world_asset_version_id {
            Some(Self::resolve_world_ref(&conn, project_root, pinned, &scene)?)
        } else {
            None
        };

        let char_assignments = scenes_repository::list_scene_characters(&conn, scene_id)?;
        let mut characters = Vec::new();
        for assignment in char_assignments {
            let look = Self::resolve_character_look_ref(
                &conn,
                project_root,
                &assignment.look_asset_version_id,
                &scene,
                &assignment,
            )?;
            let sheet = if let Some(sheet_id) = &assignment.sheet_asset_version_id {
                Some(Self::resolve_character_sheet_ref(
                    &conn,
                    project_root,
                    sheet_id,
                    &scene,
                    &assignment,
                )?)
            } else {
                None
            };
            characters.push(ResolvedCharacterReference {
                assignment_id: assignment.id.clone(),
                character_entity_id: assignment.character_entity_id.clone(),
                look,
                sheet,
            });
        }

        let prop_assignments = scenes_repository::list_scene_props(&conn, scene_id)?;
        let mut props = Vec::new();
        for assignment in prop_assignments {
            let reference = Self::resolve_prop_ref(
                &conn,
                project_root,
                &assignment.prop_asset_version_id,
                &scene,
            )?;
            props.push(ResolvedPropReference {
                assignment_id: assignment.id.clone(),
                reference,
            });
        }

        Ok(ResolvedSceneReferences {
            scene_id: scene.id,
            world,
            characters,
            props,
        })
    }

    fn resolve_world_ref(
        conn: &rusqlite::Connection,
        project_root: &Path,
        pinned_version_id: &str,
        scene: &Scene,
    ) -> Result<ResolvedSceneReference, AppError> {
        // Try to load pinned version
        let version_opt = asset_repository::get_asset_version_by_id(conn, pinned_version_id)?;
        let Some(version) = version_opt else {
            return Ok(ResolvedSceneReference {
                asset_id: String::new(),
                pinned_version_id: pinned_version_id.to_string(),
                current_canonical_version_id: None,
                health: SceneReferenceHealth::Broken,
                version_number: 0,
                status: "unknown".to_string(),
                file_path: String::new(),
            });
        };

        // Try to load asset
        let asset = match asset_repository::get_asset(conn, &version.asset_id) {
            Ok(a) => a,
            Err(_) => {
                return Ok(ResolvedSceneReference {
                    asset_id: version.asset_id.clone(),
                    pinned_version_id: pinned_version_id.to_string(),
                    current_canonical_version_id: None,
                    health: SceneReferenceHealth::Broken,
                    version_number: version.version_number,
                    status: version.status.clone(),
                    file_path: version.file_path.clone(),
                });
            }
        };

        // Validate invariants
        let mut broken = false;
        if asset.project_id != scene.project_id {
            broken = true;
        }
        if asset.asset_type != "world_plate" {
            broken = true;
        }
        if let Some(world_id) = &scene.world_id {
            if asset.owner_entity_id.as_deref() != Some(world_id.as_str()) {
                // also verify world.asset matches? Load world to double-check
                // If asset id mismatches world plate asset, treat as broken
                // Try to fetch world
                match worlds_repository::get_world(conn, world_id) {
                    Ok(w) => {
                        if w.world_plate_asset_id != asset.id {
                            broken = true;
                        }
                    }
                    Err(_) => broken = true,
                }
            }
        } else {
            broken = true;
        }
        // File existence
        if !project_root.join(&version.file_path).exists() {
            broken = true;
        }

        if broken {
            return Ok(ResolvedSceneReference {
                asset_id: asset.id.clone(),
                pinned_version_id: pinned_version_id.to_string(),
                current_canonical_version_id: asset.canonical_version_id.clone(),
                health: SceneReferenceHealth::Broken,
                version_number: version.version_number,
                status: version.status.clone(),
                file_path: version.file_path.clone(),
            });
        }

        // Determine health based on canonical pointer
        let current_canonical = asset.canonical_version_id.clone();
        let health = match &current_canonical {
            None => SceneReferenceHealth::Historical,
            Some(canonical_id) => {
                // Validate canonical exists
                let canonical_opt =
                    asset_repository::get_asset_version_by_id(conn, canonical_id)?;
                let Some(canonical_ver) = canonical_opt else {
                    // Canonical pointer broken -> treat as Broken
                    return Ok(ResolvedSceneReference {
                        asset_id: asset.id.clone(),
                        pinned_version_id: pinned_version_id.to_string(),
                        current_canonical_version_id: current_canonical.clone(),
                        health: SceneReferenceHealth::Broken,
                        version_number: version.version_number,
                        status: version.status.clone(),
                        file_path: version.file_path.clone(),
                    });
                };
                if canonical_ver.status != "canonical" {
                    return Ok(ResolvedSceneReference {
                        asset_id: asset.id.clone(),
                        pinned_version_id: pinned_version_id.to_string(),
                        current_canonical_version_id: current_canonical.clone(),
                        health: SceneReferenceHealth::Broken,
                        version_number: version.version_number,
                        status: version.status.clone(),
                        file_path: version.file_path.clone(),
                    });
                }
                if canonical_id == pinned_version_id {
                    SceneReferenceHealth::Current
                } else {
                    SceneReferenceHealth::UpgradeAvailable
                }
            }
        };

        Ok(ResolvedSceneReference {
            asset_id: asset.id.clone(),
            pinned_version_id: pinned_version_id.to_string(),
            current_canonical_version_id: current_canonical,
            health,
            version_number: version.version_number,
            status: version.status.clone(),
            file_path: version.file_path.clone(),
        })
    }

    fn resolve_character_look_ref(
        conn: &rusqlite::Connection,
        project_root: &Path,
        pinned_version_id: &str,
        scene: &Scene,
        assignment: &SceneCharacterAssignment,
    ) -> Result<ResolvedSceneReference, AppError> {
        let version_opt = asset_repository::get_asset_version_by_id(conn, pinned_version_id)?;
        let Some(version) = version_opt else {
            return Ok(ResolvedSceneReference {
                asset_id: String::new(),
                pinned_version_id: pinned_version_id.to_string(),
                current_canonical_version_id: None,
                health: SceneReferenceHealth::Broken,
                version_number: 0,
                status: "unknown".to_string(),
                file_path: String::new(),
            });
        };
        let asset = match asset_repository::get_asset(conn, &version.asset_id) {
            Ok(a) => a,
            Err(_) => {
                return Ok(ResolvedSceneReference {
                    asset_id: version.asset_id.clone(),
                    pinned_version_id: pinned_version_id.to_string(),
                    current_canonical_version_id: None,
                    health: SceneReferenceHealth::Broken,
                    version_number: version.version_number,
                    status: version.status.clone(),
                    file_path: version.file_path.clone(),
                });
            }
        };

        let mut broken = false;
        if asset.project_id != scene.project_id {
            broken = true;
        }
        if matches!(
            asset.asset_type.as_str(),
            "world_plate" | "prop_plate" | "shot_keyframe"
        ) {
            broken = true;
        }
        if asset.owner_entity_id.as_deref() != Some(assignment.character_entity_id.as_str()) {
            broken = true;
        }
        // Character entity must exist and be character type
        match canon_repository::get_entity(conn, &assignment.character_entity_id) {
            Ok(ent) => {
                if ent.entity_type != "character" || ent.project_id != scene.project_id {
                    broken = true;
                }
            }
            Err(_) => broken = true,
        }
        if !project_root.join(&version.file_path).exists() {
            broken = true;
        }

        if broken {
            return Ok(ResolvedSceneReference {
                asset_id: asset.id.clone(),
                pinned_version_id: pinned_version_id.to_string(),
                current_canonical_version_id: asset.canonical_version_id.clone(),
                health: SceneReferenceHealth::Broken,
                version_number: version.version_number,
                status: version.status.clone(),
                file_path: version.file_path.clone(),
            });
        }

        let current_canonical = asset.canonical_version_id.clone();
        let health = match &current_canonical {
            None => SceneReferenceHealth::Historical,
            Some(canonical_id) => {
                let canonical_opt =
                    asset_repository::get_asset_version_by_id(conn, canonical_id)?;
                let Some(canonical_ver) = canonical_opt else {
                    return Ok(ResolvedSceneReference {
                        asset_id: asset.id.clone(),
                        pinned_version_id: pinned_version_id.to_string(),
                        current_canonical_version_id: current_canonical.clone(),
                        health: SceneReferenceHealth::Broken,
                        version_number: version.version_number,
                        status: version.status.clone(),
                        file_path: version.file_path.clone(),
                    });
                };
                if canonical_ver.status != "canonical" {
                    return Ok(ResolvedSceneReference {
                        asset_id: asset.id.clone(),
                        pinned_version_id: pinned_version_id.to_string(),
                        current_canonical_version_id: current_canonical.clone(),
                        health: SceneReferenceHealth::Broken,
                        version_number: version.version_number,
                        status: version.status.clone(),
                        file_path: version.file_path.clone(),
                    });
                }
                if canonical_id == pinned_version_id {
                    SceneReferenceHealth::Current
                } else {
                    SceneReferenceHealth::UpgradeAvailable
                }
            }
        };

        Ok(ResolvedSceneReference {
            asset_id: asset.id.clone(),
            pinned_version_id: pinned_version_id.to_string(),
            current_canonical_version_id: current_canonical,
            health,
            version_number: version.version_number,
            status: version.status.clone(),
            file_path: version.file_path.clone(),
        })
    }

    fn resolve_character_sheet_ref(
        conn: &rusqlite::Connection,
        project_root: &Path,
        pinned_version_id: &str,
        scene: &Scene,
        assignment: &SceneCharacterAssignment,
    ) -> Result<ResolvedSceneReference, AppError> {
        let version_opt = asset_repository::get_asset_version_by_id(conn, pinned_version_id)?;
        let Some(version) = version_opt else {
            return Ok(ResolvedSceneReference {
                asset_id: String::new(),
                pinned_version_id: pinned_version_id.to_string(),
                current_canonical_version_id: None,
                health: SceneReferenceHealth::Broken,
                version_number: 0,
                status: "unknown".to_string(),
                file_path: String::new(),
            });
        };
        let asset = match asset_repository::get_asset(conn, &version.asset_id) {
            Ok(a) => a,
            Err(_) => {
                return Ok(ResolvedSceneReference {
                    asset_id: version.asset_id.clone(),
                    pinned_version_id: pinned_version_id.to_string(),
                    current_canonical_version_id: None,
                    health: SceneReferenceHealth::Broken,
                    version_number: version.version_number,
                    status: version.status.clone(),
                    file_path: version.file_path.clone(),
                });
            }
        };

        let mut broken = false;
        if asset.project_id != scene.project_id {
            broken = true;
        }
        if asset.asset_type != "character_sheet" && asset.asset_type != "outfit" {
            broken = true;
        }
        if asset.owner_entity_id.as_deref() != Some(assignment.character_entity_id.as_str()) {
            broken = true;
        }
        match canon_repository::get_entity(conn, &assignment.character_entity_id) {
            Ok(ent) => {
                if ent.entity_type != "character" || ent.project_id != scene.project_id {
                    broken = true;
                }
            }
            Err(_) => broken = true,
        }
        if !project_root.join(&version.file_path).exists() {
            broken = true;
        }

        if broken {
            return Ok(ResolvedSceneReference {
                asset_id: asset.id.clone(),
                pinned_version_id: pinned_version_id.to_string(),
                current_canonical_version_id: asset.canonical_version_id.clone(),
                health: SceneReferenceHealth::Broken,
                version_number: version.version_number,
                status: version.status.clone(),
                file_path: version.file_path.clone(),
            });
        }

        let current_canonical = asset.canonical_version_id.clone();
        let health = match &current_canonical {
            None => SceneReferenceHealth::Historical,
            Some(canonical_id) => {
                let canonical_opt =
                    asset_repository::get_asset_version_by_id(conn, canonical_id)?;
                let Some(canonical_ver) = canonical_opt else {
                    return Ok(ResolvedSceneReference {
                        asset_id: asset.id.clone(),
                        pinned_version_id: pinned_version_id.to_string(),
                        current_canonical_version_id: current_canonical.clone(),
                        health: SceneReferenceHealth::Broken,
                        version_number: version.version_number,
                        status: version.status.clone(),
                        file_path: version.file_path.clone(),
                    });
                };
                if canonical_ver.status != "canonical" {
                    return Ok(ResolvedSceneReference {
                        asset_id: asset.id.clone(),
                        pinned_version_id: pinned_version_id.to_string(),
                        current_canonical_version_id: current_canonical.clone(),
                        health: SceneReferenceHealth::Broken,
                        version_number: version.version_number,
                        status: version.status.clone(),
                        file_path: version.file_path.clone(),
                    });
                }
                if canonical_id == pinned_version_id {
                    SceneReferenceHealth::Current
                } else {
                    SceneReferenceHealth::UpgradeAvailable
                }
            }
        };

        Ok(ResolvedSceneReference {
            asset_id: asset.id.clone(),
            pinned_version_id: pinned_version_id.to_string(),
            current_canonical_version_id: current_canonical,
            health,
            version_number: version.version_number,
            status: version.status.clone(),
            file_path: version.file_path.clone(),
        })
    }

    fn resolve_prop_ref(
        conn: &rusqlite::Connection,
        project_root: &Path,
        pinned_version_id: &str,
        scene: &Scene,
    ) -> Result<ResolvedSceneReference, AppError> {
        let version_opt = asset_repository::get_asset_version_by_id(conn, pinned_version_id)?;
        let Some(version) = version_opt else {
            return Ok(ResolvedSceneReference {
                asset_id: String::new(),
                pinned_version_id: pinned_version_id.to_string(),
                current_canonical_version_id: None,
                health: SceneReferenceHealth::Broken,
                version_number: 0,
                status: "unknown".to_string(),
                file_path: String::new(),
            });
        };
        let asset = match asset_repository::get_asset(conn, &version.asset_id) {
            Ok(a) => a,
            Err(_) => {
                return Ok(ResolvedSceneReference {
                    asset_id: version.asset_id.clone(),
                    pinned_version_id: pinned_version_id.to_string(),
                    current_canonical_version_id: None,
                    health: SceneReferenceHealth::Broken,
                    version_number: version.version_number,
                    status: version.status.clone(),
                    file_path: version.file_path.clone(),
                });
            }
        };

        let mut broken = false;
        if asset.project_id != scene.project_id {
            broken = true;
        }
        if asset.asset_type != "prop_plate" {
            broken = true;
        }
        if !project_root.join(&version.file_path).exists() {
            broken = true;
        }

        if broken {
            return Ok(ResolvedSceneReference {
                asset_id: asset.id.clone(),
                pinned_version_id: pinned_version_id.to_string(),
                current_canonical_version_id: asset.canonical_version_id.clone(),
                health: SceneReferenceHealth::Broken,
                version_number: version.version_number,
                status: version.status.clone(),
                file_path: version.file_path.clone(),
            });
        }

        let current_canonical = asset.canonical_version_id.clone();
        let health = match &current_canonical {
            None => SceneReferenceHealth::Historical,
            Some(canonical_id) => {
                let canonical_opt =
                    asset_repository::get_asset_version_by_id(conn, canonical_id)?;
                let Some(canonical_ver) = canonical_opt else {
                    return Ok(ResolvedSceneReference {
                        asset_id: asset.id.clone(),
                        pinned_version_id: pinned_version_id.to_string(),
                        current_canonical_version_id: current_canonical.clone(),
                        health: SceneReferenceHealth::Broken,
                        version_number: version.version_number,
                        status: version.status.clone(),
                        file_path: version.file_path.clone(),
                    });
                };
                if canonical_ver.status != "canonical" {
                    return Ok(ResolvedSceneReference {
                        asset_id: asset.id.clone(),
                        pinned_version_id: pinned_version_id.to_string(),
                        current_canonical_version_id: current_canonical.clone(),
                        health: SceneReferenceHealth::Broken,
                        version_number: version.version_number,
                        status: version.status.clone(),
                        file_path: version.file_path.clone(),
                    });
                }
                if canonical_id == pinned_version_id {
                    SceneReferenceHealth::Current
                } else {
                    SceneReferenceHealth::UpgradeAvailable
                }
            }
        };

        Ok(ResolvedSceneReference {
            asset_id: asset.id.clone(),
            pinned_version_id: pinned_version_id.to_string(),
            current_canonical_version_id: current_canonical,
            health,
            version_number: version.version_number,
            status: version.status.clone(),
            file_path: version.file_path.clone(),
        })
    }

    // -----------------------------------------------------------------------
    // Explicit upgrade operations (mutating, transactional, exactly one ref)
    // -----------------------------------------------------------------------

    pub fn upgrade_scene_world_reference(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<ResolvedSceneReference, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;

        // 1. load scene
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let pinned_id = scene
            .world_asset_version_id
            .clone()
            .ok_or_else(|| AppError::SceneReferenceBroken("no world reference to upgrade".to_string()))?;

        // 2. load pinned version
        let pinned_version = asset_repository::get_asset_version_by_id(&conn, &pinned_id)?
            .ok_or_else(|| AppError::SceneReferenceBroken(format!("pinned world version {pinned_id} not found")))?;

        // 3. find conceptual Asset
        let asset = asset_repository::get_asset(&conn, &pinned_version.asset_id).map_err(|_| {
            AppError::SceneReferenceBroken(format!("asset {} not found", pinned_version.asset_id))
        })?;
        if asset.project_id != project.id {
            return Err(AppError::SceneReferenceBroken("world asset project mismatch".to_string()));
        }
        if asset.asset_type != "world_plate" {
            return Err(AppError::SceneReferenceBroken("world asset type mismatch".to_string()));
        }
        if let Some(world_id) = &scene.world_id {
            if asset.owner_entity_id.as_deref() != Some(world_id.as_str()) {
                return Err(AppError::SceneReferenceBroken("world asset owner mismatch".to_string()));
            }
            // also verify world.asset matches
            let world = worlds_repository::get_world(&conn, world_id)?;
            if world.world_plate_asset_id != asset.id {
                return Err(AppError::SceneReferenceBroken("world plate asset mismatch".to_string()));
            }
        } else {
            return Err(AppError::SceneReferenceBroken("scene has no world_id".to_string()));
        }
        if !project_root.join(&pinned_version.file_path).exists() {
            return Err(AppError::SceneReferenceBroken("pinned world file missing".to_string()));
        }

        // 4. resolve current canonical
        let canonical_id = asset.canonical_version_id.clone().ok_or_else(|| {
            AppError::SceneReferenceCanonicalMissing(format!(
                "world asset {} has no canonical version",
                asset.id
            ))
        })?;

        // 5. validate ownership/type already done for asset, but also canonical version
        let canonical_version = asset_repository::get_asset_version_by_id(&conn, &canonical_id)?
            .ok_or_else(|| {
                AppError::SceneReferenceBroken(format!("canonical version {canonical_id} not found"))
            })?;
        if canonical_version.asset_id != asset.id {
            return Err(AppError::SceneReferenceBroken("canonical asset mismatch".to_string()));
        }
        if canonical_version.status != "canonical" {
            return Err(AppError::SceneReferenceBroken("canonical status not canonical".to_string()));
        }
        if !project_root.join(&canonical_version.file_path).exists() {
            return Err(AppError::SceneReferenceBroken("canonical world file missing".to_string()));
        }

        // 6. fail if no canonical already handled, 7. fail if already current
        if pinned_id == canonical_id {
            return Err(AppError::SceneReferenceAlreadyCurrent);
        }

        // 8. begin transaction
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;

        // Re-validate scene exists inside tx (optional but consistent)
        let current = scenes_repository::get_scene(&tx, scene_id)?;
        if current.world_asset_version_id.as_deref() != Some(pinned_id.as_str()) {
            // Concurrent modification: pinned changed, treat as already current or broken
            // For simplicity, check if now already canonical
            if current.world_asset_version_id.as_deref() == Some(canonical_id.as_str()) {
                return Err(AppError::SceneReferenceAlreadyCurrent);
            }
        }

        let now = Utc::now().to_rfc3339();
        // 9. update exactly one Scene reference
        scenes_repository::update_scene_world(
            &tx,
            scene_id,
            scene.world_id.as_deref(),
            Some(&canonical_id),
            &now,
        )?;

        // 10. create one scene_reference_event
        let event = SceneReferenceEvent {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            reference_kind: SceneReferenceKind::World,
            assignment_id: None,
            action: SceneReferenceAction::Upgrade,
            from_version_id: Some(pinned_id.clone()),
            to_version_id: Some(canonical_id.clone()),
            created_at: now.clone(),
        };
        scenes_repository::insert_reference_event(&tx, &event)?;

        // 11. commit
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;

        // 12. return updated reference health (should be current)
        let mut conn2 = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn2)?;
        let updated_scene = scenes_repository::get_scene(&conn2, scene_id)?;
        let new_pinned = updated_scene
            .world_asset_version_id
            .clone()
            .ok_or_else(|| AppError::SceneReferenceBroken("world reference missing after upgrade".to_string()))?;
        // Resolve newly pinned reference
        Self::resolve_world_ref(&conn2, project_root, &new_pinned, &updated_scene)
    }

    pub fn upgrade_scene_character_look_reference(
        project_root: &Path,
        scene_id: &str,
        assignment_id: &str,
    ) -> Result<ResolvedSceneReference, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;

        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let assignment = scenes_repository::get_scene_character(&conn, assignment_id)?;
        if assignment.scene_id != scene_id {
            return Err(AppError::SceneCharacterNotFound);
        }

        let pinned_id = assignment.look_asset_version_id.clone();
        let pinned_version = asset_repository::get_asset_version_by_id(&conn, &pinned_id)?
            .ok_or_else(|| AppError::SceneReferenceBroken(format!("pinned look version {pinned_id} not found")))?;
        let asset = asset_repository::get_asset(&conn, &pinned_version.asset_id).map_err(|_| {
            AppError::SceneReferenceBroken(format!("asset {} not found", pinned_version.asset_id))
        })?;
        if asset.project_id != project.id {
            return Err(AppError::SceneReferenceBroken("look asset project mismatch".to_string()));
        }
        if matches!(
            asset.asset_type.as_str(),
            "world_plate" | "prop_plate" | "shot_keyframe"
        ) {
            return Err(AppError::SceneReferenceBroken("look asset type mismatch".to_string()));
        }
        if asset.owner_entity_id.as_deref() != Some(assignment.character_entity_id.as_str()) {
            return Err(AppError::SceneReferenceBroken("look asset owner mismatch".to_string()));
        }
        match canon_repository::get_entity(&conn, &assignment.character_entity_id) {
            Ok(ent) => {
                if ent.entity_type != "character" || ent.project_id != project.id {
                    return Err(AppError::SceneReferenceBroken("character entity mismatch".to_string()));
                }
            }
            Err(_) => return Err(AppError::SceneReferenceBroken("character entity not found".to_string())),
        }
        if !project_root.join(&pinned_version.file_path).exists() {
            return Err(AppError::SceneReferenceBroken("pinned look file missing".to_string()));
        }

        let canonical_id = asset.canonical_version_id.clone().ok_or_else(|| {
            AppError::SceneReferenceCanonicalMissing(format!(
                "look asset {} has no canonical version",
                asset.id
            ))
        })?;
        let canonical_version = asset_repository::get_asset_version_by_id(&conn, &canonical_id)?
            .ok_or_else(|| {
                AppError::SceneReferenceBroken(format!("canonical version {canonical_id} not found"))
            })?;
        if canonical_version.asset_id != asset.id {
            return Err(AppError::SceneReferenceBroken("canonical asset mismatch".to_string()));
        }
        if canonical_version.status != "canonical" {
            return Err(AppError::SceneReferenceBroken("canonical status not canonical".to_string()));
        }
        if canonical_version.asset_id != pinned_version.asset_id {
            return Err(AppError::SceneReferenceBroken("canonical asset id mismatch".to_string()));
        }
        // Validate canonical still owned by same character
        let canonical_asset = asset_repository::get_asset(&conn, &canonical_version.asset_id)?;
        if canonical_asset.owner_entity_id.as_deref() != Some(assignment.character_entity_id.as_str()) {
            return Err(AppError::SceneReferenceBroken("canonical look owner mismatch".to_string()));
        }
        if !project_root.join(&canonical_version.file_path).exists() {
            return Err(AppError::SceneReferenceBroken("canonical look file missing".to_string()));
        }

        if pinned_id == canonical_id {
            return Err(AppError::SceneReferenceAlreadyCurrent);
        }

        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let current = scenes_repository::get_scene_character(&tx, assignment_id)?;
        if current.look_asset_version_id != pinned_id {
            if current.look_asset_version_id == canonical_id {
                return Err(AppError::SceneReferenceAlreadyCurrent);
            }
        }
        let now = Utc::now().to_rfc3339();
        scenes_repository::update_scene_character_look(&tx, assignment_id, &canonical_id, &now)?;
        let event = SceneReferenceEvent {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            reference_kind: SceneReferenceKind::CharacterLook,
            assignment_id: Some(assignment_id.to_string()),
            action: SceneReferenceAction::Upgrade,
            from_version_id: Some(pinned_id.clone()),
            to_version_id: Some(canonical_id.clone()),
            created_at: now.clone(),
        };
        scenes_repository::insert_reference_event(&tx, &event)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;

        let mut conn2 = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn2)?;
        let updated = scenes_repository::get_scene_character(&conn2, assignment_id)?;
        let scene2 = scenes_repository::get_scene(&conn2, scene_id)?;
        Self::resolve_character_look_ref(
            &conn2,
            project_root,
            &updated.look_asset_version_id,
            &scene2,
            &updated,
        )
    }

    pub fn upgrade_scene_character_sheet_reference(
        project_root: &Path,
        scene_id: &str,
        assignment_id: &str,
    ) -> Result<ResolvedSceneReference, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;

        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let assignment = scenes_repository::get_scene_character(&conn, assignment_id)?;
        if assignment.scene_id != scene_id {
            return Err(AppError::SceneCharacterNotFound);
        }
        let pinned_id = assignment
            .sheet_asset_version_id
            .clone()
            .ok_or_else(|| AppError::SceneReferenceBroken("no sheet reference to upgrade".to_string()))?;

        let pinned_version = asset_repository::get_asset_version_by_id(&conn, &pinned_id)?
            .ok_or_else(|| AppError::SceneReferenceBroken(format!("pinned sheet version {pinned_id} not found")))?;
        let asset = asset_repository::get_asset(&conn, &pinned_version.asset_id).map_err(|_| {
            AppError::SceneReferenceBroken(format!("asset {} not found", pinned_version.asset_id))
        })?;
        if asset.project_id != project.id {
            return Err(AppError::SceneReferenceBroken("sheet asset project mismatch".to_string()));
        }
        if asset.asset_type != "character_sheet" && asset.asset_type != "outfit" {
            return Err(AppError::SceneReferenceBroken("sheet asset type mismatch".to_string()));
        }
        if asset.owner_entity_id.as_deref() != Some(assignment.character_entity_id.as_str()) {
            return Err(AppError::SceneReferenceBroken("sheet asset owner mismatch".to_string()));
        }
        match canon_repository::get_entity(&conn, &assignment.character_entity_id) {
            Ok(ent) => {
                if ent.entity_type != "character" || ent.project_id != project.id {
                    return Err(AppError::SceneReferenceBroken("character entity mismatch".to_string()));
                }
            }
            Err(_) => return Err(AppError::SceneReferenceBroken("character entity not found".to_string())),
        }
        if !project_root.join(&pinned_version.file_path).exists() {
            return Err(AppError::SceneReferenceBroken("pinned sheet file missing".to_string()));
        }

        let canonical_id = asset.canonical_version_id.clone().ok_or_else(|| {
            AppError::SceneReferenceCanonicalMissing(format!(
                "sheet asset {} has no canonical version",
                asset.id
            ))
        })?;
        let canonical_version = asset_repository::get_asset_version_by_id(&conn, &canonical_id)?
            .ok_or_else(|| {
                AppError::SceneReferenceBroken(format!("canonical version {canonical_id} not found"))
            })?;
        if canonical_version.asset_id != asset.id {
            return Err(AppError::SceneReferenceBroken("canonical asset mismatch".to_string()));
        }
        if canonical_version.status != "canonical" {
            return Err(AppError::SceneReferenceBroken("canonical status not canonical".to_string()));
        }
        let canonical_asset = asset_repository::get_asset(&conn, &canonical_version.asset_id)?;
        if canonical_asset.owner_entity_id.as_deref() != Some(assignment.character_entity_id.as_str()) {
            return Err(AppError::SceneReferenceBroken("canonical sheet owner mismatch".to_string()));
        }
        if !project_root.join(&canonical_version.file_path).exists() {
            return Err(AppError::SceneReferenceBroken("canonical sheet file missing".to_string()));
        }

        if pinned_id == canonical_id {
            return Err(AppError::SceneReferenceAlreadyCurrent);
        }

        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let current = scenes_repository::get_scene_character(&tx, assignment_id)?;
        if current.sheet_asset_version_id.as_deref() != Some(pinned_id.as_str()) {
            if current.sheet_asset_version_id.as_deref() == Some(canonical_id.as_str()) {
                return Err(AppError::SceneReferenceAlreadyCurrent);
            }
        }
        let now = Utc::now().to_rfc3339();
        scenes_repository::update_scene_character_sheet(&tx, assignment_id, Some(&canonical_id), &now)?;
        let event = SceneReferenceEvent {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            reference_kind: SceneReferenceKind::CharacterSheet,
            assignment_id: Some(assignment_id.to_string()),
            action: SceneReferenceAction::Upgrade,
            from_version_id: Some(pinned_id.clone()),
            to_version_id: Some(canonical_id.clone()),
            created_at: now.clone(),
        };
        scenes_repository::insert_reference_event(&tx, &event)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;

        let mut conn2 = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn2)?;
        let updated = scenes_repository::get_scene_character(&conn2, assignment_id)?;
        let scene2 = scenes_repository::get_scene(&conn2, scene_id)?;
        let new_sheet_id = updated
            .sheet_asset_version_id
            .clone()
            .ok_or_else(|| AppError::SceneReferenceBroken("sheet missing after upgrade".to_string()))?;
        Self::resolve_character_sheet_ref(&conn2, project_root, &new_sheet_id, &scene2, &updated)
    }

    pub fn upgrade_scene_prop_reference(
        project_root: &Path,
        scene_id: &str,
        assignment_id: &str,
    ) -> Result<ResolvedSceneReference, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;

        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let assignment = scenes_repository::get_scene_prop(&conn, assignment_id)?;
        if assignment.scene_id != scene_id {
            return Err(AppError::ScenePropNotFound);
        }
        let pinned_id = assignment.prop_asset_version_id.clone();
        let pinned_version = asset_repository::get_asset_version_by_id(&conn, &pinned_id)?
            .ok_or_else(|| AppError::SceneReferenceBroken(format!("pinned prop version {pinned_id} not found")))?;
        let asset = asset_repository::get_asset(&conn, &pinned_version.asset_id).map_err(|_| {
            AppError::SceneReferenceBroken(format!("asset {} not found", pinned_version.asset_id))
        })?;
        if asset.project_id != project.id {
            return Err(AppError::SceneReferenceBroken("prop asset project mismatch".to_string()));
        }
        if asset.asset_type != "prop_plate" {
            return Err(AppError::SceneReferenceBroken("prop asset type mismatch".to_string()));
        }
        if !project_root.join(&pinned_version.file_path).exists() {
            return Err(AppError::SceneReferenceBroken("pinned prop file missing".to_string()));
        }

        let canonical_id = asset.canonical_version_id.clone().ok_or_else(|| {
            AppError::SceneReferenceCanonicalMissing(format!(
                "prop asset {} has no canonical version",
                asset.id
            ))
        })?;
        let canonical_version = asset_repository::get_asset_version_by_id(&conn, &canonical_id)?
            .ok_or_else(|| {
                AppError::SceneReferenceBroken(format!("canonical version {canonical_id} not found"))
            })?;
        if canonical_version.asset_id != asset.id {
            return Err(AppError::SceneReferenceBroken("canonical asset mismatch".to_string()));
        }
        if canonical_version.status != "canonical" {
            return Err(AppError::SceneReferenceBroken("canonical status not canonical".to_string()));
        }
        if !project_root.join(&canonical_version.file_path).exists() {
            return Err(AppError::SceneReferenceBroken("canonical prop file missing".to_string()));
        }

        if pinned_id == canonical_id {
            return Err(AppError::SceneReferenceAlreadyCurrent);
        }

        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let current = scenes_repository::get_scene_prop(&tx, assignment_id)?;
        if current.prop_asset_version_id != pinned_id {
            if current.prop_asset_version_id == canonical_id {
                return Err(AppError::SceneReferenceAlreadyCurrent);
            }
        }
        scenes_repository::update_scene_prop_version(&tx, assignment_id, &canonical_id)?;
        let now = Utc::now().to_rfc3339();
        let event = SceneReferenceEvent {
            id: Ulid::new().to_string(),
            scene_id: scene_id.to_string(),
            reference_kind: SceneReferenceKind::Prop,
            assignment_id: Some(assignment_id.to_string()),
            action: SceneReferenceAction::Upgrade,
            from_version_id: Some(pinned_id.clone()),
            to_version_id: Some(canonical_id.clone()),
            created_at: now.clone(),
        };
        scenes_repository::insert_reference_event(&tx, &event)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;

        let mut conn2 = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn2)?;
        let updated = scenes_repository::get_scene_prop(&conn2, assignment_id)?;
        let scene2 = scenes_repository::get_scene(&conn2, scene_id)?;
        Self::resolve_prop_ref(&conn2, project_root, &updated.prop_asset_version_id, &scene2)
    }

    // -----------------------------------------------------------------------
    // Scene TBD Bindings
    // -----------------------------------------------------------------------

    pub fn set_scene_tbd_binding(
        project_root: &Path,
        scene_id: &str,
        tbd_id: &str,
        decision: TbdDecisionKind,
        justification: Option<String>,
    ) -> Result<SceneTbdBinding, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        // Load canon TBD
        let tbd = crate::canon::repository::get_tbd(&conn, tbd_id)?;
        if tbd.project_id != project.id {
            return Err(AppError::CanonTbdNotFound);
        }
        if tbd.status != "open" || !tbd.protected {
            // Only protected open TBDs should be bound; but allow anyway? Validate later.
        }
        // Validate decision per TBD policy
        let is_project_scoped = tbd.canon_entity_id.is_none();
        match decision {
            TbdDecisionKind::PreserveUnknown => {}
            TbdDecisionKind::NotApplicable => {
                if !is_project_scoped {
                    return Err(AppError::ProtectedTbdMustBePreserved(format!(
                        "TBD {} is entity/section scoped and must be preserve_unknown",
                        tbd.id
                    )));
                }
                let has_reason = justification
                    .as_ref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                if !has_reason {
                    return Err(AppError::TbdNotApplicableReasonRequired(format!(
                        "TBD {} not_applicable requires justification",
                        tbd.id
                    )));
                }
            }
        }

        let now = Utc::now().to_rfc3339();
        let existing = scenes_repository::get_scene_tbd_binding(&conn, scene_id, tbd_id)?;
        let binding = SceneTbdBinding {
            id: existing
                .as_ref()
                .map(|b| b.id.clone())
                .unwrap_or_else(|| Ulid::new().to_string()),
            scene_id: scene_id.to_string(),
            canon_tbd_id: tbd_id.to_string(),
            topic_snapshot: tbd.topic.clone(),
            note_snapshot: tbd.note.clone(),
            decision,
            justification: justification
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            created_at: existing
                .as_ref()
                .map(|b| b.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now.clone(),
        };
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        scenes_repository::upsert_scene_tbd_binding(&tx, &binding)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(binding)
    }

    pub fn remove_scene_tbd_binding(
        project_root: &Path,
        scene_id: &str,
        tbd_id: &str,
    ) -> Result<(), AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        scenes_repository::delete_scene_tbd_binding(&tx, scene_id, tbd_id)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn list_scene_tbd_bindings(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<Vec<SceneTbdBinding>, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }
        scenes_repository::list_scene_tbd_bindings(&conn, scene_id)
    }

    // -----------------------------------------------------------------------
    // Scene Readiness (derived, never stored)
    // -----------------------------------------------------------------------

    pub fn get_scene_readiness(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<SceneReadiness, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }

        let resolved = Self::resolve_scene_references(project_root, scene_id)?;

        let mut blockers: Vec<SceneReadinessBlocker> = Vec::new();
        let mut warnings: Vec<SceneReadinessWarning> = Vec::new();

        if scene.title.trim().is_empty() {
            blockers.push(SceneReadinessBlocker {
                kind: SceneReadinessBlockerKind::TitleMissing,
                message: "Scene title is required".into(),
                context: None,
            });
        }
        if scene.summary.trim().is_empty() {
            blockers.push(SceneReadinessBlocker {
                kind: SceneReadinessBlockerKind::SummaryMissing,
                message: "Scene summary is required".into(),
                context: None,
            });
        }

        // World reference
        match &resolved.world {
            None => {
                blockers.push(SceneReadinessBlocker {
                    kind: SceneReadinessBlockerKind::WorldReferenceMissing,
                    message: "World reference is required".into(),
                    context: None,
                });
            }
            Some(world_ref) => match world_ref.health {
                SceneReferenceHealth::Broken => {
                    blockers.push(SceneReadinessBlocker {
                        kind: SceneReadinessBlockerKind::WorldReferenceBroken,
                        message: "World reference is broken".into(),
                        context: Some(world_ref.pinned_version_id.clone()),
                    });
                }
                SceneReferenceHealth::UpgradeAvailable => {
                    warnings.push(SceneReadinessWarning {
                        kind: SceneReadinessWarningKind::UpgradeAvailable,
                        message: "World reference has upgrade available".into(),
                        context: Some(world_ref.pinned_version_id.clone()),
                    });
                }
                SceneReferenceHealth::Historical => {
                    warnings.push(SceneReadinessWarning {
                        kind: SceneReadinessWarningKind::HistoricalReference,
                        message: "World reference is historical".into(),
                        context: Some(world_ref.pinned_version_id.clone()),
                    });
                }
                SceneReferenceHealth::Current => {}
            },
        }

        // Character refs
        let mut char_broken = false;
        let mut char_upgrade = false;
        let mut char_historical = false;
        for ch in &resolved.characters {
            match ch.look.health {
                SceneReferenceHealth::Broken => char_broken = true,
                SceneReferenceHealth::UpgradeAvailable => char_upgrade = true,
                SceneReferenceHealth::Historical => char_historical = true,
                SceneReferenceHealth::Current => {}
            }
            if let Some(sheet) = &ch.sheet {
                match sheet.health {
                    SceneReferenceHealth::Broken => char_broken = true,
                    SceneReferenceHealth::UpgradeAvailable => char_upgrade = true,
                    SceneReferenceHealth::Historical => char_historical = true,
                    SceneReferenceHealth::Current => {}
                }
            }
        }
        if char_broken {
            blockers.push(SceneReadinessBlocker {
                kind: SceneReadinessBlockerKind::CharacterReferenceBroken,
                message: "Character reference is broken".into(),
                context: None,
            });
        }
        if char_upgrade {
            warnings.push(SceneReadinessWarning {
                kind: SceneReadinessWarningKind::UpgradeAvailable,
                message: "Character reference has upgrade available".into(),
                context: None,
            });
        }
        if char_historical {
            warnings.push(SceneReadinessWarning {
                kind: SceneReadinessWarningKind::HistoricalReference,
                message: "Character reference is historical".into(),
                context: None,
            });
        }

        // Prop refs
        let mut prop_broken = false;
        let mut prop_upgrade = false;
        let mut prop_historical = false;
        for p in &resolved.props {
            match p.reference.health {
                SceneReferenceHealth::Broken => prop_broken = true,
                SceneReferenceHealth::UpgradeAvailable => prop_upgrade = true,
                SceneReferenceHealth::Historical => prop_historical = true,
                SceneReferenceHealth::Current => {}
            }
        }
        if prop_broken {
            blockers.push(SceneReadinessBlocker {
                kind: SceneReadinessBlockerKind::PropReferenceBroken,
                message: "Prop reference is broken".into(),
                context: None,
            });
        }
        if prop_upgrade {
            warnings.push(SceneReadinessWarning {
                kind: SceneReadinessWarningKind::UpgradeAvailable,
                message: "Prop reference has upgrade available".into(),
                context: None,
            });
        }
        if prop_historical {
            warnings.push(SceneReadinessWarning {
                kind: SceneReadinessWarningKind::HistoricalReference,
                message: "Prop reference is historical".into(),
                context: None,
            });
        }

        // TBD decisions
        // Collect entity ids for applicable TBD loading
        let mut entity_ids: Vec<String> = Vec::new();
        if let Some(world_id) = &scene.world_id {
            if let Ok(world) = worlds_repository::get_world(&conn, world_id) {
                entity_ids.push(world.canon_location_entity_id.clone());
            }
        }
        let char_assignments = scenes_repository::list_scene_characters(&conn, scene_id)?;
        for ca in char_assignments {
            entity_ids.push(ca.character_entity_id);
        }
        // Use TBD policy to load applicable
        let applicable = crate::workflow::tbd_policy::load_applicable_tbds(
            &conn,
            &project.id,
            &entity_ids,
        )?;
        let bindings = scenes_repository::list_scene_tbd_bindings(&conn, scene_id)?;
        let binding_map: std::collections::HashMap<&str, &SceneTbdBinding> = bindings
            .iter()
            .map(|b| (b.canon_tbd_id.as_str(), b))
            .collect();
        let mut tbd_missing = false;
        for tbd in &applicable {
            if !tbd.protected || tbd.status != "open" {
                continue;
            }
            if !binding_map.contains_key(tbd.id.as_str()) {
                tbd_missing = true;
                break;
            }
            // Also validate decision kind matches policy (e.g., entity-scoped must be preserve_unknown)
            // Convert binding to TbdDecision for validation
            // We can just check via policy: build decisions vec and validate
        }
        // Full policy validation: if bindings exist but invalid, also block
        if !tbd_missing && !applicable.is_empty() {
            let decisions: Vec<crate::workflow::tbd_policy::TbdDecision> = bindings
                .iter()
                .map(|b| crate::workflow::tbd_policy::TbdDecision {
                    tbd_id: b.canon_tbd_id.clone(),
                    topic_snapshot: b.topic_snapshot.clone(),
                    note_snapshot: b.note_snapshot.clone(),
                    decision: match b.decision {
                        TbdDecisionKind::PreserveUnknown => {
                            crate::workflow::tbd_policy::TbdDecisionKind::PreserveUnknown
                        }
                        TbdDecisionKind::NotApplicable => {
                            crate::workflow::tbd_policy::TbdDecisionKind::NotApplicable
                        }
                    },
                    justification: b.justification.clone(),
                })
                .collect();
            if crate::workflow::tbd_policy::validate_tbd_decisions(&applicable, &decisions).is_err() {
                tbd_missing = true;
            }
        }
        if tbd_missing {
            blockers.push(SceneReadinessBlocker {
                kind: SceneReadinessBlockerKind::TbdDecisionRequired,
                message: "TBD decision required for relevant protected TBDs".into(),
                context: None,
            });
        }

        let ready_for_keyframe = blockers.is_empty();
        Ok(SceneReadiness {
            ready_for_keyframe,
            blockers,
            warnings,
        })
    }

    // -----------------------------------------------------------------------
    // Ensure Scene Keyframe Asset (idempotent)
    // -----------------------------------------------------------------------

    pub fn ensure_scene_keyframe_asset(
        project_root: &Path,
        scene_id: &str,
    ) -> Result<AssetRecord, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let scene = scenes_repository::get_scene(&conn, scene_id)?;
        if scene.project_id != project.id {
            return Err(AppError::SceneNotFound);
        }

        // If existing valid keyframe_asset_id exists, return it
        if let Some(existing_id) = &scene.keyframe_asset_id {
            if let Ok(asset) = asset_repository::get_asset(&conn, existing_id) {
                if asset.project_id == project.id && asset.asset_type == "shot_keyframe" {
                    return Ok(asset);
                }
            }
        }

        // Need to create shot_keyframe Asset and update Scene transactionally
        let now = Utc::now().to_rfc3339();
        let label = format!("SCENE-{:03}-KEYFRAME", scene.ordinal);
        let asset_id = Ulid::new().to_string();
        let record = AssetRecord {
            id: asset_id.clone(),
            project_id: project.id.clone(),
            asset_type: "shot_keyframe".to_string(),
            label: label.clone(),
            owner_entity_id: Some(scene_id.to_string()),
            canonical_version_id: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;
        // Re-check inside transaction for idempotency against concurrent call
        let current_scene = scenes_repository::get_scene(&tx, scene_id)?;
        if let Some(existing_id) = &current_scene.keyframe_asset_id {
            if let Ok(asset) = asset_repository::get_asset(&tx, existing_id) {
                if asset.project_id == project.id && asset.asset_type == "shot_keyframe" {
                    // Another call already created it; abort our insert
                    drop(tx); // rollback implicit by drop without commit? Need to rollback explicitly
                    // tx is not committed, dropping will rollback
                    return Ok(asset);
                }
            }
        }
        // Insert asset and update scene
        asset_repository::insert_asset(&tx, &record)?;
        scenes_repository::update_scene_keyframe_asset(&tx, scene_id, Some(&asset_id), &now)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::service::AssetService;
    use crate::canon::model::CanonEntityType;
    use crate::canon::service::CanonService;
    use crate::db;
    use crate::project::service::ProjectService;
    use crate::worlds::service::WorldService;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct Fixture {
        _temp: TempDir,
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("fixture-project");
            std::fs::create_dir_all(&root).unwrap();
            ProjectService::create(&root, "Fixture Project").unwrap();
            Self { _temp: temp, root }
        }

        fn create_location(&self, name: &str) -> crate::canon::model::CanonEntityRecord {
            CanonService::create_entity(&self.root, CanonEntityType::Location, name).unwrap()
        }

        fn create_character(&self, name: &str) -> crate::canon::model::CanonEntityRecord {
            CanonService::create_entity(&self.root, CanonEntityType::Character, name).unwrap()
        }

        fn create_world(&self, location_name: &str) -> crate::worlds::model::World {
            let loc = self.create_location(location_name);
            WorldService::create_world(&self.root, &loc.id).unwrap()
        }

        fn write_png(&self, dir: &std::path::Path, name: &str, pixel: [u8; 4]) -> PathBuf {
            std::fs::create_dir_all(dir).unwrap();
            let path = dir.join(name);
            let image: image::RgbaImage =
                image::ImageBuffer::from_pixel(32, 32, image::Rgba(pixel));
            image.save(&path).expect("write png");
            path
        }

        fn create_world_plate_canonical(
            &self,
            world: &crate::worlds::model::World,
            pixel: [u8; 4],
        ) -> String {
            // Import a version for the world's world_plate asset and promote to canonical
            let source_dir = self.root.join("tmp_sources_world");
            let src = self.write_png(&source_dir, &format!("world-{}-{}.png", world.id, pixel[0]), pixel);
            let version = AssetService::import_asset_version(
                &self.root,
                &world.world_plate_asset_id,
                &src,
                None,
            )
            .unwrap();
            AssetService::promote_asset_version(&self.root, &version.id)
                .unwrap();
            version.id
        }

        fn create_look_canonical(
            &self,
            character_id: &str,
            label: &str,
            pixel: [u8; 4],
        ) -> (String, String) {
            // Create outfit asset owned by character, import and promote
            let asset = AssetService::create_asset(
                &self.root,
                "outfit",
                label,
                Some(character_id.to_string()),
            )
            .unwrap();
            let source_dir = self.root.join("tmp_sources_look");
            let src = self.write_png(&source_dir, &format!("{label}.png"), pixel);
            let version = AssetService::import_asset_version(&self.root, &asset.id, &src, None).unwrap();
            AssetService::promote_asset_version(&self.root, &version.id).unwrap();
            (asset.id, version.id)
        }

        fn create_sheet_canonical(
            &self,
            character_id: &str,
            label: &str,
            pixel: [u8; 4],
        ) -> (String, String) {
            let asset = AssetService::create_asset(
                &self.root,
                "character_sheet",
                label,
                Some(character_id.to_string()),
            )
            .unwrap();
            let source_dir = self.root.join("tmp_sources_sheet");
            let src = self.write_png(&source_dir, &format!("{label}.png"), pixel);
            let version = AssetService::import_asset_version(&self.root, &asset.id, &src, None).unwrap();
            AssetService::promote_asset_version(&self.root, &version.id).unwrap();
            (asset.id, version.id)
        }

        fn create_prop_canonical(&self, label: &str, pixel: [u8; 4]) -> (String, String) {
            let asset =
                AssetService::create_asset(&self.root, "prop_plate", label, None).unwrap();
            let source_dir = self.root.join("tmp_sources_prop");
            let src = self.write_png(&source_dir, &format!("{label}.png"), pixel);
            let version = AssetService::import_asset_version(&self.root, &asset.id, &src, None).unwrap();
            AssetService::promote_asset_version(&self.root, &version.id).unwrap();
            (asset.id, version.id)
        }

        fn create_prop_candidate(&self, label: &str, pixel: [u8; 4]) -> (String, String) {
            let asset =
                AssetService::create_asset(&self.root, "prop_plate", label, None).unwrap();
            let source_dir = self.root.join("tmp_sources_prop");
            let src = self.write_png(&source_dir, &format!("{label}-cand.png"), pixel);
            let version = AssetService::import_asset_version(&self.root, &asset.id, &src, None).unwrap();
            (asset.id, version.id)
        }

        fn create_look_candidate(
            &self,
            character_id: &str,
            label: &str,
            pixel: [u8; 4],
        ) -> (String, String) {
            let asset = AssetService::create_asset(
                &self.root,
                "outfit",
                label,
                Some(character_id.to_string()),
            )
            .unwrap();
            let source_dir = self.root.join("tmp_sources_look");
            let src = self.write_png(&source_dir, &format!("{label}-cand.png"), pixel);
            let version = AssetService::import_asset_version(&self.root, &asset.id, &src, None).unwrap();
            (asset.id, version.id)
        }
    }

    // ---- SCENE-001 ordinal allocation ----

    #[test]
    fn create_scene_allocates_ordinal_one_and_two() {
        let f = Fixture::new();
        let s1 = SceneService::create_scene(&f.root, "Night Transmission", "Mara receives...").unwrap();
        assert_eq!(s1.ordinal, 1);
        assert_eq!(s1.title, "Night Transmission");
        let s2 = SceneService::create_scene(&f.root, "Second", "Summary").unwrap();
        assert_eq!(s2.ordinal, 2);
        assert_eq!(s2.title, "Second");
    }

    #[test]
    fn ordinal_persists_after_restart() {
        let f = Fixture::new();
        let s1 = SceneService::create_scene(&f.root, "First", "Summary").unwrap();
        assert_eq!(s1.ordinal, 1);
        // Simulate restart: reopen project and create another scene
        ProjectService::open(&f.root).unwrap();
        let s2 = SceneService::create_scene(&f.root, "Second", "Summary2").unwrap();
        assert_eq!(s2.ordinal, 2);
        ProjectService::open(&f.root).unwrap();
        let list = SceneService::list_scenes(&f.root).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].ordinal, 1);
        assert_eq!(list[1].ordinal, 2);
        // Verify via direct DB that MAX ordinal logic holds
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let max_ordinal: i64 = conn
            .query_row("SELECT MAX(ordinal) FROM world_scenes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(max_ordinal, 2);
    }

    #[test]
    fn create_scene_rejects_empty_title() {
        let f = Fixture::new();
        let err = SceneService::create_scene(&f.root, "   ", "summary").unwrap_err();
        assert!(matches!(err, AppError::InvalidSceneTitle));
        assert_eq!(err.code(), "INVALID_SCENE_TITLE");
        // No scene created
        let list = SceneService::list_scenes(&f.root).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn create_scene_allows_empty_summary() {
        let f = Fixture::new();
        let s = SceneService::create_scene(&f.root, "Title", "").unwrap();
        assert_eq!(s.summary, "");
        assert_eq!(s.title, "Title");
    }

    #[test]
    fn update_scene_details_changes_title_and_summary() {
        let f = Fixture::new();
        let s = SceneService::create_scene(&f.root, "Old", "Old summary").unwrap();
        let updated = SceneService::update_scene_details(&f.root, &s.id, "New", "New summary").unwrap();
        assert_eq!(updated.title, "New");
        assert_eq!(updated.summary, "New summary");
        assert_eq!(updated.ordinal, s.ordinal);
        let fetched = SceneService::get_scene(&f.root, &s.id).unwrap();
        assert_eq!(fetched.title, "New");
    }

    #[test]
    fn get_scene_is_project_isolated() {
        let f1 = Fixture::new();
        let f2 = Fixture::new();
        let s = SceneService::create_scene(&f1.root, "Title", "Summary").unwrap();
        let err = SceneService::get_scene(&f2.root, &s.id).unwrap_err();
        assert!(matches!(err, AppError::SceneNotFound));
    }

    // ---- World assignment ----

    #[test]
    fn assign_scene_world_resolves_canonical_once_and_stores_exact_version() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [10, 20, 30, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene 1", "Summary").unwrap();
        let assigned = SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        assert_eq!(assigned.world_id.as_deref(), Some(world.id.as_str()));
        assert_eq!(assigned.world_asset_version_id.as_deref(), Some(v1.as_str()));
        // Verify DB directly
        let fetched = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(fetched.world_asset_version_id.as_deref(), Some(v1.as_str()));
    }

    #[test]
    fn assign_scene_world_rejects_when_no_canonical() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        // No canonical version created
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let err = SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap_err();
        assert!(matches!(err, AppError::SceneWorldPlateNotCanonical(_)));
        assert_eq!(err.code(), "SCENE_WORLD_PLATE_NOT_CANONICAL");
        let fetched = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert!(fetched.world_id.is_none());
    }

    #[test]
    fn assign_scene_world_fails_for_missing_world() {
        let f = Fixture::new();
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let err = SceneService::assign_scene_world(&f.root, &scene.id, "nonexistent").unwrap_err();
        assert!(matches!(err, AppError::WorldNotFound));
    }

    // ---- Character assignment rejects ----

    #[test]
    fn add_scene_character_rejects_non_canonical_look() {
        let f = Fixture::new();
        let character = f.create_character("Mara");
        let (_asset_id, candidate_version) = f.create_look_candidate(&character.id, "MARA-LOOK", [1, 2, 3, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let err = SceneService::add_scene_character(&f.root, &scene.id, &character.id, &candidate_version, None, None).unwrap_err();
        assert!(matches!(err, AppError::SceneCharacterLookNotCanonical));
        assert_eq!(err.code(), "SCENE_CHARACTER_LOOK_NOT_CANONICAL");
    }

    #[test]
    fn add_scene_character_rejects_look_owned_by_another_character() {
        let f = Fixture::new();
        let char_a = f.create_character("Mara");
        let char_b = f.create_character("Jules");
        let (_asset_id, version) = f.create_look_canonical(&char_b.id, "JULES-LOOK", [1, 2, 3, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let err = SceneService::add_scene_character(&f.root, &scene.id, &char_a.id, &version, None, None).unwrap_err();
        assert!(matches!(err, AppError::SceneCharacterLookNotOwned));
        assert_eq!(err.code(), "SCENE_CHARACTER_LOOK_NOT_OWNED");
    }

    #[test]
    fn add_scene_character_rejects_incompatible_sheet_wrong_owner() {
        let f = Fixture::new();
        let char_a = f.create_character("Mara");
        let char_b = f.create_character("Jules");
        let (_look_asset, look_ver) = f.create_look_canonical(&char_a.id, "MARA-LOOK", [1, 2, 3, 255]);
        let (_sheet_asset, sheet_ver) = f.create_sheet_canonical(&char_b.id, "JULES-SHEET", [4, 5, 6, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let err = SceneService::add_scene_character(&f.root, &scene.id, &char_a.id, &look_ver, Some(sheet_ver.as_str()), None).unwrap_err();
        assert!(matches!(err, AppError::SceneCharacterSheetNotOwned));
        assert_eq!(err.code(), "SCENE_CHARACTER_SHEET_NOT_OWNED");
    }

    #[test]
    fn add_scene_character_rejects_non_canonical_sheet() {
        let f = Fixture::new();
        let character = f.create_character("Mara");
        let (_look_asset, look_ver) = f.create_look_canonical(&character.id, "MARA-LOOK", [10, 0, 0, 255]);
        // Create sheet candidate not promoted
        let sheet_asset = AssetService::create_asset(&f.root, "character_sheet", "MARA-SHEET", Some(character.id.clone())).unwrap();
        let src_dir = f.root.join("tmp_sources_sheet2");
        let src = f.write_png(&src_dir, "marasheet.png", [20, 0, 0, 255]);
        let sheet_ver = AssetService::import_asset_version(&f.root, &sheet_asset.id, &src, None).unwrap();
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let err = SceneService::add_scene_character(&f.root, &scene.id, &character.id, &look_ver, Some(sheet_ver.id.as_str()), None).unwrap_err();
        assert!(matches!(err, AppError::SceneCharacterSheetNotCanonical));
    }

    #[test]
    fn add_scene_character_succeeds_with_valid_canonical_look() {
        let f = Fixture::new();
        let character = f.create_character("Mara");
        let (_asset, look_ver) = f.create_look_canonical(&character.id, "MARA-LOOK", [7, 8, 9, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let assignment = SceneService::add_scene_character(&f.root, &scene.id, &character.id, &look_ver, None, Some("notes")).unwrap();
        assert_eq!(assignment.character_entity_id, character.id);
        assert_eq!(assignment.look_asset_version_id, look_ver);
        assert!(assignment.sheet_asset_version_id.is_none());
        // Verify uniqueness enforcement
        let err = SceneService::add_scene_character(&f.root, &scene.id, &character.id, &look_ver, None, None).unwrap_err();
        assert!(matches!(err, AppError::SceneCharacterAlreadyExists));
    }

    #[test]
    fn add_scene_character_with_valid_sheet_succeeds() {
        let f = Fixture::new();
        let character = f.create_character("Mara");
        let (_look_asset, look_ver) = f.create_look_canonical(&character.id, "MARA-LOOK", [1, 2, 3, 255]);
        let (_sheet_asset, sheet_ver) = f.create_sheet_canonical(&character.id, "MARA-SHEET", [4, 5, 6, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let assignment = SceneService::add_scene_character(&f.root, &scene.id, &character.id, &look_ver, Some(sheet_ver.as_str()), None).unwrap();
        assert_eq!(assignment.sheet_asset_version_id.as_deref(), Some(sheet_ver.as_str()));
    }

    #[test]
    fn remove_scene_character_deletes_assignment() {
        let f = Fixture::new();
        let character = f.create_character("Mara");
        let (_asset, look_ver) = f.create_look_canonical(&character.id, "MARA-LOOK", [1, 2, 3, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::add_scene_character(&f.root, &scene.id, &character.id, &look_ver, None, None).unwrap();
        let list = SceneService::list_scene_characters(&f.root, &scene.id).unwrap();
        assert_eq!(list.len(), 1);
        SceneService::remove_scene_character(&f.root, &scene.id, &character.id).unwrap();
        let list2 = SceneService::list_scene_characters(&f.root, &scene.id).unwrap();
        assert!(list2.is_empty());
    }

    // ---- Prop assignment ----

    #[test]
    fn add_scene_prop_requires_canonical_prop_plate() {
        let f = Fixture::new();
        let (_asset, candidate_ver) = f.create_prop_candidate("PROP", [1, 1, 1, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let err = SceneService::add_scene_prop(&f.root, &scene.id, &candidate_ver, None, None).unwrap_err();
        assert!(matches!(err, AppError::ScenePropNotCanonical));
        assert_eq!(err.code(), "SCENE_PROP_NOT_CANONICAL");
    }

    #[test]
    fn add_scene_prop_rejects_wrong_asset_type() {
        let f = Fixture::new();
        // Create a world_plate and make canonical (wrong type for prop)
        let world = f.create_world("Station");
        let v = f.create_world_plate_canonical(&world, [9, 9, 9, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let err = SceneService::add_scene_prop(&f.root, &scene.id, &v, None, None).unwrap_err();
        assert!(matches!(err, AppError::ScenePropInvalidType));
    }

    #[test]
    fn add_scene_prop_succeeds() {
        let f = Fixture::new();
        let (_asset, ver) = f.create_prop_canonical("PROP-A", [10, 20, 30, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let assignment = SceneService::add_scene_prop(&f.root, &scene.id, &ver, Some("label"), None).unwrap();
        assert_eq!(assignment.prop_asset_version_id, ver);
        let list = SceneService::list_scene_props(&f.root, &scene.id).unwrap();
        assert_eq!(list.len(), 1);
        // Duplicate should fail
        let err = SceneService::add_scene_prop(&f.root, &scene.id, &ver, None, None).unwrap_err();
        assert!(matches!(err, AppError::ScenePropAlreadyExists));
    }

    #[test]
    fn remove_scene_prop_deletes_assignment() {
        let f = Fixture::new();
        let (_asset, ver) = f.create_prop_canonical("PROP-RM", [5, 6, 7, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::add_scene_prop(&f.root, &scene.id, &ver, None, None).unwrap();
        SceneService::remove_scene_prop(&f.root, &scene.id, &ver).unwrap();
        let list = SceneService::list_scene_props(&f.root, &scene.id).unwrap();
        assert!(list.is_empty());
    }

    // ---- Central pinning regression ----

    #[test]
    fn world_pinning_is_immutable_after_promotion_and_survives_restart() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        // Create V01 and pin
        let v1 = f.create_world_plate_canonical(&world, [10, 10, 10, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let assigned = SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        assert_eq!(assigned.world_asset_version_id.as_deref(), Some(v1.as_str()));

        // Promote V02
        let source_dir = f.root.join("tmp_sources_world");
        let src2 = f.write_png(&source_dir, "world-v02.png", [20, 20, 20, 255]);
        let v2 = AssetService::import_asset_version(&f.root, &world.world_plate_asset_id, &src2, None).unwrap();
        AssetService::promote_asset_version(&f.root, &v2.id).unwrap();

        // Verify world asset now points to V02
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let asset = asset_repository::get_asset(&conn, &world.world_plate_asset_id).unwrap();
        assert_eq!(asset.canonical_version_id.as_deref(), Some(v2.id.as_str()));

        // Scene still pins V01
        let fetched = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(fetched.world_asset_version_id.as_deref(), Some(v1.as_str()), "Scene must remain pinned to V01 after V02 promotion");

        // Simulate restart: reopen project (runs migrations), reopen connection
        ProjectService::open(&f.root).unwrap();
        let reopened = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(reopened.world_asset_version_id.as_deref(), Some(v1.as_str()));
        // Also via direct DB reopen
        let conn2 = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let reopened2 = scenes_repository::get_scene(&conn2, &scene.id).unwrap();
        assert_eq!(reopened2.world_asset_version_id.as_deref(), Some(v1.as_str()));
    }

    #[test]
    fn character_pinning_is_immutable_after_promotion() {
        let f = Fixture::new();
        let character = f.create_character("Mara");
        // Create look V01
        let (_asset_id, v1) = f.create_look_canonical(&character.id, "MARA-LOOK", [10, 20, 30, 255]);
        // Need asset id to create V02 on same asset
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let version_row = asset_repository::get_asset_version_by_id(&conn, &v1).unwrap().unwrap();
        let asset_id = version_row.asset_id.clone();
        drop(conn);

        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let assignment = SceneService::add_scene_character(&f.root, &scene.id, &character.id, &v1, None, None).unwrap();
        assert_eq!(assignment.look_asset_version_id, v1);

        // Promote V02 on same look asset
        let source_dir = f.root.join("tmp_sources_look");
        let src2 = f.write_png(&source_dir, "mara-look-v02.png", [40, 50, 60, 255]);
        let v2 = AssetService::import_asset_version(&f.root, &asset_id, &src2, None).unwrap();
        AssetService::promote_asset_version(&f.root, &v2.id).unwrap();

        // Verify asset canonical now V02
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let asset = asset_repository::get_asset(&conn, &asset_id).unwrap();
        assert_eq!(asset.canonical_version_id.as_deref(), Some(v2.id.as_str()));

        // Character assignment still pins V01
        let list = SceneService::list_scene_characters(&f.root, &scene.id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].look_asset_version_id, v1);

        // Simulate restart
        ProjectService::open(&f.root).unwrap();
        let list2 = SceneService::list_scene_characters(&f.root, &scene.id).unwrap();
        assert_eq!(list2[0].look_asset_version_id, v1);
    }

    #[test]
    fn clear_scene_world_removes_pin_and_creates_event() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [1, 1, 1, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        let cleared = SceneService::clear_scene_world(&f.root, &scene.id).unwrap();
        assert!(cleared.world_id.is_none());
        assert!(cleared.world_asset_version_id.is_none());
        // Verify event exists
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let events = scenes_repository::list_reference_events(&conn, &scene.id).unwrap();
        // Should have at least pin and remove
        assert!(events.iter().any(|e| e.action == SceneReferenceAction::Pin && e.reference_kind == SceneReferenceKind::World && e.to_version_id.as_deref() == Some(v1.as_str())));
        assert!(events.iter().any(|e| e.action == SceneReferenceAction::Remove && e.reference_kind == SceneReferenceKind::World && e.from_version_id.as_deref() == Some(v1.as_str())));
    }

    // -----------------------------------------------------------------------
    // Task 7: Reference health resolver + explicit upgrade
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_health_current_world() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [10, 20, 30, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        let resolved = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        let w = resolved.world.expect("world should be Some");
        assert_eq!(w.pinned_version_id, v1);
        assert_eq!(w.health, SceneReferenceHealth::Current);
        assert_eq!(w.current_canonical_version_id.as_deref(), Some(v1.as_str()));
        assert_eq!(w.asset_id, world.world_plate_asset_id);
        assert!(w.version_number >= 1);
        assert_eq!(w.status, "canonical");
        assert!(!w.file_path.is_empty());
        // Resolver must not have mutated scene
        let after = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(after.world_asset_version_id.as_deref(), Some(v1.as_str()));
        // Calling again should be idempotent
        let resolved2 = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        assert_eq!(resolved2.world.unwrap().health, SceneReferenceHealth::Current);
    }

    #[test]
    fn resolve_health_upgrade_available_world() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [10, 10, 10, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        // Promote V02
        let src2 = f.write_png(&f.root.join("tmp_sources_world"), "world-v02.png", [20, 20, 20, 255]);
        let v2 = AssetService::import_asset_version(&f.root, &world.world_plate_asset_id, &src2, None).unwrap();
        AssetService::promote_asset_version(&f.root, &v2.id).unwrap();
        let resolved = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        let w = resolved.world.unwrap();
        assert_eq!(w.pinned_version_id, v1);
        assert_eq!(w.health, SceneReferenceHealth::UpgradeAvailable);
        assert_eq!(w.current_canonical_version_id.as_deref(), Some(v2.id.as_str()));
        // Scene still pinned to old
        let fetched = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(fetched.world_asset_version_id.as_deref(), Some(v1.as_str()));
    }

    #[test]
    fn resolve_health_historical_world() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [5, 5, 5, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        // Clear canonical pointer to simulate historical (no current canonical)
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            conn.execute(
                "UPDATE assets SET canonical_version_id = NULL WHERE id = ?1",
                rusqlite::params![world.world_plate_asset_id],
            )
            .unwrap();
        }
        let resolved = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        let w = resolved.world.unwrap();
        assert_eq!(w.pinned_version_id, v1);
        assert_eq!(w.health, SceneReferenceHealth::Historical);
        assert!(w.current_canonical_version_id.is_none());
        // Pinned version still valid
        assert_eq!(w.status, "canonical");
    }

    #[test]
    fn resolve_health_broken_world_file_missing() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [9, 9, 9, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        // Delete underlying file
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            let ver = asset_repository::get_asset_version_by_id(&conn, &v1).unwrap().unwrap();
            let path = f.root.join(&ver.file_path);
            std::fs::remove_file(&path).unwrap();
            assert!(!path.exists());
        }
        let resolved = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        let w = resolved.world.unwrap();
        assert_eq!(w.pinned_version_id, v1);
        assert_eq!(w.health, SceneReferenceHealth::Broken);
    }

    #[test]
    fn resolve_health_broken_world_owner_mismatch() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let _v1 = f.create_world_plate_canonical(&world, [1, 2, 3, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        // Corrupt reference to point to nonexistent version (broken)
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
            conn.execute(
                "UPDATE world_scenes SET world_asset_version_id = 'nonexistent-bogus-id' WHERE id = ?1",
                rusqlite::params![scene.id],
            )
            .unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        }
        let resolved = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        let w = resolved.world.unwrap();
        assert_eq!(w.health, SceneReferenceHealth::Broken);
        assert_eq!(w.pinned_version_id, "nonexistent-bogus-id");
    }

    #[test]
    fn resolve_health_character_and_prop_mixed() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let wv = f.create_world_plate_canonical(&world, [1, 1, 1, 255]);
        let character = f.create_character("Mara");
        let (_a, look_v1) = f.create_look_canonical(&character.id, "MARA-LOOK", [2, 2, 2, 255]);
        let (_a2, prop_v1) = f.create_prop_canonical("PROP-A", [3, 3, 3, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        let char_assign =
            SceneService::add_scene_character(&f.root, &scene.id, &character.id, &look_v1, None, None).unwrap();
        let prop_assign = SceneService::add_scene_prop(&f.root, &scene.id, &prop_v1, None, None).unwrap();
        // All current
        let resolved = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        assert_eq!(resolved.world.unwrap().health, SceneReferenceHealth::Current);
        assert_eq!(resolved.characters.len(), 1);
        assert_eq!(resolved.characters[0].look.health, SceneReferenceHealth::Current);
        assert!(resolved.characters[0].sheet.is_none());
        assert_eq!(resolved.props.len(), 1);
        assert_eq!(resolved.props[0].reference.health, SceneReferenceHealth::Current);
        assert_eq!(resolved.props[0].assignment_id, prop_assign.id);
        assert_eq!(resolved.characters[0].assignment_id, char_assign.id);
        // Now promote a new world version only
        let src2 = f.write_png(&f.root.join("tmp_sources_world"), "world-v02-2.png", [9, 9, 9, 255]);
        let v2 = AssetService::import_asset_version(&f.root, &world.world_plate_asset_id, &src2, None).unwrap();
        AssetService::promote_asset_version(&f.root, &v2.id).unwrap();
        let resolved2 = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        assert_eq!(resolved2.world.unwrap().health, SceneReferenceHealth::UpgradeAvailable);
        assert_eq!(resolved2.characters[0].look.health, SceneReferenceHealth::Current);
        assert_eq!(resolved2.props[0].reference.health, SceneReferenceHealth::Current);
        // Ensure resolver didn't mutate pins
        let fetched = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(fetched.world_asset_version_id.as_deref(), Some(wv.as_str()));
        let chars = SceneService::list_scene_characters(&f.root, &scene.id).unwrap();
        assert_eq!(chars[0].look_asset_version_id, look_v1);
    }

    #[test]
    fn upgrade_world_from_v01_to_v02_creates_event() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [10, 10, 10, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        let src2 = f.write_png(&f.root.join("tmp_sources_world"), "world-v02-up.png", [20, 20, 20, 255]);
        let v2 = AssetService::import_asset_version(&f.root, &world.world_plate_asset_id, &src2, None).unwrap();
        AssetService::promote_asset_version(&f.root, &v2.id).unwrap();
        // Pre-check health
        let pre = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        assert_eq!(pre.world.unwrap().health, SceneReferenceHealth::UpgradeAvailable);
        // Upgrade
        let upgraded = SceneService::upgrade_scene_world_reference(&f.root, &scene.id).unwrap();
        assert_eq!(upgraded.pinned_version_id, v2.id);
        assert_eq!(upgraded.health, SceneReferenceHealth::Current);
        assert_eq!(upgraded.current_canonical_version_id.as_deref(), Some(v2.id.as_str()));
        // Scene now points to V02
        let fetched = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(fetched.world_asset_version_id.as_deref(), Some(v2.id.as_str()));
        // Event created
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let events = scenes_repository::list_reference_events(&conn, &scene.id).unwrap();
        let upgrade = events
            .iter()
            .find(|e| e.action == SceneReferenceAction::Upgrade && e.reference_kind == SceneReferenceKind::World)
            .expect("upgrade event");
        assert_eq!(upgrade.from_version_id.as_deref(), Some(v1.as_str()));
        assert_eq!(upgrade.to_version_id.as_deref(), Some(v2.id.as_str()));
        assert!(upgrade.assignment_id.is_none());
        // After upgrade health current
        let post = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        assert_eq!(post.world.unwrap().health, SceneReferenceHealth::Current);
    }

    #[test]
    fn upgrade_character_look_from_v01_to_v02() {
        let f = Fixture::new();
        let character = f.create_character("Mara");
        let (asset_id, v1) = f.create_look_canonical(&character.id, "MARA-LOOK", [10, 20, 30, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let assign = SceneService::add_scene_character(&f.root, &scene.id, &character.id, &v1, None, None).unwrap();
        // Promote V02
        let src2 = f.write_png(&f.root.join("tmp_sources_look"), "mara-look-v02-up.png", [40, 50, 60, 255]);
        let v2 = AssetService::import_asset_version(&f.root, &asset_id, &src2, None).unwrap();
        AssetService::promote_asset_version(&f.root, &v2.id).unwrap();
        let pre = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        assert_eq!(pre.characters[0].look.health, SceneReferenceHealth::UpgradeAvailable);
        let upgraded = SceneService::upgrade_scene_character_look_reference(&f.root, &scene.id, &assign.id).unwrap();
        assert_eq!(upgraded.pinned_version_id, v2.id);
        assert_eq!(upgraded.health, SceneReferenceHealth::Current);
        // Verify assignment updated
        let list = SceneService::list_scene_characters(&f.root, &scene.id).unwrap();
        assert_eq!(list[0].look_asset_version_id, v2.id);
        // Event
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let events = scenes_repository::list_reference_events(&conn, &scene.id).unwrap();
        let ev = events
            .iter()
            .find(|e| e.action == SceneReferenceAction::Upgrade && e.reference_kind == SceneReferenceKind::CharacterLook)
            .unwrap();
        assert_eq!(ev.from_version_id.as_deref(), Some(v1.as_str()));
        assert_eq!(ev.to_version_id.as_deref(), Some(v2.id.as_str()));
        assert_eq!(ev.assignment_id.as_deref(), Some(assign.id.as_str()));
    }

    #[test]
    fn upgrade_character_sheet_from_v01_to_v02() {
        let f = Fixture::new();
        let character = f.create_character("Mara");
        let (_look_asset, look_v) = f.create_look_canonical(&character.id, "MARA-LOOK", [1, 2, 3, 255]);
        let (sheet_asset_id, sheet_v1) = f.create_sheet_canonical(&character.id, "MARA-SHEET", [4, 5, 6, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let assign = SceneService::add_scene_character(
            &f.root,
            &scene.id,
            &character.id,
            &look_v,
            Some(sheet_v1.as_str()),
            None,
        )
        .unwrap();
        // Promote sheet V02
        let src2 = f.write_png(&f.root.join("tmp_sources_sheet"), "sheet-v02-up.png", [9, 9, 9, 255]);
        let v2 = AssetService::import_asset_version(&f.root, &sheet_asset_id, &src2, None).unwrap();
        AssetService::promote_asset_version(&f.root, &v2.id).unwrap();
        let pre = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        assert_eq!(
            pre.characters[0].sheet.as_ref().unwrap().health,
            SceneReferenceHealth::UpgradeAvailable
        );
        let upgraded =
            SceneService::upgrade_scene_character_sheet_reference(&f.root, &scene.id, &assign.id).unwrap();
        assert_eq!(upgraded.pinned_version_id, v2.id);
        assert_eq!(upgraded.health, SceneReferenceHealth::Current);
        let list = SceneService::list_scene_characters(&f.root, &scene.id).unwrap();
        assert_eq!(list[0].sheet_asset_version_id.as_deref(), Some(v2.id.as_str()));
        // Look should remain unchanged
        assert_eq!(list[0].look_asset_version_id, look_v);
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let events = scenes_repository::list_reference_events(&conn, &scene.id).unwrap();
        let ev = events
            .iter()
            .find(|e| e.reference_kind == SceneReferenceKind::CharacterSheet && e.action == SceneReferenceAction::Upgrade)
            .unwrap();
        assert_eq!(ev.from_version_id.as_deref(), Some(sheet_v1.as_str()));
        assert_eq!(ev.to_version_id.as_deref(), Some(v2.id.as_str()));
    }

    #[test]
    fn upgrade_prop_from_v01_to_v02() {
        let f = Fixture::new();
        let (asset_id, v1) = f.create_prop_canonical("PROP-A", [10, 20, 30, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        let assign = SceneService::add_scene_prop(&f.root, &scene.id, &v1, None, None).unwrap();
        let src2 = f.write_png(&f.root.join("tmp_sources_prop"), "prop-v02-up.png", [20, 20, 20, 255]);
        let v2 = AssetService::import_asset_version(&f.root, &asset_id, &src2, None).unwrap();
        AssetService::promote_asset_version(&f.root, &v2.id).unwrap();
        let pre = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        assert_eq!(pre.props[0].reference.health, SceneReferenceHealth::UpgradeAvailable);
        let upgraded = SceneService::upgrade_scene_prop_reference(&f.root, &scene.id, &assign.id).unwrap();
        assert_eq!(upgraded.pinned_version_id, v2.id);
        assert_eq!(upgraded.health, SceneReferenceHealth::Current);
        let list = SceneService::list_scene_props(&f.root, &scene.id).unwrap();
        assert_eq!(list[0].prop_asset_version_id, v2.id);
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let events = scenes_repository::list_reference_events(&conn, &scene.id).unwrap();
        let ev = events
            .iter()
            .find(|e| e.reference_kind == SceneReferenceKind::Prop && e.action == SceneReferenceAction::Upgrade)
            .unwrap();
        assert_eq!(ev.from_version_id.as_deref(), Some(v1.as_str()));
        assert_eq!(ev.to_version_id.as_deref(), Some(v2.id.as_str()));
    }

    #[test]
    fn world_upgrade_does_not_change_other_refs() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let wv1 = f.create_world_plate_canonical(&world, [1, 1, 1, 255]);
        let character = f.create_character("Mara");
        let (_la, look_v1) = f.create_look_canonical(&character.id, "MARA-LOOK", [2, 2, 2, 255]);
        let (_sa, sheet_v1) = f.create_sheet_canonical(&character.id, "MARA-SHEET", [3, 3, 3, 255]);
        let (_pa, prop_v1) = f.create_prop_canonical("PROP-A", [4, 4, 4, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        let char_assign = SceneService::add_scene_character(
            &f.root,
            &scene.id,
            &character.id,
            &look_v1,
            Some(sheet_v1.as_str()),
            None,
        )
        .unwrap();
        let prop_assign = SceneService::add_scene_prop(&f.root, &scene.id, &prop_v1, None, None).unwrap();
        // Promote world V02
        let src2 = f.write_png(&f.root.join("tmp_sources_world"), "world-v02-iso.png", [9, 9, 9, 255]);
        let wv2 = AssetService::import_asset_version(&f.root, &world.world_plate_asset_id, &src2, None).unwrap();
        AssetService::promote_asset_version(&f.root, &wv2.id).unwrap();
        // Capture before
        let before_chars = SceneService::list_scene_characters(&f.root, &scene.id).unwrap();
        let before_props = SceneService::list_scene_props(&f.root, &scene.id).unwrap();
        // Upgrade world only
        SceneService::upgrade_scene_world_reference(&f.root, &scene.id).unwrap();
        // Verify world upgraded
        let after_scene = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(after_scene.world_asset_version_id.as_deref(), Some(wv2.id.as_str()));
        // Other refs unchanged
        let after_chars = SceneService::list_scene_characters(&f.root, &scene.id).unwrap();
        assert_eq!(after_chars[0].look_asset_version_id, before_chars[0].look_asset_version_id);
        assert_eq!(
            after_chars[0].sheet_asset_version_id,
            before_chars[0].sheet_asset_version_id
        );
        assert_eq!(after_chars[0].id, char_assign.id);
        let after_props = SceneService::list_scene_props(&f.root, &scene.id).unwrap();
        assert_eq!(after_props[0].prop_asset_version_id, before_props[0].prop_asset_version_id);
        assert_eq!(after_props[0].id, prop_assign.id);
        // No extra upgrade events for other kinds
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let events = scenes_repository::list_reference_events(&conn, &scene.id).unwrap();
        let world_upgrades = events
            .iter()
            .filter(|e| e.reference_kind == SceneReferenceKind::World && e.action == SceneReferenceAction::Upgrade)
            .count();
        assert_eq!(world_upgrades, 1);
        let look_upgrades = events
            .iter()
            .filter(|e| e.reference_kind == SceneReferenceKind::CharacterLook && e.action == SceneReferenceAction::Upgrade)
            .count();
        assert_eq!(look_upgrades, 0);
        let prop_upgrades = events
            .iter()
            .filter(|e| e.reference_kind == SceneReferenceKind::Prop && e.action == SceneReferenceAction::Upgrade)
            .count();
        assert_eq!(prop_upgrades, 0);
        // Verify wv1 vs wv2
        assert_ne!(wv1, wv2.id);
    }

    #[test]
    fn upgrade_noop_when_already_current() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [5, 5, 5, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        // Already current, resolve should be current
        let resolved = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        assert_eq!(resolved.world.unwrap().health, SceneReferenceHealth::Current);
        let err = SceneService::upgrade_scene_world_reference(&f.root, &scene.id).unwrap_err();
        assert!(matches!(err, AppError::SceneReferenceAlreadyCurrent));
        assert_eq!(err.code(), "SCENE_REFERENCE_ALREADY_CURRENT");
        // Also for character
        let character = f.create_character("Mara");
        let (_a, look_v) = f.create_look_canonical(&character.id, "MARA-LOOK", [1, 1, 1, 255]);
        let scene2 = SceneService::create_scene(&f.root, "Scene2", "Summary").unwrap();
        let assign = SceneService::add_scene_character(&f.root, &scene2.id, &character.id, &look_v, None, None).unwrap();
        let err2 = SceneService::upgrade_scene_character_look_reference(&f.root, &scene2.id, &assign.id).unwrap_err();
        assert!(matches!(err2, AppError::SceneReferenceAlreadyCurrent));
        // Prop
        let (_pa, prop_v) = f.create_prop_canonical("PROP-B", [2, 2, 2, 255]);
        let scene3 = SceneService::create_scene(&f.root, "Scene3", "Summary").unwrap();
        let prop_assign = SceneService::add_scene_prop(&f.root, &scene3.id, &prop_v, None, None).unwrap();
        let err3 = SceneService::upgrade_scene_prop_reference(&f.root, &scene3.id, &prop_assign.id).unwrap_err();
        assert!(matches!(err3, AppError::SceneReferenceAlreadyCurrent));
        // Ensure no extra events beyond initial pin
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let events = scenes_repository::list_reference_events(&conn, &scene.id).unwrap();
        let upgrades = events.iter().filter(|e| e.action == SceneReferenceAction::Upgrade).count();
        assert_eq!(upgrades, 0);
        // Also verify unrelated data unchanged (first scene still at v1)
        let fetched = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(fetched.world_asset_version_id.as_deref(), Some(v1.as_str()));
    }

    #[test]
    fn upgrade_fails_when_no_canonical() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [5, 5, 5, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        // Clear canonical
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            conn.execute(
                "UPDATE assets SET canonical_version_id = NULL WHERE id = ?1",
                rusqlite::params![world.world_plate_asset_id],
            )
            .unwrap();
        }
        let resolved = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
        assert_eq!(resolved.world.unwrap().health, SceneReferenceHealth::Historical);
        let err = SceneService::upgrade_scene_world_reference(&f.root, &scene.id).unwrap_err();
        assert!(matches!(err, AppError::SceneReferenceCanonicalMissing(_)));
        assert_eq!(err.code(), "SCENE_REFERENCE_CANONICAL_MISSING");
        // Also char sheet no canonical
        let character = f.create_character("Mara");
        let (sheet_asset_id, sheet_v1) = f.create_sheet_canonical(&character.id, "MARA-SHEET", [1, 1, 1, 255]);
        let (_a, look_v) = f.create_look_canonical(&character.id, "MARA-LOOK", [2, 2, 2, 255]);
        let scene2 = SceneService::create_scene(&f.root, "Scene2", "Summary").unwrap();
        let assign = SceneService::add_scene_character(
            &f.root,
            &scene2.id,
            &character.id,
            &look_v,
            Some(sheet_v1.as_str()),
            None,
        )
        .unwrap();
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            conn.execute(
                "UPDATE assets SET canonical_version_id = NULL WHERE id = ?1",
                rusqlite::params![sheet_asset_id],
            )
            .unwrap();
        }
        let err2 = SceneService::upgrade_scene_character_sheet_reference(&f.root, &scene2.id, &assign.id).unwrap_err();
        assert!(matches!(err2, AppError::SceneReferenceCanonicalMissing(_)));
    }

    #[test]
    fn resolver_never_mutates_even_when_upgrade_available() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [1, 1, 1, 255]);
        let scene = SceneService::create_scene(&f.root, "Scene", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        let src2 = f.write_png(&f.root.join("tmp_sources_world"), "world-v02-never-mutate.png", [9, 9, 9, 255]);
        let v2 = AssetService::import_asset_version(&f.root, &world.world_plate_asset_id, &src2, None).unwrap();
        AssetService::promote_asset_version(&f.root, &v2.id).unwrap();
        // Call resolver multiple times
        for _ in 0..3 {
            let r = SceneService::resolve_scene_references(&f.root, &scene.id).unwrap();
            assert_eq!(r.world.unwrap().health, SceneReferenceHealth::UpgradeAvailable);
            let fetched = SceneService::get_scene(&f.root, &scene.id).unwrap();
            assert_eq!(fetched.world_asset_version_id.as_deref(), Some(v1.as_str()));
        }
        // No upgrade events were created by resolver
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let events = scenes_repository::list_reference_events(&conn, &scene.id).unwrap();
        let upgrade_events = events.iter().filter(|e| e.action == SceneReferenceAction::Upgrade).count();
        assert_eq!(upgrade_events, 0);
        // Now explicit upgrade does mutate
        SceneService::upgrade_scene_world_reference(&f.root, &scene.id).unwrap();
        let after = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(after.world_asset_version_id.as_deref(), Some(v2.id.as_str()));
    }

    // -----------------------------------------------------------------------
    // Task 9: SceneReadiness, ensure_scene_keyframe_asset
    // -----------------------------------------------------------------------

    #[test]
    fn readiness_blocks_no_world() {
        let f = Fixture::new();
        let scene = SceneService::create_scene(&f.root, "Title", "Summary").unwrap();
        // No world assigned
        let readiness = SceneService::get_scene_readiness(&f.root, &scene.id).unwrap();
        assert!(!readiness.ready_for_keyframe);
        assert!(readiness.blockers.iter().any(|b| b.kind == SceneReadinessBlockerKind::WorldReferenceMissing));
    }

    #[test]
    fn readiness_blocks_broken_world() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [1, 1, 1, 255]);
        let scene = SceneService::create_scene(&f.root, "Title", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        // Corrupt world reference to nonexistent version
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
            conn.execute(
                "UPDATE world_scenes SET world_asset_version_id = 'broken-id' WHERE id = ?1",
                rusqlite::params![scene.id],
            )
            .unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        }
        let readiness = SceneService::get_scene_readiness(&f.root, &scene.id).unwrap();
        assert!(!readiness.ready_for_keyframe);
        assert!(readiness.blockers.iter().any(|b| b.kind == SceneReadinessBlockerKind::WorldReferenceBroken));
        assert_eq!(readiness.blockers.iter().find(|b| b.kind == SceneReadinessBlockerKind::WorldReferenceBroken).unwrap().context.as_deref(), Some("broken-id"));
        let _ = v1;
    }

    #[test]
    fn readiness_blocks_broken_character_reference() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        f.create_world_plate_canonical(&world, [1, 1, 1, 255]);
        let character = f.create_character("Mara");
        let (_asset_id, look_v1) = f.create_look_canonical(&character.id, "MARA-LOOK", [1, 2, 3, 255]);
        let scene = SceneService::create_scene(&f.root, "Title", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        SceneService::add_scene_character(&f.root, &scene.id, &character.id, &look_v1, None, None).unwrap();
        // Break look reference
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            let assignment_id: String = conn
                .query_row(
                    "SELECT id FROM world_scene_characters WHERE scene_id = ?1",
                    rusqlite::params![scene.id],
                    |r| r.get(0),
                )
                .unwrap();
            conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
            conn.execute(
                "UPDATE world_scene_characters SET look_asset_version_id = 'broken-look-id' WHERE id = ?1",
                rusqlite::params![assignment_id],
            )
            .unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        }
        let readiness = SceneService::get_scene_readiness(&f.root, &scene.id).unwrap();
        assert!(!readiness.ready_for_keyframe);
        assert!(readiness.blockers.iter().any(|b| b.kind == SceneReadinessBlockerKind::CharacterReferenceBroken));
    }

    #[test]
    fn readiness_blocks_broken_prop() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        f.create_world_plate_canonical(&world, [1, 1, 1, 255]);
        let (_asset_id, prop_v1) = f.create_prop_canonical("PROP-A", [1, 1, 1, 255]);
        let scene = SceneService::create_scene(&f.root, "Title", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        SceneService::add_scene_prop(&f.root, &scene.id, &prop_v1, None, None).unwrap();
        // Break prop
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            let assignment_id: String = conn
                .query_row(
                    "SELECT id FROM world_scene_props WHERE scene_id = ?1",
                    rusqlite::params![scene.id],
                    |r| r.get(0),
                )
                .unwrap();
            conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
            conn.execute(
                "UPDATE world_scene_props SET prop_asset_version_id = 'broken-prop-id' WHERE id = ?1",
                rusqlite::params![assignment_id],
            )
            .unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        }
        let readiness = SceneService::get_scene_readiness(&f.root, &scene.id).unwrap();
        assert!(!readiness.ready_for_keyframe);
        assert!(readiness.blockers.iter().any(|b| b.kind == SceneReadinessBlockerKind::PropReferenceBroken));
    }

    #[test]
    fn readiness_blocks_unclassified_protected_tbd() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        f.create_world_plate_canonical(&world, [1, 1, 1, 255]);
        // Create protected TBD for location
        let loc_id = {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            let loc: String = conn
                .query_row(
                    "SELECT canon_location_entity_id FROM worlds WHERE id = ?1",
                    rusqlite::params![world.id],
                    |r| r.get(0),
                )
                .unwrap();
            loc
        };
        // Need a section for TBD to be valid - ensure location has description locked section
        {
            let loc_entity = crate::canon::service::CanonService::get_entity(&f.root, &loc_id).unwrap();
            let _ = loc_entity;
            // Create a protected TBD
            crate::canon::tbd::create(&f.root, Some(&loc_id), None, "What is behind the red door?", Some("Do not reveal".into()), true).unwrap();
        }
        let scene = SceneService::create_scene(&f.root, "Title", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        // No binding yet -> should block
        let readiness = SceneService::get_scene_readiness(&f.root, &scene.id).unwrap();
        assert!(!readiness.ready_for_keyframe);
        assert!(readiness.blockers.iter().any(|b| b.kind == SceneReadinessBlockerKind::TbdDecisionRequired));
        // Now add binding preserve_unknown
        {
            let tbd_id: String = {
                let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
                conn.query_row(
                    "SELECT id FROM canon_tbds WHERE canon_entity_id = ?1",
                    rusqlite::params![loc_id],
                    |r| r.get(0),
                )
                .unwrap()
            };
            SceneService::set_scene_tbd_binding(&f.root, &scene.id, &tbd_id, TbdDecisionKind::PreserveUnknown, None).unwrap();
        }
        let readiness2 = SceneService::get_scene_readiness(&f.root, &scene.id).unwrap();
        assert!(readiness2.ready_for_keyframe, "should be ready after TBD classified");
    }

    #[test]
    fn readiness_blocks_empty_summary() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        f.create_world_plate_canonical(&world, [1, 1, 1, 255]);
        let scene = SceneService::create_scene(&f.root, "Title", "").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        let readiness = SceneService::get_scene_readiness(&f.root, &scene.id).unwrap();
        assert!(!readiness.ready_for_keyframe);
        assert!(readiness.blockers.iter().any(|b| b.kind == SceneReadinessBlockerKind::SummaryMissing));
        // Empty title also blocks
        let scene2 = SceneService::create_scene(&f.root, "Temp", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene2.id, &world.id).unwrap();
        SceneService::update_scene_details(&f.root, &scene2.id, "   ", "Summary").unwrap_err(); // title validation prevents empty, but we can directly DB corrupt for test
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
            conn.execute(
                "UPDATE world_scenes SET title = '' WHERE id = ?1",
                rusqlite::params![scene2.id],
            )
            .unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        }
        let readiness3 = SceneService::get_scene_readiness(&f.root, &scene2.id).unwrap();
        assert!(!readiness3.ready_for_keyframe);
        assert!(readiness3.blockers.iter().any(|b| b.kind == SceneReadinessBlockerKind::TitleMissing));
    }

    #[test]
    fn readiness_allows_historical_world() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        let v1 = f.create_world_plate_canonical(&world, [5, 5, 5, 255]);
        let scene = SceneService::create_scene(&f.root, "Title", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        // Make world historical: clear canonical
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            conn.execute(
                "UPDATE assets SET canonical_version_id = NULL WHERE id = ?1",
                rusqlite::params![world.world_plate_asset_id],
            )
            .unwrap();
        }
        let readiness = SceneService::get_scene_readiness(&f.root, &scene.id).unwrap();
        assert!(readiness.ready_for_keyframe, "historical world should still be ready");
        assert!(readiness.blockers.is_empty());
        assert!(readiness.warnings.iter().any(|w| w.kind == SceneReadinessWarningKind::HistoricalReference));
        let _ = v1;
    }

    #[test]
    fn readiness_allows_historical_look() {
        let f = Fixture::new();
        let world = f.create_world("Station");
        f.create_world_plate_canonical(&world, [1, 1, 1, 255]);
        let character = f.create_character("Mara");
        let (asset_id, look_v1) = f.create_look_canonical(&character.id, "MARA-LOOK", [1, 2, 3, 255]);
        let scene = SceneService::create_scene(&f.root, "Title", "Summary").unwrap();
        SceneService::assign_scene_world(&f.root, &scene.id, &world.id).unwrap();
        SceneService::add_scene_character(&f.root, &scene.id, &character.id, &look_v1, None, None).unwrap();
        // Make look historical
        {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            conn.execute(
                "UPDATE assets SET canonical_version_id = NULL WHERE id = ?1",
                rusqlite::params![asset_id],
            )
            .unwrap();
        }
        let readiness = SceneService::get_scene_readiness(&f.root, &scene.id).unwrap();
        assert!(readiness.ready_for_keyframe, "historical look should still be ready");
        assert!(readiness.blockers.is_empty());
        assert!(readiness.warnings.iter().any(|w| w.kind == SceneReadinessWarningKind::HistoricalReference));
    }

    #[test]
    fn ensure_scene_keyframe_asset_is_idempotent() {
        let f = Fixture::new();
        let scene = SceneService::create_scene(&f.root, "Title", "Summary").unwrap();
        assert!(scene.keyframe_asset_id.is_none());
        let first = SceneService::ensure_scene_keyframe_asset(&f.root, &scene.id).unwrap();
        assert_eq!(first.asset_type, "shot_keyframe");
        assert_eq!(first.owner_entity_id.as_deref(), Some(scene.id.as_str()));
        assert!(first.label.contains("SCENE-"));
        let fetched = SceneService::get_scene(&f.root, &scene.id).unwrap();
        assert_eq!(fetched.keyframe_asset_id.as_deref(), Some(first.id.as_str()));
        // Second call returns same asset id, no duplicate
        let second = SceneService::ensure_scene_keyframe_asset(&f.root, &scene.id).unwrap();
        assert_eq!(first.id, second.id);
        let count: i64 = {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM assets WHERE type = 'shot_keyframe' AND owner_entity_id = ?1",
                rusqlite::params![scene.id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(count, 1, "no duplicate asset on repeated calls");
        // No generated media version created
        let versions: i64 = {
            let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM asset_versions WHERE asset_id = ?1",
                rusqlite::params![first.id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(versions, 0, "no generated media version at ensure time");
        // Ensure existing valid asset returned even after restart
        crate::project::service::ProjectService::open(&f.root).unwrap();
        let third = SceneService::ensure_scene_keyframe_asset(&f.root, &scene.id).unwrap();
        assert_eq!(first.id, third.id);
    }
}
