use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::workflow::runtime::WorkflowRuntime;
use rusqlite::params;
use serde_json::json;
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempdir().unwrap();
    let root = temp.path().join("golden-generation");
    ProjectService::create(&root, "Golden Generation").unwrap();
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
    conn.execute("INSERT INTO assets (id, project_id, type, label, created_at, updated_at) VALUES ('face-asset', ?1, 'face_lock', 'MARA-FACE', 'now', 'now')", [&project_id]).unwrap();
    conn.execute("INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) VALUES ('face-v002', 'face-asset', 2, 'canonical', 'assets/face-asset/v002/face.png', 'thumbnails/face-asset/face-v002.webp', 'd', 'face.png', 'image/png', 1, 'now')", []).unwrap();
    conn.execute("UPDATE assets SET canonical_version_id = 'face-v002' WHERE id = 'face-asset'", []).unwrap();
    (temp, root)
}

#[test]
fn golden_face_lock_generation_persists_candidates_and_defers_asset_promotion() {
    let (_temp, root) = fixture();
    let created = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.0.0",
        "character.create_face_lock",
        json!({
            "projectRootPath": root.to_string_lossy(),
            "characterEntityId": "mara",
            "sourceAssetVersionId": "face-v002",
            "visualSpec": {"head":"oval","eyes":"brown","brows":"straight","nose":"narrow","lips":"neutral","skin":"olive","hair":"black","build":"athletic","expression":"neutral"},
            "baselineWardrobe": "charcoal",
            "providerId": "mock",
            "modelId": "mock-image-v1"
        }),
    ).unwrap();
    WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();
    let completed = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let result_sets: i64 = conn.query_row("SELECT COUNT(*) FROM generation_result_sets", [], |row| row.get(0)).unwrap();
    let artifacts: i64 = conn.query_row("SELECT COUNT(*) FROM generated_artifacts WHERE capture_status = 'available'", [], |row| row.get(0)).unwrap();
    let asset_versions: i64 = conn.query_row("SELECT COUNT(*) FROM asset_versions", [], |row| row.get(0)).unwrap();
    let source: String = conn.query_row("SELECT asset_version_id FROM generated_artifact_sources LIMIT 1", [], |row| row.get(0)).unwrap();
    assert_eq!(result_sets, 1);
    assert_eq!(artifacts, 4);
    assert_eq!(asset_versions, 1);
    assert_eq!(source, "face-v002");
}
