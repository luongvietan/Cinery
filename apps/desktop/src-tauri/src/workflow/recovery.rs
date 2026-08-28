use crate::db;
use crate::error::AppError;
use crate::workflow::repository::append_event_in_transaction;
use chrono::Utc;
use rusqlite::{params, TransactionBehavior};
use std::path::Path;

pub fn recover_interrupted_runs(project_root: &Path) -> Result<usize, AppError> {
    let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let run_ids = {
        let mut statement = conn
            .prepare("SELECT id FROM workflow_runs WHERE status = 'running' OR EXISTS (SELECT 1 FROM workflow_steps WHERE workflow_run_id = workflow_runs.id AND status = 'running') ORDER BY id")
            .map_err(db_error)?;
        let result = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        result
    };

    for run_id in &run_ids {
        let has_durable_provider_job: bool = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM workflow_step_executions
                    WHERE workflow_run_id = ?1
                      AND provider_job_id IS NOT NULL
                      AND status IN ('submitted', 'running', 'cancellation_requested', 'unknown')
                )",
                [run_id],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        if has_durable_provider_job {
            continue;
        }
        let now = Utc::now().to_rfc3339();
        let payload = serde_json::json!({"code":"INTERRUPTED_DURING_STEP"}).to_string();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        tx.execute(
            "UPDATE workflow_steps SET status = 'failed', completed_at = ?1 WHERE workflow_run_id = ?2 AND status = 'running'",
            params![now, run_id],
        )
        .map_err(db_error)?;
        tx.execute(
            "UPDATE workflow_runs SET status = 'failed', failure_code = 'INTERRUPTED_DURING_STEP', failure_message = 'Workflow step was interrupted by application shutdown', completed_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, run_id],
        )
        .map_err(db_error)?;
        append_event_in_transaction(&tx, run_id, "run_failed", None, Some(&payload), &now)?;
        tx.commit().map_err(db_error)?;
        crate::qa::repository::fail_for_workflow(
            &conn,
            run_id,
            "INTERRUPTED_DURING_STEP",
            "Visual QA was interrupted by application shutdown",
            &now,
        )?;
    }
    Ok(run_ids.len())
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}
