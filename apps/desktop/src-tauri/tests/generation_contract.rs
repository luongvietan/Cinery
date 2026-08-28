use cinematic_desktop_lib::generation::model::{
    ArtifactLineage, ArtifactPromotion, GeneratedArtifact, GeneratedArtifactSource,
    GenerationResultSet,
};

#[test]
fn generation_models_round_trip_stable_camel_case_contracts() {
    let result_set = GenerationResultSet {
        id: "result-1".into(),
        project_id: "project-1".into(),
        workflow_run_id: "run-1".into(),
        workflow_step_key: "execute".into(),
        provider_attempt_id: "attempt-1".into(),
        media_kind: "image".into(),
        requested_output_count: 4,
        created_at: "2026-08-28T00:00:00Z".into(),
    };
    let value = serde_json::to_value(&result_set).unwrap();
    assert_eq!(value["projectId"], "project-1");
    assert_eq!(value["requestedOutputCount"], 4);
}

#[test]
fn generation_models_hold_sources_lineage_and_explicit_promotion() {
    let artifact = GeneratedArtifact {
        id: "artifact-1".into(),
        result_set_id: "result-1".into(),
        ordinal: 1,
        media_kind: "image".into(),
        mime_type: "image/png".into(),
        width: Some(1280),
        height: Some(1280),
        byte_size: 42,
        sha256: "a".repeat(64),
        storage_path: "generated/run-1/attempt-1/0001.png".into(),
        capture_status: "available".into(),
        capture_error_code: None,
        created_at: "2026-08-28T00:00:00Z".into(),
    };
    let source = GeneratedArtifactSource {
        artifact_id: artifact.id.clone(),
        asset_version_id: "version-2".into(),
        role: "identity_reference".into(),
        ordinal: 1,
    };
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
        canon_snapshot_id: Some("canon-1".into()),
        canon_snapshot_sha256: Some("c".repeat(64)),
        provider_attempt_id: "attempt-1".into(),
        provider_id: "mock".into(),
        model_id: "mock-image-v1".into(),
        source_asset_version_ids: vec![source.asset_version_id.clone()],
        created_at: artifact.created_at.clone(),
    };
    let promotion = ArtifactPromotion {
        id: "promotion-1".into(),
        artifact_id: artifact.id,
        asset_id: "asset-1".into(),
        asset_version_id: "version-3".into(),
        set_canonical: false,
        created_at: artifact.created_at,
    };

    assert_eq!(source.asset_version_id, lineage.source_asset_version_ids[0]);
    assert!(!promotion.set_canonical);
}
