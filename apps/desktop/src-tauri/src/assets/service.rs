use crate::assets::model::{AssetRecord, AssetWithVersions};
use crate::assets::repository;
use crate::db;
use crate::db::migrations::run_migrations;
use crate::error::AppError;
use crate::project::repository as project_repository;
use chrono::Utc;
use std::path::Path;
use ulid::Ulid;

/// The full set of asset types recognized by the schema. Mirrors
/// `packages/domain/src/asset.ts`'s `ASSET_TYPES`.
const ASSET_TYPES: &[&str] = &[
    "face_lock",
    "outfit",
    "character_sheet",
    "world_plate",
    "shot_keyframe",
    "prop_plate",
    "image",
    "video",
    "audio",
];

/// Asset types excluded from Sprint 1, even though they are part of the
/// stable, declared enum.
const SPRINT_ONE_UNSUPPORTED_TYPES: &[&str] = &["video", "audio"];

pub struct AssetService;

impl AssetService {
    /// Creates a new asset (with no versions and no canonical version) in
    /// the project rooted at `project_root`.
    ///
    /// The owning `project_id` is always read from the project's own
    /// database row -- it is never accepted from the caller, so a caller
    /// cannot create an asset under a project id it does not actually
    /// control.
    pub fn create_asset(
        project_root: &Path,
        asset_type: &str,
        label: &str,
        owner_entity_id: Option<String>,
    ) -> Result<AssetRecord, AppError> {
        let asset_type = validate_asset_type(asset_type)?;
        let label = validate_label(label)?;

        let mut conn = open_project_db(project_root)?;
        let project_id = project_repository::read_project(&conn)?.id;

        let now = Utc::now().to_rfc3339();
        let record = AssetRecord {
            id: Ulid::new().to_string(),
            project_id,
            asset_type: asset_type.to_string(),
            label,
            owner_entity_id,
            canonical_version_id: None,
            created_at: now.clone(),
            updated_at: now,
        };

        let tx = conn
            .transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;
        repository::insert_asset(&tx, &record)?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;

        Ok(record)
    }

    /// Lists every asset belonging to the project rooted at `project_root`.
    pub fn list_assets(project_root: &Path) -> Result<Vec<AssetRecord>, AppError> {
        let conn = open_project_db(project_root)?;
        let project_id = project_repository::read_project(&conn)?.id;
        repository::list_assets(&conn, &project_id)
    }

    /// Fetches a single asset (scoped to the open project) along with all
    /// of its versions, newest first.
    pub fn get_asset_with_versions(
        project_root: &Path,
        asset_id: &str,
    ) -> Result<AssetWithVersions, AppError> {
        let conn = open_project_db(project_root)?;
        // Reading the project row also confirms `project_root` is a valid,
        // bootstrapped project before we touch the asset it names.
        project_repository::read_project(&conn)?;

        let asset = repository::get_asset(&conn, asset_id)?;
        let versions = repository::list_asset_versions(&conn, asset_id)?;

        Ok(AssetWithVersions { asset, versions })
    }
}

/// Opens the project's database and ensures it is fully migrated, the same
/// way `ProjectService::open` does.
fn open_project_db(project_root: &Path) -> Result<rusqlite::Connection, AppError> {
    let db_path = project_root.join("project.db");
    let mut conn = db::open_connection(&db_path)?;
    run_migrations(&mut conn)?;
    Ok(conn)
}

fn validate_label(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 160 {
        return Err(AppError::InvalidAssetLabel);
    }
    Ok(trimmed.to_string())
}

/// Validates that `value` is one of the declared asset types. A type that
/// is declared but excluded from Sprint 1 (`video`, `audio`) is rejected
/// with `UnsupportedAssetTypeForSprint`; a genuinely unknown string is a
/// different failure mode and is rejected with `InvalidAssetType` instead,
/// since the two errors mean different things to a caller (one names a
/// real, future capability; the other is simply malformed input).
fn validate_asset_type(value: &str) -> Result<&str, AppError> {
    if SPRINT_ONE_UNSUPPORTED_TYPES.contains(&value) {
        return Err(AppError::UnsupportedAssetTypeForSprint);
    }
    if !ASSET_TYPES.contains(&value) {
        return Err(AppError::InvalidAssetType);
    }
    Ok(value)
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::project::model::ProjectSummary;
    use crate::project::service::ProjectService;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// A real, on-disk, fully bootstrapped project for use in tests that
    /// need a genuine `project.db` (migrations applied, `projects` row
    /// populated) rather than a mock.
    ///
    /// `_temp` is kept alive for the lifetime of the fixture so the
    /// directory isn't cleaned up out from under `root`.
    pub struct ProjectFixture {
        _temp: TempDir,
        pub root: PathBuf,
        pub summary: ProjectSummary,
    }

    impl ProjectFixture {
        pub fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("fixture-project");
            std::fs::create_dir(&root).unwrap();

            let summary = ProjectService::create(&root, "Fixture Project").unwrap();

            Self {
                _temp: temp,
                root,
                summary,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::ProjectFixture;
    use super::*;
    use crate::error::AppError;
    use crate::project::service::ProjectService;

    #[test]
    fn creates_face_lock_asset_without_canonical_version() {
        let fixture = ProjectFixture::new();
        let asset =
            AssetService::create_asset(&fixture.root, "face_lock", "MARA-FACE", None).unwrap();

        assert_eq!(asset.asset_type, "face_lock");
        assert_eq!(asset.label, "MARA-FACE");
        assert!(asset.canonical_version_id.is_none());
        assert_eq!(asset.project_id, fixture.summary.id);
    }

    #[test]
    fn lists_assets_for_only_the_open_project() {
        let first = ProjectFixture::new();
        let second = ProjectFixture::new();

        AssetService::create_asset(&first.root, "face_lock", "MARA-FACE", None).unwrap();
        AssetService::create_asset(&second.root, "face_lock", "OTHER-FACE", None).unwrap();

        let assets = AssetService::list_assets(&first.root).unwrap();

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].label, "MARA-FACE");
    }

    #[test]
    fn lists_no_assets_for_a_freshly_created_empty_project() {
        let fixture = ProjectFixture::new();

        let assets = AssetService::list_assets(&fixture.root).unwrap();

        assert!(assets.is_empty());
    }

    #[test]
    fn gets_asset_with_no_versions() {
        let fixture = ProjectFixture::new();
        let asset =
            AssetService::create_asset(&fixture.root, "face_lock", "MARA-FACE", None).unwrap();

        let with_versions =
            AssetService::get_asset_with_versions(&fixture.root, &asset.id).unwrap();

        assert_eq!(with_versions.asset.id, asset.id);
        assert!(with_versions.versions.is_empty());
    }

    #[test]
    fn getting_an_unknown_asset_returns_not_found() {
        let fixture = ProjectFixture::new();

        let error =
            AssetService::get_asset_with_versions(&fixture.root, "not-a-real-id").unwrap_err();

        assert!(matches!(error, AppError::AssetNotFound));
    }

    #[test]
    fn rejects_video_asset_type_in_sprint_one() {
        let fixture = ProjectFixture::new();

        let error = AssetService::create_asset(&fixture.root, "video", "B-ROLL", None)
            .unwrap_err();

        assert!(matches!(error, AppError::UnsupportedAssetTypeForSprint));
    }

    #[test]
    fn rejects_audio_asset_type_in_sprint_one() {
        let fixture = ProjectFixture::new();

        let error = AssetService::create_asset(&fixture.root, "audio", "VO", None).unwrap_err();

        assert!(matches!(error, AppError::UnsupportedAssetTypeForSprint));
    }

    #[test]
    fn rejects_unknown_asset_type() {
        let fixture = ProjectFixture::new();

        let error =
            AssetService::create_asset(&fixture.root, "bogus", "SOMETHING", None).unwrap_err();

        assert!(matches!(error, AppError::InvalidAssetType));
    }

    #[test]
    fn rejects_blank_label() {
        let fixture = ProjectFixture::new();

        let error = AssetService::create_asset(&fixture.root, "face_lock", "   ", None)
            .unwrap_err();

        assert!(matches!(error, AppError::InvalidAssetLabel));
    }

    #[test]
    fn trims_label_before_persisting() {
        let fixture = ProjectFixture::new();

        let asset =
            AssetService::create_asset(&fixture.root, "face_lock", "  MARA-FACE  ", None)
                .unwrap();

        assert_eq!(asset.label, "MARA-FACE");
    }

    #[test]
    fn asset_schema_survives_project_reopen() {
        let fixture = ProjectFixture::new();
        let created =
            AssetService::create_asset(&fixture.root, "face_lock", "MARA-FACE", None).unwrap();

        // Re-opening the project (as the app would on a fresh launch) must
        // still see the asset -- migrations are idempotent and the row
        // persisted to disk.
        ProjectService::open(&fixture.root).unwrap();
        let assets = AssetService::list_assets(&fixture.root).unwrap();

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].id, created.id);
    }
}
