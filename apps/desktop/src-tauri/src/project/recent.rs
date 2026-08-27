use crate::error::AppError;
use crate::project::model::ProjectSummary;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const RECENT_PROJECTS_FILE_NAME: &str = "recent-projects.json";
const MAX_RECENT_PROJECTS: usize = 20;

/// One entry in the global recent-projects registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub project_id: String,
    pub root_path: String,
    pub name: String,
    pub last_opened_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentProjectsFile {
    #[serde(default)]
    projects: Vec<RecentProject>,
}

/// Records that `project` was just opened (or created), moving it to the
/// front of the registry. One entry is kept per project ID; the registry
/// is capped at `MAX_RECENT_PROJECTS`, evicting the oldest entries first.
///
/// Stale paths are intentionally left in place until an open attempt
/// fails against them -- staleness detection is not implemented here.
pub fn record_recent_project(config_dir: &Path, project: &ProjectSummary) -> Result<(), AppError> {
    let mut file = read_recent_file(config_dir);

    file.projects.retain(|entry| entry.project_id != project.id);

    file.projects.insert(
        0,
        RecentProject {
            project_id: project.id.clone(),
            root_path: project.root_path.clone(),
            name: project.name.clone(),
            last_opened_at: Utc::now().to_rfc3339(),
        },
    );

    file.projects.truncate(MAX_RECENT_PROJECTS);

    write_recent_file(config_dir, &file)
}

/// Lists recent projects, most recently opened first.
pub fn list_recent_projects(config_dir: &Path) -> Result<Vec<RecentProject>, AppError> {
    Ok(read_recent_file(config_dir).projects)
}

fn recent_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join(RECENT_PROJECTS_FILE_NAME)
}

/// Reads the recent-projects registry. This file is a purely cosmetic
/// convenience cache, not a source of truth -- a missing, empty, or
/// corrupted file must never block project creation/opening, so any read
/// or parse failure falls back to an empty registry rather than erroring.
fn read_recent_file(config_dir: &Path) -> RecentProjectsFile {
    let path = recent_file_path(config_dir);
    if !path.exists() {
        return RecentProjectsFile::default();
    }

    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(e) => {
            eprintln!("recent-projects.json could not be read, ignoring: {e}");
            return RecentProjectsFile::default();
        }
    };

    match serde_json::from_str(&contents) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("recent-projects.json is malformed, resetting registry: {e}");
            RecentProjectsFile::default()
        }
    }
}

fn write_recent_file(config_dir: &Path, file: &RecentProjectsFile) -> Result<(), AppError> {
    fs::create_dir_all(config_dir).map_err(|e| AppError::FileSystem(e.to_string()))?;

    let json =
        serde_json::to_string_pretty(file).map_err(|e| AppError::FileSystem(e.to_string()))?;

    let tmp_path = config_dir.join(format!("{RECENT_PROJECTS_FILE_NAME}.tmp"));
    fs::write(&tmp_path, json).map_err(|e| AppError::FileSystem(e.to_string()))?;
    fs::rename(&tmp_path, recent_file_path(config_dir))
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_summary(id: &str, name: &str) -> ProjectSummary {
        ProjectSummary {
            id: id.to_string(),
            name: name.to_string(),
            root_path: format!("/projects/{name}"),
            schema_version: 1,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn duplicate_project_id_updates_instead_of_appending() {
        let temp = tempdir().unwrap();

        record_recent_project(temp.path(), &sample_summary("1", "Red Door")).unwrap();
        record_recent_project(temp.path(), &sample_summary("1", "Red Door Renamed")).unwrap();

        let recents = list_recent_projects(temp.path()).unwrap();

        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].name, "Red Door Renamed");
    }

    #[test]
    fn ordering_is_newest_first() {
        let temp = tempdir().unwrap();

        record_recent_project(temp.path(), &sample_summary("1", "First")).unwrap();
        record_recent_project(temp.path(), &sample_summary("2", "Second")).unwrap();

        let recents = list_recent_projects(temp.path()).unwrap();

        assert_eq!(recents[0].project_id, "2");
        assert_eq!(recents[1].project_id, "1");
    }

    #[test]
    fn corrupted_registry_file_is_treated_as_empty_instead_of_erroring() {
        let temp = tempdir().unwrap();
        fs::write(
            temp.path().join(RECENT_PROJECTS_FILE_NAME),
            b"{ this is not valid json",
        )
        .unwrap();

        let recents = list_recent_projects(temp.path()).unwrap();
        assert!(recents.is_empty());

        // Recording after corruption should recover the file rather than
        // propagating the earlier corruption.
        record_recent_project(temp.path(), &sample_summary("1", "Red Door")).unwrap();
        let recents = list_recent_projects(temp.path()).unwrap();
        assert_eq!(recents.len(), 1);
    }

    #[test]
    fn empty_registry_file_is_treated_as_empty_instead_of_erroring() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join(RECENT_PROJECTS_FILE_NAME), b"").unwrap();

        let recents = list_recent_projects(temp.path()).unwrap();

        assert!(recents.is_empty());
    }

    #[test]
    fn evicts_oldest_entry_beyond_twenty() {
        let temp = tempdir().unwrap();

        for i in 0..21 {
            record_recent_project(
                temp.path(),
                &sample_summary(&i.to_string(), &format!("Project {i}")),
            )
            .unwrap();
        }

        let recents = list_recent_projects(temp.path()).unwrap();

        assert_eq!(recents.len(), MAX_RECENT_PROJECTS);
        assert_eq!(recents[0].project_id, "20");
        assert!(!recents.iter().any(|entry| entry.project_id == "0"));
    }
}
