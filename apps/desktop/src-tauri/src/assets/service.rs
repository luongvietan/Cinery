use crate::assets::import;
use crate::assets::model::{
    AssetRecord, AssetVersionRecord, AssetWithVersions, CanonicalPromotionResult,
};
use crate::assets::repository;
use crate::assets::thumbnail;
use crate::db;
use crate::db::migrations::run_migrations;
use crate::error::AppError;
use crate::project::repository as project_repository;
use chrono::Utc;
use rusqlite::TransactionBehavior;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
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

/// Asset types excluded from the current MVP slice, even though they are
/// part of the stable, declared enum. `video` is supported as of P10.0
/// (generated scene videos persist into per-scene `video` assets); `audio`
/// remains unsupported until an audio pipeline exists.
const SPRINT_ONE_UNSUPPORTED_TYPES: &[&str] = &["audio"];

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
    pub fn list_assets(
        project_root: &Path,
    ) -> Result<Vec<crate::assets::model::AssetSummaryRecord>, AppError> {
        let conn = open_project_db(project_root)?;
        let project_id = project_repository::read_project(&conn)?.id;
        repository::list_asset_summaries(&conn, &project_id)
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

    pub fn promote_asset_version(
        project_root: &Path,
        asset_version_id: &str,
    ) -> Result<CanonicalPromotionResult, AppError> {
        let mut conn = open_project_db(project_root)?;
        project_repository::read_project(&conn)?;
        repository::promote_canonical_version(&mut conn, asset_version_id)
    }

    /// Imports `source_path` as a brand-new, immutable version of
    /// `asset_id`.
    ///
    /// Every imported version starts life as `candidate` -- importing never
    /// promotes anything, and this function never touches
    /// `canonical_version_id`. Promotion is a separate concern (Task 7).
    ///
    /// `source_path` is only ever read from, never modified, moved, or
    /// deleted: the file is copied into a new, unique, project-managed
    /// location under `assets/`, and a WebP thumbnail is generated under
    /// `thumbnails/`. Duplicate content on the same asset (by SHA-256) is
    /// rejected before anything is written to disk.
    ///
    /// If `parent_version_id` is given, it must name an existing version of
    /// *this* asset; a parent that doesn't exist at all, or that belongs to
    /// a different asset, is reported the same way
    /// (`AppError::ParentVersionMismatch`) -- callers only need to know the
    /// lineage they asked for isn't valid, not which of the two reasons
    /// caused it.
    pub fn import_asset_version(
        project_root: &Path,
        asset_id: &str,
        source_path: &Path,
        parent_version_id: Option<String>,
    ) -> Result<AssetVersionRecord, AppError> {
        let mut conn = open_project_db(project_root)?;

        // Confirm the asset itself exists before doing anything else.
        repository::get_asset(&conn, asset_id)?;

        // A parent version, if named, must belong to this same asset.
        if let Some(parent_id) = &parent_version_id {
            match repository::get_asset_version_by_id(&conn, parent_id)? {
                Some(parent) if parent.asset_id == asset_id => {}
                _ => return Err(AppError::ParentVersionMismatch),
            }
        }

        // Pure filesystem read, no DB side effects yet -- do this before
        // opening a transaction.
        let inspected = import::inspect_image(source_path)?;

        // Acquire the write lock up front (IMMEDIATE) rather than letting
        // SQLite upgrade a DEFERRED transaction later, which can fail under
        // concurrent access.
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;

        // Reject duplicate content on this asset. Nothing has been written
        // to disk yet, so this leaves zero filesystem footprint.
        if repository::find_version_by_hash(&tx, asset_id, &inspected.sha256)?.is_some() {
            return Err(AppError::DuplicateAssetVersion);
        }

        let version_number = repository::next_version_number(&tx, asset_id)?;
        let version_id = Ulid::new().to_string();

        let media_relative =
            managed_media_path(asset_id, version_number, &version_id, inspected.extension);
        let thumbnail_relative = managed_thumbnail_path(asset_id, &version_id);
        let media_absolute = project_root.join(&media_relative);
        let thumbnail_absolute = project_root.join(&thumbnail_relative);

        if let Err(err) =
            materialize_version_files(source_path, &media_absolute, &thumbnail_absolute)
        {
            cleanup_failed_import(&media_absolute, &thumbnail_absolute);
            return Err(err);
        }

        let original_filename = source_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        let record = AssetVersionRecord {
            id: version_id,
            asset_id: asset_id.to_string(),
            version_number,
            status: "candidate".to_string(),
            file_path: to_forward_slash(&media_relative),
            thumbnail_path: to_forward_slash(&thumbnail_relative),
            sha256: inspected.sha256,
            original_filename,
            mime_type: inspected.mime_type.to_string(),
            byte_size: inspected.byte_size as i64,
            width: Some(inspected.width as i64),
            height: Some(inspected.height as i64),
            parent_version_id,
            created_at: Utc::now().to_rfc3339(),
            origin: "imported".into(),
            generation_artifact_id: None,
        };

        if let Err(err) = repository::insert_asset_version(&tx, &record) {
            cleanup_failed_import(&media_absolute, &thumbnail_absolute);
            return Err(err);
        }

        if let Err(e) = tx.commit() {
            // SQLite already rolled back the transaction on its own here,
            // so there's no DB row -- but the media file and thumbnail this
            // attempt wrote to disk are now orphaned (nothing references
            // them) unless we clean them up ourselves too.
            cleanup_failed_import(&media_absolute, &thumbnail_absolute);
            return Err(AppError::Database(e.to_string()));
        }

        Ok(record)
    }

    /// Imports a generated video (MP4) as a new asset version. Videos get no
    /// generated thumbnail (the column keeps an empty string; previews fall
    /// back to the file icon path in the UI).
    pub fn import_media_version(
        project_root: &Path,
        asset_id: &str,
        source_path: &Path,
        parent_version_id: Option<String>,
    ) -> Result<AssetVersionRecord, AppError> {
        let mut conn = open_project_db(project_root)?;

        repository::get_asset(&conn, asset_id)?;

        if let Some(parent_id) = &parent_version_id {
            match repository::get_asset_version_by_id(&conn, parent_id)? {
                Some(parent) if parent.asset_id == asset_id => {}
                _ => return Err(AppError::ParentVersionMismatch),
            }
        }

        let inspected = import::inspect_video(source_path)?;

        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| AppError::Database(e.to_string()))?;

        if repository::find_version_by_hash(&tx, asset_id, &inspected.sha256)?.is_some() {
            return Err(AppError::DuplicateAssetVersion);
        }

        let version_number = repository::next_version_number(&tx, asset_id)?;
        let version_id = Ulid::new().to_string();

        let media_relative =
            managed_media_path(asset_id, version_number, &version_id, inspected.extension);
        let media_absolute = project_root.join(&media_relative);

        if let Some(parent) = media_absolute.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::FileSystem(e.to_string()))?;
        }
        let tmp_media_path = with_tmp_suffix(&media_absolute);
        fs::copy(source_path, &tmp_media_path)
            .map_err(|e| AppError::FileSystem(format!("copy failed: {e}")))?;
        let file = fs::OpenOptions::new()
            .write(true)
            .open(&tmp_media_path)
            .map_err(|e| AppError::FileSystem(format!("open-for-sync failed: {e}")))?;
        file.sync_all()
            .map_err(|e| AppError::FileSystem(format!("sync_all failed: {e}")))?;
        drop(file);
        fs::rename(&tmp_media_path, &media_absolute)
            .map_err(|e| AppError::FileSystem(format!("rename failed: {e}")))?;

        let original_filename = source_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        let record = AssetVersionRecord {
            id: version_id,
            asset_id: asset_id.to_string(),
            version_number,
            status: "candidate".to_string(),
            file_path: to_forward_slash(&media_relative),
            thumbnail_path: String::new(),
            sha256: inspected.sha256,
            original_filename,
            mime_type: inspected.mime_type.to_string(),
            byte_size: inspected.byte_size as i64,
            width: None,
            height: None,
            parent_version_id,
            created_at: Utc::now().to_rfc3339(),
            origin: "imported".into(),
            generation_artifact_id: None,
        };

        if let Err(err) = repository::insert_asset_version(&tx, &record) {
            let _ = fs::remove_file(&media_absolute);
            return Err(err);
        }

        if let Err(e) = tx.commit() {
            let _ = fs::remove_file(&media_absolute);
            return Err(AppError::Database(e.to_string()));
        }

        Ok(record)
    }
}

/// Copies `source_path` into `media_absolute` (via a temporary sibling
/// file, fsynced before the atomic rename into place) and generates a
/// thumbnail for it at `thumbnail_absolute`.
fn materialize_version_files(
    source_path: &Path,
    media_absolute: &Path,
    thumbnail_absolute: &Path,
) -> Result<(), AppError> {
    if let Some(parent) = media_absolute.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::FileSystem(e.to_string()))?;
    }

    let tmp_media_path = with_tmp_suffix(media_absolute);
    fs::copy(source_path, &tmp_media_path)
        .map_err(|e| AppError::FileSystem(format!("copy failed: {e}")))?;

    // Fsync the copy before it is renamed into place, for durability.
    // Opened with write access: on Windows, FlushFileBuffers (which
    // `sync_all` calls) requires GENERIC_WRITE, so a read-only handle from
    // `File::open` would fail here with "Access is denied".
    let file = fs::OpenOptions::new()
        .write(true)
        .open(&tmp_media_path)
        .map_err(|e| AppError::FileSystem(format!("open-for-sync failed: {e}")))?;
    file.sync_all()
        .map_err(|e| AppError::FileSystem(format!("sync_all failed: {e}")))?;
    drop(file);

    fs::rename(&tmp_media_path, media_absolute)
        .map_err(|e| AppError::FileSystem(format!("rename failed: {e}")))?;

    // Thumbnail from the immutable managed copy, not the (mutable, caller
    // owned) original source.
    thumbnail::generate_thumbnail(media_absolute, thumbnail_absolute)?;

    Ok(())
}

/// Removes only the filesystem entries a failed import attempt itself may
/// have created. Never touches the user's original source file.
fn cleanup_failed_import(media_absolute: &Path, thumbnail_absolute: &Path) {
    let _ = fs::remove_file(with_tmp_suffix(media_absolute));
    let _ = fs::remove_file(media_absolute);
    let _ = fs::remove_file(with_tmp_suffix(thumbnail_absolute));
    let _ = fs::remove_file(thumbnail_absolute);
}

fn with_tmp_suffix(path: &Path) -> PathBuf {
    let mut os_string: OsString = path.as_os_str().to_os_string();
    os_string.push(".tmp");
    PathBuf::from(os_string)
}

/// Relative (to the project root) path of the managed media file for a
/// version: `assets/<asset-id>/v<NNN>/<version-id>.<extension>`.
fn managed_media_path(
    asset_id: &str,
    version_number: i64,
    version_id: &str,
    extension: &str,
) -> PathBuf {
    Path::new("assets")
        .join(asset_id)
        .join(format!("v{version_number:03}"))
        .join(format!("{version_id}.{extension}"))
}

/// Relative (to the project root) path of a version's thumbnail:
/// `thumbnails/<asset-id>/<version-id>.webp`.
fn managed_thumbnail_path(asset_id: &str, version_id: &str) -> PathBuf {
    Path::new("thumbnails")
        .join(asset_id)
        .join(format!("{version_id}.webp"))
}

/// Normalizes a path built with the OS-native separator into the
/// forward-slash form stored in the database, so stored paths stay
/// portable regardless of the platform that created them.
fn to_forward_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Opens the project's database and ensures it is fully migrated, the same
/// way `ProjectService::open` does.
///
/// Uses `db::open_existing_connection` rather than `db::open_connection`
/// directly: a `project.db` that has gone missing (e.g. the directory was
/// corrupted or partially deleted) must surface as
/// `AppError::InvalidProjectDirectory`, not silently recreate an empty
/// database file and then fail later with a confusing generic error once
/// the (now-empty) `projects` table is queried.
fn open_project_db(project_root: &Path) -> Result<rusqlite::Connection, AppError> {
    let db_path = project_root.join("project.db");
    let mut conn = db::open_existing_connection(&db_path)?;
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
    use crate::assets::service::AssetService;
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

    /// A real, on-disk project (via `ProjectFixture`) that already has one
    /// `face_lock` asset created in it, plus a *separate* temp directory --
    /// standing in for something like the user's Downloads folder -- to
    /// write candidate source images into. Keeping the source directory
    /// distinct from the project root is what lets tests assert that
    /// imported originals are left untouched and are never confused with
    /// project-managed files.
    pub struct AssetFixture {
        _project: ProjectFixture,
        _source_temp: TempDir,
        pub project_root: PathBuf,
        pub asset_id: String,
    }

    impl AssetFixture {
        pub fn face_asset() -> Self {
            let project = ProjectFixture::new();
            let asset =
                AssetService::create_asset(&project.root, "face_lock", "MARA-FACE", None).unwrap();
            let source_temp = tempfile::tempdir().unwrap();
            let project_root = project.root.clone();

            Self {
                _project: project,
                _source_temp: source_temp,
                project_root,
                asset_id: asset.id,
            }
        }

        /// Writes a real PNG with a default fill color outside the project
        /// directory and returns its path.
        pub fn write_png(&self, name: &str, width: u32, height: u32) -> PathBuf {
            self.write_png_with_pixel(name, width, height, [12, 34, 56, 255])
        }

        /// Same as `write_png`, but with a caller-chosen fill color, so two
        /// images can be guaranteed to differ (or match) in content.
        pub fn write_png_with_pixel(
            &self,
            name: &str,
            width: u32,
            height: u32,
            pixel: [u8; 4],
        ) -> PathBuf {
            let path = self._source_temp.path().join(name);
            let image: image::RgbaImage =
                image::ImageBuffer::from_pixel(width, height, image::Rgba(pixel));
            image.save(&path).expect("failed to write test PNG");
            path
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{AssetFixture, ProjectFixture};
    use super::*;
    use crate::error::AppError;
    use crate::project::service::ProjectService;

    fn fixture_with_two_versions() -> (AssetFixture, String, String) {
        let fixture = AssetFixture::face_asset();
        let first_source = fixture.write_png("first.png", 32, 32);
        let second_source = fixture.write_png_with_pixel("second.png", 32, 32, [90, 91, 92, 255]);

        let first = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &first_source,
            None,
        )
        .unwrap();
        let second = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &second_source,
            None,
        )
        .unwrap();

        (fixture, first.id, second.id)
    }

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
    fn creates_a_video_asset_for_generated_scene_videos() {
        let fixture = ProjectFixture::new();

        // P10.0: `video` is a supported asset type; the scene video
        // workflow find-or-creates one per scene.
        let asset = AssetService::create_asset(&fixture.root, "video", "SCENE 001 — Video", None)
            .unwrap();
        assert_eq!(asset.asset_type, "video");
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

        let error =
            AssetService::create_asset(&fixture.root, "face_lock", "   ", None).unwrap_err();

        assert!(matches!(error, AppError::InvalidAssetLabel));
    }

    #[test]
    fn trims_label_before_persisting() {
        let fixture = ProjectFixture::new();

        let asset =
            AssetService::create_asset(&fixture.root, "face_lock", "  MARA-FACE  ", None).unwrap();

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

    #[test]
    fn rejects_asset_call_when_project_db_is_missing() {
        let fixture = ProjectFixture::new();
        AssetService::create_asset(&fixture.root, "face_lock", "MARA-FACE", None).unwrap();

        std::fs::remove_file(fixture.root.join("project.db")).unwrap();

        let error = AssetService::list_assets(&fixture.root).unwrap_err();

        assert!(matches!(error, AppError::InvalidProjectDirectory));
    }

    #[test]
    fn imports_png_as_candidate_version_one() {
        let fixture = AssetFixture::face_asset();
        let source = fixture.write_png("candidate.png", 64, 64);

        let version = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &source,
            None,
        )
        .unwrap();

        assert_eq!(version.version_number, 1);
        assert_eq!(version.status, "candidate");
        assert!(fixture.project_root.join(&version.file_path).exists());
        assert!(fixture.project_root.join(&version.thumbnail_path).exists());
        assert!(source.exists());
        assert_eq!(version.mime_type, "image/png");
        assert_eq!(version.original_filename, "candidate.png");
        assert!(!version.file_path.contains('\\'));
        assert!(!version.thumbnail_path.contains('\\'));
    }

    #[test]
    fn second_distinct_import_becomes_version_two() {
        let fixture = AssetFixture::face_asset();
        let first = fixture.write_png("first.png", 64, 64);
        let second = fixture.write_png_with_pixel("second.png", 64, 64, [20, 30, 40, 255]);

        AssetService::import_asset_version(&fixture.project_root, &fixture.asset_id, &first, None)
            .unwrap();
        let version = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &second,
            None,
        )
        .unwrap();

        assert_eq!(version.version_number, 2);
    }

    #[test]
    fn rejects_duplicate_content_on_same_asset() {
        let fixture = AssetFixture::face_asset();
        let source = fixture.write_png("same.png", 64, 64);

        AssetService::import_asset_version(&fixture.project_root, &fixture.asset_id, &source, None)
            .unwrap();
        let error = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &source,
            None,
        )
        .unwrap_err();

        assert!(matches!(error, AppError::DuplicateAssetVersion));
    }

    #[test]
    fn duplicate_rejection_leaves_no_new_filesystem_footprint() {
        let fixture = AssetFixture::face_asset();
        let source = fixture.write_png("same.png", 64, 64);

        AssetService::import_asset_version(&fixture.project_root, &fixture.asset_id, &source, None)
            .unwrap();

        let assets_dir = fixture.project_root.join("assets").join(&fixture.asset_id);
        let entries_before: Vec<_> = walk_all_files(&assets_dir);

        let error = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &source,
            None,
        )
        .unwrap_err();

        assert!(matches!(error, AppError::DuplicateAssetVersion));
        let entries_after: Vec<_> = walk_all_files(&assets_dir);
        assert_eq!(entries_before, entries_after);
    }

    fn walk_all_files(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if !dir.exists() {
            return out;
        }
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk_all_files(&path));
            } else {
                out.push(path);
            }
        }
        out.sort();
        out
    }

    #[test]
    fn rejects_non_image_input() {
        let fixture = AssetFixture::face_asset();
        let source = fixture.project_root.join("notes.txt");
        std::fs::write(&source, b"not an image").unwrap();

        let error = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &source,
            None,
        )
        .unwrap_err();

        assert!(matches!(error, AppError::UnsupportedImageFormat));
    }

    #[test]
    fn rejects_parent_version_from_a_different_asset() {
        let fixture = AssetFixture::face_asset();
        let other_asset =
            AssetService::create_asset(&fixture.project_root, "face_lock", "OTHER-FACE", None)
                .unwrap();
        let other_source = fixture.write_png("other.png", 32, 32);
        let other_version = AssetService::import_asset_version(
            &fixture.project_root,
            &other_asset.id,
            &other_source,
            None,
        )
        .unwrap();

        let source = fixture.write_png("mine.png", 32, 32);
        let error = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &source,
            Some(other_version.id),
        )
        .unwrap_err();

        assert!(matches!(error, AppError::ParentVersionMismatch));
    }

    #[test]
    fn rejects_a_parent_version_id_that_does_not_exist() {
        let fixture = AssetFixture::face_asset();
        let source = fixture.write_png("candidate.png", 32, 32);

        let error = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &source,
            Some("not-a-real-version-id".to_string()),
        )
        .unwrap_err();

        assert!(matches!(error, AppError::ParentVersionMismatch));
    }

    #[test]
    fn accepts_a_valid_parent_on_the_same_asset() {
        let fixture = AssetFixture::face_asset();
        let first = fixture.write_png("first.png", 32, 32);
        let first_version = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &first,
            None,
        )
        .unwrap();

        let second = fixture.write_png_with_pixel("second.png", 32, 32, [90, 91, 92, 255]);
        let second_version = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &second,
            Some(first_version.id.clone()),
        )
        .unwrap();

        assert_eq!(second_version.parent_version_id, Some(first_version.id));
    }

    #[test]
    fn importing_never_sets_a_canonical_version() {
        let fixture = AssetFixture::face_asset();
        let source = fixture.write_png("candidate.png", 32, 32);

        AssetService::import_asset_version(&fixture.project_root, &fixture.asset_id, &source, None)
            .unwrap();

        let asset = repository::get_asset(
            &db::open_connection(&fixture.project_root.join("project.db")).unwrap(),
            &fixture.asset_id,
        )
        .unwrap();
        assert!(asset.canonical_version_id.is_none());
    }

    #[test]
    fn importing_to_an_unknown_asset_returns_not_found() {
        let fixture = AssetFixture::face_asset();
        let source = fixture.write_png("candidate.png", 32, 32);

        let error = AssetService::import_asset_version(
            &fixture.project_root,
            "not-a-real-asset-id",
            &source,
            None,
        )
        .unwrap_err();

        assert!(matches!(error, AppError::AssetNotFound));
    }

    #[test]
    fn promotes_candidate_to_canonical() {
        let (fixture, version_one_id, _) = fixture_with_two_versions();

        let result =
            AssetService::promote_asset_version(&fixture.project_root, &version_one_id).unwrap();

        assert_eq!(
            result.asset.canonical_version_id.as_deref(),
            Some(version_one_id.as_str())
        );
        assert_eq!(result.promoted_version.status, "canonical");
        assert!(result.superseded_version_id.is_none());
    }

    #[test]
    fn promoting_second_version_supersedes_first() {
        let (fixture, version_one_id, version_two_id) = fixture_with_two_versions();

        AssetService::promote_asset_version(&fixture.project_root, &version_one_id).unwrap();
        let result =
            AssetService::promote_asset_version(&fixture.project_root, &version_two_id).unwrap();

        let reloaded =
            AssetService::get_asset_with_versions(&fixture.project_root, &fixture.asset_id)
                .unwrap();
        let first = reloaded
            .versions
            .iter()
            .find(|version| version.id == version_one_id)
            .unwrap();
        let second = reloaded
            .versions
            .iter()
            .find(|version| version.id == version_two_id)
            .unwrap();

        assert_eq!(first.status, "superseded");
        assert_eq!(second.status, "canonical");
        assert_eq!(
            result.superseded_version_id.as_deref(),
            Some(version_one_id.as_str())
        );
    }

    #[test]
    fn superseded_version_can_be_promoted_again() {
        let (fixture, version_one_id, version_two_id) = fixture_with_two_versions();

        AssetService::promote_asset_version(&fixture.project_root, &version_one_id).unwrap();
        AssetService::promote_asset_version(&fixture.project_root, &version_two_id).unwrap();
        AssetService::promote_asset_version(&fixture.project_root, &version_one_id).unwrap();

        let reloaded =
            AssetService::get_asset_with_versions(&fixture.project_root, &fixture.asset_id)
                .unwrap();
        let first = reloaded
            .versions
            .iter()
            .find(|version| version.id == version_one_id)
            .unwrap();
        let second = reloaded
            .versions
            .iter()
            .find(|version| version.id == version_two_id)
            .unwrap();

        assert_eq!(
            reloaded.asset.canonical_version_id.as_deref(),
            Some(version_one_id.as_str())
        );
        assert_eq!(first.status, "canonical");
        assert_eq!(second.status, "superseded");
    }

    #[test]
    fn failed_promotion_does_not_change_existing_canonical() {
        let (fixture, version_one_id, _) = fixture_with_two_versions();

        AssetService::promote_asset_version(&fixture.project_root, &version_one_id).unwrap();
        let error =
            AssetService::promote_asset_version(&fixture.project_root, "01JNONEXISTENTVERSION")
                .unwrap_err();

        assert!(matches!(error, AppError::AssetVersionNotFound));

        let reloaded =
            AssetService::get_asset_with_versions(&fixture.project_root, &fixture.asset_id)
                .unwrap();
        assert_eq!(
            reloaded.asset.canonical_version_id.as_deref(),
            Some(version_one_id.as_str())
        );
    }

    #[test]
    fn repromoting_current_canonical_is_a_noop() {
        let (fixture, version_one_id, _) = fixture_with_two_versions();

        let first =
            AssetService::promote_asset_version(&fixture.project_root, &version_one_id).unwrap();
        let before =
            AssetService::get_asset_with_versions(&fixture.project_root, &fixture.asset_id)
                .unwrap();
        let before_statuses: Vec<_> = before
            .versions
            .iter()
            .map(|version| (version.id.clone(), version.status.clone()))
            .collect();

        let repeated =
            AssetService::promote_asset_version(&fixture.project_root, &version_one_id).unwrap();
        let after = AssetService::get_asset_with_versions(&fixture.project_root, &fixture.asset_id)
            .unwrap();

        assert!(repeated.superseded_version_id.is_none());
        assert_eq!(repeated.promoted_version.status, "canonical");
        assert_eq!(repeated.asset.updated_at, first.asset.updated_at);
        assert_eq!(after.asset.updated_at, before.asset.updated_at);
        let after_statuses: Vec<_> = after
            .versions
            .iter()
            .map(|version| (version.id.clone(), version.status.clone()))
            .collect();
        assert_eq!(after_statuses, before_statuses);
    }

    #[test]
    fn repromoting_current_canonical_rejects_a_broken_canonical_invariant() {
        let (fixture, version_one_id, version_two_id) = fixture_with_two_versions();

        AssetService::promote_asset_version(&fixture.project_root, &version_one_id).unwrap();
        let conn = db::open_connection(&fixture.project_root.join("project.db")).unwrap();
        conn.execute(
            "UPDATE asset_versions SET status = 'canonical' WHERE id = ?1",
            rusqlite::params![version_two_id],
        )
        .unwrap();

        let error = AssetService::promote_asset_version(&fixture.project_root, &version_one_id)
            .unwrap_err();

        assert!(matches!(error, AppError::Database(_)));
    }

    /// Forces a failure *after* the media file has already been copied and
    /// renamed into place, but *during* thumbnail generation, by
    /// pre-occupying the exact filesystem path `generate_thumbnail` needs
    /// to create as the thumbnail's parent directory with a plain file
    /// instead. This is a realistic failure mode (some other process or a
    /// leftover file already sits where a directory needs to go) and
    /// requires no test-only hooks in production code -- it exercises the
    /// same `materialize_version_files` error path a real disk failure,
    /// permission error, or out-of-space condition would.
    #[test]
    fn rolls_back_and_cleans_up_when_thumbnail_generation_fails_after_media_is_copied() {
        let fixture = AssetFixture::face_asset();
        let source = fixture.write_png("candidate.png", 64, 64);

        // `thumbnails/` itself already exists (created by `ProjectService::
        // create`); occupy the subdirectory this import would need with a
        // plain file so `fs::create_dir_all` fails inside
        // `generate_thumbnail`.
        let thumbnail_parent_path = fixture
            .project_root
            .join("thumbnails")
            .join(&fixture.asset_id);
        fs::write(&thumbnail_parent_path, b"not a directory").unwrap();
        let source_bytes_before = fs::read(&source).unwrap();

        let error = AssetService::import_asset_version(
            &fixture.project_root,
            &fixture.asset_id,
            &source,
            None,
        )
        .unwrap_err();

        assert!(matches!(error, AppError::FileSystem(_)));

        // The DB transaction rolled back -- no version row exists.
        let conn = db::open_connection(&fixture.project_root.join("project.db")).unwrap();
        let versions = repository::list_asset_versions(&conn, &fixture.asset_id).unwrap();
        assert!(versions.is_empty());

        // The media file this attempt copied in (and its .tmp
        // intermediate) were removed by `cleanup_failed_import`.
        let asset_media_dir = fixture.project_root.join("assets").join(&fixture.asset_id);
        assert!(walk_all_files(&asset_media_dir).is_empty());

        // The user's original source file was never touched.
        assert!(source.exists());
        assert_eq!(fs::read(&source).unwrap(), source_bytes_before);
    }
}
