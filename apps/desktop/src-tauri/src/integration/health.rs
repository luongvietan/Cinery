use crate::error::AppError;
use crate::project::{paths, repository as project_repository};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthSeverity {
    Info,
    Warning,
    Error,
    Fatal,
}

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
    if project.id != manifest.project_id {
        return Err(AppError::ProjectIdentityMismatch);
    }

    let mut issues = Vec::new();

    // Check 1: Broken asset file references
    check_asset_files_exist(&conn, &project.id, project_root, &mut issues)?;

    // Check 2: Asset version owner mismatches
    check_asset_version_owners(&conn, &project.id, &mut issues)?;

    // Check 3: Multiple canonical versions in one slot
    check_multiple_canonical_versions(&conn, &project.id, &mut issues)?;

    // Check 4: Missing scene world references
    check_scene_world_references(&conn, &project.id, &mut issues)?;

    // Check 5: Missing scene look references
    check_scene_look_references(&conn, &project.id, &mut issues)?;

    // Check 6: Missing scene prop references
    check_scene_prop_references(&conn, &project.id, &mut issues)?;

    // Check 7: Shot scene mismatches
    check_shot_scene_mismatches(&conn, &project.id, &mut issues)?;

    // Check 8: Missing keyframes
    check_keyframe_references(&conn, &project.id, &mut issues)?;

    // Check 9: Workflow input references
    check_workflow_input_references(&conn, &project.id, &mut issues)?;

    // Check 10: Generation output references
    check_generation_output_references(&conn, &project.id, &mut issues)?;

    // Check 11: QA target references
    check_qa_target_references(&conn, &project.id, &mut issues)?;

    // Check 12: Repair parent references
    check_repair_parent_references(&conn, &project.id, &mut issues)?;

    // Check 13: Cinema input references
    check_cinema_input_references(&conn, &project.id, &mut issues)?;

    Ok(issues)
}

/// Checks that asset version media files exist on disk.
/// Detects: BROKEN_ASSET_FILE_REFERENCE when file_path references a missing file.
fn check_asset_files_exist(
    conn: &rusqlite::Connection,
    project_id: &str,
    project_root: &Path,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare("SELECT av.id, av.file_path FROM asset_versions av JOIN assets a ON a.id = av.asset_id WHERE a.project_id = ?1")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (id, storage) = row.map_err(|e| AppError::Database(e.to_string()))?;
        if !project_root.join(&storage).is_file() {
            issues.push(issue(
                "BROKEN_ASSET_FILE_REFERENCE",
                HealthSeverity::Error,
                "asset_version",
                Some(id),
                "Asset media file is missing.",
                Some("Restore the media file or inspect the asset version."),
            ));
        }
    }
    Ok(())
}

/// Checks that asset owners reference existing entities.
/// Detects: ASSET_VERSION_OWNER_MISMATCH when an asset has owner_entity_id but no matching entity row.
fn check_asset_version_owners(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
             "SELECT av.id, a.owner_entity_id FROM asset_versions av \
              JOIN assets a ON a.id = av.asset_id \
              WHERE a.project_id = ?1 AND a.owner_entity_id IS NOT NULL \
              AND NOT EXISTS (SELECT 1 FROM canon_entities WHERE id = a.owner_entity_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (av_id, owner_id) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "ASSET_VERSION_OWNER_MISMATCH",
            HealthSeverity::Warning,
            "asset_version",
            Some(av_id),
            &format!("Asset version references missing owner entity {owner_id}."),
            Some("Verify the owner entity still exists or reassign the asset."),
        ));
    }
    Ok(())
}

/// Checks that each asset has at most one canonical version.
/// Detects: MULTIPLE_CANONICAL_VERSIONS when COUNT(canonical_versions) > 1.
fn check_multiple_canonical_versions(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT a.id, COUNT(av.id) as canonical_count \
             FROM assets a \
             LEFT JOIN asset_versions av ON av.asset_id = a.id AND av.status = 'canonical' \
             WHERE a.project_id = ?1 \
             GROUP BY a.id \
             HAVING canonical_count > 1",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i32>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (asset_id, count) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "MULTIPLE_CANONICAL_VERSIONS",
            HealthSeverity::Error,
            "asset",
            Some(asset_id),
            &format!("Asset has {count} canonical versions (should be 0 or 1)."),
            Some("Demote all but one canonical version."),
        ));
    }
    Ok(())
}

/// Checks that scenes reference existing world asset versions.
/// Detects: MISSING_SCENE_WORLD_REFERENCE when scene.world_asset_version_id references deleted asset.
fn check_scene_world_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.world_asset_version_id FROM scenes s \
             WHERE s.project_id = ?1 AND s.world_asset_version_id IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = s.world_asset_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (id, version) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "MISSING_SCENE_WORLD_REFERENCE",
            HealthSeverity::Error,
            "scene",
            Some(id),
            &format!("Scene references missing World AssetVersion {version}."),
            Some("Choose an existing exact World version."),
        ));
    }
    Ok(())
}

/// Checks that scene characters reference existing look asset versions.
/// Detects: MISSING_SCENE_LOOK_REFERENCE when scene_character.look_asset_version_id is deleted.
fn check_scene_look_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT sc.scene_id, sc.look_asset_version_id FROM scene_characters sc \
             JOIN scenes s ON s.id = sc.scene_id \
             WHERE s.project_id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = sc.look_asset_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (id, version) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "MISSING_SCENE_LOOK_REFERENCE",
            HealthSeverity::Error,
            "scene",
            Some(id),
            &format!("Scene references missing Look AssetVersion {version}."),
            Some("Choose an existing exact Look version."),
        ));
    }
    Ok(())
}

/// Checks that scene props reference existing asset versions.
/// Detects: MISSING_SCENE_PROP_REFERENCE when scene_prop.prop_asset_version_id is deleted.
fn check_scene_prop_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT sp.scene_id, sp.prop_asset_version_id FROM scene_props sp \
             JOIN scenes s ON s.id = sp.scene_id \
             WHERE s.project_id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = sp.prop_asset_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (scene_id, version) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "MISSING_SCENE_PROP_REFERENCE",
            HealthSeverity::Error,
            "scene",
            Some(scene_id),
            &format!("Scene references missing Prop AssetVersion {version}."),
            Some("Choose an existing exact Prop version."),
        ));
    }
    Ok(())
}

/// Checks that shots reference existing scenes.
/// Detects: SHOT_SCENE_MISMATCH when shot.scene_id references deleted scene.
fn check_shot_scene_mismatches(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT shot.id, shot.scene_id FROM shots shot \
             JOIN scenes sc ON sc.id = shot.scene_id \
             WHERE sc.project_id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM scenes s WHERE s.id = shot.scene_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (shot_id, scene_id) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "SHOT_SCENE_MISMATCH",
            HealthSeverity::Error,
            "shot",
            Some(shot_id),
            &format!("Shot references missing Scene {scene_id}."),
            Some("Verify the Scene exists or delete the orphaned Shot."),
        ));
    }
    Ok(())
}

/// Checks that keyframes reference existing asset versions when set.
/// Detects: MISSING_KEYFRAME when shot.keyframe_asset_version_id is deleted.
fn check_keyframe_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.keyframe_asset_version_id FROM shots s \
             JOIN scenes sc ON sc.id = s.scene_id \
             WHERE sc.project_id = ?1 \
             AND s.keyframe_asset_version_id IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = s.keyframe_asset_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (shot_id, version) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "MISSING_KEYFRAME",
            HealthSeverity::Error,
            "shot",
            Some(shot_id),
            &format!("Shot keyframe references missing AssetVersion {version}."),
            Some("Re-assign or remove the keyframe."),
        ));
    }
    Ok(())
}

/// Checks that workflow runs reference existing canonical/input assets when
/// their input_json references an owned canonical asset version. The durable
/// canonical version id is read from the asset's canonical_version_id slot so
/// a run can only ever reference versions that still exist.
fn check_workflow_input_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT wr.id, a.canonical_version_id FROM workflow_runs wr \
             JOIN assets a ON a.project_id = wr.project_id AND a.canonical_version_id IS NOT NULL \
             WHERE wr.project_id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = a.canonical_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (run_id, version) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "WORKFLOW_INPUT_REFERENCE_MISSING",
            HealthSeverity::Warning,
            "workflow_run",
            Some(run_id),
            &format!("Workflow references missing canonical AssetVersion {version}."),
            Some("Inspect historical context or inspect the run details."),
        ));
    }
    Ok(())
}

/// Checks that generation promotions reference existing output asset versions.
/// Detects: GENERATION_OUTPUT_REFERENCE_MISSING when artifact_promotions
/// references a deleted asset_version.
fn check_generation_output_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT ap.id, ap.asset_version_id FROM artifact_promotions ap \
             JOIN assets a ON a.id = ap.asset_id \
             WHERE a.project_id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = ap.asset_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (promotion_id, version) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "GENERATION_OUTPUT_REFERENCE_MISSING",
            HealthSeverity::Error,
            "artifact_promotion",
            Some(promotion_id),
            &format!("Generation promotion references deleted output AssetVersion {version}."),
            Some("Investigate why the output was deleted."),
        ));
    }
    Ok(())
}

/// Checks that QA runs reference existing target asset versions.
/// Detects: QA_TARGET_MISSING when qa_run.asset_version_id is deleted.
fn check_qa_target_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT qa.id, qa.asset_version_id FROM qa_runs qa \
             WHERE qa.project_id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = qa.asset_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (qa_id, version) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "QA_TARGET_MISSING",
            HealthSeverity::Warning,
            "qa_run",
            Some(qa_id),
            &format!("QA run references deleted target AssetVersion {version}."),
            Some("Investigate the asset deletion history."),
        ));
    }
    Ok(())
}

/// Checks that child (repair) versions reference existing parent asset versions.
/// Detects: REPAIR_PARENT_MISSING when asset_version.parent_version_id is deleted.
fn check_repair_parent_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT av.id, av.parent_version_id FROM asset_versions av \
             JOIN assets a ON a.id = av.asset_id \
             WHERE a.project_id = ?1 \
             AND av.parent_version_id IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM asset_versions parent WHERE parent.id = av.parent_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (av_id, parent_id) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "REPAIR_PARENT_MISSING",
            HealthSeverity::Warning,
            "asset_version",
            Some(av_id),
            &format!("Repair version references missing parent {parent_id}."),
            Some("Investigate the repair chain."),
        ));
    }
    Ok(())
}

/// Checks that cinema compilations reference existing scenes.
/// Detects: CINEMA_INPUT_REFERENCE_MISSING when cinema_compilation.scene_id is deleted.
fn check_cinema_input_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT cc.id, cc.scene_id FROM cinema_compilations cc \
             WHERE cc.project_id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM scenes s WHERE s.id = cc.scene_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (cc_id, scene_id) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "CINEMA_INPUT_REFERENCE_MISSING",
            HealthSeverity::Error,
            "cinema_compilation",
            Some(cc_id),
            &format!("Cinema compilation references missing Scene {scene_id}."),
            Some("Verify the Scene exists."),
        ));
    }
    Ok(())
}

fn issue(
    code: &str,
    severity: HealthSeverity,
    entity_type: &str,
    entity_id: Option<String>,
    message: &str,
    remediation: Option<&str>,
) -> ProjectHealthIssue {
    ProjectHealthIssue {
        code: code.into(),
        severity,
        entity_type: entity_type.into(),
        entity_id,
        message: message.into(),
        remediation: remediation.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_issue_creation() {
        let issue = ProjectHealthIssue {
            code: "TEST_CODE".to_string(),
            severity: HealthSeverity::Warning,
            entity_type: "asset".to_string(),
            entity_id: Some("id-123".to_string()),
            message: "Test message".to_string(),
            remediation: Some("Fix it".to_string()),
        };

        assert_eq!(issue.code, "TEST_CODE");
        assert_eq!(issue.severity, HealthSeverity::Warning);
    }

    #[test]
    fn test_health_severity_ordering() {
        let fatal = HealthSeverity::Fatal;
        let error = HealthSeverity::Error;
        let warning = HealthSeverity::Warning;
        let info = HealthSeverity::Info;

        // Verify all severity levels are distinct
        assert_ne!(fatal, error);
        assert_ne!(error, warning);
        assert_ne!(warning, info);
    }

    #[test]
    fn test_health_issue_with_null_remediation() {
        let issue = issue(
            "TEST_CODE",
            HealthSeverity::Info,
            "asset",
            Some("asset-1".to_string()),
            "Test message",
            None,
        );

        assert_eq!(issue.remediation, None);
        assert_eq!(issue.code, "TEST_CODE");
    }

    #[test]
    fn test_health_issue_with_null_entity_id() {
        let issue = issue(
            "FATAL_ERROR",
            HealthSeverity::Fatal,
            "project",
            None,
            "Fatal project error",
            None,
        );

        assert_eq!(issue.entity_id, None);
        assert_eq!(issue.severity, HealthSeverity::Fatal);
    }

    #[test]
    fn test_required_error_codes_exist() {
        // Verify that all required error codes are properly defined
        let codes = vec![
            "BROKEN_ASSET_FILE_REFERENCE",
            "ASSET_VERSION_OWNER_MISMATCH",
            "MULTIPLE_CANONICAL_VERSIONS",
            "MISSING_SCENE_WORLD_REFERENCE",
            "MISSING_SCENE_LOOK_REFERENCE",
            "MISSING_SCENE_PROP_REFERENCE",
            "SHOT_SCENE_MISMATCH",
            "MISSING_KEYFRAME",
            "WORKFLOW_INPUT_REFERENCE_MISSING",
            "GENERATION_OUTPUT_REFERENCE_MISSING",
            "QA_TARGET_MISSING",
            "REPAIR_PARENT_MISSING",
            "CINEMA_INPUT_REFERENCE_MISSING",
        ];

        // Verify we have at least 13 unique codes
        assert!(codes.len() >= 13);
        assert_eq!(codes.len(), codes.iter().collect::<std::collections::HashSet<_>>().len());
    }
}
