//! P10.3 end-to-end evidence: an exact generated Shot video candidate (V1)
//! is evaluated against its immutable P10.2 provenance, reviewed, and
//! explicitly promoted -- all before and independent of a second candidate
//! (V2) generated from a drifted keyframe (K2) and unrelated Canon growth.
//! V1's Video QA history must read back byte-identical after every one of
//! those later mutations.

use std::path::Path;

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::CanonService;
use cinematic_desktop_lib::cinema::promotion::promote_shot_video_candidate;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::generation::model::GeneratedArtifact;
use cinematic_desktop_lib::generation::service::{GenerationCaptureInput, GenerationService};
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::providers::model::{ProviderOutput, ProviderResult};
use cinematic_desktop_lib::qa::models::{QaReviewStatus, QaRunStatus};
use cinematic_desktop_lib::qa::service::QaService;
use cinematic_desktop_lib::workflow::background;
use cinematic_desktop_lib::workflow::runtime::WorkflowRuntime;

mod support;

fn open_db(root: &Path) -> rusqlite::Connection {
    cinematic_desktop_lib::db::open_existing_connection(&root.join("project.db")).unwrap()
}

fn background_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap()
}

fn pin_keyframe(root: &Path, scene_id: &str, shot_id: &str, name: &str, pixel: [u8; 4]) -> String {
    let keyframe_asset =
        cinematic_desktop_lib::scenes::service::SceneService::ensure_scene_keyframe_asset(
            root, scene_id,
        )
        .unwrap();
    let source = support::test_image(root, name, pixel);
    let version =
        AssetService::import_asset_version(root, &keyframe_asset.id, &source, None).unwrap();
    AssetService::promote_asset_version(root, &version.id).unwrap();
    CinemaService::set_shot_keyframe(root, shot_id, Some(&version.id)).unwrap();
    version.id
}

fn start_shot_i2v_run(
    root: &Path,
    scene_id: &str,
    shot_id: &str,
    prompt: &str,
) -> cinematic_desktop_lib::workflow::model::WorkflowRunDetail {
    let created = WorkflowRuntime::create_run(
        root,
        "scene-builder",
        "1.0.0",
        "shot.image_to_video",
        serde_json::json!({
            "sceneId": scene_id,
            "shotId": shot_id,
            "providerId": "fake_async_video",
            "modelId": "fake-video-v1",
            "prompt": prompt,
        }),
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    WorkflowRuntime::approve_run_step(root, &created.run.id, "approve-request", None).unwrap();
    WorkflowRuntime::advance_run(root, &created.run.id).unwrap()
}

fn drive_background_to_completion(root: &Path) -> bool {
    for _ in 0..10 {
        let tick = background::run_pending_jobs(root).unwrap();
        if tick.completed > 0 {
            return true;
        }
    }
    false
}

fn only_artifact(root: &Path, run_id: &str) -> GeneratedArtifact {
    let results = GenerationService::list_results(root, Some(run_id)).unwrap();
    assert_eq!(results.len(), 1, "exactly one result set for the run");
    assert_eq!(results[0].artifacts.len(), 1, "exactly one artifact");
    results[0].artifacts[0].artifact.clone()
}

fn shot_row(root: &Path, shot_id: &str) -> cinematic_desktop_lib::cinema::model::ShotRecord {
    let conn = open_db(root);
    conn.query_row(
        "SELECT id, scene_id, ordering, duration_seconds, keyframe_asset_version_id, \
         generated_video_asset_version_id, intent, action, camera, created_at, updated_at \
         FROM scene_shots WHERE id = ?1",
        [shot_id],
        |row| {
            Ok(cinematic_desktop_lib::cinema::model::ShotRecord {
                id: row.get(0)?,
                scene_id: row.get(1)?,
                ordering: row.get(2)?,
                duration_seconds: row.get(3)?,
                keyframe_asset_version_id: row.get(4)?,
                generated_video_asset_version_id: row.get(5)?,
                intent: row.get(6)?,
                action: row.get(7)?,
                camera: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        },
    )
    .unwrap()
}

/// Resolves the exact completion-time-imported candidate AssetVersion for
/// an artifact -- the same content-identity resolution Video QA itself now
/// uses for an unpromoted candidate (see `qa::video_context`).
fn candidate_asset_version(
    root: &Path,
    project_id: &str,
    scene_id: &str,
    artifact: &GeneratedArtifact,
) -> String {
    let conn = open_db(root);
    conn.query_row(
        "SELECT av.id FROM asset_versions av
         JOIN assets a ON a.id = av.asset_id
         WHERE a.project_id = ?1 AND a.type = 'video' AND a.owner_entity_id = ?2 AND av.sha256 = ?3",
        rusqlite::params![project_id, scene_id, artifact.sha256],
        |row| row.get(0),
    )
    .unwrap()
}

/// Creates, approves, and runs Video QA to completion for one exact
/// candidate. Returns the workflow run id.
fn run_video_qa_to_completion(root: &Path, asset_version_id: &str) -> String {
    let created = WorkflowRuntime::create_run(
        root,
        "video-qa",
        "1.0.0",
        "asset.run_video_qa",
        serde_json::json!({
            "assetVersionId": asset_version_id,
            "adapterId": "mock",
            "modelId": "mock-video-qa",
        }),
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    WorkflowRuntime::approve_run_step(
        root,
        &created.run.id,
        "approve-video-qa",
        Some("Approved exact video evidence and execution disclosure".into()),
    )
    .unwrap();
    let completed = WorkflowRuntime::advance_run(root, &created.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");
    created.run.id
}

fn scene_video_asset_id(root: &Path, project_id: &str, scene_id: &str) -> String {
    let conn = open_db(root);
    conn.query_row(
        "SELECT id FROM assets WHERE project_id = ?1 AND type = 'video' AND owner_entity_id = ?2 \
         ORDER BY created_at ASC, id ASC LIMIT 1",
        rusqlite::params![project_id, scene_id],
        |row| row.get(0),
    )
    .unwrap()
}

/// Captures and imports a second, independently generated video candidate.
/// The only built-in video provider (`fake_async_video`) is deterministic
/// (byte-identical output regardless of input), so a real second generation
/// through it would content-dedup back onto V1. This drives the same real
/// capture/import pipeline that `shot.image_to_video` completion uses
/// (`GenerationService::capture_provider_result` then
/// `AssetService::import_media_version`), with distinct bytes standing in
/// for what a real (non-deterministic) provider would return.
fn capture_second_video_candidate(
    root: &Path,
    project_id: &str,
    scene_id: &str,
    shot_id: &str,
    source_version_id: &str,
    prompt: &str,
) -> GeneratedArtifact {
    const RUN_ID: &str = "run-v2-manual";
    const ATTEMPT_ID: &str = "attempt-v2-manual";
    let video_bytes: &[u8] = &[
        0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'm', b'p', b'4', b'2', 0x00, 0x00, 0x00,
        0x02, b'm', b'p', b'4', b'2', b'i', b's', b'o', b'm',
    ];
    let conn = open_db(root);
    conn.execute(
        "INSERT INTO workflow_runs
         (id, project_id, skill_id, skill_version, operation_id, status, input_json,
          created_at, updated_at, completed_at)
         VALUES (?1, ?2, 'scene-builder', '1.0.0', 'shot.image_to_video', 'completed', ?3,
                 '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
        rusqlite::params![
            RUN_ID,
            project_id,
            serde_json::json!({
                "sceneId": scene_id,
                "shotId": shot_id,
                "providerId": "fake_async_video",
                "modelId": "fake-video-v1",
                "prompt": prompt,
                "sourceAssetVersionId": source_version_id,
            })
            .to_string(),
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO workflow_step_executions
         (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id,
          provider_id, model_id, adapter_version, idempotency_key, status, started_at, completed_at)
         VALUES (?1, ?2, 'execute', 1, 'compiled-v2-manual', 'fake_async_video', 'fake-video-v1', 1,
                 ?3, 'succeeded', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
        rusqlite::params![ATTEMPT_ID, RUN_ID, format!("{RUN_ID}:execute:1")],
    )
    .unwrap();
    drop(conn);

    let result = ProviderResult {
        outputs: vec![ProviderOutput {
            uri: format!(
                "data:video/mp4;base64,{}",
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, video_bytes)
            ),
            mime_type: "video/mp4".into(),
            filename: Some("v2.mp4".into()),
        }],
        provider_reported_model: Some("fake-video-v1".into()),
        metadata: serde_json::json!({}),
    };
    let captured = GenerationService::capture_provider_result(
        root,
        &GenerationCaptureInput {
            project_id: project_id.into(),
            workflow_run_id: RUN_ID.into(),
            workflow_step_key: "execute".into(),
            workflow_definition_id: "shot.image_to_video".into(),
            workflow_version: "1.0.0".into(),
            skill_id: "scene-builder".into(),
            skill_version: "1.0.0".into(),
            compiled_execution_artifact_id: "compiled-v2-manual".into(),
            compiled_request_sha256: "d".repeat(64),
            canon_snapshot_id: None,
            canon_snapshot_sha256: None,
            provider_attempt_id: ATTEMPT_ID.into(),
            provider_id: "fake_async_video".into(),
            model_id: "fake-video-v1".into(),
            source_asset_version_ids: vec![source_version_id.into()],
            requested_output_count: 1,
            media_kind: "video".into(),
        },
        &result,
    )
    .unwrap();
    let artifact = captured.artifacts[0].clone();

    let video_asset_id = scene_video_asset_id(root, project_id, scene_id);
    let source_path = root.join(&artifact.storage_path);
    AssetService::import_media_version(root, &video_asset_id, &source_path, None).unwrap();
    artifact
}

#[test]
fn immutable_video_qa_golden_path_survives_review_promotion_and_later_mutation() {
    let _guard = background_test_guard();
    background::reset_provider_cache_for_tests();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];

    // --- K1: the exact frozen source keyframe. ---
    let k1 = pin_keyframe(
        root,
        &scene.id,
        &shot.id,
        "qa-golden-k1.png",
        [29, 47, 83, 255],
    );

    // --- I2V -> V1: a completion-time-imported candidate, never promoted. ---
    let run1 = start_shot_i2v_run(root, &scene.id, &shot.id, "A measured push-in from K1");
    assert_eq!(run1.run.status, "running");
    // Restart guard: a durable background job survives a process restart.
    ProjectService::open(root).unwrap();
    background::reset_provider_cache_for_tests();
    assert!(
        drive_background_to_completion(root),
        "the restarted runner must complete the durable I2V job for V1"
    );
    let artifact_v1 = only_artifact(root, &run1.run.id);
    let v1 = candidate_asset_version(root, &scene.project_id, &scene.id, &artifact_v1);

    // Completion never auto-pins the Shot: V1 is a candidate only.
    assert_eq!(
        shot_row(root, &shot.id).generated_video_asset_version_id,
        None
    );

    // --- Video QA on the unpromoted V1: create -> approve -> execute. ---
    let qa_workflow_run_id = run_video_qa_to_completion(root, &v1);
    let qa_runs_for_v1 = QaService::list_runs(root, &v1).unwrap();
    assert_eq!(qa_runs_for_v1.len(), 1, "exactly one QA run for V1");
    let qa_run_id = qa_runs_for_v1[0].id.clone();
    let qa_detail = QaService::get_run(root, &qa_run_id).unwrap();
    assert_eq!(qa_detail.run.status, QaRunStatus::Succeeded);
    assert_eq!(qa_detail.run.asset_version_id, v1, "QA target is exact V1");
    assert_eq!(
        qa_detail.run.workflow_run_id.as_deref(),
        Some(qa_workflow_run_id.as_str())
    );
    assert_eq!(
        qa_detail.run.context_snapshot["target"]["assetVersionId"], v1,
        "resolved context target is exact V1"
    );
    assert_eq!(
        qa_detail.run.context_snapshot["sourceKeyframe"]["assetVersionId"], k1,
        "resolved context source is exact K1"
    );
    assert!(!qa_detail.checks.is_empty());

    // QA does not auto-promote: the Shot's video pin is still unset.
    assert_eq!(
        shot_row(root, &shot.id).generated_video_asset_version_id,
        None
    );

    // --- Human review: confirm one finding. ---
    let first_check_id = qa_detail.checks[0].check_id.clone();
    let reviewed = QaService::review_run_check(
        root,
        &qa_run_id,
        &first_check_id,
        QaReviewStatus::Confirmed,
        Some("Reviewed against K1 and the frozen generation intent"),
    )
    .unwrap();
    assert_eq!(reviewed.checks[0].review_status, QaReviewStatus::Confirmed);

    // --- Explicit promotion: independent of, and after, QA. ---
    let promoted_v1 =
        promote_shot_video_candidate(root, &shot.id, &artifact_v1.id, None, None).unwrap();
    assert_eq!(promoted_v1.asset_version_id, v1);
    assert_eq!(
        shot_row(root, &shot.id)
            .generated_video_asset_version_id
            .as_deref(),
        Some(v1.as_str())
    );

    // Snapshot V1's QA state before any further mutation, for the final
    // immutability comparison.
    let qa_detail_before_mutation = QaService::get_run(root, &qa_run_id).unwrap();

    // --- Mutate the keyframe pin: K2 drifts the Shot away from K1. ---
    let k2 = pin_keyframe(
        root,
        &scene.id,
        &shot.id,
        "qa-golden-k2.png",
        [200, 100, 50, 255],
    );
    assert_ne!(k2, k1);

    // --- Mutate Canon: an entirely unrelated entity joins the project. ---
    CanonService::create_entity(root, CanonEntityType::Faction, "Unrelated Faction").unwrap();

    // --- Generate again from the drifted keyframe -> V2. ---
    let artifact_v2 = capture_second_video_candidate(
        root,
        &scene.project_id,
        &scene.id,
        &shot.id,
        &k2,
        "A slow pan from K2",
    );
    let v2 = candidate_asset_version(root, &scene.project_id, &scene.id, &artifact_v2);
    assert_ne!(v2, v1);

    // --- Explicitly promote V2, superseding V1's Shot pin. ---
    let promoted_v2 =
        promote_shot_video_candidate(root, &shot.id, &artifact_v2.id, Some(&v1), None).unwrap();
    assert_eq!(promoted_v2.asset_version_id, v2);
    assert_eq!(
        shot_row(root, &shot.id)
            .generated_video_asset_version_id
            .as_deref(),
        Some(v2.as_str())
    );

    // --- V1's QA history is unaffected by every mutation above. ---
    let qa_detail_after_mutation = QaService::get_run(root, &qa_run_id).unwrap();
    assert_eq!(qa_detail_after_mutation, qa_detail_before_mutation);
    assert_eq!(qa_detail_after_mutation.run.asset_version_id, v1);
    assert_eq!(
        qa_detail_after_mutation.run.context_snapshot["target"]["assetVersionId"],
        v1
    );
    assert_eq!(
        qa_detail_after_mutation.run.context_snapshot["sourceKeyframe"]["assetVersionId"], k1,
        "V1's QA history still reads K1, never the drifted K2"
    );
    assert_eq!(
        qa_detail_after_mutation.checks[0].review_status,
        QaReviewStatus::Confirmed
    );

    // Candidate-local ownership: V2 has no QA history of its own yet, and
    // V1's history never leaked onto it.
    let qa_runs_for_v2 = QaService::list_runs(root, &v2).unwrap();
    assert!(qa_runs_for_v2.is_empty());
}
