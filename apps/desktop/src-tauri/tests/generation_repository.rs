use cinematic_desktop_lib::db::migrations::run_migrations;
use cinematic_desktop_lib::generation::model::{
    ArtifactLineage, GeneratedArtifact, GeneratedArtifactSource, GenerationResultSet,
};
use cinematic_desktop_lib::generation::repository;
use rusqlite::Connection;

fn fixture() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&mut conn).unwrap();
    conn.execute(
        "INSERT INTO projects (id, name, created_at, updated_at, schema_version)
         VALUES ('project-1', 'Project', 'now', 'now', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO workflow_runs
         (id, project_id, skill_id, skill_version, operation_id, status, input_json, created_at, updated_at)
         VALUES ('run-1', 'project-1', 'character-builder', '1.0.0',
                 'character.create_face_lock', 'completed', '{}', 'now', 'now')",
        [],
    )
    .unwrap();
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
         VALUES ('asset-1', 'project-1', 'face_lock', 'MARA-FACE', 'now', 'now')",
        [],
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
    conn
}

fn result_set() -> GenerationResultSet {
    GenerationResultSet {
        id: "result-1".into(),
        project_id: "project-1".into(),
        workflow_run_id: "run-1".into(),
        workflow_step_key: "execute".into(),
        provider_attempt_id: "attempt-1".into(),
        media_kind: "image".into(),
        requested_output_count: 4,
        created_at: "now".into(),
    }
}

#[test]
fn repository_round_trips_artifact_and_its_immutable_lineage() {
    let conn = fixture();
    repository::insert_result_set(&conn, &result_set()).unwrap();
    let artifact = GeneratedArtifact {
        id: "artifact-1".into(),
        result_set_id: "result-1".into(),
        ordinal: 2,
        media_kind: "image".into(),
        mime_type: "image/png".into(),
        width: Some(2),
        height: Some(3),
        byte_size: 42,
        sha256: "a".repeat(64),
        storage_path: "generated/run-1/attempt-1/0002.png".into(),
        capture_status: "available".into(),
        capture_error_code: None,
        created_at: "now".into(),
    };
    repository::insert_artifact(&conn, &artifact).unwrap();
    repository::insert_sources(
        &conn,
        &[GeneratedArtifactSource {
            artifact_id: artifact.id.clone(),
            asset_version_id: "version-2".into(),
            role: "identity_reference".into(),
            ordinal: 1,
        }],
    )
    .unwrap();
    let lineage = ArtifactLineage {
        artifact_id: artifact.id.clone(),
        workflow_run_id: "run-1".into(),
        workflow_step_key: "execute".into(),
        workflow_definition_id: "character-builder".into(),
        workflow_version: "1.0.0".into(),
        skill_id: "character-builder".into(),
        skill_version: "1.0.0".into(),
        compiled_execution_artifact_id: "compiled-1".into(),
        compiled_request_sha256: "b".repeat(64),
        canon_snapshot_id: None,
        canon_snapshot_sha256: None,
        provider_attempt_id: "attempt-1".into(),
        provider_id: "mock".into(),
        model_id: "mock-image-v1".into(),
        source_asset_version_ids: vec!["version-2".into()],
        created_at: "now".into(),
    };
    repository::insert_lineage(&conn, &lineage).unwrap();

    let loaded = repository::get_artifact_for_project(&conn, "project-1", "artifact-1")
        .unwrap()
        .unwrap();
    assert_eq!(loaded.sha256, artifact.sha256);
    assert_eq!(
        repository::get_lineage(&conn, "artifact-1")
            .unwrap()
            .unwrap(),
        lineage
    );
}

#[test]
fn repository_rejects_a_second_result_set_for_one_provider_attempt() {
    let conn = fixture();
    repository::insert_result_set(&conn, &result_set()).unwrap();
    assert!(repository::insert_result_set(
        &conn,
        &GenerationResultSet {
            id: "result-2".into(),
            ..result_set()
        }
    )
    .is_err());
}
