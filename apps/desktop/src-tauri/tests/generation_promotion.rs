use cinematic_desktop_lib::db;
use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::generation::service::{
    GenerationCaptureInput, GenerationService,
};
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::providers::model::{ProviderOutput, ProviderResult};
use serde_json::json;
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempdir().unwrap();
    let root = temp.path().join("promotion-project");
    ProjectService::create(&root, "Promotion Project").unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let project_id: String = conn.query_row("SELECT id FROM projects", [], |row| row.get(0)).unwrap();
    conn.execute("INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, status, input_json, created_at, updated_at) VALUES ('run-1', ?1, 'character-builder', '1.0.0', 'character.create_face_lock', 'completed', '{}', 'now', 'now')", [&project_id]).unwrap();
    conn.execute("INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id, provider_id, model_id, adapter_version, idempotency_key, status, started_at) VALUES ('attempt-1', 'run-1', 'execute', 1, 'compiled-1', 'mock', 'mock-image-v1', 1, 'run-1:execute:1', 'succeeded', 'now')", []).unwrap();
    conn.execute("INSERT INTO assets (id, project_id, type, label, created_at, updated_at) VALUES ('asset-1', ?1, 'face_lock', 'MARA-FACE', 'now', 'now')", [&project_id]).unwrap();
    conn.execute("INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) VALUES ('face-v002', 'asset-1', 2, 'canonical', 'assets/face.png', 'thumbnails/face.webp', 'd', 'face.png', 'image/png', 1, 'now')", []).unwrap();
    conn.execute("UPDATE assets SET canonical_version_id = 'face-v002' WHERE id = 'asset-1'", []).unwrap();
    (temp, root)
}

fn capture(root: &std::path::Path) -> Vec<cinematic_desktop_lib::generation::model::GeneratedArtifact> {
    let project_id: String = db::open_existing_connection(&root.join("project.db")).unwrap()
        .query_row("SELECT id FROM projects", [], |row| row.get(0)).unwrap();
    GenerationService::capture_provider_result(
        root,
        &GenerationCaptureInput {
            project_id,
            workflow_run_id: "run-1".into(),
            workflow_step_key: "execute".into(),
            workflow_definition_id: "character-builder".into(),
            workflow_version: "1.0.0".into(),
            skill_id: "character-builder".into(),
            skill_version: "1.0.0".into(),
            compiled_execution_artifact_id: "compiled-1".into(),
            compiled_request_sha256: "b".repeat(64),
            canon_snapshot_id: Some("canon-1".into()),
            canon_snapshot_sha256: Some("c".repeat(64)),
            provider_attempt_id: "attempt-1".into(),
            provider_id: "mock".into(),
            model_id: "mock-image-v1".into(),
            source_asset_version_ids: vec!["face-v002".into()],
            requested_output_count: 1,
        },
        &ProviderResult {
            outputs: vec![ProviderOutput { uri: "mock://promotion.png".into(), mime_type: "image/png".into(), filename: Some("promotion.png".into()) }],
            provider_reported_model: Some("mock-image-v1".into()),
            metadata: json!({}),
        },
    ).unwrap().artifacts
}

#[test]
fn promotion_creates_a_new_version_and_retries_idempotently() {
    let (_temp, root) = fixture();
    let artifacts = capture(&root);
    let first = GenerationService::promote_generated_artifact(&root, &artifacts[0].id, "asset-1", false).unwrap();
    let second = GenerationService::promote_generated_artifact(&root, &artifacts[0].id, "asset-1", false).unwrap();

    assert_eq!(first.id, second.id);
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let version_count: i64 = conn.query_row("SELECT COUNT(*) FROM asset_versions", [], |row| row.get(0)).unwrap();
    let promotion_count: i64 = conn.query_row("SELECT COUNT(*) FROM artifact_promotions", [], |row| row.get(0)).unwrap();
    let canonical: Option<String> = conn.query_row("SELECT canonical_version_id FROM assets WHERE id = 'asset-1'", [], |row| row.get(0)).unwrap();
    assert_eq!(version_count, 2);
    assert_eq!(promotion_count, 1);
    assert_eq!(canonical.as_deref(), Some("face-v002"));
    let asset = AssetService::get_asset_with_versions(&root, "asset-1").unwrap();
    let generated = asset.versions.iter().find(|version| version.id == first.id).unwrap();
    assert_eq!(generated.origin, "generated");
    assert_eq!(generated.generation_artifact_id.as_deref(), Some(artifacts[0].id.as_str()));
    let audit_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM provider_audit_events WHERE event_type = 'generation.artifact.promoted'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(audit_count, 1);
}

#[test]
fn promotion_can_explicitly_make_the_new_version_canonical() {
    let (_temp, root) = fixture();
    let artifacts = capture(&root);
    let promoted = GenerationService::promote_generated_artifact(&root, &artifacts[0].id, "asset-1", true).unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let canonical: String = conn.query_row("SELECT canonical_version_id FROM assets WHERE id = 'asset-1'", [], |row| row.get(0)).unwrap();
    assert_eq!(canonical, promoted.id);
}
