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
    let location =
        CanonService::create_entity(std::path::Path::new(&f.root), CanonEntityType::Location, "The Station")
            .unwrap();
    let world = WorldService::create_world(std::path::Path::new(&f.root), &location.id).unwrap();
    {
        // Canonicalize the World's plate asset so the scene can pin it.
        let source = image_file(std::path::Path::new(&f.root), "world-plate.png", [10, 20, 30, 255]);
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
    scene_commands::assign_scene_world(f.root.clone(), scene.id.clone(), world.id.clone())
        .unwrap();
    (scene.id, world.id)
}

#[test]
fn scene_commands_round_trip_with_stable_error_codes() {
    let f = fixture();

    let scene = scene_commands::create_world_scene(
        f.root.clone(),
        "Scene 001".into(),
        "Summary".into(),
    )
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

    let error =
        scene_commands::update_scene_details(f.root.clone(), scene_id.clone(), String::new(), String::new())
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
