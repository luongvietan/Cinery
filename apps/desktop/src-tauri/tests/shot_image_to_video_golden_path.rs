//! P10.2 end-to-end evidence: exact keyframe binding, immutable compilation,
//! one remote submission, restart-safe completion, exact-source lineage,
//! explicit conflict-safe promotion, unchanged (drifted) keyframe pin, and
//! replay idempotency.

use std::path::Path;

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::cinema::promotion::promote_shot_video_candidate;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::generation::service::GenerationService;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::providers::commands::retry_workflow_execution;
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
    provider_id: &str,
    model_id: &str,
) -> cinematic_desktop_lib::workflow::model::WorkflowRunDetail {
    let created = WorkflowRuntime::create_run(
        root,
        "scene-builder",
        "1.0.0",
        "shot.image_to_video",
        serde_json::json!({
            "sceneId": scene_id,
            "shotId": shot_id,
            "providerId": provider_id,
            "modelId": model_id,
            "prompt": "A measured push-in from the frozen keyframe",
        }),
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    WorkflowRuntime::approve_run_step(root, &created.run.id, "approve-request", None).unwrap();
    WorkflowRuntime::advance_run(root, &created.run.id).unwrap()
}

fn compiled_request_json(root: &Path, run_id: &str) -> String {
    let conn = open_db(root);
    conn.query_row(
        "SELECT output_json FROM workflow_steps
         WHERE workflow_run_id = ?1 AND step_type = 'compile_request'",
        [run_id],
        |row| row.get(0),
    )
    .unwrap()
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

fn only_artifact(
    root: &Path,
    run_id: &str,
) -> cinematic_desktop_lib::generation::model::GeneratedArtifact {
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

#[test]
fn shot_image_to_video_golden_path_survives_restart_and_promotes_exact_output() {
    let _guard = background_test_guard();
    background::reset_provider_cache_for_tests();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];

    // The exact frozen source at run creation.
    let source_version = pin_keyframe(
        root,
        &scene.id,
        &shot.id,
        "golden-kf.png",
        [29, 47, 83, 255],
    );

    // Submit with the fake async video adapter (durable, restartable).
    let running = start_shot_i2v_run(
        root,
        &scene.id,
        &shot.id,
        "fake_async_video",
        "fake-video-v1",
    );
    assert_eq!(running.run.status, "running");
    let submitted_attempt: i64 = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT e.attempt_number FROM provider_jobs pj
             JOIN workflow_step_executions e ON e.id = pj.execution_id
             WHERE e.workflow_run_id = ?1",
            [&running.run.id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(submitted_attempt, 1);

    // Drift the Shot's keyframe AFTER submission: the frozen compiled
    // request and lineage must keep the original source.
    let replacement = pin_keyframe(
        root,
        &scene.id,
        &shot.id,
        "golden-kf-2.png",
        [200, 100, 50, 255],
    );
    assert_ne!(replacement, source_version);

    // Simulate a restart and drive the background runner to completion.
    ProjectService::open(root).unwrap();
    background::reset_provider_cache_for_tests();
    assert!(
        drive_background_to_completion(root),
        "the restarted runner must complete the durable I2V job"
    );
    let completed = WorkflowRuntime::get_run(root, &running.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");

    // One artifact, exact frozen-source lineage.
    let artifact = only_artifact(root, &running.run.id);
    let detail = GenerationService::get_artifact_detail(root, &artifact.id).unwrap();
    let lineage = detail.lineage.expect("lineage present");
    assert_eq!(
        lineage.source_asset_version_ids,
        vec![source_version.clone()]
    );

    // Completion imported a candidate but never auto-pinned the Shot.
    let before_promotion = shot_row(root, &shot.id);
    assert_eq!(before_promotion.generated_video_asset_version_id, None);
    assert_eq!(
        before_promotion.keyframe_asset_version_id.as_deref(),
        Some(replacement.as_str()),
        "the drifted keyframe pin is untouched by generation"
    );

    // The compiled request still references the frozen source version.
    let compiled: serde_json::Value =
        serde_json::from_str(&compiled_request_json(root, &running.run.id)).unwrap();
    assert_eq!(compiled["references"][0]["reference"], source_version);

    // Explicit promotion: drift is allowed (the artifact is pinned under
    // human review) — this mirrors the product decision that the user may
    // accept an I2V result generated from a prior keyframe.
    let promoted = promote_shot_video_candidate(root, &shot.id, &artifact.id, None, None).unwrap();
    let final_shot = shot_row(root, &shot.id);
    assert_eq!(
        final_shot.generated_video_asset_version_id.as_deref(),
        Some(promoted.asset_version_id.as_str())
    );
    assert_eq!(
        final_shot.keyframe_asset_version_id.as_deref(),
        Some(replacement.as_str()),
        "promotion never modifies the keyframe pin"
    );

    // Replay with the now-current pin is idempotent: same version, no new
    // audit row.
    let replay = promote_shot_video_candidate(
        root,
        &shot.id,
        &artifact.id,
        Some(&promoted.asset_version_id),
        None,
    )
    .unwrap();
    assert_eq!(replay.asset_version_id, promoted.asset_version_id);
    let audit_count: i64 = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT COUNT(*) FROM provider_audit_events WHERE event_type = 'shot.video.promoted'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        audit_count, 1,
        "replay must not append a second audit event"
    );
}

#[test]
fn conflicting_shot_promotions_yield_one_winner_and_one_conflict() {
    let _guard = background_test_guard();
    background::reset_provider_cache_for_tests();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];
    let source_version = pin_keyframe(root, &scene.id, &shot.id, "race-kf.png", [29, 47, 83, 255]);

    // First run -> first artifact.
    let first = start_shot_i2v_run(
        root,
        &scene.id,
        &shot.id,
        "fake_async_video",
        "fake-video-v1",
    );
    assert!(drive_background_to_completion(root), "first run completes");
    let first_artifact = only_artifact(root, &first.run.id);

    // Second run (a new generation of the same keyframe) -> second artifact.
    let second = start_shot_i2v_run(
        root,
        &scene.id,
        &shot.id,
        "fake_async_video",
        "fake-video-v1",
    );
    assert!(drive_background_to_completion(root), "second run completes");
    let second_artifact = only_artifact(root, &second.run.id);
    assert_ne!(first_artifact.id, second_artifact.id);

    // Two conflicting promotions (both null-expected): exactly one winner.
    let outcome_a = promote_shot_video_candidate(root, &shot.id, &first_artifact.id, None, None);
    let outcome_b = promote_shot_video_candidate(root, &shot.id, &second_artifact.id, None, None);
    let winner_version = match (&outcome_a, &outcome_b) {
        (Ok(a), Err(_)) => a.asset_version_id.clone(),
        (Err(_), Ok(b)) => b.asset_version_id.clone(),
        _ => panic!("exactly one promotion must win: {outcome_a:?} vs {outcome_b:?}"),
    };
    assert!(
        matches!(
            outcome_a,
            Err(cinematic_desktop_lib::error::AppError::PromotionConflict)
        ) || matches!(
            outcome_b,
            Err(cinematic_desktop_lib::error::AppError::PromotionConflict)
        )
    );
    let pinned = shot_row(root, &shot.id);
    assert_eq!(
        pinned.generated_video_asset_version_id.as_deref(),
        Some(winner_version.as_str())
    );
    assert_eq!(
        pinned.keyframe_asset_version_id.as_deref(),
        Some(source_version.as_str())
    );
}

#[test]
fn completion_is_idempotent_when_run_twice() {
    let _guard = background_test_guard();
    background::reset_provider_cache_for_tests();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];
    pin_keyframe(
        root,
        &scene.id,
        &shot.id,
        "replay-kf.png",
        [29, 47, 83, 255],
    );
    let running = start_shot_i2v_run(
        root,
        &scene.id,
        &shot.id,
        "fake_async_video",
        "fake-video-v1",
    );

    assert!(drive_background_to_completion(root), "first completion");
    let completed = WorkflowRuntime::get_run(root, &running.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");
    let artifact_before = only_artifact(root, &running.run.id);

    // A second completion pass (e.g. a duplicated job delivery) must not
    // capture a second result set, artifact, or candidate version.
    background::reset_provider_cache_for_tests();
    let tick = background::run_pending_jobs(root).unwrap();
    assert_eq!(
        tick.completed, 0,
        "no pending job remains after terminal completion"
    );

    let results = GenerationService::list_results(root, Some(&running.run.id)).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].artifacts.len(), 1);
    assert_eq!(results[0].artifacts[0].artifact.id, artifact_before.id);
    let candidates: i64 = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT COUNT(*) FROM asset_versions av
             JOIN assets a ON a.id = av.asset_id
             WHERE a.type = 'video' AND a.owner_entity_id = ?1",
            [&scene.id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        candidates, 1,
        "content-dedup keeps exactly one candidate version"
    );
}

#[test]
fn changed_prompt_allows_a_new_generation_run() {
    let _guard = background_test_guard();
    background::reset_provider_cache_for_tests();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];
    pin_keyframe(
        root,
        &scene.id,
        &shot.id,
        "prompt-kf.png",
        [29, 47, 83, 255],
    );

    let first = WorkflowRuntime::create_run(
        root,
        "scene-builder",
        "1.0.0",
        "shot.image_to_video",
        serde_json::json!({
            "sceneId": scene.id,
            "shotId": shot.id,
            "providerId": "fake_async_video",
            "modelId": "fake-video-v1",
            "prompt": "First motion",
        }),
    )
    .unwrap();
    let second = WorkflowRuntime::create_run(
        root,
        "scene-builder",
        "1.0.0",
        "shot.image_to_video",
        serde_json::json!({
            "sceneId": scene.id,
            "shotId": shot.id,
            "providerId": "fake_async_video",
            "modelId": "fake-video-v1",
            "prompt": "Different motion",
        }),
    )
    .unwrap();
    assert_ne!(
        first.run.id, second.run.id,
        "a changed prompt is a new generation"
    );
}

#[test]
fn retry_after_failure_preserves_the_exact_frozen_source() {
    let _guard = background_test_guard();
    background::reset_provider_cache_for_tests();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];
    let source_version = pin_keyframe(root, &scene.id, &shot.id, "retry-kf.png", [29, 47, 83, 255]);
    let running = start_shot_i2v_run(
        root,
        &scene.id,
        &shot.id,
        "fake_async_video",
        "fake-video-v1",
    );

    // Fail the provider job, execution, step, and run before retrying.
    {
        let conn = open_db(root);
        conn.execute(
            "UPDATE provider_jobs SET status = 'failed' WHERE execution_id IN
             (SELECT id FROM workflow_step_executions WHERE workflow_run_id = ?1)",
            [&running.run.id],
        )
        .unwrap();
        conn.execute(
            "UPDATE workflow_step_executions SET status = 'failed',
             normalized_error_json = '{\"message\":\"simulated provider failure\"}'
             WHERE workflow_run_id = ?1",
            [&running.run.id],
        )
        .unwrap();
        conn.execute(
            "UPDATE workflow_steps SET status = 'failed'
             WHERE workflow_run_id = ?1 AND step_type = 'execute'",
            [&running.run.id],
        )
        .unwrap();
        conn.execute(
            "UPDATE workflow_runs SET status = 'failed',
             failure_code = 'PROVIDER_EXECUTION_FAILED',
             failure_message = 'simulated provider failure'
             WHERE id = ?1",
            [&running.run.id],
        )
        .unwrap();
    }
    let replacement = pin_keyframe(
        root,
        &scene.id,
        &shot.id,
        "retry-kf-2.png",
        [90, 90, 90, 255],
    );

    let retried = retry_workflow_execution(
        root.to_string_lossy().into(),
        running.run.id.clone(),
        "execute".into(),
    )
    .unwrap();
    assert_eq!(retried.run.status, "ready_for_execution");
    let rerun = WorkflowRuntime::advance_run(root, &running.run.id).unwrap();
    assert_eq!(rerun.run.status, "running");
    assert!(
        drive_background_to_completion(root),
        "the retry completes with the frozen source"
    );
    let artifact = only_artifact(root, &running.run.id);
    let detail = GenerationService::get_artifact_detail(root, &artifact.id).unwrap();
    let lineage = detail.lineage.expect("lineage present");
    assert_eq!(
        lineage.source_asset_version_ids,
        vec![source_version],
        "retry must never reread the drifted Shot keyframe"
    );
    let final_shot = shot_row(root, &shot.id);
    assert_eq!(
        final_shot.keyframe_asset_version_id.as_deref(),
        Some(replacement.as_str())
    );
}
