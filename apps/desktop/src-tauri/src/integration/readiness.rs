use crate::db;
use crate::error::AppError;
use crate::project::service::ProjectService;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessStatus {
    Pending,
    Complete,
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewAction {
    pub id: String,
    pub title: String,
    pub destination: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessStep {
    pub id: String,
    pub title: String,
    pub status: ReadinessStatus,
    pub detail: String,
    pub action: Option<OverviewAction>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReadiness {
    pub status: ReadinessStatus,
    pub next_action: Option<OverviewAction>,
    pub steps: Vec<ReadinessStep>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHealthSummary {
    pub open_protected_tbd_count: i64,
    pub open_tbd_count: i64,
    pub active_job_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundJobSummary {
    pub id: String,
    pub operation_id: String,
    pub status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOverview {
    pub readiness: ProjectReadiness,
    pub health_summary: ProjectHealthSummary,
    pub recent_activity: Vec<ActivityItem>,
    pub active_jobs: Vec<BackgroundJobSummary>,
}

/// Produces a product-level read model from existing P0-P8 records. It does
/// not persist a readiness state or rewrite exact scene references: a scene's
/// pinned versions remain evidence even after their asset moves on.
pub fn get_project_overview(project_root: &Path) -> Result<ProjectOverview, AppError> {
    let project = ProjectService::open(project_root)?;
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let readiness = derive_readiness(&conn, &project.id)?;
    let active_jobs = list_active_jobs(&conn, &project.id)?;
    let health_summary = ProjectHealthSummary {
        open_protected_tbd_count: count_tbds(&conn, &project.id, true)?,
        open_tbd_count: count_tbds(&conn, &project.id, false)?,
        active_job_count: active_jobs.len() as i64,
    };
    Ok(ProjectOverview {
        readiness,
        health_summary,
        recent_activity: list_recent_activity(&conn, &project.id)?,
        active_jobs,
    })
}

fn derive_readiness(conn: &Connection, project_id: &str) -> Result<ProjectReadiness, AppError> {
    let characters = character_ids(conn, project_id)?;
    if characters.is_empty() {
        return Ok(with_next(
            vec![pending_step(
                "story_canon",
                "Story Canon",
                "Create the story foundation before production.",
                "canon",
            )],
            action("story_canon", "Story Canon", "canon"),
        ));
    }

    let mut steps = vec![complete_step(
        "story_canon",
        "Story Canon",
        "A character production path is active.",
    )];
    for character_id in &characters {
        if !has_canonical_asset(conn, project_id, "face_lock", Some(character_id))? {
            steps.push(pending_step(
                "face_lock",
                "Face Lock",
                "Promote a canonical face for every character.",
                "production",
            ));
            return Ok(with_next(
                steps,
                action("face_lock", "Face Lock", "production"),
            ));
        }
    }
    steps.push(complete_step(
        "face_lock",
        "Canonical Face",
        "Every character has a canonical face reference.",
    ));

    for character_id in &characters {
        if !has_canonical_asset(conn, project_id, "outfit", Some(character_id))? {
            steps.push(pending_step(
                "character_look",
                "Character Look",
                "Promote a canonical look for every character.",
                "assets",
            ));
            return Ok(with_next(
                steps,
                action("character_look", "Character Look", "assets"),
            ));
        }
    }
    steps.push(complete_step(
        "character_look",
        "Canonical Look",
        "Every character has a canonical look reference.",
    ));

    for character_id in &characters {
        if !has_canonical_asset(conn, project_id, "character_sheet", Some(character_id))? {
            steps.push(pending_step(
                "character_sheet",
                "Character Sheet",
                "Promote a canonical sheet for every character.",
                "assets",
            ));
            return Ok(with_next(
                steps,
                action("character_sheet", "Character Sheet", "assets"),
            ));
        }
    }
    steps.push(complete_step(
        "character_sheet",
        "Character Sheet",
        "Every character has a canonical sheet.",
    ));

    if !has_canonical_asset(conn, project_id, "world_plate", None)? {
        steps.push(pending_step(
            "world_plate",
            "World Plate",
            "Promote a canonical world plate before staging scenes.",
            "assets",
        ));
        return Ok(with_next(
            steps,
            action("world_plate", "World Plate", "assets"),
        ));
    }
    steps.push(complete_step(
        "world_plate",
        "World Plate",
        "A canonical world plate is available.",
    ));

    let scene_id = valid_scene_id(conn, project_id)?;
    let Some(scene_id) = scene_id else {
        steps.push(pending_step(
            "scene",
            "Scene",
            "Stage a scene with pinned character references and a shot.",
            "production",
        ));
        return Ok(with_next(steps, action("scene", "Scene", "production")));
    };
    steps.push(complete_step(
        "scene",
        "Scene",
        "A staged scene has durable exact references.",
    ));

    if has_compilation(conn, project_id, &scene_id)? {
        steps.push(complete_step(
            "cinema_compilation",
            "Cinema Compilation",
            "A provider-neutral cinema prompt was compiled and persisted.",
        ));
        return Ok(ProjectReadiness {
            status: ReadinessStatus::Complete,
            next_action: None,
            steps,
        });
    }

    if has_blocking_scene_tbd(conn, project_id, &scene_id)? {
        steps.push(blocked_step(
            "cinema_compilation",
            "Cinema Compilation",
            "A protected open TBD blocks this scene from compilation.",
            "canon",
        ));
        return Ok(ProjectReadiness {
            status: ReadinessStatus::Blocked,
            next_action: Some(action(
                "resolve_protected_tbd",
                "Resolve protected TBD",
                "canon",
            )),
            steps,
        });
    }

    steps.push(pending_step(
        "cinema_compilation",
        "Cinema Compilation",
        "Compile the staged scene into a durable provider-neutral prompt.",
        "production",
    ));
    Ok(with_next(
        steps,
        action("cinema_compilation", "Cinema Compilation", "production"),
    ))
}

fn with_next(steps: Vec<ReadinessStep>, next_action: OverviewAction) -> ProjectReadiness {
    ProjectReadiness {
        status: ReadinessStatus::Pending,
        next_action: Some(next_action),
        steps,
    }
}

fn action(id: &str, title: &str, destination: &str) -> OverviewAction {
    OverviewAction {
        id: id.into(),
        title: title.into(),
        destination: destination.into(),
    }
}

fn complete_step(id: &str, title: &str, detail: &str) -> ReadinessStep {
    ReadinessStep {
        id: id.into(),
        title: title.into(),
        status: ReadinessStatus::Complete,
        detail: detail.into(),
        action: None,
    }
}

fn pending_step(id: &str, title: &str, detail: &str, destination: &str) -> ReadinessStep {
    ReadinessStep {
        id: id.into(),
        title: title.into(),
        status: ReadinessStatus::Pending,
        detail: detail.into(),
        action: Some(action(id, title, destination)),
    }
}

fn blocked_step(id: &str, title: &str, detail: &str, destination: &str) -> ReadinessStep {
    ReadinessStep {
        id: id.into(),
        title: title.into(),
        status: ReadinessStatus::Blocked,
        detail: detail.into(),
        action: Some(action(
            "resolve_protected_tbd",
            "Resolve protected TBD",
            destination,
        )),
    }
}

fn character_ids(conn: &Connection, project_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT id FROM canon_entities WHERE project_id = ?1 AND type = 'character' ORDER BY name COLLATE NOCASE, id")
        .map_err(database)?;
    let rows = stmt
        .query_map([project_id], |row| row.get(0))
        .map_err(database)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database)
}

fn has_canonical_asset(
    conn: &Connection,
    project_id: &str,
    asset_type: &str,
    owner_entity_id: Option<&str>,
) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM assets a JOIN asset_versions av ON av.id = a.canonical_version_id WHERE a.project_id = ?1 AND a.type = ?2 AND (?3 IS NULL OR a.owner_entity_id = ?3) AND av.status = 'canonical')",
        params![project_id, asset_type, owner_entity_id],
        |row| row.get(0),
    ).map_err(database)
}

fn valid_scene_id(conn: &Connection, project_id: &str) -> Result<Option<String>, AppError> {
    conn.query_row(
        "SELECT s.id FROM scenes s WHERE s.project_id = ?1 AND s.world_asset_version_id IS NOT NULL AND EXISTS(SELECT 1 FROM asset_versions av WHERE av.id = s.world_asset_version_id) AND EXISTS(SELECT 1 FROM scene_characters sc JOIN asset_versions look ON look.id = sc.look_asset_version_id WHERE sc.scene_id = s.id) AND EXISTS(SELECT 1 FROM shots sh WHERE sh.scene_id = s.id) ORDER BY s.created_at ASC, s.id ASC LIMIT 1",
        [project_id],
        |row| row.get(0),
    ).optional().map_err(database)
}

fn has_compilation(conn: &Connection, project_id: &str, scene_id: &str) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM cinema_compilations WHERE project_id = ?1 AND scene_id = ?2)",
        params![project_id, scene_id],
        |row| row.get(0),
    )
    .map_err(database)
}

fn has_blocking_scene_tbd(
    conn: &Connection,
    project_id: &str,
    scene_id: &str,
) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM canon_tbds t WHERE t.project_id = ?1 AND t.status = 'open' AND t.protected = 1 AND (t.canon_entity_id IS NULL OR EXISTS(SELECT 1 FROM scene_characters sc WHERE sc.scene_id = ?2 AND sc.character_entity_id = t.canon_entity_id)))",
        params![project_id, scene_id],
        |row| row.get(0),
    ).map_err(database)
}

fn count_tbds(conn: &Connection, project_id: &str, protected_only: bool) -> Result<i64, AppError> {
    conn.query_row(
        "SELECT COUNT(*) FROM canon_tbds WHERE project_id = ?1 AND status = 'open' AND (?2 = 0 OR protected = 1)",
        params![project_id, protected_only],
        |row| row.get(0),
    ).map_err(database)
}

fn list_active_jobs(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<BackgroundJobSummary>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, operation_id, status, updated_at FROM workflow_runs WHERE project_id = ?1 AND status IN ('created', 'running', 'waiting_for_approval', 'ready_for_execution') ORDER BY updated_at DESC, id DESC",
    ).map_err(database)?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok(BackgroundJobSummary {
                id: row.get(0)?,
                operation_id: row.get(1)?,
                status: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .map_err(database)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database)
}

fn list_recent_activity(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<ActivityItem>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, label, occurred_at FROM (SELECT id, 'asset' AS kind, label, updated_at AS occurred_at FROM assets WHERE project_id = ?1 UNION ALL SELECT id, 'scene' AS kind, title AS label, updated_at AS occurred_at FROM scenes WHERE project_id = ?1 UNION ALL SELECT id, 'cinema_compilation' AS kind, 'Cinema compilation' AS label, created_at AS occurred_at FROM cinema_compilations WHERE project_id = ?1) ORDER BY occurred_at DESC, id DESC LIMIT 8",
    ).map_err(database)?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok(ActivityItem {
                id: row.get(0)?,
                kind: row.get(1)?,
                label: row.get(2)?,
                occurred_at: row.get(3)?,
            })
        })
        .map_err(database)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database)
}

fn database(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}
