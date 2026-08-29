use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::qa::repository;
use cinematic_desktop_lib::workflow::runtime::WorkflowRuntime;
use serde_json::json;

struct Fixture {
    _temp: tempfile::TempDir,
    root: std::path::PathBuf,
    project_id: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let project = ProjectService::create(&root, "QA Project").unwrap();
        let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
        conn.execute_batch(&format!(
            "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at)
             VALUES ('character-1', '{}', 'character', 'Mara', 'mara', 'now', 'now');
             INSERT INTO canon_sections
             (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at)
             VALUES ('locks', 'character-1', 'visual_locks',
                     '{{\"locks\":[{{\"id\":\"scar\",\"key\":\"right_eyebrow_scar\",\"description\":\"Scar on character-right eyebrow\",\"severity\":\"required\",\"validatorHint\":null}}]}}',
                     'locked', 1, 'now', 'now', 'now');
             INSERT INTO assets
             (id, project_id, type, label, owner_entity_id, canonical_version_id, created_at, updated_at)
             VALUES
             ('face', '{}', 'face_lock', 'Face', 'character-1', 'face-v1', 'now', 'now'),
             ('look', '{}', 'character_sheet', 'Look', 'character-1', 'look-v1', 'now', 'now'),
             ('target', '{}', 'image', 'Candidate', 'character-1', NULL, 'now', 'now');
             INSERT INTO asset_versions
             (id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
              original_filename, mime_type, byte_size, created_at)
             VALUES
             ('face-v1', 'face', 1, 'canonical', 'face.png', 'face-thumb.png',
              'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'face.png', 'image/png', 1, 'now'),
             ('look-v1', 'look', 1, 'canonical', 'look.png', 'look-thumb.png',
              'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'look.png', 'image/png', 1, 'now'),
             ('target-v1', 'target', 1, 'candidate', 'target.png', 'target-thumb.png',
              'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'target.png', 'image/png', 1, 'now');",
            project.id, project.id, project.id, project.id
        ))
        .unwrap();
        Self {
            _temp: temp,
            root,
            project_id: project.id,
        }
    }

    fn valid_response(&self) -> serde_json::Value {
        let checks = [
            "artifact:unexpected",
            "artifact:watermark",
            "lock:right_eyebrow_scar",
            "reference:identity",
            "reference:look",
        ]
        .into_iter()
        .map(|check_id| {
            json!({
                "checkId": check_id,
                "status": "pass",
                "confidence": 0.95,
                "observed": "Matches",
                "reason": "Fixture evidence",
                "repairHint": null
            })
        })
        .collect::<Vec<_>>();
        json!({"schemaVersion":1,"checks":checks,"modelSummary":"All checks pass"})
    }

    fn input(&self, response: serde_json::Value) -> serde_json::Value {
        json!({
            "projectRootPath": self.root,
            "assetVersionId": "target-v1",
            "adapterId": "mock",
            "modelId": "mock-vlm",
            "expectations": [],
            "mockResponse": response
        })
    }
}

#[test]
fn visual_qa_waits_for_approval_then_persists_auditable_results() {
    let fixture = Fixture::new();
    let created = WorkflowRuntime::create_run(
        &fixture.root,
        "visual-qa",
        "1.0.0",
        "asset.run_visual_qa",
        fixture.input(fixture.valid_response()),
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(&fixture.root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");

    let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
    let queued =
        repository::list_runs_for_asset_version(&conn, &fixture.project_id, "target-v1").unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(
        queued[0].workflow_run_id.as_deref(),
        Some(created.run.id.as_str())
    );
    drop(conn);

    WorkflowRuntime::approve_run_step(
        &fixture.root,
        &created.run.id,
        "approve-qa",
        Some("Run disclosed local QA".into()),
    )
    .unwrap();
    let completed = WorkflowRuntime::advance_run(&fixture.root, &created.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");

    let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
    let qa_id = repository::list_runs_for_asset_version(&conn, &fixture.project_id, "target-v1")
        .unwrap()[0]
        .id
        .clone();
    let qa = repository::get_run(&conn, &fixture.project_id, &qa_id)
        .unwrap()
        .unwrap();
    assert_eq!(qa.run.status.to_string(), "succeeded");
    assert_eq!(qa.run.overall_status.unwrap().to_string(), "pass");
    assert_eq!(qa.checks.len(), 5);
}

#[test]
fn malformed_adapter_output_fails_qa_without_fabricating_checks() {
    let fixture = Fixture::new();
    let created = WorkflowRuntime::create_run(
        &fixture.root,
        "visual-qa",
        "1.0.0",
        "asset.run_visual_qa",
        fixture.input(json!({"invalid": true})),
    )
    .unwrap();
    WorkflowRuntime::advance_run(&fixture.root, &created.run.id).unwrap();
    WorkflowRuntime::approve_run_step(&fixture.root, &created.run.id, "approve-qa", None).unwrap();
    assert!(WorkflowRuntime::advance_run(&fixture.root, &created.run.id).is_err());

    let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
    let qa_run = repository::list_runs_for_asset_version(&conn, &fixture.project_id, "target-v1")
        .unwrap()
        .pop()
        .unwrap();
    let qa = repository::get_run(&conn, &fixture.project_id, &qa_run.id)
        .unwrap()
        .unwrap();
    assert_eq!(qa.run.status.to_string(), "failed");
    assert!(qa.checks.is_empty());
}

#[test]
fn rejecting_visual_qa_cancels_the_queued_qa_run() {
    let fixture = Fixture::new();
    let created = WorkflowRuntime::create_run(
        &fixture.root,
        "visual-qa",
        "1.0.0",
        "asset.run_visual_qa",
        fixture.input(fixture.valid_response()),
    )
    .unwrap();
    WorkflowRuntime::advance_run(&fixture.root, &created.run.id).unwrap();
    WorkflowRuntime::reject_run_step(
        &fixture.root,
        &created.run.id,
        "approve-qa",
        Some("Not now".into()),
    )
    .unwrap();

    let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
    let qa_run = repository::list_runs_for_asset_version(&conn, &fixture.project_id, "target-v1")
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(qa_run.status.to_string(), "cancelled");
}

#[test]
fn reopening_project_marks_interrupted_visual_qa_failed_without_reexecution() {
    let fixture = Fixture::new();
    let created = WorkflowRuntime::create_run(
        &fixture.root,
        "visual-qa",
        "1.0.0",
        "asset.run_visual_qa",
        fixture.input(fixture.valid_response()),
    )
    .unwrap();
    WorkflowRuntime::advance_run(&fixture.root, &created.run.id).unwrap();
    let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
    conn.execute(
        "UPDATE workflow_runs SET status = 'running' WHERE id = ?1",
        [&created.run.id],
    )
    .unwrap();
    conn.execute(
        "UPDATE workflow_steps SET status = 'running' WHERE workflow_run_id = ?1 AND step_definition_id = 'execute'",
        [&created.run.id],
    )
    .unwrap();
    conn.execute(
        "UPDATE qa_runs SET status = 'running' WHERE workflow_run_id = ?1",
        [&created.run.id],
    )
    .unwrap();
    drop(conn);

    ProjectService::open(&fixture.root).unwrap();

    let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
    let qa_run = repository::list_runs_for_asset_version(&conn, &fixture.project_id, "target-v1")
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(qa_run.status.to_string(), "failed");
    assert_eq!(
        qa_run.error_code.as_deref(),
        Some("INTERRUPTED_DURING_STEP")
    );
    assert!(repository::get_run(&conn, &fixture.project_id, &qa_run.id)
        .unwrap()
        .unwrap()
        .checks
        .is_empty());
}
