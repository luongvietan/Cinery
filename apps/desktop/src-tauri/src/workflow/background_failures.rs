//! Run-level terminal transitions owned by the background runner (P10.1).
//!
//! The blocking runtime did these inline after `finish_submission`; the
//! background runner does them after its own capture/failure/cancel
//! resolution. All transitions are guarded by the run still being
//! non-terminal so a racing cancel command can never have a completed run
//! flipped back underneath it (terminal states never flip).

use crate::db;
use crate::error::AppError;
use crate::workflow::repository::append_event_in_transaction;
use chrono::Utc;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use std::path::Path;

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}

/// Marks the execute step completed, the complete step completed, and the
/// run completed — the exact sequence the synchronous runtime performed,
/// now driven by the runner. Idempotent: a run already terminal (or step
/// already completed by an earlier pass that crashed before the run
/// transition) changes nothing except skipping ahead.
pub fn complete_run_from_background(
    project_root: &Path,
    workflow_run_id: &str,
) -> Result<(), AppError> {
    let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let now = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let execute_index: Option<i64> = tx
        .query_row(
            "SELECT step_index FROM workflow_steps
             WHERE workflow_run_id = ?1 AND step_type = 'execute' AND status = 'running'",
            params![workflow_run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let Some(execute_index) = execute_index else {
        // Step already completed by a prior pass; ensure the run itself is
        // completed (crash between step and run transitions).
        complete_run_record_only(&tx, workflow_run_id, &now)?;
        tx.commit().map_err(db_error)?;
        return Ok(());
    };
    let run_status: Option<String> = tx
        .query_row(
            "SELECT status FROM workflow_runs WHERE id = ?1",
            params![workflow_run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let Some(status) = run_status else {
        tx.commit().map_err(db_error)?;
        return Ok(());
    };
    if matches!(status.as_str(), "completed" | "rejected" | "cancelled" | "failed") {
        // Run terminal through another path (e.g. cancel raced ahead);
        // terminal states never flip.
        tx.commit().map_err(db_error)?;
        return Ok(());
    }
    let execute_step_id: String = tx
        .query_row(
            "SELECT step_definition_id FROM workflow_steps
             WHERE workflow_run_id = ?1 AND step_index = ?2",
            params![workflow_run_id, execute_index],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    let (complete_index, complete_step_id): (i64, String) = match tx
        .query_row(
            "SELECT step_index, step_definition_id FROM workflow_steps
             WHERE workflow_run_id = ?1 AND step_type = 'complete'",
            params![workflow_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(db_error)?
    {
        Some(found) => found,
        None => {
            tx.commit().map_err(db_error)?;
            return Ok(());
        }
    };
    tx.execute(
        "UPDATE workflow_steps SET status = 'completed', completed_at = ?1
         WHERE workflow_run_id = ?2 AND step_index = ?3",
        params![now, workflow_run_id, execute_index],
    )
    .map_err(db_error)?;
    tx.execute(
        "UPDATE workflow_steps SET status = 'completed', completed_at = ?1
         WHERE workflow_run_id = ?2 AND step_index = ?3 AND status IN ('pending', 'running')",
        params![now, workflow_run_id, complete_index],
    )
    .map_err(db_error)?;
    tx.execute(
        "UPDATE workflow_runs
         SET status = 'completed', current_step_index = ?1, completed_at = ?2, updated_at = ?2
         WHERE id = ?3 AND status NOT IN ('completed', 'rejected', 'cancelled', 'failed')",
        params![complete_index + 1, now, workflow_run_id],
    )
    .map_err(db_error)?;
    append_event_in_transaction(
        &tx,
        workflow_run_id,
        "execution_completed",
        Some(&execute_step_id),
        None,
        &now,
    )?;
    append_event_in_transaction(
        &tx,
        workflow_run_id,
        "step_completed",
        Some(&execute_step_id),
        None,
        &now,
    )?;
    append_event_in_transaction(
        &tx,
        workflow_run_id,
        "step_completed",
        Some(&complete_step_id),
        None,
        &now,
    )?;
    append_event_in_transaction(
        &tx,
        workflow_run_id,
        "run_completed",
        Some(&complete_step_id),
        None,
        &now,
    )?;
    tx.commit().map_err(db_error)?;
    Ok(())
}

fn complete_run_record_only(
    tx: &rusqlite::Transaction<'_>,
    workflow_run_id: &str,
    now: &str,
) -> Result<(), AppError> {
    let run_status: Option<String> = tx
        .query_row(
            "SELECT status FROM workflow_runs WHERE id = ?1",
            params![workflow_run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let Some(status) = run_status else {
        return Ok(());
    };
    if matches!(status.as_str(), "completed" | "rejected" | "cancelled" | "failed") {
        return Ok(());
    }
    let complete_index: Option<(i64, String)> = tx
        .query_row(
            "SELECT step_index, step_definition_id FROM workflow_steps
             WHERE workflow_run_id = ?1 AND step_type = 'complete'",
            params![workflow_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(db_error)?;
    let Some((complete_index, complete_step_id)) = complete_index else {
        return Ok(());
    };
    tx.execute(
        "UPDATE workflow_runs
         SET status = 'completed', current_step_index = ?1, completed_at = ?2, updated_at = ?2
         WHERE id = ?3 AND status NOT IN ('completed', 'rejected', 'cancelled', 'failed')",
        params![complete_index + 1, now, workflow_run_id],
    )
    .map_err(db_error)?;
    append_event_in_transaction(
        tx,
        workflow_run_id,
        "run_completed",
        Some(&complete_step_id),
        None,
        now,
    )?;
    Ok(())
}

/// Fails the run from the background runner with a redacted message. Never
/// flips an already-terminal run.
pub fn fail_run_from_background(
    project_root: &Path,
    workflow_run_id: &str,
    message: &str,
) -> Result<(), AppError> {
    let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let now = Utc::now().to_rfc3339();
    let payload = serde_json::json!({"code":"PROVIDER_EXECUTION_FAILED"}).to_string();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    tx.execute(
        "UPDATE workflow_steps SET status = 'failed', completed_at = ?1
         WHERE workflow_run_id = ?2 AND status = 'running'",
        params![now, workflow_run_id],
    )
    .map_err(db_error)?;
    let updated = tx
        .execute(
            "UPDATE workflow_runs
             SET status = 'failed', failure_code = 'PROVIDER_EXECUTION_FAILED',
                 failure_message = ?1, completed_at = ?2, updated_at = ?2
             WHERE id = ?3 AND status NOT IN ('completed', 'rejected', 'cancelled', 'failed')",
            params![message, now, workflow_run_id],
        )
        .map_err(db_error)?;
    if updated > 0 {
        append_event_in_transaction(
            &tx,
            workflow_run_id,
            "run_failed",
            None,
            Some(&payload),
            &now,
        )?;
    }
    tx.commit().map_err(db_error)?;
    crate::qa::repository::fail_for_workflow(
        &conn,
        workflow_run_id,
        "PROVIDER_EXECUTION_FAILED",
        message,
        &now,
    )?;
    Ok(())
}

/// Cancels the run from the background runner (durable cancellation
/// observed and resolved). Mirrors `WorkflowRuntime::cancel_run`'s
/// terminal-guarded transition.
pub fn cancel_run_from_background(
    project_root: &Path,
    workflow_run_id: &str,
) -> Result<(), AppError> {
    let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let now = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let run_status: Option<String> = tx
        .query_row(
            "SELECT status FROM workflow_runs WHERE id = ?1",
            params![workflow_run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    if matches!(
        run_status.as_deref(),
        None | Some("completed" | "rejected" | "cancelled" | "failed")
    ) {
        tx.commit().map_err(db_error)?;
        return Ok(());
    }
    tx.execute(
        "UPDATE workflow_steps SET status = 'skipped', completed_at = ?1
         WHERE workflow_run_id = ?2 AND status IN ('pending', 'waiting', 'running')",
        params![now, workflow_run_id],
    )
    .map_err(db_error)?;
    tx.execute(
        "UPDATE workflow_runs SET status = 'cancelled', completed_at = ?1, updated_at = ?1
         WHERE id = ?2 AND status NOT IN ('completed', 'rejected', 'cancelled', 'failed')",
        params![now, workflow_run_id],
    )
    .map_err(db_error)?;
    append_event_in_transaction(&tx, workflow_run_id, "run_cancelled", None, None, &now)?;
    tx.commit().map_err(db_error)?;
    crate::qa::repository::cancel_for_workflow(&conn, workflow_run_id, &now)?;
    Ok(())
}
