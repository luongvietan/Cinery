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

fn check_asset_files_exist(
    conn: &rusqlite::Connection,
    project_id: &str,
    project_root: &Path,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare("SELECT av.id, av.storage_path FROM asset_versions av JOIN assets a ON a.id = av.asset_id WHERE a.project_id = ?1")
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

fn check_asset_version_owners(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT av.id, a.owner_entity_id FROM asset_versions av \
             JOIN assets a ON a.id = av.asset_id \
             WHERE a.project_id = ?1 AND a.owner_entity_id IS NOT NULL",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (av_id, owner_id) = row.map_err(|e| AppError::Database(e.to_string()))?;
        if let Some(owner) = owner_id {
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM entities WHERE id = ?1",
                    [&owner],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if !exists {
                issues.push(issue(
                    "ASSET_VERSION_OWNER_MISMATCH",
                    HealthSeverity::Warning,
                    "asset_version",
                    Some(av_id),
                    &format!("Asset version references missing owner entity {owner}."),
                    Some("Verify the owner entity still exists or reassign the asset."),
                ));
            }
        }
    }
    Ok(())
}

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

fn check_shot_scene_mismatches(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT shot.id, shot.scene_id FROM shots shot \
             WHERE shot.project_id = ?1 \
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

fn check_keyframe_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT kf.id, kf.asset_version_id FROM shot_keyframes kf \
             JOIN shots s ON s.id = kf.shot_id \
             WHERE s.project_id = ?1 \
             AND kf.asset_version_id IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = kf.asset_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (kf_id, version) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "MISSING_KEYFRAME",
            HealthSeverity::Error,
            "shot_keyframe",
            Some(kf_id),
            &format!("Keyframe references missing AssetVersion {version}."),
            Some("Re-assign or remove the keyframe."),
        ));
    }
    Ok(())
}

fn check_workflow_input_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT wr.id FROM workflow_runs wr \
             WHERE wr.project_id = ?1 \
             AND wr.input_asset_version_id IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = wr.input_asset_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| Ok(r.get::<_, String>(0)?))
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let run_id = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "WORKFLOW_INPUT_REFERENCE_MISSING",
            HealthSeverity::Warning,
            "workflow_run",
            Some(run_id),
            "Workflow run references deleted input asset version.",
            Some("Inspect historical context or inspect the run details."),
        ));
    }
    Ok(())
}

fn check_generation_output_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT gen.id, gen.output_asset_version_id FROM generations gen \
             JOIN workflow_runs wr ON wr.id = gen.workflow_run_id \
             WHERE wr.project_id = ?1 \
             AND gen.output_asset_version_id IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = gen.output_asset_version_id)",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    for row in rows {
        let (gen_id, version) = row.map_err(|e| AppError::Database(e.to_string()))?;
        issues.push(issue(
            "GENERATION_OUTPUT_REFERENCE_MISSING",
            HealthSeverity::Error,
            "generation",
            Some(gen_id),
            &format!("Generation references deleted output AssetVersion {version}."),
            Some("Investigate why the output was deleted."),
        ));
    }
    Ok(())
}

fn check_qa_target_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT qa.id, qa.target_asset_version_id FROM qa_runs qa \
             WHERE qa.project_id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM asset_versions av WHERE av.id = qa.target_asset_version_id)",
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

fn check_repair_parent_references(
    conn: &rusqlite::Connection,
    project_id: &str,
    issues: &mut Vec<ProjectHealthIssue>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT av.id, av.repair_parent_version_id FROM asset_versions av \
             JOIN assets a ON a.id = av.asset_id \
             WHERE a.project_id = ?1 \
             AND av.repair_parent_version_id IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM asset_versions parent WHERE parent.id = av.repair_parent_version_id)",
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
