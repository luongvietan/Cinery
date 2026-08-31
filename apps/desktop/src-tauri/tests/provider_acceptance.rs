use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::providers::commands::cancel_workflow_execution;
use cinematic_desktop_lib::providers::commands::retry_workflow_execution;
use cinematic_desktop_lib::workflow::runtime::WorkflowRuntime;
use rusqlite::params;
use serde_json::json;
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempdir().unwrap();
    let root = temp.path().join("provider-project");
    ProjectService::create(&root, "Provider Project").unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();
    conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('mara', ?1, 'character', 'Mara', 'mara', 'now', 'now')", [&project_id]).unwrap();
    for (id, key, value, revision) in [
        ("role", "role_tag", json!({"text":"Protagonist"}), 1),
        (
            "summary",
            "visual_summary",
            json!({"text":"Angular face."}),
            2,
        ),
        ("locks", "visual_locks", json!({"locks":[]}), 3),
    ] {
        conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at) VALUES (?1, 'mara', ?2, ?3, 'locked', ?4, 'now', 'now', 'now')", params![id, key, value.to_string(), revision]).unwrap();
    }
    (temp, root)
}

#[test]
fn approved_mock_execution_persists_attempt_job_and_candidate_artifact() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.1.0",
        "character.create_face_lock",
        json!({
            "projectRootPath": root.to_string_lossy(),
            "characterEntityId": "mara",
            "visualSpec": {"head":"oval","eyes":"brown","brows":"straight","nose":"narrow","lips":"neutral","skin":"olive","hair":"black","build":"athletic","expression":"neutral"},
            "baselineWardrobe": "charcoal crew neck",
            "providerId": "mock",
            "modelId": "mock-image-v1"
        }),
    ).unwrap();
    let waiting = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();

    let completed = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let attempt_count: i64 = conn.query_row("SELECT COUNT(*) FROM workflow_step_executions WHERE workflow_run_id = ?1 AND status = 'succeeded'", [&created.run.id], |row| row.get(0)).unwrap();
    let job_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM provider_jobs", [], |row| row.get(0))
        .unwrap();
    // Result-set capture defers asset-version creation to explicit
    // promotion, so no version exists until the user saves a candidate.
    let asset_version_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM asset_versions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(asset_version_count, 0);
    let result_sets: i64 = conn
        .query_row("SELECT COUNT(*) FROM generation_result_sets", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(result_sets, 1);
    let audit_event_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM provider_audit_events WHERE workflow_run_id = ?1",
            [&created.run.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(attempt_count, 1);
    assert_eq!(job_count, 1);
    // queued + submitted + result_set.created + 4x artifact.materialized
    assert_eq!(audit_event_count, 8);
}

/// P10.1 regression: a synchronous provider completes inline, so its
/// durable provider_jobs row must be terminal too — never a lingering
/// 'submitted' ghost row that pollutes the Jobs panel or the runner's
/// discovery set.
#[test]
fn inline_sync_completion_terminal_sets_the_provider_job_row() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.1.0",
        "character.create_face_lock",
        json!({
            "projectRootPath": root.to_string_lossy(),
            "characterEntityId": "mara",
            "visualSpec": {"head":"oval","eyes":"brown","brows":"straight","nose":"narrow","lips":"neutral","skin":"olive","hair":"black","build":"athletic","expression":"neutral"},
            "baselineWardrobe": "charcoal crew neck",
            "providerId": "mock",
            "modelId": "mock-image-v1"
        }),
    ).unwrap();
    let waiting = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();

    let completed = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let (job_status, job_operation): (String, Option<String>) = conn
        .query_row(
            "SELECT status, operation FROM provider_jobs",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        job_status, "completed",
        "an inline-completed sync job must terminal-set its provider_jobs row"
    );
    assert_eq!(
        job_operation, None,
        "the sync mock adapter has no async operation identity"
    );
}

#[test]
fn recovery_preserves_a_durable_remote_job_for_reconciliation() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.1.0",
        "character.create_face_lock",
        json!({"projectRootPath":root.to_string_lossy(),"characterEntityId":"mara","visualSpec":{"head":"oval","eyes":"brown","brows":"straight","nose":"narrow","lips":"neutral","skin":"olive","hair":"black","build":"athletic","expression":"neutral"},"baselineWardrobe":"charcoal"}),
    ).unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    conn.execute(
        "UPDATE workflow_runs SET status = 'running' WHERE id = ?1",
        [&created.run.id],
    )
    .unwrap();
    conn.execute("UPDATE workflow_steps SET status = 'running' WHERE workflow_run_id = ?1 AND step_definition_id = 'execute'", [&created.run.id]).unwrap();
    conn.execute("INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id, provider_id, model_id, adapter_version, idempotency_key, status, provider_job_id, artifact_ids_json, started_at) VALUES ('execution-1', ?1, 'execute', 1, 'compiled', 'mock', 'mock-image-v1', 1, 'idempotency-1', 'running', 'remote-job-1', '[]', 'now')", [&created.run.id]).unwrap();
    drop(conn);

    ProjectService::open(&root).unwrap();
    let recovered = WorkflowRuntime::get_run(&root, &created.run.id).unwrap();
    assert_eq!(recovered.run.status, "running");
    assert_eq!(
        recovered
            .steps
            .iter()
            .find(|step| step.step_definition_id == "execute")
            .unwrap()
            .status,
        "running"
    );
}

#[test]
fn failed_provider_attempt_is_immutable_and_retry_creates_a_new_attempt() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.1.0",
        "character.create_face_lock",
        json!({"projectRootPath":root.to_string_lossy(),"characterEntityId":"mara","visualSpec":{"head":"oval","eyes":"brown","brows":"straight","nose":"narrow","lips":"neutral","skin":"olive","hair":"black","build":"athletic","expression":"neutral"},"baselineWardrobe":"charcoal","providerId":"missing","modelId":"missing-v1"}),
    ).unwrap();
    WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();
    assert!(WorkflowRuntime::advance_run(&root, &created.run.id).is_err());

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let status: String = conn
        .query_row(
            "SELECT status FROM workflow_step_executions WHERE workflow_run_id = ?1",
            [&created.run.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "failed");
    drop(conn);

    let retried = retry_workflow_execution(
        root.to_string_lossy().into(),
        created.run.id.clone(),
        "execute".into(),
    )
    .unwrap();
    assert_eq!(retried.run.status, "ready_for_execution");
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let attempt_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM workflow_step_executions WHERE workflow_run_id = ?1",
            [&created.run.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(attempt_count, 2);
}

#[test]
fn cancellation_marks_remote_attempt_cancelled_and_run_terminal() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.1.0",
        "character.create_face_lock",
        json!({"projectRootPath":root.to_string_lossy(),"characterEntityId":"mara","visualSpec":{"head":"oval","eyes":"brown","brows":"straight","nose":"narrow","lips":"neutral","skin":"olive","hair":"black","build":"athletic","expression":"neutral"},"baselineWardrobe":"charcoal"}),
    ).unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    conn.execute(
        "UPDATE workflow_runs SET status = 'running' WHERE id = ?1",
        [&created.run.id],
    )
    .unwrap();
    conn.execute("UPDATE workflow_steps SET status = 'running' WHERE workflow_run_id = ?1 AND step_definition_id = 'execute'", [&created.run.id]).unwrap();
    conn.execute("INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id, provider_id, model_id, adapter_version, idempotency_key, status, provider_job_id, artifact_ids_json, started_at) VALUES ('execution-cancel', ?1, 'execute', 1, 'compiled', 'mock', 'mock-image-v1', 1, 'idempotency-cancel', 'running', 'remote-job-cancel', '[]', 'now')", [&created.run.id]).unwrap();
    drop(conn);

    let cancelled = cancel_workflow_execution(
        root.to_string_lossy().into(),
        created.run.id.clone(),
        "execute".into(),
    )
    .unwrap();
    assert_eq!(cancelled.run.status, "cancelled");
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let status: String = conn
        .query_row(
            "SELECT status FROM workflow_step_executions WHERE id = 'execution-cancel'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "cancelled");
}

#[test]
fn list_providers_includes_builtin_registry_ids_for_a_project() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("provider-list");
    cinematic_desktop_lib::project::service::ProjectService::create(&root, "Provider List").unwrap();

    let providers = cinematic_desktop_lib::providers::commands::list_providers(Some(
        root.to_string_lossy().to_string(),
    ))
    .unwrap();

    // The generation forms rely on the built-ins being listed for every
    // project; customs are merged in on top.
    for builtin in ["dry_run", "mock", "openai"] {
        assert!(
            providers.iter().any(|id| id == builtin),
            "builtin provider {builtin} must be listed, got {providers:?}"
        );
    }
}
