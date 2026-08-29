use cinematic_desktop_lib::db;
use cinematic_desktop_lib::generation::service::{GenerationCaptureInput, GenerationService};
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::providers::model::{ProviderOutput, ProviderResult};
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, String) {
    let temp = tempdir().unwrap();
    let root = temp.path().join("generation-project");
    ProjectService::create(&root, "Generation Project").unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();
    conn.execute(
        "INSERT INTO workflow_runs
         (id, project_id, skill_id, skill_version, operation_id, status, input_json, created_at, updated_at)
         VALUES ('run-1', ?1, 'character-builder', '1.0.0',
                 'character.create_face_lock', 'completed', '{}', 'now', 'now')",
        [&project_id],
    ).unwrap();
    conn.execute(
        "INSERT INTO workflow_step_executions
         (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id,
          provider_id, model_id, adapter_version, idempotency_key, status, started_at)
         VALUES ('attempt-1', 'run-1', 'execute', 1, 'compiled-1', 'mock', 'mock-image-v1', 1,
                 'run-1:execute:1', 'succeeded', 'now')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO assets (id, project_id, type, label, created_at, updated_at)
         VALUES ('asset-1', ?1, 'face_lock', 'MARA-FACE', 'now', 'now')",
        [&project_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO asset_versions
         (id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
          original_filename, mime_type, byte_size, created_at)
         VALUES ('version-2', 'asset-1', 2, 'canonical', 'assets/asset-1/v002/face.png',
                 'thumbnails/asset-1/version-2.webp', 'd', 'face.png', 'image/png', 1, 'now')",
        [],
    )
    .unwrap();
    (temp, root, project_id)
}

#[test]
fn approved_provider_output_becomes_four_durable_candidates_without_an_asset_version() {
    let (_temp, root, project_id) = fixture();
    let result = ProviderResult {
        outputs: (1..=4)
            .map(|ordinal| ProviderOutput {
                uri: format!("mock://face-lock-{ordinal}.png"),
                mime_type: "image/png".into(),
                filename: Some(format!("face-lock-{ordinal}.png")),
            })
            .collect(),
        provider_reported_model: Some("mock-image-v1".into()),
        metadata: serde_json::json!({"fixture": true}),
    };
    let captured = GenerationService::capture_provider_result(
        &root,
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
            source_asset_version_ids: vec!["version-2".into()],
            requested_output_count: 4,
        },
        &result,
    )
    .unwrap();

    assert_eq!(captured.artifacts.len(), 4);
    assert!(captured
        .artifacts
        .iter()
        .all(|artifact| artifact.capture_status == "available"));
    assert!(captured
        .artifacts
        .iter()
        .all(|artifact| root.join(&artifact.storage_path).exists()));
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let version_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM asset_versions", [], |row| row.get(0))
        .unwrap();
    let lineage_source: String = conn
        .query_row(
            "SELECT asset_version_id FROM generated_artifact_sources WHERE artifact_id = ?1",
            [&captured.artifacts[0].id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version_count, 1);
    assert_eq!(lineage_source, "version-2");
}

#[test]
fn lineage_capture_failure_removes_materialized_candidate_files() {
    let (_temp, root, project_id) = fixture();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    conn.execute(
        "INSERT INTO workflow_step_executions
         (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id,
          provider_id, model_id, adapter_version, idempotency_key, status, started_at)
         VALUES ('attempt-2', 'run-1', 'execute', 2, 'compiled-1', 'mock', 'mock-image-v1', 1,
                 'run-1:execute:2', 'succeeded', 'now')",
        [],
    )
    .unwrap();
    let result = ProviderResult {
        outputs: vec![ProviderOutput {
            uri: "mock://lineage-failure.png".into(),
            mime_type: "image/png".into(),
            filename: Some("lineage-failure.png".into()),
        }],
        provider_reported_model: Some("mock-image-v1".into()),
        metadata: serde_json::json!({}),
    };

    let error = GenerationService::capture_provider_result(
        &root,
        &GenerationCaptureInput {
            project_id,
            workflow_run_id: "run-1".into(),
            workflow_step_key: "execute".into(),
            workflow_definition_id: "character-builder".into(),
            workflow_version: "1.0.0".into(),
            skill_id: "character-builder".into(),
            skill_version: "1.0.0".into(),
            compiled_execution_artifact_id: "compiled-1".into(),
            compiled_request_sha256: "not-a-sha256".into(),
            canon_snapshot_id: Some("canon-1".into()),
            canon_snapshot_sha256: Some("c".repeat(64)),
            provider_attempt_id: "attempt-2".into(),
            provider_id: "mock".into(),
            model_id: "mock-image-v1".into(),
            source_asset_version_ids: vec!["version-2".into()],
            requested_output_count: 1,
        },
        &result,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        cinematic_desktop_lib::error::AppError::GenerationLineageIncomplete
    ));
    assert!(!root.join("generated/run-1/attempt-2").exists());
}
