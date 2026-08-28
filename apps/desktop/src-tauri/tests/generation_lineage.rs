use cinematic_desktop_lib::generation::lineage::{build_lineage, LineageInput};

fn input() -> LineageInput {
    LineageInput {
        artifact_id: "artifact-1".into(),
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
        created_at: "now".into(),
    }
}

#[test]
fn lineage_builder_preserves_exact_source_and_runtime_identities() {
    let lineage = build_lineage(input()).unwrap();
    assert_eq!(lineage.source_asset_version_ids, vec!["version-2"]);
    assert_eq!(lineage.provider_attempt_id, "attempt-1");
    assert_eq!(lineage.compiled_execution_artifact_id, "compiled-1");
}

#[test]
fn lineage_builder_rejects_incomplete_character_provenance_without_echoing_secrets() {
    let mut incomplete = input();
    incomplete.source_asset_version_ids.clear();
    incomplete.provider_id = "mock SUPER_SECRET_PROVIDER_KEY_123".into();

    let error = build_lineage(incomplete).unwrap_err();
    assert_eq!(error.code(), "GENERATION_LINEAGE_INCOMPLETE");
    assert!(!error.to_string().contains("SUPER_SECRET_PROVIDER_KEY_123"));
}
