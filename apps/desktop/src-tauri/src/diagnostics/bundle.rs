use crate::diagnostics::redaction::DiagnosticsRedactor;
use crate::error::AppError;
use crate::integration::health;
use crate::project::{paths, repository as project_repository};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

/// App version stamped into the bundle so support requests can be matched
/// against a specific build.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsBundle {
    pub file_name: String,
    pub exported_at: String,
    pub files: Vec<DiagnosticsFile>,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsFile {
    pub name: String,
    pub content: String,
}

/// Collects a redacted, media-free diagnostics bundle for the project.
/// Everything is derived from durable records; no provider is contacted.
pub fn export_bundle(project_root: &Path) -> Result<DiagnosticsBundle, AppError> {
    let manifest = paths::read_manifest(project_root)?;
    let conn = crate::db::open_existing_connection(&project_root.join("project.db"))?;
    let project = project_repository::read_project(&conn)?;
    if project.id != manifest.project_id {
        return Err(AppError::ProjectIdentityMismatch);
    }

    let exported_at = chrono::Utc::now().to_rfc3339();

    let files = vec![
        DiagnosticsFile {
            name: "app-version.json".into(),
            content: to_redacted_json(&app_version_json())?,
        },
        DiagnosticsFile {
            name: "project-summary.json".into(),
            content: to_redacted_json(&project_summary_json(&conn, &project)?)?,
        },
        DiagnosticsFile {
            name: "database-version.json".into(),
            content: to_redacted_json(&database_version_json(&conn)?)?,
        },
        DiagnosticsFile {
            name: "project-health.json".into(),
            content: to_redacted_json(&project_health_json(project_root)?)?,
        },
        DiagnosticsFile {
            name: "active-jobs.json".into(),
            content: to_redacted_json(&active_jobs_json(&conn, &project.id)?)?,
        },
        DiagnosticsFile {
            name: "recent-workflows.json".into(),
            content: to_redacted_json(&recent_workflows_json(&conn, &project.id)?)?,
        },
        DiagnosticsFile {
            name: "logs.txt".into(),
            content: read_project_log(project_root)?,
        },
    ];

    let file_name = format!(
        "cinery-diagnostics-{}.zip",
        exported_at.replace(':', "-").replace("+00:00", "Z")
    );

    Ok(DiagnosticsBundle {
        file_name,
        exported_at,
        files,
        output_path: project_root
            .join("diagnostics")
            .to_string_lossy()
            .into_owned(),
    })
}

fn app_version_json() -> Value {
    json!({
        "app": "AI Cinematic Production OS",
        "version": APP_VERSION,
        "projectFormat": paths::PROJECT_FORMAT,
        "projectSchemaVersion": paths::PROJECT_SCHEMA_VERSION,
    })
}

fn project_summary_json(
    conn: &Connection,
    project: &crate::project::model::ProjectRecord,
) -> Result<Value, AppError> {
    let counts = entity_counts(conn, &project.id)?;
    Ok(json!({
        "projectId": project.id,
        "name": project.name,
        "schemaVersion": project.schema_version,
        "createdAt": project.created_at,
        "updatedAt": project.updated_at,
        "counts": counts,
    }))
}

fn entity_counts(conn: &Connection, project_id: &str) -> Result<Value, AppError> {
    let count = |sql: &str| -> Result<i64, AppError> {
        conn.query_row(sql, [project_id], |row| row.get(0))
            .map_err(|e| AppError::Database(e.to_string()))
    };
    Ok(json!({
        "canonEntities": count("SELECT COUNT(*) FROM canon_entities WHERE project_id = ?1")?,
        "assets": count("SELECT COUNT(*) FROM assets WHERE project_id = ?1")?,
        "assetVersions": count(
            "SELECT COUNT(*) FROM asset_versions av JOIN assets a ON a.id = av.asset_id WHERE a.project_id = ?1")?,
        "workflowRuns": count("SELECT COUNT(*) FROM workflow_runs WHERE project_id = ?1")?,
        "qaRuns": count("SELECT COUNT(*) FROM qa_runs WHERE project_id = ?1")?,
        "scenes": count("SELECT COUNT(*) FROM world_scenes WHERE project_id = ?1")?,
        "shots": count("SELECT COUNT(*) FROM scene_shots s JOIN world_scenes sc ON sc.id = s.scene_id WHERE sc.project_id = ?1")?,
        "cinemaCompilations": count("SELECT COUNT(*) FROM scene_compilations WHERE project_id = ?1")?,
    }))
}

fn database_version_json(conn: &Connection) -> Result<Value, AppError> {
    let mut stmt = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let versions = stmt
        .query_map([], |row| row.get::<_, i64>(0))
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;
    let latest = versions.last().copied().unwrap_or(0);
    Ok(json!({
        "appliedMigrations": versions,
        "latestVersion": latest,
    }))
}

fn project_health_json(project_root: &Path) -> Result<Value, AppError> {
    let issues = health::scan_project(project_root)?;
    serde_json::to_value(&issues).map_err(|e| AppError::Database(e.to_string()))
}

#[derive(Debug)]
struct ActiveJobRow {
    id: String,
    operation_id: String,
    status: String,
    created_at: String,
    updated_at: String,
}

fn active_jobs_json(conn: &Connection, project_id: &str) -> Result<Value, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, operation_id, status, created_at, updated_at FROM workflow_runs \
             WHERE project_id = ?1 AND status IN ('created', 'running', 'waiting_for_approval', 'ready_for_execution') \
             ORDER BY updated_at DESC, id DESC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok(ActiveJobRow {
                id: row.get(0)?,
                operation_id: row.get(1)?,
                status: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let jobs: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "workflowRunId": row.id,
                "operationId": row.operation_id,
                "status": row.status,
                "createdAt": row.created_at,
                "updatedAt": row.updated_at,
            })
        })
        .collect();
    Ok(json!({ "activeJobs": jobs }))
}

fn recent_workflows_json(conn: &Connection, project_id: &str) -> Result<Value, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, operation_id, status, failure_code, failure_message, created_at, completed_at \
             FROM workflow_runs WHERE project_id = ?1 ORDER BY created_at DESC, id DESC LIMIT 20",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut workflows = Vec::new();
    for (id, operation_id, status, failure_code, failure_message, created_at, completed_at) in rows
    {
        let duration_ms = completed_at
            .as_deref()
            .and_then(|end| parse_duration_ms(&created_at, end));
        let provider_runs = provider_runs_json(conn, &id)?;
        let qa_runs = qa_runs_json(conn, &id)?;
        let failure_stage = failure_code
            .as_deref()
            .map(|_| derive_failure_stage(&status, &provider_runs, &qa_runs));
        workflows.push(json!({
            "workflowRunId": id,
            "operationId": operation_id,
            "status": status,
            "createdAt": created_at,
            "completedAt": completed_at,
            "durationMs": duration_ms,
            "failureCode": failure_code,
            "failureMessage": failure_message,
            "failureStage": failure_stage,
            "providerRuns": provider_runs,
            "qaRuns": qa_runs,
        }));
    }
    Ok(json!({ "recentWorkflows": workflows }))
}

fn provider_runs_json(conn: &Connection, workflow_run_id: &str) -> Result<Vec<Value>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, step_definition_id, attempt_number, provider_id, model_id, status, \
                    normalized_error_json, started_at, completed_at \
             FROM workflow_step_executions WHERE workflow_run_id = ?1 ORDER BY started_at, attempt_number",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([workflow_run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut runs = Vec::new();
    for (id, step, attempt, provider_id, model_id, status, error_json, started_at, completed_at) in
        rows
    {
        let duration_ms = completed_at
            .as_deref()
            .and_then(|end| parse_duration_ms(&started_at, end));
        let error = error_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .map(|value| DiagnosticsRedactor::redact_json(&value));
        runs.push(json!({
            "executionId": id,
            "stepDefinitionId": step,
            "attemptNumber": attempt,
            "providerId": provider_id,
            "modelId": model_id,
            "status": status,
            "startedAt": started_at,
            "completedAt": completed_at,
            "durationMs": duration_ms,
            "error": error,
        }));
    }
    Ok(runs)
}

fn qa_runs_json(conn: &Connection, workflow_run_id: &str) -> Result<Vec<Value>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, status, overall_status, adapter_id, error_code, created_at, started_at, completed_at \
             FROM qa_runs WHERE workflow_run_id = ?1 ORDER BY created_at",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([workflow_run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut runs = Vec::new();
    for (
        id,
        status,
        overall_status,
        adapter_id,
        error_code,
        created_at,
        started_at,
        completed_at,
    ) in rows
    {
        let duration_ms = started_at
            .as_deref()
            .zip(completed_at.as_deref())
            .and_then(|(start, end)| parse_duration_ms(start, end))
            .or_else(|| {
                completed_at
                    .as_deref()
                    .and_then(|end| parse_duration_ms(&created_at, end))
            });
        runs.push(json!({
            "qaRunId": id,
            "status": status,
            "overallStatus": overall_status,
            "adapterId": adapter_id,
            "errorCode": error_code,
            "createdAt": created_at,
            "startedAt": started_at,
            "completedAt": completed_at,
            "durationMs": duration_ms,
        }));
    }
    Ok(runs)
}

/// Correlates where a workflow stopped using its durable records:
/// a failed provider execution means the provider stage, otherwise a failed
/// QA run means the QA stage, otherwise the workflow's own terminal state.
fn derive_failure_stage(status: &str, provider_runs: &[Value], qa_runs: &[Value]) -> String {
    let _ = status;
    if provider_runs
        .iter()
        .any(|run| run.get("status").and_then(Value::as_str) == Some("failed"))
    {
        return "provider".into();
    }
    if qa_runs
        .iter()
        .any(|run| run.get("status").and_then(Value::as_str) == Some("failed"))
    {
        return "qa".into();
    }
    "workflow".into()
}

fn parse_duration_ms(start: &str, end: &str) -> Option<i64> {
    let start = chrono::DateTime::parse_from_rfc3339(start).ok()?;
    let end = chrono::DateTime::parse_from_rfc3339(end).ok()?;
    Some((end - start).num_milliseconds().max(0))
}

fn read_project_log(project_root: &Path) -> Result<String, AppError> {
    let path = project_root.join("diagnostics").join("logs.txt");
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| AppError::FileSystem(e.to_string()))
}

fn to_redacted_json(value: &Value) -> Result<String, AppError> {
    let redacted = DiagnosticsRedactor::redact_json(value);
    serde_json::to_string_pretty(&redacted).map_err(|e| AppError::Database(e.to_string()))
}

/// Appends a structured event to the project diagnostics log. Used by
/// workflow/provider/QA execution paths and available to future call sites.
pub fn log_event(
    project_root: &Path,
    subsystem: &str,
    event: &str,
    correlation_id: Option<&str>,
    message: &str,
) -> Result<(), AppError> {
    crate::diagnostics::log::DiagnosticLog::new(project_root).append(
        subsystem,
        event,
        correlation_id,
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::service::ProjectService;
    use tempfile::tempdir;

    fn fixture_project() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempdir().unwrap();
        let root = temp.path().join("diag-project");
        ProjectService::create(&root, "Diag Project").unwrap();
        (temp, root)
    }

    #[test]
    fn bundle_contains_all_required_files() {
        let (_temp, root) = fixture_project();
        let bundle = export_bundle(&root).unwrap();

        let names: Vec<&str> = bundle.files.iter().map(|f| f.name.as_str()).collect();
        for required in [
            "app-version.json",
            "project-summary.json",
            "database-version.json",
            "project-health.json",
            "active-jobs.json",
            "recent-workflows.json",
            "logs.txt",
        ] {
            assert!(names.contains(&required), "missing {required}");
        }
        assert!(bundle.file_name.starts_with("cinery-diagnostics-"));
        assert!(bundle.file_name.ends_with(".zip"));
    }

    #[test]
    fn bundle_is_redacted_and_media_free() {
        let (temp, root) = fixture_project();
        let secret = "sk-test-secret-abcdef123456";
        log_event(
            &root,
            "providers",
            "run_failed",
            Some("run-1"),
            &format!("call failed auth {secret}"),
        )
        .unwrap();

        // A stray media file in the project must not appear in the bundle.
        std::fs::write(
            temp.path()
                .join("diag-project")
                .join("assets")
                .join("leak.png"),
            b"binary",
        )
        .unwrap();

        let bundle = export_bundle(&root).unwrap();
        for file in &bundle.files {
            assert!(
                !file.content.contains(secret),
                "{} leaked secret",
                file.name
            );
            assert!(
                !file.content.contains("leak.png"),
                "{} leaked media path",
                file.name
            );
        }
    }

    #[test]
    fn recent_workflows_include_failure_stage_and_durations() {
        let (_temp, root) = fixture_project();
        let conn = crate::db::open_existing_connection(&root.join("project.db")).unwrap();
        let project = project_repository::read_project(&conn).unwrap();
        conn.execute(
            "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, status, input_json, failure_code, failure_message, created_at, updated_at, completed_at) \
             VALUES ('run-1', ?1, 'face-lock', '1.0.0', 'build_face_lock', 'failed', '{}', 'PROVIDER_FAILED', 'provider exploded', '2026-08-28T10:00:00Z', '2026-08-28T10:01:30Z', '2026-08-28T10:01:30Z')",
            [&project.id],
        ).unwrap();
        conn.execute(
            "INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id, provider_id, model_id, adapter_version, idempotency_key, status, artifact_ids_json, started_at) \
             VALUES ('exec-1', 'run-1', 'generate', 1, 'req-1', 'mock', 'mock-large', 1, 'idem-1', 'failed', '[]', '2026-08-28T10:00:10Z')",
            [],
        ).unwrap();
        drop(conn);

        let bundle = export_bundle(&root).unwrap();
        let workflows_file = bundle
            .files
            .iter()
            .find(|f| f.name == "recent-workflows.json")
            .unwrap();
        assert!(workflows_file
            .content
            .contains("\"failureStage\": \"provider\""));
        assert!(workflows_file.content.contains("\"durationMs\": 90000"));
        assert!(workflows_file.content.contains("PROVIDER_FAILED"));
    }

    #[test]
    fn rejects_project_identity_mismatch() {
        let (_temp, root) = fixture_project();
        std::fs::write(
            root.join("project.yaml"),
            "format: ai-cinematic-production-os\nproject_id: 01WRONGID0000000000000000\nschema_version: 1\n",
        )
        .unwrap();

        let error = export_bundle(&root).unwrap_err();
        assert!(matches!(error, AppError::ProjectIdentityMismatch));
    }

    #[test]
    fn log_event_appends_to_project_log() {
        let (_temp, root) = fixture_project();
        log_event(&root, "qa", "qa_failed", Some("qa-9"), "eyebrow mismatch").unwrap();
        let contents = std::fs::read_to_string(root.join("diagnostics").join("logs.txt")).unwrap();
        assert!(contents.contains("qa\tqa_failed\tqa-9\teyebrow mismatch"));
    }
}
