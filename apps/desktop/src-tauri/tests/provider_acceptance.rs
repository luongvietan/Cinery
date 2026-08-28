use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::workflow::runtime::WorkflowRuntime;
use rusqlite::params;
use serde_json::json;
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempdir().unwrap();
    let root = temp.path().join("provider-project");
    ProjectService::create(&root, "Provider Project").unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let project_id: String = conn.query_row("SELECT id FROM projects", [], |row| row.get(0)).unwrap();
    conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('mara', ?1, 'character', 'Mara', 'mara', 'now', 'now')", [&project_id]).unwrap();
    for (id, key, value, revision) in [
        ("role", "role_tag", json!({"text":"Protagonist"}), 1),
        ("summary", "visual_summary", json!({"text":"Angular face."}), 2),
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
        "1.0.0",
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
    let job_count: i64 = conn.query_row("SELECT COUNT(*) FROM provider_jobs", [], |row| row.get(0)).unwrap();
    let asset_version_count: i64 = conn.query_row("SELECT COUNT(*) FROM asset_versions", [], |row| row.get(0)).unwrap();
    assert_eq!(attempt_count, 1);
    assert_eq!(job_count, 1);
    assert_eq!(asset_version_count, 1);
}
