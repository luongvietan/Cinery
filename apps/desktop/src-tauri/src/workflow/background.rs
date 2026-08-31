//! Durable background execution for provider jobs (P10.1).
//!
//! The workflow runtime owns *submission*: it creates the attempt, submits to
//! the provider, persists the durable `provider_jobs` row, and returns
//! control to the UI as soon as a pollable remote job exists. Everything
//! after submission â€” polling, progress, deadline enforcement, cancellation,
//! result fetch, artifact capture, attempt/step/run completion â€” is owned by
//! this module.
//!
//! SQLite is the source of truth. Every tick derives its work from persisted
//! `workflow_step_executions` + `provider_jobs` state, so a process restart
//! resumes durable remote jobs without duplicate submission. A per-project
//! daemon thread drives ticks; tests drive ticks directly via
//! [`run_pending_jobs`] so no test ever sleeps on a poll interval.

use crate::db;
use crate::error::AppError;
use crate::providers::adapter::GenerationProvider;
use crate::providers::model::{
    ProviderJobRef, ProviderJobStatus, ProviderLifecycle, ProviderResult,
};
use crate::providers::registry::ProviderRegistry;
use crate::providers::repository::append_audit_event;
use crate::providers::service::ProviderService;
use crate::project::repository::read_project;
use crate::workflow::completion::{self, CompletionOutcome};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// How often the daemon thread wakes on its own when nothing signals it.
const TICK_INTERVAL: Duration = Duration::from_millis(500);

/// A durable provider job discovered from persisted state, with everything
/// the runner needs to poll it without re-resolving any workflow context.
#[derive(Debug, Clone)]
pub struct PendingJob {
    pub execution_id: String,
    pub workflow_run_id: String,
    pub step_definition_id: String,
    pub attempt_number: i64,
    pub provider_id: String,
    pub model_id: String,
    pub idempotency_key: String,
    pub provider_job_id: String,
    pub submitted_at: String,
    /// Provider jobs table row status ('submitted' | 'polling').
    pub job_status: String,
    /// The provider operation that created the job (async declarative
    /// adapters require it to poll from a rehydrated instance).
    pub operation: Option<String>,
}

impl PendingJob {
    fn job_ref(&self) -> ProviderJobRef {
        ProviderJobRef {
            provider_id: self.provider_id.clone(),
            provider_job_id: self.provider_job_id.clone(),
            run_id: self.workflow_run_id.clone(),
            step_id: self.step_definition_id.clone(),
            submission_id: self.idempotency_key.clone(),
            submitted_at: self.submitted_at.clone(),
            operation: self.operation.clone(),
        }
    }
}

/// Process-level adapter cache for the runner, keyed by (project root,
/// provider id). Local/builtin providers keep in-memory job state (the
/// deterministic test provider's poll counter; sync adapters' result maps),
/// so every tick must observe the SAME adapter instance for a job. HTTP
/// adapters are stateless across instances, but caching them avoids
/// re-reading declarative definitions and re-resolving vault credentials
/// on every tick. A process restart rebuilds the cache and resumes from
/// durable state (remote providers poll their real remote job).
type ProviderCache = Mutex<std::collections::HashMap<(PathBuf, String), Arc<dyn GenerationProvider>>>;

fn provider_cache() -> &'static ProviderCache {
    static CACHE: OnceLock<ProviderCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// The provider adapter for a pending job. Local/builtin providers resolve
/// from the registry; user-defined AI services rehydrate from their stored
/// declarative definition with the credential resolved from the vault again
/// (never persisted anywhere).
fn provider_for(
    project_root: &Path,
    provider_id: &str,
) -> Result<Arc<dyn GenerationProvider>, AppError> {
    let key = (project_root.to_path_buf(), provider_id.to_string());
    if let Some(cached) = provider_cache().lock().unwrap().get(&key) {
        return Ok(cached.clone());
    }
    let mut registry = ProviderRegistry::builtin();
    match provider_id {
        "mock" | "dry_run" | "fake_async_video" => {}
        "openai" => {
            let token = ProviderService::resolve_openai_execution_token(project_root)?;
            registry.register_arc(ProviderService::openai_builtin_adapter(token));
        }
        _ => {
            let adapter =
                ProviderService::custom_execution_adapter(project_root, None, provider_id, true)?;
            registry.register_arc(adapter);
        }
    }
    let provider = registry.get(provider_id).map_err(|error| {
        AppError::ProviderExecution(format!(
            "provider {provider_id} is no longer available for background execution: {}",
            error.message
        ))
    })?;
    provider_cache().lock().unwrap().insert(key, provider.clone());
    Ok(provider)
}

/// Parses an RFC3339 timestamp leniently; `None` when unparsable (treated as
/// "unknown submission time", which defers deadline enforcement).
fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

/// Finds every durable provider job that still needs runner work: a
/// non-terminal attempt whose `provider_jobs` row exists and is not terminal.
pub fn discover_pending_jobs(conn: &Connection) -> Result<Vec<PendingJob>, AppError> {
    let mut statement = conn
        .prepare(
            "SELECT e.id, e.workflow_run_id, e.step_definition_id, e.attempt_number,
                e.provider_id, e.model_id, e.idempotency_key, pj.provider_job_id,
                pj.submitted_at, pj.status, pj.operation
         FROM workflow_step_executions e
         JOIN provider_jobs pj ON pj.execution_id = e.id
         WHERE e.status IN ('submitted', 'running', 'cancellation_requested')
           AND pj.status IN ('submitted', 'polling')
         ORDER BY pj.submitted_at ASC",
        )
        .map_err(db_error)?;
    let jobs = statement
        .query_map([], |row| {
            Ok(PendingJob {
                execution_id: row.get(0)?,
                workflow_run_id: row.get(1)?,
                step_definition_id: row.get(2)?,
                attempt_number: row.get(3)?,
                provider_id: row.get(4)?,
                model_id: row.get(5)?,
                idempotency_key: row.get(6)?,
                provider_job_id: row.get(7)?,
                submitted_at: row.get(8)?,
                job_status: row.get(9)?,
                operation: row.get(10)?,
            })
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(jobs)
}

/// What one runner tick decided to do with a job (for tests + observability).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickDisposition {
    /// Provider still working; keep the job durable and poll again later.
    StillRunning { progress_percent: Option<u8> },
    /// Completed and durably captured; workflow advanced to completion.
    Completed { result_set_id: Option<String> },
    /// Provider reported failure; attempt/run failed.
    Failed,
    /// Cancellation observed and resolved.
    Cancelled { remote_cancelled: bool },
    /// Deadline exceeded; job/attempt/run failed with a timeout error.
    DeadlineExceeded,
}

/// Outcome of a full runner tick over one project's durable jobs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickSummary {
    pub polled: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub deadline_exceeded: usize,
}

/// Runs one deterministic pass over every pending durable provider job in the
/// project. This is the runner's core: it never sleeps, so tests (and the
/// daemon thread) drive it tick by tick.
pub fn run_pending_jobs(project_root: &Path) -> Result<TickSummary, AppError> {
    let mut summary = TickSummary::default();
    let mut conn = open_project(project_root)?;
    let jobs = discover_pending_jobs(&conn)?;
    for job in jobs {
        // Re-open per job: no DB handle (let alone transaction) is ever held
        // across a provider network call.
        drop(conn);
        let disposition = process_one_job(project_root, &job)?;
        conn = open_project(project_root)?;
        match disposition {
            TickDisposition::StillRunning { .. } => {
                summary.polled += 1;
            }
            TickDisposition::Completed { .. } => {
                summary.completed += 1;
            }
            TickDisposition::Failed => {
                summary.failed += 1;
            }
            TickDisposition::Cancelled { .. } => {
                summary.cancelled += 1;
            }
            TickDisposition::DeadlineExceeded => {
                summary.deadline_exceeded += 1;
            }
        }
    }
    Ok(summary)
}

/// Claims the job for this runner iteration (atomic submitted/polling â†’
/// polling) so two concurrent tick sources can never double-poll the same
/// job. Returns false when the job was claimed by another writer or already
/// left the claimable set.
fn claim_job(conn: &mut Connection, job: &PendingJob) -> Result<bool, AppError> {
    let now = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let claimed = tx
        .execute(
            "UPDATE provider_jobs SET status = 'polling', updated_at = ?1
             WHERE execution_id = ?2 AND status IN ('submitted', 'polling')",
            params![now, job.execution_id],
        )
        .map_err(db_error)?
        > 0;
    if claimed {
        // A cancellation_requested attempt keeps its request status; the
        // claim only needs the provider_jobs row so this tick resolves it.
        tx.execute(
            "UPDATE workflow_step_executions SET status = 'running'
             WHERE id = ?1 AND status IN ('queued', 'submitted', 'running')",
            params![job.execution_id],
        )
        .map_err(db_error)?;
    }
    tx.commit().map_err(db_error)?;
    Ok(claimed)
}

/// Whether the durable attempt has a pending cancel request. Set by the
/// cancel command; observed by the runner between polls.
fn cancel_requested(conn: &Connection, execution_id: &str) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM workflow_step_executions
          WHERE id = ?1 AND status = 'cancellation_requested')",
        params![execution_id],
        |row| row.get(0),
    )
    .map_err(db_error)
}

/// Terminal compare-and-set on the attempt: the UPDATE only lands while the
/// attempt is still non-terminal, so a concurrent cancel command (or a second
/// completion pass) can never flip a terminal state. Returns true when this
/// caller was the one to set the terminal status.
fn terminal_set_attempt(
    conn: &Connection,
    execution_id: &str,
    status: &str,
    error_json: Option<&str>,
) -> Result<bool, AppError> {
    let updated = conn
        .execute(
            "UPDATE workflow_step_executions
             SET status = ?1, normalized_error_json = ?2,
                 completed_at = COALESCE(completed_at, ?3)
             WHERE id = ?4 AND status NOT IN ('succeeded', 'failed', 'cancelled')",
            params![status, error_json, Utc::now().to_rfc3339(), execution_id],
        )
        .map_err(db_error)?
        > 0;
    Ok(updated)
}

/// Terminal transition for the provider_jobs row (any non-terminal â†’ given).
fn terminal_set_provider_job(conn: &Connection, execution_id: &str, status: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE provider_jobs SET status = ?1, updated_at = ?2
         WHERE execution_id = ?3 AND status NOT IN ('completed', 'failed', 'cancelled')",
        params![status, Utc::now().to_rfc3339(), execution_id],
    )
    .map_err(db_error)?;
    Ok(())
}

fn open_project(root: &Path) -> Result<Connection, AppError> {
    db::open_existing_connection(&root.join("project.db"))
}

/// Processes one claimed durable job through a single poll cycle.
fn process_one_job(
    project_root: &Path,
    job: &PendingJob,
) -> Result<TickDisposition, AppError> {
    let mut conn = open_project(project_root)?;
    if !claim_job(&mut conn, job)? {
        return Ok(TickDisposition::StillRunning {
            progress_percent: None,
        });
    }

    // Cancellation first: a requested cancel always wins over further polls.
    if cancel_requested(&conn, &job.execution_id)? {
        return resolve_cancellation(project_root, job);
    }

    // Deadline: overall provider-job wall-clock timeout, distinct from any
    // per-HTTP-request timeout (P10.0 separation preserved).
    let provider = provider_for(project_root, &job.provider_id)?;
    let spec = provider.polling_spec();
    if let Some(submitted) = parse_timestamp(&job.submitted_at) {
        if Utc::now() - submitted
            >= chrono::TimeDelta::from_std(spec.timeout).unwrap_or(chrono::TimeDelta::MAX)
        {
            let message =
                format!("the AI service did not finish within {} seconds", spec.timeout.as_secs());
            fail_job(project_root, job, &message, "provider.execution.deadline_exceeded")?;
            return Ok(TickDisposition::DeadlineExceeded);
        }
    }

    // Poll (network) with no DB state held.
    drop(conn);
    let status = match provider.poll(&job.job_ref()) {
        Ok(status) => status,
        Err(error) if error.kind.retryable() => {
            // Transient poll failure: keep the job durable for the next tick.
            let conn = open_project(project_root)?;
            let _ = append_audit_event(
                &conn,
                Some(&job.execution_id),
                &job.workflow_run_id,
                "provider.execution.poll_retry",
                Some(&serde_json::json!({"error": error.display_text()})),
            );
            return Ok(TickDisposition::StillRunning {
                progress_percent: None,
            });
        }
        Err(error) => {
            fail_job(
                project_root,
                job,
                &error.display_text(),
                "provider.execution.failed",
            )?;
            return Ok(TickDisposition::Failed);
        }
    };

    let conn = open_project(project_root)?;
    match status.lifecycle {
        ProviderLifecycle::Succeeded => {
            // Fetch the result (network) with no DB state held.
            drop(conn);
            let result = match provider.fetch_result(&job.job_ref()) {
                Ok(result) => result,
                Err(error) => {
                    fail_job(
                        project_root,
                        job,
                        &error.display_text(),
                        "provider.execution.failed",
                    )?;
                    return Ok(TickDisposition::Failed);
                }
            };
            complete_job(project_root, job, &result)
        }
        ProviderLifecycle::Failed => {
            let diagnostic = status
                .diagnostic
                .as_deref()
                .map(|text| format!(": {}", crate::providers::redact_secret(text)))
                .unwrap_or_default();
            let message = format!("the AI service reported the job failed{diagnostic}");
            fail_job(project_root, job, &message, "provider.execution.failed")?;
            Ok(TickDisposition::Failed)
        }
        ProviderLifecycle::Cancelled | ProviderLifecycle::CancellationRequested => {
            // The provider itself reported the job cancelled.
            let conn = open_project(project_root)?;
            if terminal_set_attempt(&conn, &job.execution_id, "cancelled", None)? {
                terminal_set_provider_job(&conn, &job.execution_id, "cancelled")?;
                crate::workflow::background_failures::cancel_run_from_background(
                    project_root,
                    &job.workflow_run_id,
                )?;
                let _ = append_audit_event(
                    &conn,
                    Some(&job.execution_id),
                    &job.workflow_run_id,
                    "provider.execution.cancelled",
                    Some(&serde_json::json!({"source": "provider"})),
                );
            }
            Ok(TickDisposition::Cancelled {
                remote_cancelled: true,
            })
        }
        ProviderLifecycle::Unknown => {
            // Unknown before any observed progress is treated exactly like
            // the blocking loop did: a hard failure for sync adapters that
            // lost their result. After progress, keep polling.
            if job.job_status == "submitted" && status.progress_percent.is_none() {
                let message = format!(
                    "provider {} ended in an unknown state",
                    job.provider_id
                );
                fail_job(project_root, job, &message, "provider.execution.failed")?;
                return Ok(TickDisposition::Failed);
            }
            Ok(TickDisposition::StillRunning {
                progress_percent: status.progress_percent,
            })
        }
        _ => {
            record_progress(project_root, job, &status)?;
            Ok(TickDisposition::StillRunning {
                progress_percent: status.progress_percent,
            })
        }
    }
}

/// Persists observed progress. Only writes an audit event when the percent
/// actually changed (no event spam per poll).
fn record_progress(
    project_root: &Path,
    job: &PendingJob,
    status: &ProviderJobStatus,
) -> Result<(), AppError> {
    let conn = open_project(project_root)?;
    let now = Utc::now().to_rfc3339();
    let previous_percent: Option<i64> = conn
        .query_row(
            "SELECT progress_percent FROM provider_jobs WHERE execution_id = ?1",
            params![job.execution_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?
        .flatten();
    let changed = previous_percent != status.progress_percent.map(i64::from);
    conn.execute(
        "UPDATE provider_jobs
         SET progress_percent = ?1, last_polled_at = ?2, updated_at = ?2
         WHERE execution_id = ?3 AND status IN ('submitted', 'polling')",
        params![status.progress_percent, now, job.execution_id],
    )
    .map_err(db_error)?;
    if changed {
        let _ = append_audit_event(
            &conn,
            Some(&job.execution_id),
            &job.workflow_run_id,
            "provider.execution.progress",
            Some(&serde_json::json!({
                "progressPercent": status.progress_percent,
                "providerJobId": job.provider_job_id,
            })),
        );
    }
    Ok(())
}

/// Resolves a durable cancellation request: stop polling locally, ask the
/// provider to cancel when it can, persist truthful terminal state, and
/// cancel the workflow run.
fn resolve_cancellation(project_root: &Path, job: &PendingJob) -> Result<TickDisposition, AppError> {
    let provider = provider_for(project_root, &job.provider_id)?;
    let supports_cancel = provider.capabilities().supports_cancel;
    let mut remote_cancelled = false;
    let mut error_note = None;
    if supports_cancel {
        match provider.cancel(&job.job_ref()) {
            Ok(result) => {
                remote_cancelled = matches!(result.lifecycle, ProviderLifecycle::Cancelled);
            }
            Err(error) => {
                // The remote cancel call failed; the local stop still stands,
                // but the persisted note must be truthful about it.
                error_note = Some(error.display_text());
            }
        }
    }
    let conn = open_project(project_root)?;
    if terminal_set_attempt(&conn, &job.execution_id, "cancelled", None)? {
        terminal_set_provider_job(&conn, &job.execution_id, "cancelled")?;
        let _ = append_audit_event(
            &conn,
            Some(&job.execution_id),
            &job.workflow_run_id,
            "provider.execution.cancelled",
            Some(&serde_json::json!({
                "remoteCancelled": remote_cancelled,
                "supportsCancel": supports_cancel,
                "cancelError": error_note,
            })),
        );
        drop(conn);
        crate::workflow::background_failures::cancel_run_from_background(
            project_root,
            &job.workflow_run_id,
        )?;
        crate::providers::cancellation::unregister(&job.provider_id, &job.provider_job_id);
    }
    Ok(TickDisposition::Cancelled {
        remote_cancelled,
    })
}

/// Fails a durable job: terminal attempt + provider job + workflow run, with
/// the redacted message persisted on the attempt.
fn fail_job(
    project_root: &Path,
    job: &PendingJob,
    message: &str,
    audit_event: &str,
) -> Result<(), AppError> {
    let conn = open_project(project_root)?;
    let error_json = serde_json::json!({"message": message}).to_string();
    if terminal_set_attempt(&conn, &job.execution_id, "failed", Some(&error_json))? {
        terminal_set_provider_job(&conn, &job.execution_id, "failed")?;
        let _ = append_audit_event(
            &conn,
            Some(&job.execution_id),
            &job.workflow_run_id,
            audit_event,
            Some(&serde_json::json!({"error": message})),
        );
        drop(conn);
        crate::workflow::background_failures::fail_run_from_background(
            project_root,
            &job.workflow_run_id,
            message,
        )?;
        crate::providers::cancellation::unregister(&job.provider_id, &job.provider_job_id);
    }
    Ok(())
}

/// Completes a durable job: fetch already done, capture the output through
/// the shared completion module (idempotent on replay), then terminal
/// attempt/step/run transitions.
fn complete_job(
    project_root: &Path,
    job: &PendingJob,
    result: &ProviderResult,
) -> Result<TickDisposition, AppError> {
    // Capture is idempotent: a prior interrupted capture (crash between
    // fetch and persistence) replays safely because the completion module
    // reconciles by provider attempt id.
    let outcome = completion::complete_attempt(project_root, job, result)?;
    match outcome {
        CompletionOutcome::Captured {
            result_set_id,
            artifact_ids,
        } => {
            let conn = open_project(project_root)?;
            if terminal_set_attempt(&conn, &job.execution_id, "succeeded", None)? {
                terminal_set_provider_job(&conn, &job.execution_id, "completed")?;
                crate::providers::repository::update_artifact_ids(
                    &conn,
                    &job.execution_id,
                    &artifact_ids,
                )?;
                let _ = append_audit_event(
                    &conn,
                    Some(&job.execution_id),
                    &job.workflow_run_id,
                    "provider.execution.completed",
                    Some(&serde_json::json!({
                        "resultSetId": result_set_id,
                        "artifactCount": artifact_ids.len(),
                    })),
                );
                drop(conn);
                crate::workflow::background_failures::complete_run_from_background(
                    project_root,
                    &job.workflow_run_id,
                )?;
                crate::providers::cancellation::unregister(&job.provider_id, &job.provider_job_id);
            }
            Ok(TickDisposition::Completed { result_set_id })
        }
        CompletionOutcome::AlreadyTerminal => {
            // A previous completion pass landed the terminal state (crash
            // replay, double tick); nothing to do except finish the run if
            // the earlier pass died between attempt and run transitions.
            crate::workflow::background_failures::complete_run_from_background(
                project_root,
                &job.workflow_run_id,
            )?;
            Ok(TickDisposition::Completed { result_set_id: None })
        }
    }
}

// ---------------------------------------------------------------------------
// Daemon: a per-project background thread that ticks the runner.
// ---------------------------------------------------------------------------

struct DaemonHandle {
    stop: Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
    wake: Option<Sender<()>>,
}

struct DaemonRegistry {
    daemons: std::collections::HashMap<PathBuf, DaemonHandle>,
}

fn registry() -> &'static Mutex<DaemonRegistry> {
    static REGISTRY: OnceLock<Mutex<DaemonRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(DaemonRegistry {
            daemons: std::collections::HashMap::new(),
        })
    })
}

/// Attaches (or re-attaches) the background runner daemon for a project
/// root. Cinery operates on one project at a time, so attaching a new root
/// first stops every other daemon â€” the previous project's runner must
/// never keep polling a closed database. Safe to call repeatedly; tests
/// simulating a restart use this to prove durable resumption.
pub fn attach_runner(project_root: &Path) -> Result<(), AppError> {
    let root = project_root.to_path_buf();
    let mut registry = registry().lock().unwrap();
    // Stop every daemon (single-project application): re-attach for the
    // same root and project-switch both land here.
    for previous in registry.daemons.values_mut() {
        let _ = previous.stop.send(());
        if let Some(thread) = previous.thread.take() {
            let _ = thread.join();
        }
    }
    registry.daemons.clear();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let (wake_tx, wake_rx) = std::sync::mpsc::channel::<()>();
    let daemon_root = root.clone();
    let thread = std::thread::Builder::new()
        .name("cinery-background-jobs".into())
        .spawn(move || {
            daemon_loop(daemon_root, stop_rx, wake_rx);
        })
        .map_err(|error| AppError::FileSystem(error.to_string()))?;
    registry.daemons.insert(
        root,
        DaemonHandle {
            stop: stop_tx,
            thread: Some(thread),
            wake: Some(wake_tx),
        },
    );
    Ok(())
}

/// Stops the daemon for a project root (project closed/switched). The
/// durable provider jobs remain; a later attach resumes them.
pub fn detach_runner(project_root: &Path) {
    let mut registry = registry().lock().unwrap();
    if let Some(mut handle) = registry.daemons.remove(project_root) {
        let _ = handle.stop.send(());
        if let Some(thread) = handle.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Wakes the daemon for a project root so a freshly submitted job starts
/// being polled immediately instead of waiting for the next idle tick.
pub fn wake_runner(project_root: &Path) {
    let registry = registry().lock().unwrap();
    if let Some(handle) = registry.daemons.get(project_root) {
        if let Some(wake) = &handle.wake {
            let _ = wake.send(());
        }
    }
}

fn daemon_loop(root: PathBuf, stop: Receiver<()>, wake: Receiver<()>) {
    loop {
        // Honor stop immediately; drain any pending wake signals.
        match stop.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => return,
            Err(TryRecvError::Empty) => {}
        }
        loop {
            match wake.try_recv() {
                Ok(()) => {}
                Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => break,
            }
        }
        // One deterministic pass over the project's durable jobs. Errors
        // are swallowed here (the daemon must never die); the durable
        // state keeps the job for the next tick.
        let _ = run_pending_jobs(&root);
        // Wait for the next tick or an early wake, whichever comes first.
        let _ = wake.recv_timeout(TICK_INTERVAL);
    }
}

/// Whether a workflow run has a durable non-terminal provider job (used by
/// the runtime's double-advance guard).
pub fn run_has_active_provider_job(
    conn: &Connection,
    workflow_run_id: &str,
) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM workflow_step_executions e
            JOIN provider_jobs pj ON pj.execution_id = e.id
            WHERE e.workflow_run_id = ?1
              AND e.status IN ('submitted', 'running', 'cancellation_requested', 'unknown')
        )",
        params![workflow_run_id],
        |row| row.get(0),
    )
    .map_err(db_error)
}

/// Test-only: empties the process-wide adapter cache so the next
/// `provider_for` rehydrates a fresh instance — exactly what a process
/// restart does to a durable job (the submitting instance is gone).
/// Hidden from docs; production callers never need it (the cache only
/// dies with the process).
#[doc(hidden)]
pub fn reset_provider_cache_for_tests() {
    provider_cache().lock().unwrap().clear();
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}

/// Lists provider jobs for the JobsPanel surface: durable job rows joined
/// with attempt and run context.
pub fn list_provider_jobs(
    project_root: &Path,
) -> Result<Vec<ProviderJobView>, AppError> {
    let conn = open_project(project_root)?;
    let project = read_project(&conn)?;
    let mut statement = conn
        .prepare(
            "SELECT pj.id, pj.provider_id, pj.provider_job_id, pj.status,
                pj.progress_percent, pj.submitted_at, pj.updated_at, pj.last_polled_at,
                e.id, e.workflow_run_id, e.step_definition_id, e.attempt_number,
                e.model_id, e.status, wr.operation_id, wr.status
         FROM provider_jobs pj
         JOIN workflow_step_executions e ON e.id = pj.execution_id
         JOIN workflow_runs wr ON wr.id = e.workflow_run_id
         WHERE wr.project_id = ?1
         ORDER BY pj.submitted_at DESC",
        )
        .map_err(db_error)?;
    let jobs = statement
        .query_map(params![project.id], |row| {
            Ok(ProviderJobView {
                id: row.get(0)?,
                provider_id: row.get(1)?,
                provider_job_id: row.get(2)?,
                status: row.get(3)?,
                progress_percent: row.get::<_, Option<i64>>(4)?.map(|v| v.clamp(0, 100) as u8),
                submitted_at: row.get(5)?,
                updated_at: row.get(6)?,
                last_polled_at: row.get(7)?,
                execution_id: row.get(8)?,
                workflow_run_id: row.get(9)?,
                step_definition_id: row.get(10)?,
                attempt_number: row.get(11)?,
                model_id: row.get(12)?,
                attempt_status: row.get(13)?,
                operation_id: row.get(14)?,
                run_status: row.get(15)?,
            })
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(jobs)
}

/// A provider job as seen by the UI: identifiers, status, progress, and the
/// run/attempt context needed to navigate back to the workflow.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderJobView {
    pub id: String,
    pub provider_id: String,
    pub provider_job_id: String,
    pub status: String,
    pub progress_percent: Option<u8>,
    pub submitted_at: String,
    pub updated_at: String,
    pub last_polled_at: Option<String>,
    pub execution_id: String,
    pub workflow_run_id: String,
    pub step_definition_id: String,
    pub attempt_number: i64,
    pub model_id: String,
    pub attempt_status: String,
    pub operation_id: String,
    pub run_status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_math_uses_the_submitted_timestamp() {
        let submitted = "2026-08-30T00:00:00Z";
        let parsed = parse_timestamp(submitted).unwrap();
        assert_eq!(parsed.to_rfc3339(), "2026-08-30T00:00:00+00:00");
        assert!(parse_timestamp("not-a-time").is_none());
    }
}
