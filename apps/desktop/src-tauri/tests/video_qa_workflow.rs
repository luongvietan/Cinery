use cinematic_desktop_lib::cinema::promotion::promote_shot_video_candidate;
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::qa::models::{
    QaCheckStatus, QaMediaKind, QaOverallStatus, QaReviewStatus, QaRunStatus,
};
use cinematic_desktop_lib::qa::repository;
use cinematic_desktop_lib::qa::service::QaService;
use cinematic_desktop_lib::workflow::runtime::WorkflowRuntime;
use rusqlite::{params, Connection};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

const CREATED_AT: &str = "2026-09-01T09:30:00Z";

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    project_id: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempdir().unwrap();
        let root = temp.path().join("video-qa-workflow");
        let project_id = ProjectService::create(&root, "Video QA Workflow")
            .unwrap()
            .id;
        let fixture = Self {
            _temp: temp,
            root,
            project_id,
        };
        fixture.insert_generated_video(true);
        fixture
    }

    /// Matches the real completion-time state: the candidate is imported
    /// but never explicitly promoted ("Use for Shot" is a separate, later
    /// human action). QA must be runnable here -- this is the golden-path
    /// precondition.
    fn new_unpromoted() -> Self {
        let temp = tempdir().unwrap();
        let root = temp.path().join("video-qa-workflow-unpromoted");
        let project_id = ProjectService::create(&root, "Video QA Workflow Unpromoted")
            .unwrap()
            .id;
        let fixture = Self {
            _temp: temp,
            root,
            project_id,
        };
        fixture.insert_generated_video(false);
        fixture
    }

    fn conn(&self) -> Connection {
        db::open_existing_connection(&self.root.join("project.db")).unwrap()
    }

    fn input(&self, adapter_id: &str) -> serde_json::Value {
        json!({
            "assetVersionId": "video-v1",
            "adapterId": adapter_id,
            "modelId": "mock-video-qa"
        })
    }

    fn create(&self, adapter_id: &str) -> String {
        WorkflowRuntime::create_run(
            &self.root,
            "video-qa",
            "1.0.0",
            "asset.run_video_qa",
            self.input(adapter_id),
        )
        .unwrap()
        .run
        .id
    }

    fn wait_for_approval(&self, run_id: &str) {
        let waiting = WorkflowRuntime::advance_run(&self.root, run_id).unwrap();
        assert_eq!(waiting.run.status, "waiting_for_approval");
    }

    fn approve(&self, run_id: &str) {
        WorkflowRuntime::approve_run_step(
            &self.root,
            run_id,
            "approve-video-qa",
            Some("Approved exact video evidence and execution disclosure".into()),
        )
        .unwrap();
    }

    fn qa_run(&self, workflow_run_id: &str) -> cinematic_desktop_lib::qa::models::QaRunDetail {
        let conn = self.conn();
        let qa_run_id: String = conn
            .query_row(
                "SELECT id FROM qa_runs WHERE workflow_run_id = ?1",
                [workflow_run_id],
                |row| row.get(0),
            )
            .unwrap();
        repository::get_run(&conn, &self.project_id, &qa_run_id)
            .unwrap()
            .unwrap()
    }

    fn insert_generated_video(&self, with_promotion: bool) {
        let keyframe = b"immutable keyframe K1";
        let video = b"video candidate V1";
        insert_asset_version(
            self,
            "keyframe-asset",
            "keyframe-v1",
            "shot_keyframe",
            "image/png",
            "assets/keyframe-v1.png",
            keyframe,
        );
        insert_asset_version(
            self,
            "video-asset",
            "video-v1",
            "video",
            "video/mp4",
            "assets/video-v1.mp4",
            video,
        );
        // The video asset is owned by its scene (matches
        // `find_or_create_scene_video_asset` at completion-time import),
        // which unpromoted Video QA provenance resolution relies on.
        self.conn()
            .execute(
                "UPDATE assets SET owner_entity_id = 'scene-1' WHERE id = 'video-asset'",
                [],
            )
            .unwrap();
        let generated_path = self.root.join("generated/run-1/attempt-1/0001.mp4");
        fs::create_dir_all(generated_path.parent().unwrap()).unwrap();
        fs::write(generated_path, video).unwrap();

        let compiled_request = json!({
            "requestVersion": 1,
            "task": "shot_image_to_video",
            "mediaType": "video",
            "prompt": "A measured push-in from K1",
            "references": [{
                "type": "asset_version",
                "reference": "keyframe-v1",
                "description": "Exact source keyframe K1",
                "role": "source_image"
            }],
            "constraints": [],
            "expectedOutput": {
                "assetType": "video",
                "mediaType": "video",
                "desiredStatus": "candidate",
                "ownerEntityInputRef": "sceneId"
            },
            "provenance": {
                "workflowRunId": "run-1",
                "skillId": "scene-builder",
                "skillVersion": "1.0.0",
                "operationId": "shot.image_to_video"
            },
            "generationParameters": {
                "aspectRatio": "16:9",
                "durationSeconds": 4.0,
                "fps": 24,
                "seed": 42
            }
        });
        let frozen_input = json!({
            "sceneId": "scene-1",
            "shotId": "shot-1",
            "providerId": "fake_async_video",
            "modelId": "fake-video-v1",
            "prompt": "A measured push-in from K1",
            "sourceAssetVersionId": "keyframe-v1",
            "generationParameters": {
                "aspectRatio": "16:9",
                "durationSeconds": 4.0,
                "fps": 24,
                "seed": 42
            },
            "motionRequirement": "Mara makes one continuous turn",
            "cameraRequirement": "One measured push-in with no cut"
        });
        let compiled_hash = sha256(compiled_request.to_string().as_bytes());
        let conn = self.conn();
        conn.execute(
            "INSERT INTO world_scenes
             (id, project_id, ordinal, title, summary, created_at, updated_at)
             VALUES ('scene-1', ?1, 0, 'Scene', '', ?2, ?2)",
            params![self.project_id, CREATED_AT],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO scene_shots
             (id, scene_id, ordering, duration_seconds, keyframe_asset_version_id,
              generated_video_asset_version_id, intent, action, camera, created_at, updated_at)
             VALUES ('shot-1', 'scene-1', 0, 4.0, 'keyframe-v1', 'video-v1',
                     'intent', 'action', 'camera', ?1, ?1)",
            [CREATED_AT],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workflow_runs
             (id, project_id, skill_id, skill_version, operation_id, status, input_json,
              created_at, updated_at, completed_at)
             VALUES ('run-1', ?1, 'scene-builder', '1.0.0', 'shot.image_to_video',
                     'completed', ?2, ?3, ?3, ?3)",
            params![self.project_id, frozen_input.to_string(), CREATED_AT],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workflow_steps
             (id, workflow_run_id, step_definition_id, step_index, step_type, status,
              output_json, started_at, completed_at)
             VALUES ('compile-step-1', 'run-1', 'compile-request', 2,
                     'compile_request', 'completed', ?1, ?2, ?2)",
            params![compiled_request.to_string(), CREATED_AT],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workflow_step_executions
             (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id,
              provider_id, model_id, adapter_version, idempotency_key, status,
              artifact_ids_json, started_at, completed_at)
             VALUES ('attempt-1', 'run-1', 'execute', 1, ?1,
                     'fake_async_video', 'fake-video-v1', 1, 'run-1:execute:1',
                     'succeeded', '[\"artifact-1\"]', ?2, ?2)",
            params![compiled_hash, CREATED_AT],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO generation_result_sets
             (id, project_id, workflow_run_id, workflow_step_key, provider_attempt_id,
              media_kind, requested_output_count, created_at)
             VALUES ('result-set-1', ?1, 'run-1', 'execute', 'attempt-1', 'video', 1, ?2)",
            params![self.project_id, CREATED_AT],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO generated_artifacts
             (id, result_set_id, ordinal, media_kind, mime_type, byte_size, sha256,
              storage_path, capture_status, created_at)
             VALUES ('artifact-1', 'result-set-1', 1, 'video', 'video/mp4', ?1, ?2,
                     'generated/run-1/attempt-1/0001.mp4', 'available', ?3)",
            params![video.len() as i64, sha256(video), CREATED_AT],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO generated_artifact_sources
             (artifact_id, asset_version_id, role, ordinal)
             VALUES ('artifact-1', 'keyframe-v1', 'identity_reference', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO artifact_lineage
             (artifact_id, workflow_run_id, workflow_step_key, workflow_definition_id,
              workflow_version, skill_id, skill_version, compiled_execution_artifact_id,
              compiled_request_sha256, provider_attempt_id, provider_id, model_id, created_at)
             VALUES ('artifact-1', 'run-1', 'execute', 'shot.image_to_video',
                     '1.0.0', 'scene-builder', '1.0.0', ?1, ?1,
                     'attempt-1', 'fake_async_video', 'fake-video-v1', ?2)",
            params![compiled_hash, CREATED_AT],
        )
        .unwrap();
        if with_promotion {
            conn.execute(
                "INSERT INTO artifact_promotions
                 (id, artifact_id, asset_id, asset_version_id, set_canonical, created_at)
                 VALUES ('promotion-1', 'artifact-1', 'video-asset', 'video-v1', 0, ?1)",
                [CREATED_AT],
            )
            .unwrap();
        }
    }
}

fn insert_asset_version(
    fixture: &Fixture,
    asset_id: &str,
    version_id: &str,
    asset_type: &str,
    mime_type: &str,
    relative_path: &str,
    bytes: &[u8],
) {
    let path = fixture.root.join(relative_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    let conn = fixture.conn();
    conn.execute(
        "INSERT INTO assets
         (id, project_id, type, label, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![
            asset_id,
            fixture.project_id,
            asset_type,
            asset_id,
            CREATED_AT
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO asset_versions
         (id, asset_id, version_number, status, file_path, thumbnail_path,
          sha256, original_filename, mime_type, byte_size, created_at)
         VALUES (?1, ?2, 1, 'candidate', ?3, '', ?4, ?5, ?6, ?7, ?8)",
        params![
            version_id,
            asset_id,
            relative_path,
            sha256(bytes),
            Path::new(relative_path)
                .file_name()
                .unwrap()
                .to_string_lossy(),
            mime_type,
            bytes.len() as i64,
            CREATED_AT,
        ],
    )
    .unwrap();
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn context_and_plan_are_durable_before_approval_and_rejection_invokes_nothing() {
    let fixture = Fixture::new();
    let run_id = fixture.create("mock_adapter_failure");
    fixture.wait_for_approval(&run_id);

    let queued = fixture.qa_run(&run_id);
    assert_eq!(queued.run.status, QaRunStatus::Queued);
    assert_eq!(queued.run.media_kind, QaMediaKind::Video);
    assert_eq!(queued.run.asset_version_id, "video-v1");
    assert_eq!(
        queued.run.context_snapshot["target"]["assetVersionId"],
        "video-v1"
    );
    assert_eq!(queued.run.check_plan["assetVersionId"], "video-v1");
    assert_eq!(queued.run.execution_location, "local");
    assert!(!queued.run.check_plan["checks"]
        .as_array()
        .unwrap()
        .is_empty());

    let rejected = WorkflowRuntime::reject_run_step(
        &fixture.root,
        &run_id,
        "approve-video-qa",
        Some("Do not send this evidence".into()),
    )
    .unwrap();
    assert_eq!(rejected.run.status, "rejected");
    let cancelled = fixture.qa_run(&run_id);
    assert_eq!(cancelled.run.status, QaRunStatus::Cancelled);
    assert!(cancelled.run.started_at.is_none());
    assert!(cancelled.run.raw_response_metadata.is_none());
    assert!(cancelled.checks.is_empty());
    let conn = fixture.conn();
    let invocations: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM workflow_events WHERE workflow_run_id = ?1 AND type = 'execution_started'",
            [&run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(invocations, 0);
}

#[test]
fn identical_active_request_is_deduplicated_but_terminal_request_is_an_explicit_rerun() {
    let fixture = Fixture::new();
    let first = fixture.create("mock");
    let duplicate = fixture.create("mock");
    assert_eq!(duplicate, first);

    fixture.wait_for_approval(&first);
    assert_eq!(fixture.create("mock"), first);
    fixture.approve(&first);
    let completed = WorkflowRuntime::advance_run(&fixture.root, &first).unwrap();
    assert_eq!(completed.run.status, "completed");

    let rerun = fixture.create("mock");
    assert_ne!(rerun, first);
    assert_eq!(
        repository::list_runs_for_asset_version(&fixture.conn(), &fixture.project_id, "video-v1")
            .unwrap()
            .len(),
        1,
        "a terminal rerun does not create QA history until its context is durably resolved"
    );
}

#[test]
fn external_adapter_failure_persists_only_stable_bounded_sanitized_diagnostics() {
    let fixture = Fixture::new();
    let run_id = fixture.create("mock_adapter_failure");
    fixture.wait_for_approval(&run_id);
    fixture.approve(&run_id);

    assert!(WorkflowRuntime::advance_run(&fixture.root, &run_id).is_err());
    let failed = fixture.qa_run(&run_id);
    assert_eq!(failed.run.status, QaRunStatus::Failed);
    assert_eq!(failed.run.error_code.as_deref(), Some("QA_ADAPTER_FAILED"));
    assert_eq!(
        failed.run.error_message.as_deref(),
        Some("Video QA adapter request failed")
    );
    assert!(failed.checks.is_empty());

    let metadata = failed.run.raw_response_metadata.unwrap();
    assert_eq!(metadata["adapterErrorKind"], "network");
    assert_eq!(metadata["failureCode"], "adapter_network");
    let diagnostic = metadata["diagnostic"].as_str().unwrap();
    assert!(diagnostic.len() <= 512);
    assert!(diagnostic.contains("[REDACTED]"));
    assert!(!diagnostic.contains("sk-review-secret"));
    assert!(!diagnostic.contains('<'));
    assert!(!diagnostic.contains('>'));
    assert!(!diagnostic.chars().any(char::is_control));
}

#[test]
fn invalid_response_persists_only_structural_codes_and_counts_for_untrusted_check_ids() {
    let fixture = Fixture::new();
    let run_id = fixture.create("mock_invalid_response");
    fixture.wait_for_approval(&run_id);
    fixture.approve(&run_id);

    assert!(WorkflowRuntime::advance_run(&fixture.root, &run_id).is_err());
    let failed = fixture.qa_run(&run_id);
    assert_eq!(failed.run.status, QaRunStatus::Failed);
    assert_eq!(
        failed.run.error_code.as_deref(),
        Some("INVALID_VLM_RESPONSE")
    );
    assert_eq!(
        failed.run.error_message.as_deref(),
        Some("Video QA response failed structural validation")
    );
    assert!(failed.checks.is_empty());

    let metadata = failed.run.raw_response_metadata.unwrap();
    assert_eq!(metadata["validationCode"], "check_identity_mismatch");
    assert_eq!(metadata["reportedCheckCount"], 1);
    assert!(metadata["plannedCheckCount"].as_u64().unwrap() > 1);
    let persisted = metadata.to_string();
    assert!(persisted.len() < 256);
    assert!(!persisted.contains("sk-untrusted"));
    assert!(!persisted.contains("script"));
}

#[test]
fn qa_and_workflow_completion_are_atomic() {
    let fixture = Fixture::new();
    let run_id = fixture.create("mock");
    fixture.wait_for_approval(&run_id);
    fixture.approve(&run_id);
    let conn = fixture.conn();
    conn.execute_batch(&format!(
        "CREATE TRIGGER reject_video_qa_workflow_completion
         BEFORE UPDATE OF status ON workflow_runs
         WHEN NEW.id = '{}' AND NEW.status = 'completed'
         BEGIN SELECT RAISE(ABORT, 'forced completion failure'); END;",
        run_id
    ))
    .unwrap();
    drop(conn);

    assert!(WorkflowRuntime::advance_run(&fixture.root, &run_id).is_err());

    let qa = fixture.qa_run(&run_id);
    assert_eq!(qa.run.status, QaRunStatus::Failed);
    assert!(qa.checks.is_empty());
    let workflow = WorkflowRuntime::get_run(&fixture.root, &run_id).unwrap();
    assert_eq!(workflow.run.status, "failed");
}

#[test]
fn completed_history_restores_after_reopen_and_review_preserves_raw_status() {
    let fixture = Fixture::new();
    let run_id = fixture.create("mock");
    fixture.wait_for_approval(&run_id);
    fixture.approve(&run_id);
    WorkflowRuntime::advance_run(&fixture.root, &run_id).unwrap();
    drop(fixture.conn());

    ProjectService::open(&fixture.root).unwrap();
    let restored = fixture.qa_run(&run_id);
    assert_eq!(restored.run.status, QaRunStatus::Succeeded);
    assert_eq!(restored.run.media_kind, QaMediaKind::Video);
    assert!(!restored.checks.is_empty());

    let raw_check = restored
        .checks
        .iter()
        .find(|check| check.check_id == "video:integrity")
        .unwrap()
        .clone();
    assert_eq!(raw_check.status, QaCheckStatus::Pass);
    let reviewed = QaService::review_run_check(
        &fixture.root,
        &restored.run.id,
        &raw_check.check_id,
        QaReviewStatus::OverriddenFail,
        Some("Human review found a temporal defect"),
    )
    .unwrap();
    let same_check = reviewed
        .checks
        .iter()
        .find(|check| check.check_id == raw_check.check_id)
        .unwrap();
    assert_eq!(same_check.status, QaCheckStatus::Pass);
    assert_eq!(same_check.effective_status(), QaCheckStatus::Fail);
    assert_eq!(reviewed.run.overall_status, Some(QaOverallStatus::Fail));
}

#[test]
fn reopen_fails_interrupted_non_durable_video_qa_without_external_reexecution() {
    let fixture = Fixture::new();
    let run_id = fixture.create("mock_adapter_failure");
    fixture.wait_for_approval(&run_id);
    fixture.approve(&run_id);
    let conn = fixture.conn();
    conn.execute(
        "UPDATE workflow_runs SET status = 'running' WHERE id = ?1",
        [&run_id],
    )
    .unwrap();
    conn.execute(
        "UPDATE workflow_steps SET status = 'running'
         WHERE workflow_run_id = ?1 AND step_definition_id = 'execute'",
        [&run_id],
    )
    .unwrap();
    conn.execute(
        "UPDATE qa_runs SET status = 'running', started_at = ?2 WHERE workflow_run_id = ?1",
        params![run_id, CREATED_AT],
    )
    .unwrap();
    drop(conn);

    ProjectService::open(&fixture.root).unwrap();

    let failed = fixture.qa_run(&run_id);
    assert_eq!(failed.run.status, QaRunStatus::Failed);
    assert_eq!(
        failed.run.error_code.as_deref(),
        Some("INTERRUPTED_DURING_STEP")
    );
    assert!(failed.checks.is_empty());
    assert_eq!(
        WorkflowRuntime::get_run(&fixture.root, &run_id)
            .unwrap()
            .run
            .status,
        "failed"
    );
}

/// The real golden path: a completion-time-imported candidate (no
/// `artifact_promotions` row yet -- "Use for Shot" is a separate, later
/// human action) must support the full Video QA workflow end to end, and
/// explicit promotion must still succeed afterward regardless of the QA
/// outcome. This is the exact sequence the P10.3 spec requires and that the
/// promotion-only provenance resolution used to make impossible.
#[test]
fn runs_qa_and_still_allows_explicit_promotion_for_an_unpromoted_candidate() {
    let fixture = Fixture::new_unpromoted();
    let run_id = fixture.create("mock");
    fixture.wait_for_approval(&run_id);
    fixture.approve(&run_id);

    let completed = WorkflowRuntime::advance_run(&fixture.root, &run_id).unwrap();
    assert_eq!(completed.run.status, "completed");
    let qa = fixture.qa_run(&run_id);
    assert_eq!(qa.run.status, QaRunStatus::Succeeded);
    assert_eq!(qa.run.asset_version_id, "video-v1");

    // The fixture's `scene_shots` row already carries 'video-v1' as its
    // generated-video pin (fixture shorthand for "the candidate exists"),
    // so that is the expected current pin for this compare-and-set.
    let promoted =
        promote_shot_video_candidate(&fixture.root, "shot-1", "artifact-1", Some("video-v1"))
            .unwrap();
    assert_eq!(promoted.asset_version_id, "video-v1");
}
