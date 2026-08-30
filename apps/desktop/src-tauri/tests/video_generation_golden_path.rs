//! P10.0 video golden path: Scene -> Shot -> keyframe pin -> compilation ->
//! scene.generate_video workflow -> approval -> fake_async_video provider
//! (submit, poll, fetch) -> durable video GeneratedArtifact -> candidate
//! AssetVersion in the scene's video asset -> explicit promotion -> exact
//! shot video pin -> restart -> everything survives and the pin never
//! drifts.
//!
//! No real network is used: the fake async video provider completes on the
//! second poll with a deterministic MP4 payload.

mod support;

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::generation::service::GenerationService;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::scenes::service::SceneService;
use cinematic_desktop_lib::workflow::runtime::WorkflowRuntime;
use rusqlite::OptionalExtension;
use std::path::Path;

fn open_db(root: &Path) -> rusqlite::Connection {
    cinematic_desktop_lib::db::open_existing_connection(&root.join("project.db")).unwrap()
}

#[test]
fn compiled_scene_generates_promotes_and_pins_an_exact_video_version() {
    let fixture = support::compilable_scene();
    let root = &fixture.root;
    let scene = &fixture.scene;

    // --- Keyframe: import one and pin it as the shot's exact reference. ---
    let shots = CinemaService::list_shots(root, &scene.id).unwrap();
    let shot = &shots[0];
    let keyframe_asset =
        SceneService::ensure_scene_keyframe_asset(root, &scene.id).unwrap();
    let keyframe_source = support::test_image(root, "keyframe.png", [200, 100, 50, 255]);
    let keyframe_version =
        AssetService::import_asset_version(root, &keyframe_asset.id, &keyframe_source, None)
            .unwrap();
    AssetService::promote_asset_version(root, &keyframe_version.id).unwrap();
    CinemaService::set_shot_keyframe(root, &shot.id, Some(&keyframe_version.id)).unwrap();

    // --- Compile the scene. ---
    let compilation = CinemaService::compile_scene(
        root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    // --- Video generation without a compilation is rejected. ---
    // (A second, fully-assembled scene -- world + cast, but never compiled --
    // isolates the compilation gate. The gate lives in the resolve-context
    // step, so the run is created but fails on the first advance.)
    let bare_scene = SceneService::create_scene(root, "Bare", "Assembled but not compiled").unwrap();
    SceneService::assign_scene_world(root, &bare_scene.id, &fixture.scene.world_id.clone().unwrap()).unwrap();
    SceneService::add_scene_character(
        root,
        &bare_scene.id,
        &fixture.character_id,
        &fixture.cast[0].look_asset_version_id,
        None,
        None,
    )
    .unwrap();
    let bare_run = WorkflowRuntime::create_run(
        root,
        "scene-builder",
        "1.0.0",
        "scene.generate_video",
        serde_json::json!({
            "sceneId": bare_scene.id,
            "providerId": "fake_async_video",
            "modelId": "fake-video-v1",
        }),
    )
    .unwrap();
    let bare_error = WorkflowRuntime::advance_run(root, &bare_run.run.id).unwrap_err();
    assert!(
        matches!(
            bare_error,
            cinematic_desktop_lib::error::AppError::SceneNotReady(ref message)
                if message.contains("compile the scene first")
        ),
        "video generation must require a persisted compilation, got: {bare_error:?}"
    );
    let bare_detail = WorkflowRuntime::get_run(root, &bare_run.run.id).unwrap();
    assert_eq!(bare_detail.run.status, "failed");

    // --- Create + advance the video workflow to the approval gate. ---
    let run = WorkflowRuntime::create_run(
        root,
        "scene-builder",
        "1.0.0",
        "scene.generate_video",
        serde_json::json!({
            "sceneId": scene.id,
            "providerId": "fake_async_video",
            "modelId": "fake-video-v1",
        }),
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(root, &run.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");

    // The compiled request is provider-neutral and references the scene.
    let compile_output = waiting
        .steps
        .iter()
        .find(|step| step.step_definition_id == "compile-request")
        .and_then(|step| step.output_json.as_deref())
        .map(serde_json::from_str::<serde_json::Value>)
        .unwrap()
        .unwrap();
    assert_eq!(compile_output["mediaType"], "video");
    assert!(compile_output["prompt"]
        .as_str()
        .unwrap()
        .contains("CINEMA PRODUCTION PROMPT"));

    // --- Approve and execute against the fake async video provider. ---
    WorkflowRuntime::approve_run_step(root, &run.run.id, "approve-request", None).unwrap();
    let completed = WorkflowRuntime::advance_run(root, &run.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");

    // The provider job + attempt are durable.
    {
        let conn = open_db(root);
        let job: Option<String> = conn
            .query_row(
                "SELECT provider_job_id FROM workflow_step_executions \
                 WHERE workflow_run_id = ?1 AND status = 'succeeded'",
                [&run.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(job.is_some(), "a durable provider job must be recorded");
    }

    // --- The video persisted: result set (video), artifact (video/mp4),
    // and a candidate AssetVersion in the scene's stable video asset. ---
    let result_sets = GenerationService::list_results(root, Some(&run.run.id)).unwrap();
    assert_eq!(result_sets.len(), 1);
    let result_set = &result_sets[0];
    assert_eq!(result_set.result_set.media_kind, "video");
    assert_eq!(result_set.artifacts.len(), 1);
    let video_artifact = &result_set.artifacts[0].artifact;
    assert_eq!(video_artifact.media_kind, "video");
    assert_eq!(video_artifact.mime_type, "video/mp4");
    assert!(root.join(&video_artifact.storage_path).is_file());

    let conn = open_db(root);
    let video_assets: Vec<(String, String)> = conn
        .prepare(
            "SELECT a.id, av.id FROM assets a JOIN asset_versions av ON av.asset_id = a.id \
             WHERE a.type = 'video' AND a.owner_entity_id = ?1 AND av.mime_type = 'video/mp4'",
        )
        .unwrap()
        .query_map([&scene.id], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert_eq!(
        video_assets.len(),
        1,
        "exactly one candidate video version in the scene's stable video asset"
    );
    let (video_asset_id, candidate_version_id) = video_assets[0].clone();
    drop(conn);

    // The lineage captured the full provenance chain.
    let detail =
        GenerationService::get_artifact_detail(root, &video_artifact.id).unwrap();
    let lineage = detail.lineage.as_ref().unwrap();
    assert_eq!(lineage.skill_id, "scene-builder");
    assert_eq!(lineage.workflow_definition_id, "scene.generate_video");
    assert_eq!(lineage.provider_id, "fake_async_video");
    // The lineage references the run's immutable canon snapshot whenever
    // the scene context captured locked canon sections (this fixture has
    // none, so the id is legitimately absent; the keyframe path covers the
    // populated case).
    if let Some(snapshot_id) = lineage.canon_snapshot_id.as_deref() {
        assert!(snapshot_id.starts_with("canon:"));
    }

    // --- Explicit promotion: candidate -> canonical video version. ---
    let promoted = GenerationService::promote_generated_artifact(
        root,
        &video_artifact.id,
        &video_asset_id,
        true,
    )
    .unwrap();
    assert_eq!(promoted.id, candidate_version_id);
    assert_eq!(promoted.status, "canonical");

    // Promotion is idempotent.
    let again = GenerationService::promote_generated_artifact(
        root,
        &video_artifact.id,
        &video_asset_id,
        true,
    )
    .unwrap();
    assert_eq!(again.id, candidate_version_id);

    // --- Exact-version pin: pin V1, promote a V2 later, the pin must not
    // drift. ---
    CinemaService::set_shot_video(root, &shot.id, Some(&candidate_version_id)).unwrap();
    {
        let shots = CinemaService::list_shots(root, &scene.id).unwrap();
        assert_eq!(
            shots[0].generated_video_asset_version_id.as_deref(),
            Some(candidate_version_id.as_str())
        );
    }

    // A second video run with identical payload content dedups by sha256;
    // generate a different deterministic content by... the fake provider
    // always returns the same bytes, so the second run's capture produces
    // the same sha256 and the import reconciles to the same version. To
    // prove drift-immunity we instead import a distinct MP4 directly.
    let distinct_source = root.join("second-video.mp4");
    std::fs::write(
        &distinct_source,
        [
            0u8, 0, 0, 24, b'f', b't', b'y', b'p', 0, 0, 0, 0, b'm', b'p', b'4', b'2',
        ],
    )
    .unwrap();
    let second_version =
        AssetService::import_media_version(root, &video_asset_id, &distinct_source, None)
            .unwrap();
    AssetService::promote_asset_version(root, &second_version.id).unwrap();

    {
        let conn = open_db(root);
        let canonical: Option<String> = conn
            .query_row(
                "SELECT canonical_version_id FROM assets WHERE id = ?1",
                [&video_asset_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            canonical.as_deref(),
            Some(second_version.id.as_str()),
            "the asset's canonical pointer moved to V2"
        );
    }
    {
        let shots = CinemaService::list_shots(root, &scene.id).unwrap();
        assert_eq!(
            shots[0].generated_video_asset_version_id.as_deref(),
            Some(candidate_version_id.as_str()),
            "the shot video pin MUST remain on the exact V1 version -- \
             canonical drift must never rewrite a pinned reference"
        );
    }

    // Pinning a non-video or non-canonical version is rejected.
    let bad_pin = CinemaService::set_shot_video(root, &shot.id, Some(&keyframe_version.id));
    assert!(bad_pin.is_err(), "a keyframe version must not be pinnable as the shot video");

    // --- Restart: reopen the project and verify exact state persists. ---
    let reopened = ProjectService::open(root).unwrap();
    assert_eq!(reopened.id, scene.project_id);
    {
        let conn = open_db(root);
        let (pinned, mime, status): (Option<String>, String, String) = conn
            .query_row(
                "SELECT sh.generated_video_asset_version_id, av.mime_type, av.status \
                 FROM scene_shots sh \
                 JOIN asset_versions av ON av.id = sh.generated_video_asset_version_id \
                 WHERE sh.id = ?1",
                [&shot.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(pinned.as_deref(), Some(candidate_version_id.as_str()));
        assert_eq!(mime, "video/mp4");
        // V1's row was flipped to `superseded` by V2's promotion (the normal
        // state machine), but the pinned version *id* -- the immutable
        // reference -- is unchanged. That is the exact-version guarantee.
        assert_eq!(status, "superseded");
    }
    let artifact_still_on_disk = root.join(&video_artifact.storage_path).is_file();
    assert!(artifact_still_on_disk);
}

#[test]
fn failed_video_capture_leaves_no_phantom_video_version() {
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

    // dry_run is image-only: the capability gate must reject video requests
    // before any submission, and no phantom asset version may appear.
    let run = WorkflowRuntime::create_run(
        root,
        "scene-builder",
        "1.0.0",
        "scene.generate_video",
        serde_json::json!({
            "sceneId": scene.id,
            "providerId": "dry_run",
            "modelId": "dry-run-v1",
        }),
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(root, &run.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    WorkflowRuntime::approve_run_step(root, &run.run.id, "approve-request", None).unwrap();
    let failed = WorkflowRuntime::advance_run(root, &run.run.id);
    assert!(failed.is_err());
    let failed_detail = WorkflowRuntime::get_run(root, &run.run.id).unwrap();
    assert_eq!(failed_detail.run.status, "failed");

    let conn = open_db(root);
    let video_versions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM asset_versions av JOIN assets a ON a.id = av.asset_id \
             WHERE a.type = 'video'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(video_versions, 0, "no phantom video version after a failed run");
    let artifact_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM generated_artifacts WHERE media_kind = 'video'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(artifact_rows, 0, "no phantom video artifact after a failed run");
}
