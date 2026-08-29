use crate::assets::model::AssetRecord;
use crate::assets::repository as asset_repository;
use crate::canon::repository as canon_repository;
use crate::db;
use crate::error::AppError;
use crate::project::service::ProjectService;
use crate::worlds::model::{World, WorldDetail};
use crate::worlds::repository as worlds_repository;
use chrono::Utc;
use rusqlite::TransactionBehavior;
use std::path::Path;
use ulid::Ulid;

pub struct WorldService;

impl WorldService {
    /// Creates a production World for an existing Canon Location.
    ///
    /// Transactionally:
    /// 1. verify project
    /// 2. verify Canon entity exists
    /// 3. verify entity type is `location`
    /// 4. reject duplicate World for same location
    /// 5. create World ULID
    /// 6. create stable conceptual `world_plate` Asset
    /// 7. set Asset owner to World
    /// 8. insert World referencing that Asset
    /// 9. commit
    ///
    /// If any insert fails: no World, no orphan World Plate Asset.
    pub fn create_world(
        project_root: &Path,
        canon_location_entity_id: &str,
    ) -> Result<World, AppError> {
        // 1. verify project (also runs migrations and validates manifest)
        let project = ProjectService::open(project_root)?;

        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        // Ensure migrations are current for this connection as well.
        crate::db::migrations::run_migrations(&mut conn)?;

        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;

        // 2. verify Canon entity exists (map generic not-found to World-specific code)
        let canon_entity = match canon_repository::get_entity(&tx, canon_location_entity_id) {
            Ok(entity) => entity,
            Err(AppError::CanonEntityNotFound) => return Err(AppError::WorldLocationNotFound),
            Err(other) => return Err(other),
        };

        // Ensure entity belongs to this project; otherwise treat as not found
        if canon_entity.project_id != project.id {
            return Err(AppError::WorldLocationNotFound);
        }

        // 3. verify entity type is `location`
        if canon_entity.entity_type != "location" {
            return Err(AppError::WorldLocationInvalidType);
        }

        // 4. reject duplicate World for same location
        if worlds_repository::find_world_by_location(
            &tx,
            &project.id,
            canon_location_entity_id,
        )?
        .is_some()
        {
            return Err(AppError::WorldAlreadyExists);
        }

        // 5. create World ULID, 6. create stable conceptual `world_plate` Asset
        let world_id = Ulid::new().to_string();
        let asset_id = Ulid::new().to_string();
        let now = Utc::now().to_rfc3339();

        // Derive a safe label from the Location name. The label must be 1..160 chars.
        let label = derive_world_plate_label(&canon_entity.name, &canon_entity.slug);

        // 7. set Asset owner to World (existing Asset model supports owner_entity_id)
        let asset_record = AssetRecord {
            id: asset_id.clone(),
            project_id: project.id.clone(),
            asset_type: "world_plate".to_string(),
            label,
            owner_entity_id: Some(world_id.clone()),
            canonical_version_id: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        // 6+8. insert Asset and World in same transaction using repository helpers
        asset_repository::insert_asset(&tx, &asset_record)?;
        let world = World {
            id: world_id.clone(),
            project_id: project.id.clone(),
            canon_location_entity_id: canon_location_entity_id.to_string(),
            world_plate_asset_id: asset_id.clone(),
            created_at: now.clone(),
            updated_at: now,
        };
        worlds_repository::insert_world(&tx, &world)?;

        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;

        Ok(world)
    }

    /// Lists every World in the project, returning persistence-only rows.
    pub fn list_worlds(project_root: &Path) -> Result<Vec<World>, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        worlds_repository::list_worlds(&conn, &project.id)
    }

    /// Gets a single World by id (scoped to the project).
    pub fn get_world(project_root: &Path, world_id: &str) -> Result<World, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let world = worlds_repository::get_world(&conn, world_id)?;
        if world.project_id != project.id {
            return Err(AppError::WorldNotFound);
        }
        Ok(world)
    }

    /// Lists Worlds enriched with Location display data and World Plate Asset,
    /// without copying narrative data into World storage.
    pub fn list_worlds_detailed(project_root: &Path) -> Result<Vec<WorldDetail>, AppError> {
        let project = ProjectService::open(project_root)?;
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        crate::db::migrations::run_migrations(&mut conn)?;
        let worlds = worlds_repository::list_worlds(&conn, &project.id)?;
        let mut details = Vec::with_capacity(worlds.len());
        for world in worlds {
            let location =
                canon_repository::get_entity(&conn, &world.canon_location_entity_id).map_err(
                    |e| match e {
                        AppError::CanonEntityNotFound => AppError::Database(format!(
                            "world {} references missing canon entity {}",
                            world.id, world.canon_location_entity_id
                        )),
                        other => other,
                    },
                )?;
            let asset = asset_repository::get_asset(&conn, &world.world_plate_asset_id)?;
            details.push(WorldDetail {
                world,
                location,
                world_plate_asset: asset,
            });
        }
        Ok(details)
    }

    /// Gets a single World enriched with Location display data and World Plate Asset.
    pub fn get_world_detailed(project_root: &Path, world_id: &str) -> Result<WorldDetail, AppError> {
        let world = Self::get_world(project_root, world_id)?;
        let conn = {
            let mut c = db::open_existing_connection(&project_root.join("project.db"))?;
            crate::db::migrations::run_migrations(&mut c)?;
            c
        };
        let location = canon_repository::get_entity(&conn, &world.canon_location_entity_id).map_err(
            |e| match e {
                AppError::CanonEntityNotFound => AppError::Database(format!(
                    "world {} references missing canon entity {}",
                    world.id, world.canon_location_entity_id
                )),
                other => other,
            },
        )?;
        let asset = asset_repository::get_asset(&conn, &world.world_plate_asset_id)?;
        Ok(WorldDetail {
            world,
            location,
            world_plate_asset: asset,
        })
    }
}

fn derive_world_plate_label(name: &str, slug: &str) -> String {
    let trimmed = name.trim();
    // Prefer name uppercased, fallback to slug uppercased
    let base = if trimmed.is_empty() {
        slug.to_uppercase()
    } else {
        trimmed.to_uppercase().replace(' ', "-").replace('_', "-")
    };
    // Collapse non-alphanum? Keep simple: ensure label is valid (1..160)
    let candidate = format!("{base}-WORLD");
    // Ensure we don't exceed 160 chars; truncate base if needed
    if candidate.chars().count() <= 160 {
        return candidate;
    }
    // Truncate base to fit
    let max_base = 160 - "-WORLD".len();
    let truncated: String = base.chars().take(max_base).collect();
    format!("{truncated}-WORLD")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::model::CanonEntityType;
    use crate::canon::service::CanonService;
    use crate::project::service::ProjectService;
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

        fn asset_count(&self) -> i64 {
            let conn = db::open_existing_connection(&self.root.join("project.db")).unwrap();
            conn.query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
                .unwrap()
        }

        fn world_count(&self) -> i64 {
            let conn = db::open_existing_connection(&self.root.join("project.db")).unwrap();
            conn.query_row("SELECT COUNT(*) FROM worlds", [], |r| r.get(0))
                .unwrap()
        }
    }

    #[test]
    fn create_world_from_location() {
        let f = Fixture::new();
        let loc = f.create_location("The Station");
        let world = WorldService::create_world(&f.root, &loc.id).unwrap();
        assert_eq!(world.canon_location_entity_id, loc.id);
        assert!(!world.id.is_empty());
        assert!(!world.world_plate_asset_id.is_empty());
        // Verify persisted
        let fetched = WorldService::get_world(&f.root, &world.id).unwrap();
        assert_eq!(fetched.id, world.id);
        assert_eq!(f.world_count(), 1);
    }

    #[test]
    fn reject_character_entity() {
        let f = Fixture::new();
        let chr = f.create_character("Mara");
        let err = WorldService::create_world(&f.root, &chr.id).unwrap_err();
        assert!(matches!(err, AppError::WorldLocationInvalidType));
        assert_eq!(err.code(), "WORLD_LOCATION_INVALID_TYPE");
        assert_eq!(f.world_count(), 0);
        assert_eq!(f.asset_count(), 0);
    }

    #[test]
    fn reject_missing_canon_entity() {
        let f = Fixture::new();
        let err = WorldService::create_world(&f.root, "01JFAKE000000000000000000").unwrap_err();
        assert!(matches!(err, AppError::WorldLocationNotFound));
        assert_eq!(err.code(), "WORLD_LOCATION_NOT_FOUND");
        assert_eq!(f.world_count(), 0);
        assert_eq!(f.asset_count(), 0);
    }

    #[test]
    fn reject_duplicate_world() {
        let f = Fixture::new();
        let loc = f.create_location("The Station");
        let first = WorldService::create_world(&f.root, &loc.id).unwrap();
        let err = WorldService::create_world(&f.root, &loc.id).unwrap_err();
        assert!(matches!(err, AppError::WorldAlreadyExists));
        assert_eq!(err.code(), "WORLD_ALREADY_EXISTS");
        // Still only one world and one asset
        assert_eq!(f.world_count(), 1);
        assert_eq!(f.asset_count(), 1);
        // First still retrievable
        let fetched = WorldService::get_world(&f.root, &first.id).unwrap();
        assert_eq!(fetched.id, first.id);
    }

    #[test]
    fn world_and_world_plate_asset_created_together() {
        let f = Fixture::new();
        let loc = f.create_location("The Station");
        let world = WorldService::create_world(&f.root, &loc.id).unwrap();
        // Verify asset exists
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let asset = asset_repository::get_asset(&conn, &world.world_plate_asset_id).unwrap();
        assert_eq!(asset.id, world.world_plate_asset_id);
        assert_eq!(asset.project_id, world.project_id);
        assert_eq!(f.asset_count(), 1);
        assert_eq!(f.world_count(), 1);
    }

    #[test]
    fn asset_type_is_exactly_world_plate() {
        let f = Fixture::new();
        let loc = f.create_location("The Station");
        let world = WorldService::create_world(&f.root, &loc.id).unwrap();
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let asset = asset_repository::get_asset(&conn, &world.world_plate_asset_id).unwrap();
        assert_eq!(asset.asset_type, "world_plate");
    }

    #[test]
    fn transaction_rollback_leaves_no_orphan_asset() {
        let f = Fixture::new();
        let loc = f.create_location("The Station");
        let chr = f.create_character("Mara");
        // First successful world
        WorldService::create_world(&f.root, &loc.id).unwrap();
        let count_before = f.asset_count();
        assert_eq!(count_before, 1);
        // Duplicate attempt should not create orphan asset
        let _ = WorldService::create_world(&f.root, &loc.id).unwrap_err();
        assert_eq!(f.asset_count(), 1);
        assert_eq!(f.world_count(), 1);
        // Invalid type attempt should not create orphan asset
        let _ = WorldService::create_world(&f.root, &chr.id).unwrap_err();
        assert_eq!(f.asset_count(), 1);
        // Missing entity attempt should not create orphan asset
        let _ = WorldService::create_world(&f.root, "01JFAKE111111111111111111").unwrap_err();
        assert_eq!(f.asset_count(), 1);
        // Cross-check: fresh fixture with no prior worlds, failed character attempt leaves zero assets
        let f2 = Fixture::new();
        let chr2 = f2.create_character("Mara2");
        let _ = WorldService::create_world(&f2.root, &chr2.id).unwrap_err();
        assert_eq!(f2.asset_count(), 0);
        assert_eq!(f2.world_count(), 0);
    }

    #[test]
    fn close_reopen_returns_same_world_and_asset_ids() {
        let f = Fixture::new();
        let loc = f.create_location("The Station");
        let world = WorldService::create_world(&f.root, &loc.id).unwrap();
        let world_id = world.id.clone();
        let asset_id = world.world_plate_asset_id.clone();

        // Simulate close/reopen: ProjectService::open re-runs migrations and validates
        ProjectService::open(&f.root).unwrap();

        let reopened = WorldService::get_world(&f.root, &world_id).unwrap();
        assert_eq!(reopened.id, world_id);
        assert_eq!(reopened.world_plate_asset_id, asset_id);
        assert_eq!(reopened.canon_location_entity_id, loc.id);

        let list = WorldService::list_worlds(&f.root).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, world_id);

        // Also detailed path
        let detailed = WorldService::get_world_detailed(&f.root, &world_id).unwrap();
        assert_eq!(detailed.world.id, world_id);
        assert_eq!(detailed.world_plate_asset.id, asset_id);
        assert_eq!(detailed.location.id, loc.id);
        assert_eq!(detailed.location.name, loc.name);
    }

    #[test]
    fn list_returns_location_display_without_copying() {
        let f = Fixture::new();
        let loc = f.create_location("The Station");
        let world = WorldService::create_world(&f.root, &loc.id).unwrap();

        let detailed_list = WorldService::list_worlds_detailed(&f.root).unwrap();
        assert_eq!(detailed_list.len(), 1);
        assert_eq!(detailed_list[0].world.id, world.id);
        assert_eq!(detailed_list[0].location.id, loc.id);
        assert_eq!(detailed_list[0].location.name, "The Station");
        // Verify World storage does not contain description copy (schema has no description column)
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let has_description_column: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('worlds') WHERE name='description'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_description_column, 0, "World should not copy Location narrative");
        // Asset owner points to World
        let asset = asset_repository::get_asset(&conn, &world.world_plate_asset_id).unwrap();
        assert_eq!(asset.owner_entity_id.as_deref(), Some(world.id.as_str()));
    }

    #[test]
    fn get_returns_location_display_without_copying() {
        let f = Fixture::new();
        let loc = f.create_location("Rooftop");
        let world = WorldService::create_world(&f.root, &loc.id).unwrap();
        let detail = WorldService::get_world_detailed(&f.root, &world.id).unwrap();
        assert_eq!(detail.location.name, "Rooftop");
        assert_eq!(detail.location.entity_type, "location");
        assert_eq!(detail.world_plate_asset.asset_type, "world_plate");
    }

    #[test]
    fn world_plate_asset_has_owner_world_id() {
        let f = Fixture::new();
        let loc = f.create_location("Lab");
        let world = WorldService::create_world(&f.root, &loc.id).unwrap();
        let conn = db::open_existing_connection(&f.root.join("project.db")).unwrap();
        let asset = asset_repository::get_asset(&conn, &world.world_plate_asset_id).unwrap();
        assert_eq!(asset.owner_entity_id.as_deref(), Some(world.id.as_str()));
    }

    #[test]
    fn error_codes_are_screaming_snake() {
        assert_eq!(
            AppError::WorldLocationNotFound.code(),
            "WORLD_LOCATION_NOT_FOUND"
        );
        assert_eq!(
            AppError::WorldLocationInvalidType.code(),
            "WORLD_LOCATION_INVALID_TYPE"
        );
        assert_eq!(AppError::WorldAlreadyExists.code(), "WORLD_ALREADY_EXISTS");
        assert_eq!(
            AppError::WorldPlateAssetInvalid("bad".into()).code(),
            "WORLD_PLATE_ASSET_INVALID"
        );
        assert_eq!(AppError::WorldNotFound.code(), "WORLD_NOT_FOUND");
    }
}
