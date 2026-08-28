use cinematic_desktop_lib::db;
use cinematic_desktop_lib::error::AppError;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::workflow::runtime::WorkflowRuntime;
use rusqlite::params;
use serde_json::json;
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempdir().unwrap();
    let root = temp.path().join("red-door");
    ProjectService::create(&root, "Red Door").unwrap();
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
            json!({"text":"Angular face and dark hair."}),
            2,
        ),
        (
            "locks",
            "visual_locks",
            json!({"locks":[{"id":"scar","key":"right_eyebrow_scar","description":"Small healed scar on character-right eyebrow.","severity":"required","validatorHint":"viewer-left when frontal"}]}),
            3,
        ),
    ] {
        conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at) VALUES (?1, 'mara', ?2, ?3, 'locked', ?4, 'now', 'now', 'now')", params![id, key, value.to_string(), revision]).unwrap();
    }
    conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at) VALUES ('draft', 'mara', 'psychology', '{\"text\":\"draft\"}', 'draft', 1, 'now', 'now')", []).unwrap();
    (temp, root)
}

fn face_lock_input(root: &std::path::Path) -> serde_json::Value {
    json!({
        "projectRootPath": root.to_string_lossy(),
        "characterEntityId": "mara",
        "visualSpec": {
            "head": "oval", "eyes": "brown", "brows": "straight",
            "nose": "narrow", "lips": "neutral", "skin": "olive",
            "hair": "black shoulder-length", "build": "athletic", "expression": "neutral"
        },
        "baselineWardrobe": "charcoal crew neck"
    })
}

#[test]
fn face_lock_waits_for_approval_then_requires_explicit_dry_run() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.0.0",
        "character.create_face_lock",
        face_lock_input(&root),
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    assert!(!root
        .join("workflows")
        .join(&created.run.id)
        .join("dry-run-result.json")
        .exists());
    let waiting_event_count = waiting.events.len();
    assert!(matches!(
        WorkflowRuntime::advance_run(&root, &created.run.id).unwrap_err(),
        AppError::WorkflowApprovalRequired
    ));
    assert_eq!(
        WorkflowRuntime::get_run(&root, &created.run.id)
            .unwrap()
            .events
            .len(),
        waiting_event_count
    );

    let request: serde_json::Value = serde_json::from_str(
        waiting
            .steps
            .iter()
            .find(|step| step.step_type == "compile_request")
            .unwrap()
            .output_json
            .as_deref()
            .unwrap(),
    )
    .unwrap();
    assert!(request.get("provider").is_none());
    assert!(request.get("model").is_none());
    assert!(request["prompt"]
        .as_str()
        .unwrap()
        .contains("right_eyebrow_scar"));
    for section in ["POSE / EXPRESSION", "BIOLOGICAL REALISM"] {
        assert!(request["prompt"].as_str().unwrap().contains(section));
    }
    assert!(request["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|constraint| constraint["type"] == "preserve_visual_lock"
            && constraint["key"] == "right_eyebrow_scar"));

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    conn.execute(
        "UPDATE canon_sections SET value_json = '{\"text\":\"Mutated after launch.\"}', revision = 99 WHERE id = 'summary'",
        [],
    )
    .unwrap();
    drop(conn);
    ProjectService::open(&root).unwrap();
    assert_eq!(
        WorkflowRuntime::get_run(&root, &created.run.id)
            .unwrap()
            .run
            .context_snapshot_json,
        waiting.run.context_snapshot_json
    );

    assert!(matches!(
        WorkflowRuntime::approve_run_step(&root, &created.run.id, "validate-input", None)
            .unwrap_err(),
        AppError::WorkflowStepNotFound(_)
    ));
    assert_eq!(
        WorkflowRuntime::get_run(&root, &created.run.id)
            .unwrap()
            .run
            .status,
        "waiting_for_approval"
    );

    let ready =
        WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();
    assert_eq!(ready.run.status, "ready_for_execution");
    assert!(!root
        .join("workflows")
        .join(&created.run.id)
        .join("dry-run-result.json")
        .exists());
    assert_eq!(
        WorkflowRuntime::get_run(&root, &created.run.id)
            .unwrap()
            .run
            .status,
        "ready_for_execution"
    );
    ProjectService::open(&root).unwrap();
    assert_eq!(
        WorkflowRuntime::get_run(&root, &created.run.id)
            .unwrap()
            .run
            .status,
        "ready_for_execution"
    );
    assert!(matches!(
        WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None)
            .unwrap_err(),
        AppError::WorkflowApprovalAlreadyDecided(_)
    ));
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let approved_artifact: String = conn
        .query_row(
            "SELECT artifact_json FROM workflow_approvals WHERE workflow_run_id = ?1",
            [&created.run.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&approved_artifact).unwrap(),
        request
    );
    drop(conn);

    let completed = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");
    assert!(root
        .join("workflows")
        .join(&created.run.id)
        .join("dry-run-result.json")
        .exists());
    assert_eq!(
        completed
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        (1..=completed.events.len() as i64).collect::<Vec<_>>()
    );

    let event_types = completed
        .events
        .iter()
        .map(|event| event.event_type.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        event_types,
        vec![
            "run_created",
            "run_started",
            "step_started",
            "step_completed",
            "step_started",
            "step_completed",
            "step_started",
            "step_completed",
            "step_started",
            "approval_requested",
            "approval_granted",
            "step_completed",
            "step_started",
            "execution_started",
            "execution_completed",
            "step_completed",
            "step_started",
            "step_completed",
            "run_completed",
        ]
    );

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let asset_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM asset_versions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(asset_count, 0);
}

#[test]
fn rejected_run_is_terminal_and_skips_remaining_steps() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.0.0",
        "character.create_face_lock",
        face_lock_input(&root),
    )
    .unwrap();
    WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();

    let rejected = WorkflowRuntime::reject_run_step(
        &root,
        &created.run.id,
        "approve-request",
        Some("Identity drift".into()),
    )
    .unwrap();

    assert_eq!(rejected.run.status, "rejected");
    assert!(rejected
        .steps
        .iter()
        .filter(|step| step.step_index > 3)
        .all(|step| step.status == "skipped"));
    assert_eq!(
        rejected.events.last().unwrap().event_type,
        "approval_rejected"
    );
    assert!(matches!(
        WorkflowRuntime::advance_run(&root, &created.run.id).unwrap_err(),
        AppError::WorkflowRunTerminal
    ));
}

#[test]
fn invalid_or_missing_character_input_creates_no_persisted_run() {
    let (_temp, root) = fixture();
    let mut invalid = face_lock_input(&root);
    invalid["visualSpec"]["eyes"] = json!("");
    assert!(matches!(
        WorkflowRuntime::create_run(
            &root,
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            invalid
        )
        .unwrap_err(),
        AppError::WorkflowInputInvalid(_)
    ));

    let mut missing = face_lock_input(&root);
    missing["characterEntityId"] = json!("not-a-character");
    assert!(matches!(
        WorkflowRuntime::create_run(
            &root,
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            missing
        )
        .unwrap_err(),
        AppError::WorkflowPrerequisiteFailed(_)
    ));

    assert!(WorkflowRuntime::list_runs(&root).unwrap().is_empty());
}

#[test]
fn cancelled_run_is_terminal_and_skips_pending_work() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.0.0",
        "character.create_face_lock",
        face_lock_input(&root),
    )
    .unwrap();

    let cancelled = WorkflowRuntime::cancel_run(&root, &created.run.id).unwrap();

    assert_eq!(cancelled.run.status, "cancelled");
    assert!(cancelled.steps.iter().all(|step| step.status == "skipped"));
    assert_eq!(cancelled.events.last().unwrap().event_type, "run_cancelled");
    assert!(matches!(
        WorkflowRuntime::advance_run(&root, &created.run.id).unwrap_err(),
        AppError::WorkflowRunTerminal
    ));
}

#[test]
fn recovery_fails_interrupted_running_work_without_replaying_it() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.0.0",
        "character.create_face_lock",
        face_lock_input(&root),
    )
    .unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    conn.execute(
        "UPDATE workflow_runs SET status = 'running' WHERE id = ?1",
        [&created.run.id],
    )
    .unwrap();
    conn.execute("UPDATE workflow_steps SET status = 'running' WHERE workflow_run_id = ?1 AND step_index = 0", [&created.run.id]).unwrap();
    drop(conn);

    ProjectService::open(&root).unwrap();
    let recovered = WorkflowRuntime::get_run(&root, &created.run.id).unwrap();
    assert_eq!(recovered.run.status, "failed");
    assert_eq!(
        recovered.run.failure_code.as_deref(),
        Some("INTERRUPTED_DURING_STEP")
    );
    assert_eq!(recovered.steps[0].status, "failed");
    assert_eq!(recovered.events.last().unwrap().event_type, "run_failed");
}

#[test]
fn recovery_fails_ready_run_with_an_interrupted_execute_step() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.0.0",
        "character.create_face_lock",
        face_lock_input(&root),
    )
    .unwrap();
    WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    conn.execute(
        "UPDATE workflow_steps SET status = 'running' WHERE workflow_run_id = ?1 AND step_definition_id = 'execute'",
        [&created.run.id],
    )
    .unwrap();
    drop(conn);

    ProjectService::open(&root).unwrap();

    let recovered = WorkflowRuntime::get_run(&root, &created.run.id).unwrap();
    assert_eq!(recovered.run.status, "failed");
    assert_eq!(
        recovered.run.failure_code.as_deref(),
        Some("INTERRUPTED_DURING_STEP")
    );
}

#[test]
fn execution_failure_transitions_run_to_failed_with_an_audit_event() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.0.0",
        "character.create_face_lock",
        face_lock_input(&root),
    )
    .unwrap();
    WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    conn.execute(
        "UPDATE workflow_steps SET output_json = 'not-json' WHERE workflow_run_id = ?1 AND step_type = 'compile_request'",
        [&created.run.id],
    )
    .unwrap();
    drop(conn);

    assert!(WorkflowRuntime::advance_run(&root, &created.run.id).is_err());

    let failed = WorkflowRuntime::get_run(&root, &created.run.id).unwrap();
    assert_eq!(failed.run.status, "failed");
    assert_eq!(
        failed.run.failure_code.as_deref(),
        Some("WORKFLOW_STEP_FAILED")
    );
    assert_eq!(failed.events.last().unwrap().event_type, "run_failed");
}
