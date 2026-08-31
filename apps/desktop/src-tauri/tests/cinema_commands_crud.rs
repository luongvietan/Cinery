//! Command-boundary tests for the unified scene + cinema commands. Scene
//! identity is created through the `scenes` command boundary (authoritative
//! `world_scenes` aggregate); shots and compilations go through the `cinema`
//! commands. Every mutation goes through public Tauri command functions.

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::CanonService;
use cinematic_desktop_lib::cinema::commands::*;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::scenes::commands as scene_commands;
use cinematic_desktop_lib::worlds::service::WorldService;
use tempfile::tempdir;

fn image_file(root: &std::path::Path, name: &str, pixel: [u8; 4]) -> std::path::PathBuf {
    let path = root.join(name);
    let image: image::RgbaImage = image::ImageBuffer::from_pixel(8, 8, image::Rgba(pixel));
    image.save(&path).unwrap();
    path
}

fn canonical_version(
    root: &std::path::Path,
    asset_type: &str,
    label: &str,
    pixel: [u8; 4],
) -> String {
    let asset = AssetService::create_asset(root, asset_type, label, None).unwrap();
    let source = image_file(root, &format!("{label}.png"), pixel);
    let version = AssetService::import_asset_version(root, &asset.id, &source, None).unwrap();
    AssetService::promote_asset_version(root, &version.id).unwrap();
    version.id
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: String,
}

fn fixture() -> Fixture {
    let temp = tempdir().unwrap();
    let path = temp.path().join("cinema-commands");
    ProjectService::create(&path, "Cinema Commands").unwrap();
    Fixture {
        _temp: temp,
        root: path.to_string_lossy().to_string(),
    }
}

fn scene_with_world(f: &Fixture) -> (String, String) {
    let location = CanonService::create_entity(
        std::path::Path::new(&f.root),
        CanonEntityType::Location,
        "The Station",
    )
    .unwrap();
    let world = WorldService::create_world(std::path::Path::new(&f.root), &location.id).unwrap();
    {
        // Canonicalize the World's plate asset so the scene can pin it.
        let source = image_file(
            std::path::Path::new(&f.root),
            "world-plate.png",
            [10, 20, 30, 255],
        );
        let version = AssetService::import_asset_version(
            std::path::Path::new(&f.root),
            &world.world_plate_asset_id,
            &source,
            None,
        )
        .unwrap();
        AssetService::promote_asset_version(std::path::Path::new(&f.root), &version.id).unwrap();
    }
    let scene = scene_commands::create_world_scene(
        f.root.clone(),
        "Scene 001".into(),
        "Ops room stand-off".into(),
    )
    .unwrap();
    scene_commands::assign_scene_world(f.root.clone(), scene.id.clone(), world.id.clone()).unwrap();
    (scene.id, world.id)
}

#[test]
fn scene_commands_round_trip_with_stable_error_codes() {
    let f = fixture();

    let scene =
        scene_commands::create_world_scene(f.root.clone(), "Scene 001".into(), "Summary".into())
            .unwrap();
    let renamed = scene_commands::update_scene_details(
        f.root.clone(),
        scene.id.clone(),
        "Scene 001 - Ops".into(),
        "Summary".into(),
    )
    .unwrap();
    assert_eq!(renamed.title, "Scene 001 - Ops");

    let fetched = scene_commands::get_world_scene(f.root.clone(), scene.id.clone()).unwrap();
    assert_eq!(fetched.id, scene.id);

    // Stable error code for a missing scene.
    let error =
        scene_commands::get_world_scene(f.root.clone(), "01ARZ3NDEKTSV4RRFFQ69G5FAV".into())
            .unwrap_err();
    assert_eq!(error.code, "SCENE_NOT_FOUND");
}

#[test]
fn world_assignment_and_shot_lifecycle_flow_through_commands() {
    let f = fixture();
    let keyframe = canonical_version(
        std::path::Path::new(&f.root),
        "shot_keyframe",
        "KF",
        [40, 50, 60, 255],
    );

    let (scene_id, _world_id) = scene_with_world(&f);
    let scene = scene_commands::get_world_scene(f.root.clone(), scene_id.clone()).unwrap();
    assert!(scene.world_asset_version_id.is_some());

    let shot = create_shot(
        f.root.clone(),
        scene_id.clone(),
        None,
        4.0,
        "Establish".into(),
        None,
        None,
    )
    .unwrap();
    let updated = update_shot(
        f.root.clone(),
        shot.id.clone(),
        Some(6.0),
        Some("Close".into()),
        Some("lean".into()),
        Some("medium".into()),
    )
    .unwrap();
    assert_eq!(updated.duration_seconds, 6.0);

    set_shot_keyframe(f.root.clone(), shot.id.clone(), Some(keyframe.clone())).unwrap();
    let shots = list_shots(f.root.clone(), scene_id.clone()).unwrap();
    assert_eq!(
        shots[0].keyframe_asset_version_id.as_deref(),
        Some(keyframe.as_str())
    );

    // Clearing the keyframe leaves the shot valid.
    set_shot_keyframe(f.root.clone(), shot.id.clone(), None).unwrap();

    // Second shot for reorder coverage.
    let shot2 = create_shot(
        f.root.clone(),
        scene_id.clone(),
        None,
        4.0,
        "Second".into(),
        None,
        None,
    )
    .unwrap();
    let reordered = reorder_shots(
        f.root.clone(),
        scene_id.clone(),
        vec![shot2.id.clone(), shot.id.clone()],
    )
    .unwrap();
    assert_eq!(reordered[0].id, shot2.id);
    assert_eq!(
        reordered.iter().map(|s| s.ordering).collect::<Vec<_>>(),
        vec![0, 1]
    );

    // Readiness: no cast -> blocker, stable action target.
    let readiness = get_scene_readiness(f.root.clone(), scene_id.clone()).unwrap();
    assert!(!readiness.ready);
    assert!(readiness
        .blockers
        .iter()
        .any(|b| b.code == "missing_cast_look" && b.action_target == "cast"));

    delete_shot(f.root.clone(), scene_id.clone(), shot.id.clone()).unwrap();
    let shots = list_shots(f.root.clone(), scene_id).unwrap();
    assert_eq!(shots.len(), 1);
}

#[test]
fn invalid_mutations_return_stable_error_codes_not_panics() {
    let f = fixture();
    let (scene_id, _world_id) = scene_with_world(&f);

    let error = scene_commands::update_scene_details(
        f.root.clone(),
        scene_id.clone(),
        String::new(),
        String::new(),
    )
    .unwrap_err();
    assert_eq!(error.code, "INVALID_SCENE_TITLE");

    let error = create_shot(
        f.root.clone(),
        scene_id.clone(),
        None,
        99.0,
        "Too long".into(),
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "INVALID_CINEMA_DURATION");

    let error = reorder_shots(f.root.clone(), scene_id.clone(), vec!["shot-x".into()]).unwrap_err();
    assert_eq!(error.code, "WORKFLOW_INPUT_INVALID");

    // Shots cannot be attached to a scene of another project / missing scene.
    let error = create_shot(
        f.root.clone(),
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
        None,
        4.0,
        "Orphan".into(),
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "SCENE_NOT_FOUND");
}

#[test]
fn shot_i2v_source_returns_the_exact_pinned_version_not_latest() {
    use cinematic_desktop_lib::cinema::service::CinemaService;
    use cinematic_desktop_lib::scenes::service::SceneService;

    let f = fixture();
    let root = std::path::Path::new(&f.root);
    let scene =
        scene_commands::create_world_scene(f.root.clone(), "Scene 001".into(), "S".into()).unwrap();
    let shot = create_shot(
        f.root.clone(),
        scene.id.clone(),
        None,
        4.0,
        "Establish".into(),
        None,
        None,
    )
    .unwrap();

    // No keyframe yet -> typed missing-source error.
    let error = get_shot_image_to_video_source(f.root.clone(), shot.id.clone()).unwrap_err();
    assert_eq!(error.code, "SOURCE_KEYFRAME_MISSING");

    let keyframe_asset = SceneService::ensure_scene_keyframe_asset(root, &scene.id).unwrap();
    let first_source = image_file(root, "kf-first.png", [10, 10, 10, 255]);
    let first_version =
        AssetService::import_asset_version(root, &keyframe_asset.id, &first_source, None).unwrap();
    AssetService::promote_asset_version(root, &first_version.id).unwrap();
    set_shot_keyframe(
        f.root.clone(),
        shot.id.clone(),
        Some(first_version.id.clone()),
    )
    .unwrap();

    let second_source = image_file(root, "kf-second.png", [200, 200, 200, 255]);
    let second_version =
        AssetService::import_asset_version(root, &keyframe_asset.id, &second_source, None).unwrap();
    AssetService::promote_asset_version(root, &second_version.id).unwrap();

    // The projection must return the exact pinned version, not the newest.
    let source = CinemaService::get_shot_image_to_video_source(root, &shot.id).unwrap();
    assert_eq!(source.asset_version_id, first_version.id);
    assert_eq!(source.asset_id, keyframe_asset.id);
    assert_eq!(source.version_number, 1);
    assert_eq!(source.mime_type, "image/png");
    assert!(source.thumbnail_path.is_some());
}

#[test]
fn promote_shot_video_candidate_command_pins_and_conflicts() {
    use cinematic_desktop_lib::cinema::service::CinemaService;
    use cinematic_desktop_lib::generation::service::{GenerationCaptureInput, GenerationService};
    use cinematic_desktop_lib::providers::model::{ProviderOutput, ProviderResult};
    use cinematic_desktop_lib::scenes::service::SceneService;
    use serde_json::json;

    let f = fixture();
    let root = std::path::Path::new(&f.root);

    let scene =
        scene_commands::create_world_scene(f.root.clone(), "Scene 001".into(), "S".into()).unwrap();
    let shot = create_shot(
        f.root.clone(),
        scene.id.clone(),
        None,
        4.0,
        "Establish".into(),
        None,
        None,
    )
    .unwrap();

    // Pin a canonical keyframe on the shot.
    let keyframe_asset = SceneService::ensure_scene_keyframe_asset(root, &scene.id).unwrap();
    let keyframe_source = image_file(root, "kf.png", [29, 47, 83, 255]);
    let keyframe_version =
        AssetService::import_asset_version(root, &keyframe_asset.id, &keyframe_source, None)
            .unwrap();
    AssetService::promote_asset_version(root, &keyframe_version.id).unwrap();
    set_shot_keyframe(
        f.root.clone(),
        shot.id.clone(),
        Some(keyframe_version.id.clone()),
    )
    .unwrap();

    // Durable run + attempt for the completed shot.image_to_video run.
    let conn =
        cinematic_desktop_lib::db::open_existing_connection(&root.join("project.db")).unwrap();
    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();
    let input = json!({
        "sceneId": scene.id,
        "shotId": shot.id,
        "sourceAssetVersionId": keyframe_version.id,
    });
    conn.execute(
        "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, \
         status, input_json, created_at, updated_at) \
         VALUES ('run-cmd', ?1, 'scene-builder', '1.0.0', 'shot.image_to_video', \
         'completed', ?2, 'now', 'now')",
        rusqlite::params![project_id, input.to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, \
         attempt_number, compiled_request_id, provider_id, model_id, adapter_version, \
         idempotency_key, status, started_at) \
         VALUES ('attempt-cmd', 'run-cmd', 'execute', 1, 'compiled-cmd', 'fake_async_video', \
         'fake-video-v1', 1, 'run-cmd:execute:1', 'succeeded', 'now')",
        [],
    )
    .unwrap();
    drop(conn);

    // Capture one MP4 artifact from the frozen keyframe source.
    fn mp4_bytes(seed: u8) -> Vec<u8> {
        let mut bytes = vec![0x00, 0x00, 0x00, 0x18];
        bytes.extend_from_slice(b"ftypmp42");
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, seed]);
        bytes.extend_from_slice(b"mp42isom");
        bytes
    }
    let artifacts = GenerationService::capture_provider_result(
        root,
        &GenerationCaptureInput {
            project_id: project_id.clone(),
            workflow_run_id: "run-cmd".into(),
            workflow_step_key: "execute".into(),
            workflow_definition_id: "scene-builder".into(),
            workflow_version: "1.0.0".into(),
            skill_id: "scene-builder".into(),
            skill_version: "1.0.0".into(),
            compiled_execution_artifact_id: "compiled-cmd".into(),
            compiled_request_sha256: "b".repeat(64),
            canon_snapshot_id: None,
            canon_snapshot_sha256: None,
            provider_attempt_id: "attempt-cmd".into(),
            provider_id: "fake_async_video".into(),
            model_id: "fake-video-v1".into(),
            source_asset_version_ids: vec![keyframe_version.id.clone()],
            requested_output_count: 1,
            media_kind: "video".into(),
        },
        &ProviderResult {
            outputs: vec![ProviderOutput {
                uri: format!(
                    "data:video/mp4;base64,{}",
                    base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        mp4_bytes(7)
                    )
                ),
                mime_type: "video/mp4".into(),
                filename: Some("cmd.mp4".into()),
            }],
            provider_reported_model: Some("fake-video-v1".into()),
            metadata: json!({}),
        },
    )
    .unwrap()
    .artifacts;
    let artifact_id = artifacts[0].id.clone();

    // Completion-time candidate import: the captured artifact is already
    // imported as a candidate version of the scene-owned video asset.
    let video_asset =
        AssetService::create_asset(root, "video", "Scene 001 video", Some(scene.id.clone()))
            .unwrap();
    let candidate_source = root.join("candidate-cmd.mp4");
    std::fs::write(&candidate_source, mp4_bytes(7)).unwrap();
    AssetService::import_media_version(root, &video_asset.id, &candidate_source, None).unwrap();

    // The unknown-expected-pin path conflicts before any pin happens.
    let error = promote_shot_video_candidate(
        f.root.clone(),
        shot.id.clone(),
        artifact_id.clone(),
        Some("stale".into()),
    )
    .unwrap_err();
    assert_eq!(error.code, "PROMOTION_CONFLICT");

    // Promote with the real (null) expected pin.
    let promoted =
        promote_shot_video_candidate(f.root.clone(), shot.id.clone(), artifact_id.clone(), None)
            .unwrap();
    assert_eq!(promoted.shot_id, shot.id);
    assert_eq!(promoted.artifact_id, artifact_id);
    assert_eq!(promoted.previous_asset_version_id, None);

    let pinned = CinemaService::list_shots(root, &scene.id)
        .unwrap()
        .into_iter()
        .find(|s| s.id == shot.id)
        .unwrap();
    assert_eq!(
        pinned.generated_video_asset_version_id.as_deref(),
        Some(promoted.asset_version_id.as_str())
    );
    assert_eq!(
        pinned.keyframe_asset_version_id.as_deref(),
        Some(keyframe_version.id.as_str())
    );

    // Replay is idempotent; a stale expectation conflicts.
    let replayed = promote_shot_video_candidate(
        f.root.clone(),
        shot.id.clone(),
        artifact_id.clone(),
        Some(promoted.asset_version_id.clone()),
    )
    .unwrap();
    assert_eq!(replayed.asset_version_id, promoted.asset_version_id);

    // Same-artifact replay wins even without an expected pin while the Shot
    // still holds the promoted version (hardened final-review semantics).
    let replayed = promote_shot_video_candidate(f.root.clone(), shot.id.clone(), artifact_id, None)
        .unwrap();
    assert_eq!(replayed.asset_version_id, promoted.asset_version_id);
}
