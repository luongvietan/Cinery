use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const PROJECT_FORMAT: &str = "ai-cinematic-production-os";
pub const PROJECT_SCHEMA_VERSION: u32 = 1;

const MANIFEST_FILE_NAME: &str = "project.yaml";
const MANIFEST_TMP_FILE_NAME: &str = "project.yaml.tmp";

/// Deterministic set of subdirectories every project root gets. Only
/// `assets/` and `thumbnails/` are used functionally in Sprint 1; the rest
/// exist so the layout is stable for later work.
const PROJECT_SUBDIRECTORIES: &[&str] = &[
    "assets",
    "thumbnails",
    "canon",
    "characters",
    "worlds",
    "props",
    "scenes",
    "prompts",
    "generations",
    "exports",
];

/// Stable bootstrap marker written to `project.yaml`. This is NOT the
/// mutable source of truth for project metadata -- the project name lives
/// only in SQLite. This file exists so a directory can be identified as an
/// AI Cinematic Production OS project without opening its database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub format: String,
    pub project_id: String,
    pub schema_version: u32,
}

/// Ensures `root` is usable as a brand-new project directory: either it
/// does not exist yet (in which case it is created), or it exists and is
/// completely empty. Any other state is rejected so we never silently
/// overwrite unrelated user files.
pub fn ensure_empty_or_new_directory(root: &Path) -> Result<(), AppError> {
    if !root.exists() {
        fs::create_dir_all(root).map_err(|e| AppError::FileSystem(e.to_string()))?;
        return Ok(());
    }

    let mut entries = fs::read_dir(root).map_err(|e| AppError::FileSystem(e.to_string()))?;
    if entries.next().is_some() {
        return Err(AppError::ProjectDirectoryNotEmpty);
    }

    Ok(())
}

/// Creates the full deterministic project directory layout under `root`.
pub fn create_project_directories(root: &Path) -> Result<(), AppError> {
    for subdir in PROJECT_SUBDIRECTORIES {
        fs::create_dir_all(root.join(subdir)).map_err(|e| AppError::FileSystem(e.to_string()))?;
    }
    Ok(())
}

/// Writes `project.yaml` atomically: the manifest is written to a temporary
/// file in the same directory, then renamed into place.
pub fn write_manifest(root: &Path, project_id: &str) -> Result<(), AppError> {
    let manifest = ProjectManifest {
        format: PROJECT_FORMAT.to_string(),
        project_id: project_id.to_string(),
        schema_version: PROJECT_SCHEMA_VERSION,
    };

    let yaml =
        serde_yaml::to_string(&manifest).map_err(|e| AppError::FileSystem(e.to_string()))?;

    let tmp_path = root.join(MANIFEST_TMP_FILE_NAME);
    let final_path = root.join(MANIFEST_FILE_NAME);

    fs::write(&tmp_path, yaml).map_err(|e| AppError::FileSystem(e.to_string()))?;
    fs::rename(&tmp_path, &final_path).map_err(|e| AppError::FileSystem(e.to_string()))?;

    Ok(())
}

/// Reads and validates `project.yaml`, returning
/// `AppError::InvalidProjectDirectory` if the file is missing, malformed,
/// or does not match the expected format/schema version.
pub fn read_manifest(root: &Path) -> Result<ProjectManifest, AppError> {
    let path = root.join(MANIFEST_FILE_NAME);

    let contents = fs::read_to_string(&path).map_err(|_| AppError::InvalidProjectDirectory)?;
    let manifest: ProjectManifest =
        serde_yaml::from_str(&contents).map_err(|_| AppError::InvalidProjectDirectory)?;

    if manifest.format != PROJECT_FORMAT {
        return Err(AppError::InvalidProjectDirectory);
    }
    if manifest.schema_version != PROJECT_SCHEMA_VERSION {
        return Err(AppError::InvalidProjectDirectory);
    }
    if manifest.project_id.trim().is_empty() {
        return Err(AppError::InvalidProjectDirectory);
    }

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_directory_when_missing() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("new-project");

        ensure_empty_or_new_directory(&root).unwrap();

        assert!(root.is_dir());
    }

    #[test]
    fn accepts_existing_empty_directory() {
        let temp = tempdir().unwrap();

        ensure_empty_or_new_directory(temp.path()).unwrap();
    }

    #[test]
    fn rejects_existing_non_empty_directory() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("existing.txt"), b"data").unwrap();

        let error = ensure_empty_or_new_directory(temp.path()).unwrap_err();

        assert!(matches!(error, AppError::ProjectDirectoryNotEmpty));
    }

    #[test]
    fn writes_and_reads_manifest_round_trip() {
        let temp = tempdir().unwrap();

        write_manifest(temp.path(), "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let manifest = read_manifest(temp.path()).unwrap();

        assert_eq!(manifest.format, PROJECT_FORMAT);
        assert_eq!(manifest.project_id, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        assert_eq!(manifest.schema_version, PROJECT_SCHEMA_VERSION);
        assert!(!temp.path().join(MANIFEST_TMP_FILE_NAME).exists());
    }

    #[test]
    fn rejects_manifest_with_wrong_format() {
        let temp = tempdir().unwrap();
        fs::write(
            temp.path().join(MANIFEST_FILE_NAME),
            "format: something-else\nproject_id: abc\nschema_version: 1\n",
        )
        .unwrap();

        let error = read_manifest(temp.path()).unwrap_err();

        assert!(matches!(error, AppError::InvalidProjectDirectory));
    }

    #[test]
    fn rejects_missing_manifest() {
        let temp = tempdir().unwrap();

        let error = read_manifest(temp.path()).unwrap_err();

        assert!(matches!(error, AppError::InvalidProjectDirectory));
    }

    #[test]
    fn creates_all_deterministic_subdirectories() {
        let temp = tempdir().unwrap();

        create_project_directories(temp.path()).unwrap();

        for subdir in PROJECT_SUBDIRECTORIES {
            assert!(temp.path().join(subdir).is_dir());
        }
    }
}
