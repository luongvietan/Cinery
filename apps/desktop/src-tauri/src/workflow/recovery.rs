use crate::db;
use crate::error::AppError;
use crate::workflow::repository::WorkflowRepository;
use chrono::Utc;
use rusqlite::params;
use std::path::Path;

pub fn recover_interrupted_runs(project_root: &Path) -> Result<usize, AppError> {
    let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let run_ids = {
        let mut statement = conn
            .prepare("SELECT id FROM workflow_runs WHERE status = 'running' ORDER BY id")
            .map_err(db_error)?;
        let result = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        result
    };

    for run_id in &run_ids {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE workflow_steps SET status = 'failed', completed_at = ?1 WHERE workflow_run_id = ?2 AND status = 'running'",
            params![now, run_id],
        )
        .map_err(db_error)?;
        conn.execute(
            "UPDATE workflow_runs SET status = 'failed', failure_code = 'INTERRUPTED_DURING_STEP', failure_message = 'Workflow step was interrupted by application shutdown', completed_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, run_id],
        )
        .map_err(db_error)?;
        WorkflowRepository::append_event(
            &mut conn,
            run_id,
            "run_failed",
            None,
            Some(serde_json::json!({"code":"INTERRUPTED_DURING_STEP"})),
        )?;
    }
    Ok(run_ids.len())
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}
