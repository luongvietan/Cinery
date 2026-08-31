//! P10.1 durable background video jobs: the scene.generate_video flow
//! returns control to the UI as soon as the durable ProviderJob exists, the
//! local background runner owns polling/capture/completion, restart
//! resumes the durable job without duplicate submission, cancellation is
//! durable and terminal-state safe, retry creates a fresh attempt/job, and
//! repeated completion handling never duplicates artifacts.
//!
//! No live network: the fake_async_video provider completes on its second
//! poll with a deterministic MP4 payload.

mod support;

use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::generation::service::GenerationService;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::providers::commands::{
    cancel_workflow_execution, retry_workflow_execution,
};
use cinematic_desktop_lib::workflow::background;
use cinematic_desktop_lib::workflow::runtime::WorkflowRuntime;
use rusqlite::OptionalExtension;
use std::path::Path;

fn open_db(root: &Path) -> rusqlite::Connection {
    cinematic_desktop_lib::db::open_existing_connection(&root.join("project.db")).unwrap()
}

fn background_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(())).lock().unwrap()
}

fn start_video_run(root: &Path, scene_id: &str) -> cinematic_desktop_lib::workflow::model::WorkflowRunDetail {
    let created = WorkflowRuntime::create_run(
        root,
        "scene-builder",
        "1.0.0",
        "scene.generate_video",
        serde_json::json!({
            "sceneId": scene_id,
            "providerId": "fake_async_video",
            "modelId": "fake-video-v1",
        }),
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    WorkflowRuntime::approve_run_step(root, &created.run.id, "approve-request", None).unwrap();
    // The execution advance: must return BEFORE the provider completes.
    WorkflowRuntime::advance_run(root, &created.run.id).unwrap()
}

fn pin_shot_keyframe(root: &Path, scene_id: &str, shot_id: &str) -> (String, String) {
    let keyframe_asset =
        cinematic_desktop_lib::scenes::service::SceneService::ensure_scene_keyframe_asset(
            root, scene_id,
        )
        .unwrap();
    let source = support::test_image(root, "shot-i2v-source.png", [29, 47, 83, 255]);
    let version = cinematic_desktop_lib::assets::service::AssetService::import_asset_version(
        root, &keyframe_asset.id, &source, None,
    )
    .unwrap();
    cinematic_desktop_lib::assets::service::AssetService::promote_asset_version(
        root,
        &version.id,
    )
    .unwrap();
    CinemaService::set_shot_keyframe(root, shot_id, Some(&version.id)).unwrap();
    (version.id, version.sha256)
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

fn force_provider_failure(root: &Path, run_id: &str) {
    let conn = open_db(root);
    conn.execute(
        "UPDATE provider_jobs SET status = 'failed' WHERE execution_id IN
         (SELECT id FROM workflow_step_executions WHERE workflow_run_id = ?1)",
        [run_id],
    )
    .unwrap();
    conn.execute(
        "UPDATE workflow_step_executions SET status = 'failed',
         normalized_error_json = '{\"message\":\"simulated provider failure\"}'
         WHERE workflow_run_id = ?1",
        [run_id],
    )
    .unwrap();
    conn.execute(
        "UPDATE workflow_steps SET status = 'failed'
         WHERE workflow_run_id = ?1 AND step_type = 'execute'",
        [run_id],
    )
    .unwrap();
    conn.execute(
        "UPDATE workflow_runs SET status = 'failed',
         failure_code = 'PROVIDER_EXECUTION_FAILED',
         failure_message = 'simulated provider failure'
         WHERE id = ?1",
        [run_id],
    )
    .unwrap();
}

fn install_i2v_provider(root: &Path, base_url: String) {
    let mut operations = std::collections::BTreeMap::new();
    operations.insert(
        "video.imageToVideo".to_string(),
        cinematic_desktop_lib::providers::config::EndpointConfig {
            path_template: "/submit".into(),
            request_mapping: Some(serde_json::json!({
                "prompt": "{{prompt}}",
                "image": "{{image}}",
            })),
            response: cinematic_desktop_lib::providers::config::ResponseMapping::default(),
            job: Some(cinematic_desktop_lib::providers::config::AsyncJobConfig {
                job_id_path: "result.task_id".into(),
                status: cinematic_desktop_lib::providers::config::StatusEndpointConfig {
                    method: "GET".into(),
                    path_template: "/tasks/{jobId}".into(),
                    status_path: "result.status".into(),
                    completed_values: vec!["completed".into()],
                    failed_values: vec!["failed".into()],
                    progress_path: Some("result.percent".into()),
                    error_message_path: None,
                },
                output: cinematic_desktop_lib::providers::config::FinalOutputConfig {
                    fetch_path_template: None,
                    fetch_method: "GET".into(),
                    response: cinematic_desktop_lib::providers::config::ResponseMapping {
                        url_path: Some("result.video_url".into()),
                        ..Default::default()
                    },
                },
                polling: cinematic_desktop_lib::providers::config::PollingConfig {
                    interval_ms: 1,
                    timeout_ms: 30_000,
                },
            }),
            ..Default::default()
        },
    );
    let definition = cinematic_desktop_lib::providers::model::CustomProviderDefinition {
        provider_id: "loopback_i2v".into(),
        display_name: "Loopback Image to Video".into(),
        base_url,
        purpose: cinematic_desktop_lib::providers::model::CustomProviderPurpose::Video,
        preset_id: None,
        runtime: cinematic_desktop_lib::providers::config::ProviderRuntimeConfig {
            auth: cinematic_desktop_lib::providers::config::AuthConfig::default(),
            operations,
            ..Default::default()
        },
        api_key: None,
        api_key_hint: None,
        models: vec![cinematic_desktop_lib::providers::model::CustomProviderModel {
            id: "loop-i2v-v1".into(),
            name: "Loop I2V V1".into(),
            capabilities: vec!["video.imageToVideo".into()],
        }],
        headers: Vec::new(),
    };
    let conn = open_db(root);
    cinematic_desktop_lib::providers::repository::upsert_custom_provider(&conn, &definition)
        .unwrap();
}

fn durable_job(root: &Path, run_id: &str) -> (String, String) {
    let conn = open_db(root);
    conn.query_row(
        "SELECT pj.provider_job_id, pj.status FROM provider_jobs pj
         JOIN workflow_step_executions e ON e.id = pj.execution_id
         WHERE e.workflow_run_id = ?1",
        [&run_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )
    .unwrap()
}

#[test]
fn background_video_job_acceptance_full_lifecycle() {
    let _guard = background_test_guard();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];

    // Keyframe pin (the scene must be compiled first).
    let keyframe_asset =
        cinematic_desktop_lib::scenes::service::SceneService::ensure_scene_keyframe_asset(root, &scene.id).unwrap();
    let keyframe_source = support::test_image(root, "keyframe.png", [9, 8, 7, 255]);
    let keyframe_version = cinematic_desktop_lib::assets::service::AssetService::import_asset_version(
        root, &keyframe_asset.id, &keyframe_source, None,
    )
    .unwrap();
    cinematic_desktop_lib::assets::service::AssetService::promote_asset_version(root, &keyframe_version.id).unwrap();
    CinemaService::set_shot_keyframe(root, &shot.id, Some(&keyframe_version.id)).unwrap();
    CinemaService::compile_scene(
        root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    // --- Early return: the command returns while the provider is
    // incomplete, and the durable job already exists. ---
    let running = start_video_run(root, &scene.id);
    assert_eq!(
        running.run.status, "running",
        "the execution advance must return before provider completion"
    );
    let (provider_job_id, job_status) = durable_job(root, &running.run.id);
    assert!(!provider_job_id.is_empty(), "durable ProviderJob must exist before returning");
    assert_eq!(job_status, "submitted");

    // The attempt is durable and non-terminal; the step is running.
    {
        let conn = open_db(root);
        let attempt_status: String = conn
            .query_row(
                "SELECT status FROM workflow_step_executions WHERE workflow_run_id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempt_status, "running");
        let step_status: String = conn
            .query_row(
                "SELECT status FROM workflow_steps WHERE workflow_run_id = ?1 AND step_type = 'execute'",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(step_status, "running");
    }

    // A second advance (double-click) is a guarded no-op, not a failure.
    let again = WorkflowRuntime::advance_run(root, &running.run.id).unwrap();
    assert_eq!(again.run.status, "running");
    {
        let conn = open_db(root);
        let attempts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM workflow_step_executions WHERE workflow_run_id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempts, 1, "no duplicate attempt from a double advance");
    }

    // --- First runner tick: polls once, records progress, stays running. ---
    let tick = background::run_pending_jobs(root).unwrap();
    assert_eq!(tick.polled, 1);
    {
        let conn = open_db(root);
        let (progress, polled_at): (Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT progress_percent, last_polled_at FROM provider_jobs WHERE provider_job_id = ?1",
                [&provider_job_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(progress, Some(50), "progress must be durable and readable without the runner");
        assert!(polled_at.is_some());
        let run_status: String = conn
            .query_row(
                "SELECT status FROM workflow_runs WHERE id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(run_status, "running");
    }

    // --- Simulate a process restart: reopen the project; recovery must
    // preserve the durable job; a new runner resumes polling. No duplicate
    // submit, no new attempt. ---
    ProjectService::open(root).unwrap();
    {
        let conn = open_db(root);
        let attempts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM workflow_step_executions WHERE workflow_run_id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempts, 1, "restart must not create a new attempt");
        let preserved: String = conn
            .query_row(
                "SELECT status FROM workflow_runs WHERE id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preserved, "running", "durable remote job must be preserved by recovery");
    }

    // --- Second tick: the provider completes; the runner captures the
    // artifact and completes attempt, step, and run. ---
    let tick = background::run_pending_jobs(root).unwrap();
    assert_eq!(tick.completed, 1, "the durable job must complete on the resumed runner");
    let completed = WorkflowRuntime::get_run(root, &running.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");

    // The video artifact is durable, once.
    let result_sets = GenerationService::list_results(root, Some(&running.run.id)).unwrap();
    assert_eq!(result_sets.len(), 1);
    assert_eq!(result_sets[0].artifacts.len(), 1);
    let artifact = &result_sets[0].artifacts[0].artifact;
    assert_eq!(artifact.media_kind, "video");
    assert_eq!(artifact.mime_type, "video/mp4");
    assert!(root.join(&artifact.storage_path).is_file());

    {
        let conn = open_db(root);
        let attempt_status: String = conn
            .query_row(
                "SELECT status FROM workflow_step_executions WHERE workflow_run_id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempt_status, "succeeded");
        let job_status: String = conn
            .query_row(
                "SELECT status FROM provider_jobs WHERE provider_job_id = ?1",
                [&provider_job_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(job_status, "completed");
        // Exactly one video candidate version in the scene's video asset.
        let video_versions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM asset_versions av JOIN assets a ON a.id = av.asset_id
                 WHERE a.type = 'video' AND a.owner_entity_id = ?1 AND av.status = 'candidate'",
                [&scene.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(video_versions, 1);
    }

    // --- Capture idempotency: replaying a completion pass cannot duplicate
    // artifacts or result sets. (Drive the tick again; nothing is pending,
    // so this proves replay safety at the discover level.) ---
    let idle = background::run_pending_jobs(root).unwrap();
    assert_eq!(idle.completed + idle.failed + idle.cancelled, 0);
    {
        let conn = open_db(root);
        let result_sets: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM generation_result_sets WHERE workflow_run_id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(result_sets, 1);
        let artifacts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM generated_artifacts ga
                 JOIN generation_result_sets grs ON grs.id = ga.result_set_id
                 WHERE grs.workflow_run_id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(artifacts, 1);
    }

    // --- Review and promote; exact shot pin. ---
    let video_asset_id: String = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT id FROM assets WHERE type = 'video' AND owner_entity_id = ?1",
            [&scene.id],
            |row| row.get(0),
        )
        .unwrap()
    };
    let promoted = GenerationService::promote_generated_artifact(
        root,
        &artifact.id,
        &video_asset_id,
        true,
    )
    .unwrap();
    assert_eq!(promoted.status, "canonical");
    CinemaService::set_shot_video(root, &shot.id, Some(&promoted.id)).unwrap();
    {
        let shots = CinemaService::list_shots(root, &scene.id).unwrap();
        assert_eq!(
            shots[0].generated_video_asset_version_id.as_deref(),
            Some(promoted.id.as_str())
        );
    }
}

#[test]
fn background_video_job_cancellation_is_durable_and_terminal_safe() {
    let _guard = background_test_guard();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    CinemaService::compile_scene(
        root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    let running = start_video_run(root, &scene.id);
    assert_eq!(running.run.status, "running");

    // One tick to prove the job is actively polling, then cancel.
    background::run_pending_jobs(root).unwrap();

    // Cancel must return promptly (no waiting on any provider promise).
    let cancelled = cancel_workflow_execution(
        root.to_string_lossy().into(),
        running.run.id.clone(),
        "execute".into(),
    )
    .unwrap();
    assert_eq!(cancelled.run.status, "cancelled");

    // The attempt durably records the cancellation request; the runner
    // resolves it on its next tick.
    {
        let conn = open_db(root);
        let attempt_status: String = conn
            .query_row(
                "SELECT status FROM workflow_step_executions WHERE workflow_run_id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempt_status, "cancellation_requested");
    }

    // Runner resolves the cancellation: attempt cancelled, provider job
    // cancelled, run stays terminal-cancelled, no artifacts.
    let tick = background::run_pending_jobs(root).unwrap();
    assert_eq!(tick.cancelled, 1);
    {
        let conn = open_db(root);
        let (attempt_status, job_status): (String, String) = conn
            .query_row(
                "SELECT e.status, pj.status FROM workflow_step_executions e
                 JOIN provider_jobs pj ON pj.execution_id = e.id
                 WHERE e.workflow_run_id = ?1",
                [&running.run.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(attempt_status, "cancelled");
        assert_eq!(job_status, "cancelled");
        let run_status: String = conn
            .query_row(
                "SELECT status FROM workflow_runs WHERE id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(run_status, "cancelled");
        let artifacts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM generated_artifacts",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(artifacts, 0, "a cancelled job must produce no artifacts");
    }

    // Terminal states never flip: further ticks are no-ops.
    let idle = background::run_pending_jobs(root).unwrap();
    assert_eq!(idle.completed + idle.failed + idle.cancelled, 0);
}

#[test]
fn completion_vs_cancellation_race_is_terminal_state_safe() {
    let _guard = background_test_guard();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    CinemaService::compile_scene(
        root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    // --- Race A: the cancel command lands while the provider job is still
    // non-terminal (the runner has not completed it), then the runner tick
    // resolves the durable request. The request was persisted BEFORE the
    // runner's terminal transition, so cancellation must win
    // deterministically — and never flip afterwards. ---
    let run_a = start_video_run(root, &scene.id);
    let run_b = start_video_run(root, &scene.id);
    // One tick: both jobs get their first runner poll (Running/50%); the
    // cancel request for run_b is then persisted before any terminal
    // transition exists for it.
    background::run_pending_jobs(root).unwrap();
    {
        let conn = open_db(root);
        let (attempt_status, job_status): (String, String) = conn
            .query_row(
                "SELECT e.status, pj.status FROM workflow_step_executions e
                 JOIN provider_jobs pj ON pj.execution_id = e.id
                 WHERE e.workflow_run_id = ?1",
                [&run_b.run.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (attempt_status.as_str(), job_status.as_str()),
            ("running", "polling"),
            "the raced job must still be non-terminal before the cancel"
        );
    }
    let cancel_result = cancel_workflow_execution(
        root.to_string_lossy().into(),
        run_b.run.id.clone(),
        "execute".into(),
    )
    .unwrap();
    assert_eq!(cancel_result.run.status, "cancelled");
    // Whichever writer is first, the state is terminal and never flips.
    let tick = background::run_pending_jobs(root).unwrap();
    assert!(
        tick.cancelled >= 1,
        "the durably requested cancellation must win the race"
    );
    let terminal_status: String = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT status FROM workflow_step_executions WHERE workflow_run_id = ?1",
            [&run_b.run.id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(terminal_status, "cancelled");
    // Further ticks never change it.
    background::run_pending_jobs(root).unwrap();
    {
        let conn = open_db(root);
        let after: String = conn
            .query_row(
                "SELECT status FROM workflow_step_executions WHERE workflow_run_id = ?1",
                [&run_b.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(terminal_status, after, "terminal states must never flip");
        // Attempt, provider job, and run tell one coherent story.
        let (attempt, job, run): (String, String, String) = conn
            .query_row(
                "SELECT e.status, pj.status, wr.status
                 FROM workflow_step_executions e
                 JOIN provider_jobs pj ON pj.execution_id = e.id
                 JOIN workflow_runs wr ON wr.id = e.workflow_run_id
                 WHERE e.workflow_run_id = ?1",
                [&run_b.run.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(attempt, "cancelled");
        assert_eq!(job, "cancelled", "job status must match the attempt outcome");
        assert_eq!(run, "cancelled", "the cancel command made the run terminal");
    }

    // --- Race B (the reverse): the runner completes the job first; a late
    // cancel command must NOT flip succeeded → cancelled anywhere. ---
    let run_c = start_video_run(root, &scene.id);
    for _ in 0..4 {
        let tick = background::run_pending_jobs(root).unwrap();
        if tick.completed > 0 {
            break;
        }
    }
    let completed = WorkflowRuntime::get_run(root, &run_c.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");
    let late_cancel = cancel_workflow_execution(
        root.to_string_lossy().into(),
        run_c.run.id.clone(),
        "execute".into(),
    );
    // The run is already terminal: the command refuses with a typed error
    // rather than flipping state.
    assert!(late_cancel.is_err(), "cancelling a completed run must fail");
    {
        let conn = open_db(root);
        let (attempt, job, run): (String, String, String) = conn
            .query_row(
                "SELECT e.status, pj.status, wr.status
                 FROM workflow_step_executions e
                 JOIN provider_jobs pj ON pj.execution_id = e.id
                 JOIN workflow_runs wr ON wr.id = e.workflow_run_id
                 WHERE e.workflow_run_id = ?1",
                [&run_c.run.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(attempt, "succeeded");
        assert_eq!(job, "completed");
        assert_eq!(run, "completed");
    }
    let _ = run_a;
}

#[test]
fn background_video_retry_creates_a_fresh_attempt_after_failure() {
    let _guard = background_test_guard();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    CinemaService::compile_scene(
        root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    let running = start_video_run(root, &scene.id);
    let run_id = running.run.id.clone();

    // Force the durable job to fail through the runner's failure path by
    // making the provider job fail at the DB level: mark the provider job
    // row terminal so the runner never sees it, then fail the run the way
    // the background failure module would (simulating a provider failure).
    {
        let conn = open_db(root);
        conn.execute(
            "UPDATE provider_jobs SET status = 'failed' WHERE execution_id IN
             (SELECT id FROM workflow_step_executions WHERE workflow_run_id = ?1)",
            [&run_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE workflow_step_executions SET status = 'failed',
             normalized_error_json = '{\"message\":\"simulated provider failure\"}'
             WHERE workflow_run_id = ?1",
            [&run_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE workflow_steps SET status = 'failed' WHERE workflow_run_id = ?1 AND step_type = 'execute'",
            [&run_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE workflow_runs SET status = 'failed',
             failure_code = 'PROVIDER_EXECUTION_FAILED',
             failure_message = 'simulated provider failure'
             WHERE id = ?1",
            [&run_id],
        )
        .unwrap();
    }

    // Retry: atomic, new attempt number, new idempotency key.
    let retried = retry_workflow_execution(
        root.to_string_lossy().into(),
        run_id.clone(),
        "execute".into(),
    )
    .unwrap();
    assert_eq!(retried.run.status, "ready_for_execution");
    let (attempt2_number, attempt2_key, attempt1_status): (i64, String, String) = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT attempt_number, idempotency_key,
                    (SELECT status FROM workflow_step_executions e2
                      WHERE e2.workflow_run_id = e.workflow_run_id
                        AND e2.attempt_number = 1)
             FROM workflow_step_executions e WHERE workflow_run_id = ?1 AND attempt_number = 2",
            [&run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
    };
    assert_eq!(attempt2_number, 2);
    assert!(attempt2_key.ends_with(":2"), "retry must use a fresh idempotency key");
    assert_eq!(attempt1_status, "failed", "the old attempt is immutable");

    // Execute the retry: a new provider job, and completion through the runner.
    let rerun = WorkflowRuntime::advance_run(root, &run_id).unwrap();
    assert_eq!(rerun.run.status, "running");
    let job_count: i64 = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT COUNT(*) FROM provider_jobs pj
             JOIN workflow_step_executions e ON e.id = pj.execution_id
             WHERE e.workflow_run_id = ?1",
            [&run_id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert!(job_count >= 2, "retry must create a new ProviderJob, not reuse attempt 1's (found {job_count})");

    let mut completed_seen = false;
    for _ in 0..6 {
        let tick = background::run_pending_jobs(root).unwrap();
        if tick.completed > 0 {
            completed_seen = true;
            break;
        }
    }
    assert!(completed_seen, "the retried job must complete through the runner");
    let completed = WorkflowRuntime::get_run(root, &run_id).unwrap();
    assert_eq!(completed.run.status, "completed");
}

/// P10.1 retry atomicity: two rapid retry clicks (double-click) must
/// produce exactly ONE new attempt and never surface a raw SQLite
/// unique-constraint error — the second click's guard sees the run no
/// longer `failed` and fails cleanly with the typed ProviderExecution
/// error.
#[test]
fn retry_double_click_creates_one_attempt_and_no_raw_sqlite_error() {
    let _guard = background_test_guard();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    CinemaService::compile_scene(
        root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    let running = start_video_run(root, &scene.id);
    let run_id = running.run.id.clone();

    // Force the attempt/run terminal-failed the way a provider failure
    // would (same DB shape as the retry test above).
    {
        let conn = open_db(root);
        conn.execute(
            "UPDATE provider_jobs SET status = 'failed' WHERE execution_id IN
             (SELECT id FROM workflow_step_executions WHERE workflow_run_id = ?1)",
            [&run_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE workflow_step_executions SET status = 'failed',
             normalized_error_json = '{\"message\":\"simulated provider failure\"}'
             WHERE workflow_run_id = ?1",
            [&run_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE workflow_steps SET status = 'failed' WHERE workflow_run_id = ?1 AND step_type = 'execute'",
            [&run_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE workflow_runs SET status = 'failed',
             failure_code = 'PROVIDER_EXECUTION_FAILED',
             failure_message = 'simulated provider failure'
             WHERE id = ?1",
            [&run_id],
        )
        .unwrap();
    }

    // First click: the atomic retry transaction succeeds.
    let first = retry_workflow_execution(
        root.to_string_lossy().into(),
        run_id.clone(),
        "execute".into(),
    )
    .unwrap();
    assert_eq!(first.run.status, "ready_for_execution");

    // Second click (double-click): the run is no longer `failed`, so the
    // guard fails cleanly — typed error, no raw SQLite constraint text.
    let second = retry_workflow_execution(
        root.to_string_lossy().into(),
        run_id.clone(),
        "execute".into(),
    );
    let error_text = match second {
        Err(error) => error.message,
        Ok(_) => panic!("the second retry click must not succeed"),
    };
    assert!(
        error_text.contains("only failed executions can be retried"),
        "the second click must fail with the typed guard error, got: {error_text}"
    );
    assert!(
        !error_text.to_ascii_lowercase().contains("unique"),
        "a double-click must never surface a raw SQLite unique-constraint error"
    );

    // Exactly one new attempt exists (attempt 2), and attempt 1 is intact.
    let conn = open_db(root);
    let attempts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM workflow_step_executions WHERE workflow_run_id = ?1",
            [&run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(attempts, 2, "a double-click must create exactly one new attempt");
}

#[test]
fn two_background_jobs_progress_independently() {
    let _guard = background_test_guard();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    CinemaService::compile_scene(
        root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    let run_a = start_video_run(root, &scene.id);
    let run_b = start_video_run(root, &scene.id);
    assert_eq!(run_a.run.status, "running");
    assert_eq!(run_b.run.status, "running");

    // Both jobs are pending and poll in the same tick without corrupting
    // one another (job A completes on this tick; job B still has its own
    // poll count).
    let tick = background::run_pending_jobs(root).unwrap();
    assert!(tick.polled + tick.completed >= 2, "both jobs must be worked in one tick");

    // Complete B too.
    for _ in 0..4 {
        let tick = background::run_pending_jobs(root).unwrap();
        if tick.completed > 0 {
            break;
        }
    }
    for run_id in [&run_a.run.id, &run_b.run.id] {
        let detail = WorkflowRuntime::get_run(root, run_id).unwrap();
        assert_eq!(
            detail.run.status, "completed",
            "each job must finish independently"
        );
    }

    // Two distinct artifacts, two result sets, two video candidate versions.
    let conn = open_db(root);
    let result_sets: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM generation_result_sets",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let artifacts: i64 = conn
        .query_row("SELECT COUNT(*) FROM generated_artifacts", [], |row| row.get(0))
        .unwrap();
    let video_versions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM asset_versions av JOIN assets a ON a.id = av.asset_id
             WHERE a.type = 'video' AND a.owner_entity_id = ?1",
            [&scene.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(result_sets, 2);
    assert_eq!(artifacts, 2);
    assert_eq!(video_versions, 1, "both runs dedup into the same content version");
}

#[test]
fn canon_mutation_during_background_execution_never_alters_the_request() {
    let _guard = background_test_guard();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;

    CinemaService::compile_scene(
        root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    let running = start_video_run(root, &scene.id);

    // Capture the frozen compiled request before mutating canon.
    let frozen_request = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT output_json FROM workflow_steps
             WHERE workflow_run_id = ?1 AND step_type = 'compile_request'",
            [&running.run.id],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
    };

    // Mutate canon while the job runs in the background: unlock + rewrite
    // one of the character's locked sections.
    let character_section = {
        let conn = open_db(root);
        let section: Option<(String, String, String)> = conn
            .query_row(
                "SELECT s.id, s.section_key, s.canon_entity_id FROM canon_sections s
                 JOIN canon_entities e ON e.id = s.canon_entity_id
                 WHERE e.type = 'character' LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .unwrap();
        section
    };
    if let Some((section_id, section_key, entity_id)) = character_section {
        cinematic_desktop_lib::canon::service::CanonService::unlock_section(
            root,
            &section_id,
            None,
        )
        .unwrap();
        cinematic_desktop_lib::canon::service::CanonService::upsert_section(
            root,
            &entity_id,
            &section_key,
            serde_json::json!({"text": "MUTATED DURING GENERATION"}),
            None,
        )
        .unwrap();
        cinematic_desktop_lib::canon::service::CanonService::lock_section(
            root,
            &section_id,
            None,
        )
        .unwrap();
    }

    // Drive the job to completion.
    for _ in 0..4 {
        let tick = background::run_pending_jobs(root).unwrap();
        if tick.completed > 0 {
            break;
        }
    }
    let completed = WorkflowRuntime::get_run(root, &running.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");

    // The persisted compiled request never changed.
    let after_request = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT output_json FROM workflow_steps
             WHERE workflow_run_id = ?1 AND step_type = 'compile_request'",
            [&running.run.id],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
    };
    assert_eq!(frozen_request, after_request, "background execution must never re-resolve canon");

    // The lineage references the run's snapshot, not live canon state.
    let result_sets = GenerationService::list_results(root, Some(&running.run.id)).unwrap();
    let detail = GenerationService::get_artifact_detail(root, &result_sets[0].artifacts[0].artifact.id).unwrap();
    let lineage = detail.lineage.unwrap();
    assert_eq!(lineage.workflow_definition_id, "scene.generate_video");
    assert_eq!(lineage.provider_id, "fake_async_video");
}

/// A minimal loopback HTTP server standing in for a real async video AI
/// service: POST /submit → job id; GET /tasks/{id} → running/progress for
/// the first two polls, completed (with a data-URI MP4) afterwards. A
/// background thread serves a fixed number of requests so the runner's
/// synchronous polls inside a tick are answered concurrently.
mod loopback_provider {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    pub struct Observation {
        submitted_bodies: Arc<Mutex<Vec<String>>>,
    }

    impl Observation {
        pub fn submit_count(&self) -> usize {
            self.submitted_bodies.lock().unwrap().len()
        }

        pub fn received_source_sha256(&self) -> String {
            use base64::Engine;
            use sha2::{Digest, Sha256};

            let bodies = self.submitted_bodies.lock().unwrap();
            let body: serde_json::Value =
                serde_json::from_str(bodies.first().expect("one provider submission body"))
                    .unwrap();
            let data_uri = body["image"]
                .as_str()
                .expect("submission carries the source image data URI");
            let encoded = data_uri
                .split_once(',')
                .expect("source image is a data URI")
                .1;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap();
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            format!("{:x}", hasher.finalize())
        }
    }

    pub struct LoopbackServer {
        listener: TcpListener,
        observation: Observation,
    }

    impl LoopbackServer {
        pub fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            Self {
                listener,
                observation: Observation::default(),
            }
        }

        pub fn url(&self) -> String {
            format!("http://{}", self.listener.local_addr().unwrap())
        }

        pub fn observation(&self) -> Observation {
            self.observation.clone()
        }

        /// Serves exactly `total_requests` requests on a background
        /// thread: one POST submit + `total_requests - 1` status polls.
        /// Polls 1–2 report running (40%); later polls report completed
        /// with the data-URI MP4 (the runner's result fetch re-polls the
        /// same status path and extracts the URL).
        pub fn serve_in_background(
            self,
            total_requests: usize,
        ) -> std::thread::JoinHandle<()> {
            std::thread::spawn(move || {
                let mut polls = 0;
                for _ in 0..total_requests {
                    let (mut stream, _) = self.listener.accept().unwrap();
                    // Read the FULL request (headers + Content-Length
                    // body) before responding: closing with unread
                    // receive-buffer data sends a TCP RST on Windows and
                    // the client fails to read our response.
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 4096];
                    let mut header_end = None;
                    let mut content_length = 0usize;
                    loop {
                        let read = stream.read(&mut buffer).unwrap();
                        if read == 0 {
                            break;
                        }
                        request.extend_from_slice(&buffer[..read]);
                        if header_end.is_none() {
                            if let Some(position) =
                                find_header_end(&request)
                            {
                                header_end = Some(position);
                                content_length = parse_content_length(&request[..position]);
                            }
                        }
                        if let Some(end) = header_end {
                            if request.len() >= end + content_length {
                                break;
                            }
                        }
                    }
                    let request = String::from_utf8_lossy(&request).into_owned();
                    let request_line = request.lines().next().unwrap_or_default().to_string();
                    let mut parts = request_line.split_whitespace();
                    let method = parts.next().unwrap_or_default().to_string();
                    let path = parts.next().unwrap_or_default().to_string();
                    let body = if method == "POST" && path == "/submit" {
                        let request_body = header_end
                            .and_then(|end| request.get(end..))
                            .unwrap_or_default()
                            .to_string();
                        self.observation
                            .submitted_bodies
                            .lock()
                            .unwrap()
                            .push(request_body);
                        r#"{"result":{"task_id":"loop-job-1"}}"#.to_string()
                    } else if method == "GET" && path.starts_with("/tasks/") {
                        polls += 1;
                        if polls < 3 {
                            r#"{"result":{"status":"running","percent":40}}"#.to_string()
                        } else {
                            format!(
                                r#"{{"result":{{"status":"completed","video_url":"data:video/mp4;base64,{}"}}}}"#,
                                base64::Engine::encode(
                                    &base64::engine::general_purpose::STANDARD,
                                    [
                                        0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'm',
                                        b'p', b'4', b'2', 0x00, 0x00, 0x00, 0x00, b'm', b'p',
                                        b'4', b'2', b'i', b's', b'o', b'm',
                                    ]
                                ),
                            )
                        }
                    } else {
                        continue;
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(response.as_bytes()).unwrap();
                    stream.flush().unwrap();
                }
            })
        }
    }

    /// Index just past the blank line ending the HTTP headers.
    fn find_header_end(request: &[u8]) -> Option<usize> {
        request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|position| position + 4)
    }

    fn parse_content_length(headers: &[u8]) -> usize {
        let headers = String::from_utf8_lossy(headers).to_ascii_lowercase();
        for line in headers.lines() {
            if let Some(value) = line.strip_prefix("content-length:") {
                return value.trim().parse().unwrap_or(0);
            }
        }
        0
    }
}

/// P10.1 regression (the flagship architecture goal): a REAL declarative
/// async provider (loopback HTTP, no fake adapter) runs through the full
/// durable background pipeline — the runtime submits, persists the job
/// with its `operation`, returns early; a *rehydrated* adapter (fresh
/// instance, empty in-memory job map — what a restarted process gets)
/// resumes polling through the persisted operation and completes the
/// run, capturing the video artifact. Before the durable-operation fix
/// the rehydrated adapter's first poll returned `Unknown` and the runner
/// failed the job.
#[test]
fn declarative_async_job_resumes_through_a_rehydrated_adapter() {
    let _guard = background_test_guard();
    use loopback_provider::LoopbackServer;

    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    CinemaService::compile_scene(
        root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    // A user-defined async video service pointing at the loopback server.
    // Request budget: 1 submit + 1 submit-time probe poll + 1 runner poll
    // (running) + 1 runner poll (completed) + 1 result fetch = 5.
    let server = LoopbackServer::start();
    {
        let mut operations = std::collections::BTreeMap::new();
        operations.insert(
            "video.generate".to_string(),
            cinematic_desktop_lib::providers::config::EndpointConfig {
                path_template: "/submit".into(),
                request_mapping: Some(serde_json::json!({"prompt": "{{prompt}}"})),
                response: cinematic_desktop_lib::providers::config::ResponseMapping::default(),
                job: Some(cinematic_desktop_lib::providers::config::AsyncJobConfig {
                    job_id_path: "result.task_id".into(),
                    status: cinematic_desktop_lib::providers::config::StatusEndpointConfig {
                        method: "GET".into(),
                        path_template: "/tasks/{jobId}".into(),
                        status_path: "result.status".into(),
                        completed_values: vec!["completed".into()],
                        failed_values: vec!["failed".into()],
                        progress_path: Some("result.percent".into()),
                        error_message_path: None,
                    },
                    output: cinematic_desktop_lib::providers::config::FinalOutputConfig {
                        fetch_path_template: None,
                        fetch_method: "GET".into(),
                        response: cinematic_desktop_lib::providers::config::ResponseMapping {
                            url_path: Some("result.video_url".into()),
                            ..Default::default()
                        },
                    },
                    polling: cinematic_desktop_lib::providers::config::PollingConfig {
                        interval_ms: 1,
                        timeout_ms: 30_000,
                    },
                }),
                ..Default::default()
            },
        );
        let definition = cinematic_desktop_lib::providers::model::CustomProviderDefinition {
            provider_id: "loopback_video".into(),
            display_name: "Loopback Video".into(),
            base_url: server.url(),
            purpose: cinematic_desktop_lib::providers::model::CustomProviderPurpose::Image,
            preset_id: None,
            runtime: cinematic_desktop_lib::providers::config::ProviderRuntimeConfig {
                auth: cinematic_desktop_lib::providers::config::AuthConfig::default(),
                operations,
                ..Default::default()
            },
            api_key: None,
            api_key_hint: None,
            models: vec![cinematic_desktop_lib::providers::model::CustomProviderModel {
                id: "loop-v1".into(),
                name: "Loop V1".into(),
                capabilities: Vec::new(),
            }],
            headers: Vec::new(),
        };
        let conn = open_db(root);
        cinematic_desktop_lib::providers::repository::upsert_custom_provider(
            &conn,
            &definition,
        )
        .unwrap();
    }

    // Serve the whole conversation on a background thread.
    let server = server.serve_in_background(5);

    // The runtime submits (POST) + probes (GET: running) and returns.
    let created = WorkflowRuntime::create_run(
        root,
        "scene-builder",
        "1.0.0",
        "scene.generate_video",
        serde_json::json!({
            "sceneId": scene.id,
            "providerId": "loopback_video",
            "modelId": "loop-v1",
        }),
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    WorkflowRuntime::approve_run_step(root, &created.run.id, "approve-request", None).unwrap();
    let started = WorkflowRuntime::advance_run(root, &created.run.id).unwrap();
    assert_eq!(
        started.run.status, "running",
        "the invoke must return before the async declarative provider completes"
    );

    // The durable job row must carry the provider operation.
    let (provider_job_id, operation): (String, Option<String>) = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT pj.provider_job_id, pj.operation FROM provider_jobs pj
             JOIN workflow_step_executions e ON e.id = pj.execution_id
             WHERE e.workflow_run_id = ?1",
            [&started.run.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
    };
    assert_eq!(provider_job_id, "loop-job-1");
    assert_eq!(
        operation.as_deref(),
        Some("video.generate"),
        "the durable job must record the async operation for rehydration"
    );

    // Simulate a restart: the runner's process-wide adapter cache is
    // cleared, so the next tick polls through a *fresh* adapter instance
    // whose in-memory job→operation map is empty.
    cinematic_desktop_lib::workflow::background::reset_provider_cache_for_tests();

    // Drive the runner to completion. Poll 3 (completed) + the result
    // fetch are served by the background thread.
    let mut completed_seen = false;
    for _ in 0..10 {
        let tick = background::run_pending_jobs(root).unwrap();
        if tick.completed > 0 {
            completed_seen = true;
            break;
        }
    }
    assert!(
        completed_seen,
        "the rehydrated adapter must resume polling and complete the job"
    );
    server.join().unwrap();
    let completed = WorkflowRuntime::get_run(root, &started.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");

    // The video artifact is captured exactly once.
    let result_sets = GenerationService::list_results(root, Some(&started.run.id)).unwrap();
    assert_eq!(result_sets.len(), 1);
    assert_eq!(result_sets[0].artifacts.len(), 1);
    assert_eq!(result_sets[0].artifacts[0].artifact.media_kind, "video");
    assert_eq!(result_sets[0].artifacts[0].artifact.mime_type, "video/mp4");
    assert!(root
        .join(&result_sets[0].artifacts[0].artifact.storage_path)
        .is_file());

    // The lineage used the original provider/model provenance from the
    // durable attempt, never today's default provider config.
    let detail = GenerationService::get_artifact_detail(
        root,
        &result_sets[0].artifacts[0].artifact.id,
    )
    .unwrap();
    let lineage = detail.lineage.unwrap();
    assert_eq!(lineage.provider_id, "loopback_video");
    assert_eq!(lineage.model_id, "loop-v1");
}

#[test]
fn shot_i2v_resumes_through_rehydrated_declarative_adapter_without_resubmit() {
    let _guard = background_test_guard();
    use loopback_provider::LoopbackServer;

    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];
    let (source_version_id, source_sha256) =
        pin_shot_keyframe(root, &scene.id, &shot.id);
    let server = LoopbackServer::start();
    install_i2v_provider(root, server.url());
    let observation = server.observation();
    let server = server.serve_in_background(5);

    let running = start_shot_i2v_run(
        root,
        &scene.id,
        &shot.id,
        "loopback_i2v",
        "loop-i2v-v1",
    );
    assert_eq!(running.run.status, "running");
    let (operation, attempt_count): (Option<String>, i64) = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT pj.operation,
                    (SELECT COUNT(*) FROM workflow_step_executions
                      WHERE workflow_run_id = e.workflow_run_id)
             FROM provider_jobs pj
             JOIN workflow_step_executions e ON e.id = pj.execution_id
             WHERE e.workflow_run_id = ?1",
            [&running.run.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
    };
    assert_eq!(operation.as_deref(), Some("video.imageToVideo"));
    assert_eq!(attempt_count, 1);
    assert_eq!(observation.submit_count(), 1);
    assert_eq!(observation.received_source_sha256(), source_sha256);

    ProjectService::open(root).unwrap();
    background::reset_provider_cache_for_tests();
    let mut completed_seen = false;
    for _ in 0..10 {
        let tick = background::run_pending_jobs(root).unwrap();
        if tick.completed > 0 {
            completed_seen = true;
            break;
        }
    }
    assert!(completed_seen, "the cold adapter must complete the durable I2V job");
    server.join().unwrap();
    assert_eq!(observation.submit_count(), 1, "restart must never resubmit");

    let completed = WorkflowRuntime::get_run(root, &running.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");
    let results = GenerationService::list_results(root, Some(&running.run.id)).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].result_set.requested_output_count, 1);
    assert_eq!(results[0].artifacts.len(), 1);
    assert_eq!(results[0].artifacts[0].artifact.media_kind, "video");
    let video_candidates: i64 = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT COUNT(*) FROM asset_versions av
             JOIN assets a ON a.id = av.asset_id
             WHERE a.type = 'video' AND a.owner_entity_id = ?1
               AND av.status = 'candidate'",
            [&scene.id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(video_candidates, 1, "completion must import one video candidate");
    let compiled: serde_json::Value =
        serde_json::from_str(&compiled_request_json(root, &running.run.id)).unwrap();
    assert_eq!(compiled["references"][0]["reference"], source_version_id);
    let shots = CinemaService::list_shots(root, &scene.id).unwrap();
    assert_eq!(shots[0].keyframe_asset_version_id.as_deref(), Some(source_version_id.as_str()));
    assert_eq!(
        shots[0].generated_video_asset_version_id,
        None,
        "completion must never auto-pin the Shot video"
    );
}

#[test]
fn shot_i2v_retry_preserves_exact_source() {
    let _guard = background_test_guard();
    background::reset_provider_cache_for_tests();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];
    let (source_version_id, _) = pin_shot_keyframe(root, &scene.id, &shot.id);
    let running = start_shot_i2v_run(
        root,
        &scene.id,
        &shot.id,
        "fake_async_video",
        "fake-video-v1",
    );
    let frozen_request = compiled_request_json(root, &running.run.id);
    force_provider_failure(root, &running.run.id);
    CinemaService::set_shot_keyframe(root, &shot.id, None).unwrap();

    let retried = retry_workflow_execution(
        root.to_string_lossy().into(),
        running.run.id.clone(),
        "execute".into(),
    )
    .unwrap();
    assert_eq!(retried.run.status, "ready_for_execution");
    let attempt_2_key: String = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT idempotency_key FROM workflow_step_executions
             WHERE workflow_run_id = ?1 AND attempt_number = 2",
            [&running.run.id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert!(attempt_2_key.ends_with(":2"));
    assert_eq!(compiled_request_json(root, &running.run.id), frozen_request);

    let rerun = WorkflowRuntime::advance_run(root, &running.run.id).unwrap();
    assert_eq!(rerun.run.status, "running");
    let submitted_attempt: i64 = {
        let conn = open_db(root);
        conn.query_row(
            "SELECT e.attempt_number FROM provider_jobs pj
             JOIN workflow_step_executions e ON e.id = pj.execution_id
             WHERE e.workflow_run_id = ?1 AND pj.status != 'failed'
             ORDER BY e.attempt_number DESC LIMIT 1",
            [&running.run.id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(submitted_attempt, 2, "retry must submit the pre-created attempt 2");

    let mut completed_seen = false;
    for _ in 0..6 {
        let tick = background::run_pending_jobs(root).unwrap();
        if tick.completed > 0 {
            completed_seen = true;
            break;
        }
    }
    assert!(completed_seen);
    let compiled: serde_json::Value = serde_json::from_str(&frozen_request).unwrap();
    assert_eq!(compiled["references"][0]["reference"], source_version_id);
}

#[test]
fn shot_i2v_cancellation_is_truthful_and_terminal_safe() {
    let _guard = background_test_guard();
    background::reset_provider_cache_for_tests();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];
    pin_shot_keyframe(root, &scene.id, &shot.id);
    let running = start_shot_i2v_run(
        root,
        &scene.id,
        &shot.id,
        "fake_async_video",
        "fake-video-v1",
    );

    let cancelled = cancel_workflow_execution(
        root.to_string_lossy().into(),
        running.run.id.clone(),
        "execute".into(),
    )
    .unwrap();
    assert_eq!(cancelled.run.status, "cancelled");
    let tick = background::run_pending_jobs(root).unwrap();
    assert_eq!(tick.cancelled, 1);

    let (attempt_status, job_status, audit_payload, result_sets):
        (String, String, String, i64) = {
        let conn = open_db(root);
        let (attempt_status, job_status) = conn
            .query_row(
                "SELECT e.status, pj.status FROM workflow_step_executions e
                 JOIN provider_jobs pj ON pj.execution_id = e.id
                 WHERE e.workflow_run_id = ?1",
                [&running.run.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let audit_payload = conn
            .query_row(
                "SELECT payload_json FROM provider_audit_events
                 WHERE workflow_run_id = ?1 AND event_type = 'provider.execution.cancelled'
                 ORDER BY created_at DESC LIMIT 1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        let result_sets = conn
            .query_row(
                "SELECT COUNT(*) FROM generation_result_sets WHERE workflow_run_id = ?1",
                [&running.run.id],
                |row| row.get(0),
            )
            .unwrap();
        (attempt_status, job_status, audit_payload, result_sets)
    };
    assert_eq!(attempt_status, "cancelled");
    assert_eq!(job_status, "cancelled");
    let audit_payload: serde_json::Value = serde_json::from_str(&audit_payload).unwrap();
    assert_eq!(audit_payload["supportsCancel"], false);
    assert_eq!(audit_payload["remoteCancelled"], false);
    assert_eq!(result_sets, 0, "cancelled I2V must capture no result set");
    let shots = CinemaService::list_shots(root, &scene.id).unwrap();
    assert_eq!(shots[0].generated_video_asset_version_id, None);
    let idle = background::run_pending_jobs(root).unwrap();
    assert_eq!(idle.completed + idle.failed + idle.cancelled, 0);
}

#[test]
fn shot_i2v_unsupported_selection_has_zero_execution_side_effects() {
    let _guard = background_test_guard();
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;
    let shot = &fixture.shots[0];
    pin_shot_keyframe(root, &scene.id, &shot.id);
    let created = WorkflowRuntime::create_run(
        root,
        "scene-builder",
        "1.0.0",
        "shot.image_to_video",
        serde_json::json!({
            "sceneId": scene.id,
            "shotId": shot.id,
            "providerId": "mock",
            "modelId": "mock-image-v1",
            "prompt": "This provider must be rejected before execution starts",
        }),
    )
    .unwrap();
    WorkflowRuntime::advance_run(root, &created.run.id).unwrap();
    WorkflowRuntime::approve_run_step(root, &created.run.id, "approve-request", None).unwrap();

    let error = WorkflowRuntime::advance_run(root, &created.run.id).unwrap_err();
    assert!(matches!(
        error,
        cinematic_desktop_lib::error::AppError::ImageToVideoUnsupported
    ));
    let (attempts, jobs, run_status, step_status): (i64, i64, String, String) = {
        let conn = open_db(root);
        let attempts = conn
            .query_row(
                "SELECT COUNT(*) FROM workflow_step_executions WHERE workflow_run_id = ?1",
                [&created.run.id],
                |row| row.get(0),
            )
            .unwrap();
        let jobs = conn
            .query_row(
                "SELECT COUNT(*) FROM provider_jobs pj
                 JOIN workflow_step_executions e ON e.id = pj.execution_id
                 WHERE e.workflow_run_id = ?1",
                [&created.run.id],
                |row| row.get(0),
            )
            .unwrap();
        let run_status = conn
            .query_row(
                "SELECT status FROM workflow_runs WHERE id = ?1",
                [&created.run.id],
                |row| row.get(0),
            )
            .unwrap();
        let step_status = conn
            .query_row(
                "SELECT status FROM workflow_steps
                 WHERE workflow_run_id = ?1 AND step_type = 'execute'",
                [&created.run.id],
                |row| row.get(0),
            )
            .unwrap();
        (attempts, jobs, run_status, step_status)
    };
    assert_eq!(attempts, 0);
    assert_eq!(jobs, 0);
    assert_eq!(run_status, "failed");
    assert_eq!(step_status, "pending");
}
