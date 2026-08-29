use crate::db;
use crate::error::AppError;
use crate::project::{paths, repository as project_repository};
use rusqlite::{params, Connection};
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
    pub character_entity_id: Option<String>,
    pub scene_id: Option<String>,
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
    pub scene_readiness: Vec<SceneReadiness>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneReadiness {
    pub scene_id: String,
    pub title: String,
    pub status: ReadinessStatus,
    pub detail: String,
    pub action: Option<OverviewAction>,
}

/// Produces a product-level read model from existing P0-P8 records. It does
/// not persist a readiness state or rewrite exact scene references: a scene's
/// pinned versions remain evidence even after their asset moves on.
pub fn get_project_overview(project_root: &Path) -> Result<ProjectOverview, AppError> {
    let manifest = paths::read_manifest(project_root)?;
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let project = project_repository::read_project(&conn)?;
    if project.id != manifest.project_id {
        return Err(AppError::ProjectIdentityMismatch);
    }
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
        scene_readiness: list_scene_readiness(&conn, &project.id)?,
    })
}

fn derive_readiness(conn: &Connection, project_id: &str) -> Result<ProjectReadiness, AppError> {
    let characters = character_readiness(conn, project_id)?;
    if characters.is_empty() {
        return Ok(with_next(
            vec![pending_step(
                "story_canon",
                "Story Canon",
                "Create the story foundation before production.",
                "canon",
                None,
                None,
            )],
            action("story_canon", "Story Canon", "canon", None, None),
        ));
    }

    let mut steps = vec![complete_step(
        "story_canon",
        "Story Canon",
        "A character production path is active.",
    )];
    for character in &characters {
        if !character.has_face {
            steps.push(pending_step(
                "face_lock",
                "Face Lock",
                "Promote a canonical face for every character.",
                "assets",
                Some(&character.id),
                None,
            ));
            return Ok(with_next(
                steps,
                action(
                    "face_lock",
                    "Face Lock",
                    "assets",
                    Some(&character.id),
                    None,
                ),
            ));
        }
    }
    steps.push(complete_step(
        "face_lock",
        "Canonical Face",
        "Every character has a canonical face reference.",
    ));

    for character in &characters {
        if !character.has_look {
            steps.push(pending_step(
                "character_look",
                "Character Look",
                "Promote a canonical look for every character.",
                "assets",
                Some(&character.id),
                None,
            ));
            return Ok(with_next(
                steps,
                action(
                    "character_look",
                    "Character Look",
                    "assets",
                    Some(&character.id),
                    None,
                ),
            ));
        }
    }
    steps.push(complete_step(
        "character_look",
        "Canonical Look",
        "Every character has a canonical look reference.",
    ));

    for character in &characters {
        if !character.has_sheet {
            steps.push(pending_step(
                "character_sheet",
                "Character Sheet",
                "Promote a canonical sheet for every character.",
                "assets",
                Some(&character.id),
                None,
            ));
            return Ok(with_next(
                steps,
                action(
                    "character_sheet",
                    "Character Sheet",
                    "assets",
                    Some(&character.id),
                    None,
                ),
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
            None,
            None,
        ));
        return Ok(with_next(
            steps,
            action("world_plate", "World Plate", "assets", None, None),
        ));
    }
    steps.push(complete_step(
        "world_plate",
        "World Plate",
        "A canonical world plate is available.",
    ));

    let scenes = list_scene_readiness(conn, project_id)?;
    if scenes.is_empty() {
        steps.push(pending_step(
            "scene",
            "Scene",
            "Stage a scene with pinned character references and a shot.",
            "scenes",
            None,
            None,
        ));
        return Ok(with_next(
            steps,
            action("scene", "Scene", "scenes", None, None),
        ));
    }
    steps.push(complete_step(
        "scene",
        "Scene",
        "A staged scene has durable exact references.",
    ));

    if let Some(blocked) = scenes
        .iter()
        .find(|scene| scene.status == ReadinessStatus::Blocked)
    {
        steps.push(blocked_step(
            "cinema_compilation",
            "Cinema Compilation",
            "A protected open TBD blocks this scene from compilation.",
            "canon",
            Some(&blocked.scene_id),
        ));
        return Ok(ProjectReadiness {
            status: ReadinessStatus::Blocked,
            next_action: Some(action(
                "resolve_protected_tbd",
                "Resolve protected TBD",
                "canon",
                None,
                Some(&blocked.scene_id),
            )),
            steps,
        });
    }

    if let Some(pending) = scenes
        .iter()
        .find(|scene| scene.status == ReadinessStatus::Pending)
    {
        steps.push(pending_step(
            "cinema_compilation",
            "Cinema Compilation",
            "Compile the staged scene into a durable provider-neutral prompt.",
            "scenes",
            None,
            Some(&pending.scene_id),
        ));
        return Ok(with_next(
            steps,
            action(
                "cinema_compilation",
                "Cinema Compilation",
                "scenes",
                None,
                Some(&pending.scene_id),
            ),
        ));
    }

    steps.push(complete_step(
        "cinema_compilation",
        "Cinema Compilation",
        "Every staged scene has a provider-neutral cinema prompt.",
    ));
    Ok(ProjectReadiness {
        status: ReadinessStatus::Complete,
        next_action: None,
        steps,
    })
}

fn with_next(steps: Vec<ReadinessStep>, next_action: OverviewAction) -> ProjectReadiness {
    ProjectReadiness {
        status: ReadinessStatus::Pending,
        next_action: Some(next_action),
        steps,
    }
}

fn action(
    id: &str,
    title: &str,
    destination: &str,
    character_entity_id: Option<&str>,
    scene_id: Option<&str>,
) -> OverviewAction {
    OverviewAction {
        id: id.into(),
        title: title.into(),
        destination: destination.into(),
        character_entity_id: character_entity_id.map(str::to_string),
        scene_id: scene_id.map(str::to_string),
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

fn pending_step(
    id: &str,
    title: &str,
    detail: &str,
    destination: &str,
    character_entity_id: Option<&str>,
    scene_id: Option<&str>,
) -> ReadinessStep {
    ReadinessStep {
        id: id.into(),
        title: title.into(),
        status: ReadinessStatus::Pending,
        detail: detail.into(),
        action: Some(action(
            id,
            title,
            destination,
            character_entity_id,
            scene_id,
        )),
    }
}

fn blocked_step(
    id: &str,
    title: &str,
    detail: &str,
    destination: &str,
    scene_id: Option<&str>,
) -> ReadinessStep {
    ReadinessStep {
        id: id.into(),
        title: title.into(),
        status: ReadinessStatus::Blocked,
        detail: detail.into(),
        action: Some(action(
            "resolve_protected_tbd",
            "Resolve protected TBD",
            destination,
            None,
            scene_id,
        )),
    }
}

struct CharacterReadiness {
    id: String,
    has_face: bool,
    has_look: bool,
    has_sheet: bool,
}

fn character_readiness(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<CharacterReadiness>, AppError> {
    let mut stmt = conn.prepare("SELECT c.id, MAX(CASE WHEN a.type = 'face_lock' AND av.status = 'canonical' THEN 1 ELSE 0 END), MAX(CASE WHEN a.type = 'outfit' AND av.status = 'canonical' THEN 1 ELSE 0 END), MAX(CASE WHEN a.type = 'character_sheet' AND av.status = 'canonical' THEN 1 ELSE 0 END) FROM canon_entities c LEFT JOIN assets a ON a.project_id = c.project_id AND a.owner_entity_id = c.id LEFT JOIN asset_versions av ON av.id = a.canonical_version_id WHERE c.project_id = ?1 AND c.type = 'character' GROUP BY c.id ORDER BY c.name COLLATE NOCASE, c.id")
        .map_err(database)?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok(CharacterReadiness {
                id: row.get(0)?,
                has_face: row.get::<_, i64>(1)? > 0,
                has_look: row.get::<_, i64>(2)? > 0,
                has_sheet: row.get::<_, i64>(3)? > 0,
            })
        })
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

fn list_scene_readiness(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<SceneReadiness>, AppError> {
    let mut stmt = conn.prepare("SELECT s.id, s.title, EXISTS(SELECT 1 FROM scene_compilations cc WHERE cc.project_id = s.project_id AND cc.scene_id = s.id), EXISTS(SELECT 1 FROM canon_tbds t WHERE t.project_id = s.project_id AND t.status = 'open' AND t.protected = 1 AND (t.canon_entity_id IS NULL OR EXISTS(SELECT 1 FROM world_scene_characters sc WHERE sc.scene_id = s.id AND sc.character_entity_id = t.canon_entity_id))), (NOT EXISTS(SELECT 1 FROM assets a WHERE a.canonical_version_id = s.world_asset_version_id) OR EXISTS(SELECT 1 FROM world_scene_characters sc WHERE sc.scene_id = s.id AND (NOT EXISTS(SELECT 1 FROM assets a WHERE a.canonical_version_id = sc.look_asset_version_id) OR (sc.sheet_asset_version_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM assets a WHERE a.canonical_version_id = sc.sheet_asset_version_id))))) FROM world_scenes s WHERE s.project_id = ?1 AND s.world_asset_version_id IS NOT NULL AND EXISTS(SELECT 1 FROM asset_versions av WHERE av.id = s.world_asset_version_id) AND EXISTS(SELECT 1 FROM world_scene_characters sc JOIN asset_versions look ON look.id = sc.look_asset_version_id WHERE sc.scene_id = s.id) AND EXISTS(SELECT 1 FROM scene_shots sh WHERE sh.scene_id = s.id) ORDER BY s.created_at DESC, s.id DESC").map_err(database)?;
    let rows = stmt
        .query_map([project_id], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let compiled = row.get::<_, bool>(2)?;
            let blocked = row.get::<_, bool>(3)?;
            let stale = row.get::<_, bool>(4)?;
            let (status, detail, action) = if blocked {
                (
                    ReadinessStatus::Blocked,
                    "A protected open TBD blocks this scene from compilation.".to_string(),
                    Some(action(
                        "resolve_protected_tbd",
                        "Resolve protected TBD",
                        "canon",
                        None,
                        Some(&id),
                    )),
                )
            } else if stale {
                (
                    ReadinessStatus::Blocked,
                    "This scene pins a superseded exact asset version; restage it before compilation.".to_string(),
                    Some(action("restage_scene", "Restage Scene", "scenes", None, Some(&id))),
                )
            } else if compiled {
                (
                    ReadinessStatus::Complete,
                    "A provider-neutral cinema prompt was compiled and persisted.".to_string(),
                    None,
                )
            } else {
                (
                    ReadinessStatus::Pending,
                    "Compile this staged scene into a durable provider-neutral prompt.".to_string(),
                    Some(action(
                        "cinema_compilation",
                        "Cinema Compilation",
                        "scenes",
                        None,
                        Some(&id),
                    )),
                )
            };
            Ok(SceneReadiness {
                scene_id: id,
                title,
                status,
                detail,
                action,
            })
        })
        .map_err(database)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database)
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
        "SELECT id, kind, label, occurred_at FROM (SELECT id, 'asset' AS kind, label, updated_at AS occurred_at FROM assets WHERE project_id = ?1 UNION ALL SELECT id, 'scene' AS kind, title AS label, updated_at AS occurred_at FROM world_scenes WHERE project_id = ?1 UNION ALL SELECT id, 'cinema_compilation' AS kind, 'Cinema compilation' AS label, created_at AS occurred_at FROM scene_compilations WHERE project_id = ?1) ORDER BY occurred_at DESC, id DESC LIMIT 8",
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
