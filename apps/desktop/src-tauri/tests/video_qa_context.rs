use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::qa::models::VideoQaContextRequest;
use cinematic_desktop_lib::qa::video_context::resolve_video_qa_context;
use rusqlite::{params, Connection};
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
        let root = temp.path().join("video-qa-context");
        let project_id = ProjectService::create(&root, "Video QA Context")
            .unwrap()
            .id;
        Self {
            _temp: temp,
            root,
            project_id,
        }
    }

    fn conn(&self) -> Connection {
        db::open_existing_connection(&self.root.join("project.db")).unwrap()
    }

    fn request(&self, asset_version_id: &str) -> VideoQaContextRequest {
        VideoQaContextRequest {
            project_id: self.project_id.clone(),
            asset_version_id: asset_version_id.to_string(),
            created_at: CREATED_AT.to_string(),
        }
    }

    fn resolve(
        &self,
        asset_version_id: &str,
    ) -> Result<
        cinematic_desktop_lib::qa::models::ResolvedVideoQaContext,
        cinematic_desktop_lib::error::AppError,
    > {
        let conn = self.conn();
        resolve_video_qa_context(&conn, &self.root, &self.request(asset_version_id))
    }
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
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

/// Matches real `shot.image_to_video` completion-time import: no
/// `artifact_promotions` row is written until an explicit "Use for Shot" /
/// promote action (`with_promotion` lets a test opt into that later state).
fn insert_generated_video_fixture(fixture: &Fixture, write_target_file: bool) {
    insert_generated_video_fixture_full(fixture, write_target_file, false);
}

fn insert_generated_video_fixture_full(
    fixture: &Fixture,
    write_target_file: bool,
    with_promotion: bool,
) {
    let k1 = b"immutable keyframe K1";
    let k2 = b"mutable keyframe K2";
    let video = b"video candidate V1";
    insert_asset_version(
        fixture,
        "keyframe-asset-k1",
        "keyframe-v1",
        "shot_keyframe",
        "image/png",
        "assets/keyframe-v1.png",
        k1,
    );
    insert_asset_version(
        fixture,
        "keyframe-asset-k2",
        "keyframe-v2",
        "shot_keyframe",
        "image/png",
        "assets/keyframe-v2.png",
        k2,
    );
    insert_asset_version(
        fixture,
        "video-asset",
        "video-v1",
        "video",
        "video/mp4",
        "assets/video-v1.mp4",
        video,
    );
    // The video asset is owned by its scene (matches
    // `find_or_create_scene_video_asset` at completion-time import), which
    // Video QA provenance resolution now relies on to disambiguate
    // deterministic mock/dry-run content across unrelated scenes.
    let conn = fixture.conn();
    conn.execute(
        "UPDATE assets SET owner_entity_id = 'scene-1' WHERE id = 'video-asset'",
        [],
    )
    .unwrap();
    if !write_target_file {
        fs::remove_file(fixture.root.join("assets/video-v1.mp4")).unwrap();
    }
    let generated_path = fixture.root.join("generated/run-1/attempt-1/0001.mp4");
    fs::create_dir_all(generated_path.parent().unwrap()).unwrap();
    fs::write(generated_path, video).unwrap();

    let compiled_request = serde_json::json!({
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
    let frozen_input = serde_json::json!({
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
    let video_hash = sha256(video);
    let conn = fixture.conn();
    conn.execute(
        "INSERT INTO world_scenes
         (id, project_id, ordinal, title, summary, created_at, updated_at)
         VALUES ('scene-1', ?1, 0, 'Scene', '', ?2, ?2)",
        params![fixture.project_id, CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO scene_shots
         (id, scene_id, ordering, duration_seconds, keyframe_asset_version_id,
          generated_video_asset_version_id, intent, action, camera, created_at, updated_at)
         VALUES ('shot-1', 'scene-1', 0, 4.0, 'keyframe-v2', 'video-v1',
                 'mutable intent', 'mutable action', 'mutable camera', ?1, ?1)",
        [CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO workflow_runs
         (id, project_id, skill_id, skill_version, operation_id, status, input_json,
          created_at, updated_at, completed_at)
         VALUES ('run-1', ?1, 'scene-builder', '1.0.0', 'shot.image_to_video',
                 'completed', ?2, ?3, ?3, ?3)",
        params![fixture.project_id, frozen_input.to_string(), CREATED_AT],
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
        params![fixture.project_id, CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO generated_artifacts
         (id, result_set_id, ordinal, media_kind, mime_type, byte_size, sha256,
          storage_path, capture_status, created_at)
         VALUES ('artifact-1', 'result-set-1', 1, 'video', 'video/mp4', ?1, ?2,
                 'generated/run-1/attempt-1/0001.mp4', 'available', ?3)",
        params![video.len() as i64, video_hash, CREATED_AT],
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

#[test]
fn rejects_non_video_targets() {
    let fixture = Fixture::new();
    insert_asset_version(
        &fixture,
        "image-asset",
        "image-v1",
        "image",
        "image/png",
        "assets/image-v1.png",
        b"image",
    );

    let error = fixture.resolve("image-v1").unwrap_err();

    assert_eq!(error.code(), "INVALID_QA_DATA");
}

#[test]
fn rejects_video_targets_whose_exact_file_is_missing() {
    let fixture = Fixture::new();
    insert_generated_video_fixture(&fixture, false);

    let error = fixture.resolve("video-v1").unwrap_err();

    assert_eq!(error.code(), "GENERATION_ARTIFACT_UNAVAILABLE");
}

#[test]
fn rejects_video_targets_without_immutable_generation_provenance() {
    let fixture = Fixture::new();
    insert_asset_version(
        &fixture,
        "video-asset",
        "video-v1",
        "video",
        "video/mp4",
        "assets/video-v1.mp4",
        b"orphan video",
    );

    let error = fixture.resolve("video-v1").unwrap_err();

    assert_eq!(error.code(), "VIDEO_QA_PROVENANCE_UNSUPPORTED");
}

#[test]
fn resolves_generation_k1_instead_of_the_shots_current_k2() {
    let fixture = Fixture::new();
    insert_generated_video_fixture(&fixture, true);

    let context = fixture.resolve("video-v1").unwrap();

    assert_eq!(context.target.asset_version_id, "video-v1");
    assert_eq!(context.target.content_sha256, sha256(b"video candidate V1"));
    assert_eq!(
        context.source_keyframe.unwrap().asset_version_id,
        "keyframe-v1"
    );
    assert_eq!(context.origin.source_asset_version_ids, vec!["keyframe-v1"]);
    assert_eq!(context.origin.operation_id, "shot.image_to_video");
    assert_eq!(
        context.generation_intent.prompt,
        "A measured push-in from K1"
    );
    assert_eq!(
        context.generation_intent.expected_duration_seconds,
        Some(4.0)
    );
}

#[test]
fn shot_mutation_does_not_change_the_resolved_history() {
    let fixture = Fixture::new();
    insert_generated_video_fixture(&fixture, true);
    let before = fixture.resolve("video-v1").unwrap();
    let conn = fixture.conn();
    conn.execute(
        "UPDATE scene_shots
         SET keyframe_asset_version_id = NULL,
             generated_video_asset_version_id = NULL,
             intent = 'rewritten intent', action = 'rewritten action', camera = 'rewritten camera'
         WHERE id = 'shot-1'",
        [],
    )
    .unwrap();

    let after = fixture.resolve("video-v1").unwrap();

    assert_eq!(after, before);
}

#[test]
fn canon_mutation_does_not_change_the_resolved_history() {
    let fixture = Fixture::new();
    insert_generated_video_fixture(&fixture, true);
    let conn = fixture.conn();
    conn.execute(
        "INSERT INTO canon_entities
         (id, project_id, type, name, slug, created_at, updated_at)
         VALUES ('character-1', ?1, 'character', 'Mara', 'mara', ?2, ?2)",
        params![fixture.project_id, CREATED_AT],
    )
    .unwrap();
    // Note: the video asset's own owner_entity_id stays 'scene-1' -- Video
    // QA provenance now relies on that Scene ownership to disambiguate
    // completion-time imports (see `scene_owns_video_asset`), so this test
    // exercises an unrelated Canon mutation (a referenced character's face)
    // rather than reassigning the video asset itself.
    insert_asset_version(
        &fixture,
        "face-asset",
        "face-v1",
        "face_lock",
        "image/png",
        "assets/face-v1.png",
        b"face one",
    );
    let conn = fixture.conn();
    conn.execute(
        "UPDATE assets
         SET owner_entity_id = 'character-1', canonical_version_id = 'face-v1'
         WHERE id = 'face-asset'",
        [],
    )
    .unwrap();
    let before = fixture.resolve("video-v1").unwrap();
    let face_two = b"face two";
    fs::write(fixture.root.join("assets/face-v2.png"), face_two).unwrap();
    let conn = fixture.conn();
    conn.execute(
        "INSERT INTO asset_versions
         (id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
          original_filename, mime_type, byte_size, created_at)
         VALUES ('face-v2', 'face-asset', 2, 'canonical', 'assets/face-v2.png', '',
                 ?1, 'face-v2.png', 'image/png', ?2, ?3)",
        params![sha256(face_two), face_two.len() as i64, CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "UPDATE assets SET canonical_version_id = 'face-v2' WHERE id = 'face-asset'",
        [],
    )
    .unwrap();

    let after = fixture.resolve("video-v1").unwrap();

    assert_eq!(after, before);
}

/// Video QA must succeed on the real completion-time state: a candidate
/// imported by `import_scene_video_candidate` but never explicitly promoted
/// ("Use for Shot" is a separate, later human action). This is the exact
/// golden-path precondition -- run QA, then decide on promotion.
#[test]
fn resolves_unpromoted_completion_time_candidate() {
    let fixture = Fixture::new();
    insert_generated_video_fixture_full(&fixture, true, false);

    let context = fixture.resolve("video-v1").unwrap();

    assert_eq!(context.target.asset_version_id, "video-v1");
    assert_eq!(
        context.source_keyframe.unwrap().asset_version_id,
        "keyframe-v1"
    );
}

/// An explicit promotion ("Use for Shot") must keep resolving the same way
/// it did before this candidate/import fallback existed.
#[test]
fn resolves_after_explicit_promotion() {
    let fixture = Fixture::new();
    insert_generated_video_fixture_full(&fixture, true, true);

    let context = fixture.resolve("video-v1").unwrap();

    assert_eq!(context.target.asset_version_id, "video-v1");
    assert_eq!(
        context.source_keyframe.unwrap().asset_version_id,
        "keyframe-v1"
    );
}

/// The deterministic mock/dry-run video provider emits byte-identical
/// output regardless of Scene/Shot (see `FakeAsyncVideoProvider`), so
/// content hash alone cannot identify an unpromoted candidate's producing
/// artifact. Two scenes with identical video bytes but distinct source
/// keyframes must each resolve to their own history.
#[test]
fn disambiguates_identical_unpromoted_content_by_owning_scene() {
    let fixture = Fixture::new();
    insert_generated_video_fixture_full(&fixture, true, false);

    let k1_other = b"immutable keyframe K1 (scene two)";
    insert_asset_version(
        &fixture,
        "keyframe-asset-k1-two",
        "keyframe-v1-two",
        "shot_keyframe",
        "image/png",
        "assets/keyframe-v1-two.png",
        k1_other,
    );
    insert_asset_version(
        &fixture,
        "video-asset-two",
        "video-v1-two",
        "video",
        "video/mp4",
        "assets/video-v1-two.mp4",
        b"video candidate V1", // identical bytes to scene-1's candidate
    );
    let generated_path_two = fixture.root.join("generated/run-2/attempt-2/0001.mp4");
    fs::create_dir_all(generated_path_two.parent().unwrap()).unwrap();
    fs::write(&generated_path_two, b"video candidate V1").unwrap();

    let compiled_request_two = serde_json::json!({
        "requestVersion": 1,
        "task": "shot_image_to_video",
        "mediaType": "video",
        "prompt": "A slow pan from K1-two",
        "references": [{
            "type": "asset_version",
            "reference": "keyframe-v1-two",
            "description": "Exact source keyframe K1 (scene two)",
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
            "workflowRunId": "run-2",
            "skillId": "scene-builder",
            "skillVersion": "1.0.0",
            "operationId": "shot.image_to_video"
        },
        "generationParameters": {
            "aspectRatio": "16:9",
            "durationSeconds": 4.0,
            "fps": 24,
            "seed": 7
        }
    });
    let frozen_input_two = serde_json::json!({
        "sceneId": "scene-2",
        "shotId": "shot-2",
        "providerId": "fake_async_video",
        "modelId": "fake-video-v1",
        "prompt": "A slow pan from K1-two",
        "sourceAssetVersionId": "keyframe-v1-two",
        "generationParameters": {
            "aspectRatio": "16:9",
            "durationSeconds": 4.0,
            "fps": 24,
            "seed": 7
        }
    });
    let compiled_hash_two = sha256(compiled_request_two.to_string().as_bytes());
    let video_hash = sha256(b"video candidate V1");
    let conn = fixture.conn();
    conn.execute(
        "INSERT INTO world_scenes
         (id, project_id, ordinal, title, summary, created_at, updated_at)
         VALUES ('scene-2', ?1, 1, 'Scene Two', '', ?2, ?2)",
        params![fixture.project_id, CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "UPDATE assets SET owner_entity_id = 'scene-2' WHERE id = 'video-asset-two'",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO scene_shots
         (id, scene_id, ordering, duration_seconds, keyframe_asset_version_id,
          generated_video_asset_version_id, intent, action, camera, created_at, updated_at)
         VALUES ('shot-2', 'scene-2', 0, 4.0, 'keyframe-v1-two', 'video-v1-two',
                 'intent two', 'action two', 'camera two', ?1, ?1)",
        [CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO workflow_runs
         (id, project_id, skill_id, skill_version, operation_id, status, input_json,
          created_at, updated_at, completed_at)
         VALUES ('run-2', ?1, 'scene-builder', '1.0.0', 'shot.image_to_video',
                 'completed', ?2, ?3, ?3, ?3)",
        params![fixture.project_id, frozen_input_two.to_string(), CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO workflow_steps
         (id, workflow_run_id, step_definition_id, step_index, step_type, status,
          output_json, started_at, completed_at)
         VALUES ('compile-step-2', 'run-2', 'compile-request', 2,
                 'compile_request', 'completed', ?1, ?2, ?2)",
        params![compiled_request_two.to_string(), CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO workflow_step_executions
         (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id,
          provider_id, model_id, adapter_version, idempotency_key, status,
          artifact_ids_json, started_at, completed_at)
         VALUES ('attempt-2', 'run-2', 'execute', 1, ?1,
                 'fake_async_video', 'fake-video-v1', 1, 'run-2:execute:1',
                 'succeeded', '[\"artifact-2\"]', ?2, ?2)",
        params![compiled_hash_two, CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO generation_result_sets
         (id, project_id, workflow_run_id, workflow_step_key, provider_attempt_id,
          media_kind, requested_output_count, created_at)
         VALUES ('result-set-2', ?1, 'run-2', 'execute', 'attempt-2', 'video', 1, ?2)",
        params![fixture.project_id, CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO generated_artifacts
         (id, result_set_id, ordinal, media_kind, mime_type, byte_size, sha256,
          storage_path, capture_status, created_at)
         VALUES ('artifact-2', 'result-set-2', 1, 'video', 'video/mp4', ?1, ?2,
                 'generated/run-2/attempt-2/0001.mp4', 'available', ?3)",
        params![b"video candidate V1".len() as i64, video_hash, CREATED_AT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO generated_artifact_sources
         (artifact_id, asset_version_id, role, ordinal)
         VALUES ('artifact-2', 'keyframe-v1-two', 'identity_reference', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO artifact_lineage
         (artifact_id, workflow_run_id, workflow_step_key, workflow_definition_id,
          workflow_version, skill_id, skill_version, compiled_execution_artifact_id,
          compiled_request_sha256, provider_attempt_id, provider_id, model_id, created_at)
         VALUES ('artifact-2', 'run-2', 'execute', 'shot.image_to_video',
                 '1.0.0', 'scene-builder', '1.0.0', ?1, ?1,
                 'attempt-2', 'fake_async_video', 'fake-video-v1', ?2)",
        params![compiled_hash_two, CREATED_AT],
    )
    .unwrap();

    let scene_one = fixture.resolve("video-v1").unwrap();
    let scene_two = fixture.resolve("video-v1-two").unwrap();

    assert_eq!(
        scene_one.source_keyframe.unwrap().asset_version_id,
        "keyframe-v1"
    );
    assert_eq!(
        scene_two.source_keyframe.unwrap().asset_version_id,
        "keyframe-v1-two"
    );
    assert_eq!(
        scene_one.generation_intent.prompt,
        "A measured push-in from K1"
    );
    assert_eq!(scene_two.generation_intent.prompt, "A slow pan from K1-two");
}
