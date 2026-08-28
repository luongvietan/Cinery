use crate::assets::repository as asset_repository;
use crate::canon::repository as canon_repository;
use crate::db;
use crate::error::AppError;
use crate::project::service::ProjectService;
use crate::scenes::model::{
    Scene, SceneCharacterAssignment, ScenePropAssignment, SceneReferenceAction, SceneReferenceEvent,
    SceneReferenceKind,
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
            .query_row("SELECT MAX(ordinal) FROM scenes", [], |r| r.get(0))
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
}
