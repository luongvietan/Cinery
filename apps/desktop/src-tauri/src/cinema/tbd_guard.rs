use crate::error::AppError;
use rusqlite::{params, Connection};

/// TBD firewall for cinema compilation (master plan #11).
///
/// A compilation is blocked while any **protected, open** TBD remains that
/// can influence this scene:
/// - project-scoped TBDs (no `canon_entity_id`) always block, and
/// - entity-scoped TBDs block when the entity is one of the scene's cast
///   characters.
///
/// Unprotected TBDs and resolved/reopened-then-resolved TBDs never block.
pub fn check_tbd_firewall(
    conn: &Connection,
    project_id: &str,
    scene_id: &str,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT topic, canon_entity_id FROM canon_tbds \
             WHERE project_id = ?1 AND protected = 1 AND status = 'open' \
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut topics: Vec<String> = Vec::new();
    for row in rows {
        let (topic, entity_id) = row.map_err(|e| AppError::Database(e.to_string()))?;
        match entity_id {
            // Project-scope: intentionally unresolved for the whole project.
            None => topics.push(topic),
            Some(entity_id) => {
                let cast: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM world_scene_characters \
                         WHERE scene_id = ?1 AND character_entity_id = ?2)",
                        params![scene_id, entity_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| AppError::Database(e.to_string()))?;
                if cast {
                    topics.push(topic);
                }
            }
        }
    }

    if let Some(topic) = topics.first() {
        return Err(AppError::WorkflowBlockedByProtectedTbd(format!(
            "protected TBD '{topic}' must be resolved before cinema compilation"
        )));
    }
    Ok(())
}
