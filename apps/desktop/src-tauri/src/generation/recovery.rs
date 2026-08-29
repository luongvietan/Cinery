use crate::db;
use crate::error::AppError;
use rusqlite::OptionalExtension;
use std::fs;
use std::path::{Path, PathBuf};

/// Moves only files matching the deterministic generated/<run>/<attempt>/<ordinal>.<ext>
/// layout when no corresponding database row exists. Unknown files and directories are left
/// untouched; recovery must never guess that arbitrary project data is a generated artifact.
pub fn quarantine_orphan_generated_files(project_root: &Path) -> Result<usize, AppError> {
    let generated_root = project_root.join("generated");
    if !generated_root.is_dir() {
        return Ok(0);
    }
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let mut quarantined = 0;
    for run_entry in read_directories(&generated_root)? {
        let Some(run_name) = run_entry.file_name() else {
            continue;
        };
        if run_name == "quarantine" {
            continue;
        }
        for attempt_entry in read_directories(&run_entry)? {
            let Some(attempt_name) = attempt_entry.file_name() else {
                continue;
            };
            for file_entry in read_files(&attempt_entry)? {
                let Some(filename) = file_entry.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if !is_generated_filename(filename) {
                    continue;
                }
                let storage_path = format!(
                    "generated/{}/{}/{}",
                    run_name.to_string_lossy(),
                    attempt_name.to_string_lossy(),
                    filename
                );
                let tracked: Option<i64> = conn
                    .query_row(
                        "SELECT 1 FROM generated_artifacts WHERE storage_path = ?1 LIMIT 1",
                        [&storage_path],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| AppError::Database(error.to_string()))?;
                if tracked.is_some() {
                    continue;
                }
                let destination = generated_root
                    .join("quarantine")
                    .join(run_name)
                    .join(attempt_name)
                    .join(filename);
                if destination.exists() {
                    continue;
                }
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        AppError::GenerationArtifactCaptureFailed(error.to_string())
                    })?;
                }
                fs::rename(&file_entry, &destination).map_err(|error| {
                    AppError::GenerationArtifactCaptureFailed(error.to_string())
                })?;
                quarantined += 1;
            }
        }
    }
    Ok(quarantined)
}

fn is_generated_filename(filename: &str) -> bool {
    let path = Path::new(filename);
    let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
        return false;
    };
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    stem.parse::<u64>().is_ok() && matches!(extension, "png" | "jpg" | "jpeg" | "webp")
}

fn read_directories(path: &Path) -> Result<Vec<PathBuf>, AppError> {
    let entries = fs::read_dir(path)
        .map_err(|error| AppError::GenerationArtifactCaptureFailed(error.to_string()))?;
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_dir())
                .map(|_| entry.path())
        })
        .collect())
}

fn read_files(path: &Path) -> Result<Vec<PathBuf>, AppError> {
    let entries = fs::read_dir(path)
        .map_err(|error| AppError::GenerationArtifactCaptureFailed(error.to_string()))?;
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_file())
                .map(|_| entry.path())
        })
        .collect())
}
