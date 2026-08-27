use crate::db;
use crate::db::migrations::run_migrations;
use crate::error::AppError;
use crate::project::model::{ProjectRecord, ProjectSummary};
use crate::project::paths::{self, PROJECT_SCHEMA_VERSION};
use crate::project::repository;
use chrono::Utc;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use ulid::Ulid;

pub struct ProjectService;

impl ProjectService {
    /// Bootstraps a brand-new project at `root`: validates the name,
    /// requires the directory be empty (or absent), creates the
    /// deterministic directory layout and database, records the project
    /// row, and writes the manifest last so a project is only considered
    /// valid once every prior step has succeeded.
    ///
    /// If any step fails, filesystem entries created during this call are
    /// removed. A directory the caller already had on disk before calling
    /// `create` is never itself deleted -- only files/subdirectories added
    /// during this attempt are cleaned up.
    pub fn create(root: &Path, name: &str) -> Result<ProjectSummary, AppError> {
        let trimmed_name = validate_name(name)?;

        let existed_before = root.exists();
        let pre_existing_entries = snapshot_entries(root);

        let result = Self::create_inner(root, &trimmed_name);

        if result.is_err() {
            cleanup_partial_create(root, existed_before, &pre_existing_entries);
        }

        result
    }

    fn create_inner(root: &Path, name: &str) -> Result<ProjectSummary, AppError> {
        paths::ensure_empty_or_new_directory(root)?;
        paths::create_project_directories(root)?;

        let project_id = Ulid::new().to_string();
        let now = Utc::now().to_rfc3339();

        let db_path = root.join("project.db");
        let mut conn = db::open_connection(&db_path)?;
        run_migrations(&mut conn)?;

        let record = ProjectRecord {
            id: project_id.clone(),
            name: name.to_string(),
            created_at: now.clone(),
            updated_at: now,
            schema_version: PROJECT_SCHEMA_VERSION,
        };
        repository::insert_project(&conn, &record)?;

        // Written last: a project is only "real" once its manifest exists.
        paths::write_manifest(root, &project_id)?;

        Ok(to_summary(record, root))
    }

    /// Opens an existing project at `root`: reads the manifest, opens the
    /// database, runs any pending migrations, and cross-checks the
    /// database's project ID against the manifest's.
    pub fn open(root: &Path) -> Result<ProjectSummary, AppError> {
        let manifest = paths::read_manifest(root)?;

        let db_path = root.join("project.db");
        let mut conn = db::open_connection(&db_path)?;
        run_migrations(&mut conn)?;

        let record = repository::read_project(&conn)?;

        if record.id != manifest.project_id {
            return Err(AppError::ProjectIdentityMismatch);
        }

        Ok(to_summary(record, root))
    }
}

fn to_summary(record: ProjectRecord, root: &Path) -> ProjectSummary {
    ProjectSummary {
        id: record.id,
        name: record.name,
        root_path: root.to_string_lossy().to_string(),
        schema_version: record.schema_version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn validate_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 120 {
        return Err(AppError::InvalidProjectName);
    }
    Ok(trimmed.to_string())
}

/// Validates a raw `root_path` string received from the frontend before it
/// is turned into a `Path`, mirroring
/// `packages/domain/src/project.ts`'s `validateProjectRootPath`.
pub fn validate_root_path(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::InvalidProjectPath);
    }
    Ok(())
}

fn snapshot_entries(root: &Path) -> HashSet<PathBuf> {
    fs::read_dir(root)
        .map(|entries| entries.filter_map(|entry| entry.ok().map(|e| e.path())).collect())
        .unwrap_or_default()
}

/// Removes filesystem state created by a failed `create` attempt.
///
/// - If `root` did not exist before the attempt, it is safe to remove
///   entirely (we created it).
/// - If `root` already existed, it is never deleted; only entries that
///   were not present in `pre_existing_entries` are removed, since those
///   are the ones this attempt introduced.
fn cleanup_partial_create(root: &Path, existed_before: bool, pre_existing_entries: &HashSet<PathBuf>) {
    if !root.exists() {
        return;
    }

    if !existed_before {
        let _ = fs::remove_dir_all(root);
        return;
    }

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if pre_existing_entries.contains(&path) {
                continue;
            }
            if path.is_dir() {
                let _ = fs::remove_dir_all(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_project_manifest_database_and_directories() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("red-door");
        std::fs::create_dir(&root).unwrap();

        let project = ProjectService::create(&root, "Red Door").unwrap();

        assert_eq!(project.name, "Red Door");
        assert!(root.join("project.yaml").exists());
        assert!(root.join("project.db").exists());
        assert!(root.join("assets").is_dir());
        assert!(root.join("thumbnails").is_dir());
        assert!(root.join("canon").is_dir());
        assert!(root.join("characters").is_dir());
        assert!(root.join("worlds").is_dir());
        assert!(root.join("props").is_dir());
        assert!(root.join("scenes").is_dir());
        assert!(root.join("prompts").is_dir());
        assert!(root.join("generations").is_dir());
        assert!(root.join("exports").is_dir());
    }

    #[test]
    fn rejects_non_empty_non_project_directory() {
        let temp = tempdir().unwrap();
        std::fs::write(temp.path().join("existing.txt"), b"data").unwrap();

        let error = ProjectService::create(temp.path(), "Red Door").unwrap_err();

        assert!(matches!(error, AppError::ProjectDirectoryNotEmpty));
    }

    #[test]
    fn reopens_created_project_with_same_identity() {
        let temp = tempdir().unwrap();

        let created = ProjectService::create(temp.path(), "Red Door").unwrap();
        let opened = ProjectService::open(temp.path()).unwrap();

        assert_eq!(created.id, opened.id);
        assert_eq!(created.name, opened.name);
    }

    #[test]
    fn rejects_open_when_database_id_diverges_from_manifest() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        let created = ProjectService::create(root, "Red Door").unwrap();

        // Corrupt the database row's id so it no longer matches the id
        // recorded in project.yaml, simulating a manifest/database
        // divergence (e.g. a manifest restored from a different backup).
        let conn = db::open_connection(&root.join("project.db")).unwrap();
        conn.execute(
            "UPDATE projects SET id = ?1 WHERE id = ?2",
            rusqlite::params!["different-id", created.id],
        )
        .unwrap();
        drop(conn);

        let error = ProjectService::open(root).unwrap_err();

        assert!(matches!(error, AppError::ProjectIdentityMismatch));
    }

    #[test]
    fn rejects_blank_name() {
        let temp = tempdir().unwrap();

        let error = ProjectService::create(temp.path(), "   ").unwrap_err();

        assert!(matches!(error, AppError::InvalidProjectName));
    }

    #[test]
    fn rejects_blank_root_path() {
        let error = validate_root_path("   ").unwrap_err();

        assert!(matches!(error, AppError::InvalidProjectPath));
    }

    #[test]
    fn accepts_non_blank_root_path() {
        validate_root_path("C:/projects/red-door").unwrap();
    }

    #[test]
    fn cleanup_removes_freshly_created_root_when_it_did_not_exist_before() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("brand-new");
        fs::create_dir_all(root.join("assets")).unwrap();

        cleanup_partial_create(&root, false, &HashSet::new());

        assert!(!root.exists());
    }

    #[test]
    fn cleanup_preserves_pre_existing_directory_and_only_removes_new_entries() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("keep-me.txt"), b"user data").unwrap();
        let pre_existing = snapshot_entries(root);

        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(root.join("project.db"), b"").unwrap();

        cleanup_partial_create(root, true, &pre_existing);

        assert!(root.exists());
        assert!(root.join("keep-me.txt").exists());
        assert!(!root.join("assets").exists());
        assert!(!root.join("project.db").exists());
    }

    #[test]
    fn does_not_delete_a_user_supplied_directory_on_failure() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        let error = ProjectService::create(root, "").unwrap_err();

        assert!(matches!(error, AppError::InvalidProjectName));
        // The user-supplied directory itself must still exist.
        assert!(root.exists());
    }
}
