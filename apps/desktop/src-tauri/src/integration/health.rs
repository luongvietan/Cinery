use crate::error::AppError;
use crate::project::{paths, repository as project_repository};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthSeverity { Info, Warning, Error, Fatal }

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHealthIssue {
    pub code: String,
    pub severity: HealthSeverity,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub message: String,
    pub remediation: Option<String>,
}

/// Read-only integrity scan. It never repairs, deletes, or rewrites project state.
pub fn scan_project(project_root: &Path) -> Result<Vec<ProjectHealthIssue>, AppError> {
    let manifest = paths::read_manifest(project_root)?;
    let conn = crate::db::open_existing_connection(&project_root.join("project.db"))?;
    let project = project_repository::read_project(&conn)?;
    if project.id != manifest.project_id { return Err(AppError::ProjectIdentityMismatch); }
    let mut issues = Vec::new();
    let mut assets = conn.prepare("SELECT av.id, av.storage_path FROM asset_versions av JOIN assets a ON a.id = av.asset_id WHERE a.project_id = ?1")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = assets.query_map([&project.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (id, storage) = row.map_err(|e| AppError::Database(e.to_string()))?;
        if !project_root.join(&storage).is_file() {
            issues.push(issue("BROKEN_ASSET_FILE_REFERENCE", HealthSeverity::Error, "asset_version", Some(id), "Asset media file is missing.", Some("Restore the media file or inspect the asset version.")));
        }
    }
    let mut refs = conn.prepare("SELECT s.id, s.world_asset_version_id FROM scenes s WHERE s.project_id = ?1 AND s.world_asset_version_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = s.world_asset_version_id)")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = refs.query_map([&project.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows { let (id, version) = row.map_err(|e| AppError::Database(e.to_string()))?; issues.push(issue("MISSING_SCENE_WORLD_REFERENCE", HealthSeverity::Error, "scene", Some(id), &format!("Scene references missing World AssetVersion {version}."), Some("Choose an existing exact World version."))); }
    let mut refs = conn.prepare("SELECT sc.scene_id, sc.look_asset_version_id FROM scene_characters sc JOIN scenes s ON s.id = sc.scene_id WHERE s.project_id = ?1 AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = sc.look_asset_version_id)")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = refs.query_map([&project.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows { let (id, version) = row.map_err(|e| AppError::Database(e.to_string()))?; issues.push(issue("MISSING_SCENE_LOOK_REFERENCE", HealthSeverity::Error, "scene", Some(id), &format!("Scene references missing Look AssetVersion {version}."), Some("Choose an existing exact Look version."))); }
    Ok(issues)
}

fn issue(code: &str, severity: HealthSeverity, entity_type: &str, entity_id: Option<String>, message: &str, remediation: Option<&str>) -> ProjectHealthIssue {
    ProjectHealthIssue { code: code.into(), severity, entity_type: entity_type.into(), entity_id, message: message.into(), remediation: remediation.map(str::to_string) }
}
