use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::{CanonService, VisualLockDto};
use cinematic_desktop_lib::cinema::compiler;
use cinematic_desktop_lib::cinema::model::{BehavioralLocks, SceneCharacterRecord, SceneRecord, ShotRecord};
use cinematic_desktop_lib::cinema::model::WorldContinuity;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

fn project(name: &str) -> (TempDir, PathBuf) {
    let temp = tempdir().unwrap();
    let root = temp.path().join(name);
    ProjectService::create(&root, name).unwrap();
    (temp, root)
}

fn test_image(root: &Path, name: &str, pixel: [u8; 4]) -> PathBuf {
    let path = root.join(name);
    let image: image::RgbaImage = image::ImageBuffer::from_pixel(32, 32, image::Rgba(pixel));
    image.save(&path).unwrap();
    path
}

fn canonical_version(root: &Path, asset_type: &str, label: &str) -> String {
    let asset = AssetService::create_asset(root, asset_type, label, None).unwrap();
    let source = test_image(root, "plate.png", [10, 20, 30, 255]);
    let version = AssetService::import_asset_version(root, &asset.id, &source, None).unwrap();
    AssetService::promote_asset_version(root, &version.id).unwrap();
    version.id
}

/// Everything needed to compile one scene.
struct CompiledScene {
    _temp: TempDir,
    #[allow(dead_code)]
    root: PathBuf,
    scene: SceneRecord,
    behavioral_locks: BehavioralLocks,
    world_continuity: WorldContinuity,
    visual_locks: Vec<VisualLockDto>,
    shots: Vec<ShotRecord>,
    characters: Vec<SceneCharacterRecord>,
}

fn compilable_scene() -> CompiledScene {
    let (temp, root) = project("Red Door");

    let character =
        CanonService::create_entity(&root, CanonEntityType::Character, "Mara Keene").unwrap();
    for key in ["speech", "movement", "stillness"] {
        let section = CanonService::upsert_section(
            &root,
            &character.id,
            key,
            serde_json::json!({ "text": format!("locked {key}") }),
            None,
        )
        .unwrap();
        CanonService::lock_section(&root, &section.id, None).unwrap();
    }
    let visual_locks = CanonService::upsert_section(
        &root,
        &character.id,
        "visual_locks",
        serde_json::json!({ "locks": [
            {"id": "scar", "key": "right_eyebrow_scar", "description": "Small healed scar.", "severity": "required", "validatorHint": null},
            {"id": "watch", "key": "left_wrist_watch", "description": "Dented service watch.", "severity": "important", "validatorHint": null}
        ]}),
        None,
    )
    .unwrap();
    CanonService::lock_section(&root, &visual_locks.id, None).unwrap();

    let look = canonical_version(&root, "outfit", "Mara Look");
    let world = canonical_version(&root, "world_plate", "Station interior");
    let scene =
        CinemaService::create_scene(&root, "Scene 001", Some(world.clone()), None).unwrap();
    CinemaService::add_character_to_scene(&root, &scene.id, &character.id, &look, None).unwrap();
    CinemaService::create_shot(
        &root,
        &scene.id,
        Some(0),
        4.0,
        "Establish the ops room",
        Some("Mara scans the console".into()),
        Some("wide".into()),
    )
    .unwrap();
    CinemaService::create_shot(
        &root,
        &scene.id,
        Some(1),
        4.0,
        "Close on the console",
        Some("Mara leans in".into()),
        Some("medium".into()),
    )
    .unwrap();

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let behavioral_locks =
        CinemaService::resolve_scene_behavioral_locks(&conn, &scene.id).unwrap();
    let visual_locks =
        CanonService::get_locked_character_visual_locks(&root, &character.id).unwrap();
    let characters =
        cinematic_desktop_lib::cinema::repository::list_scene_characters(&conn, &scene.id)
            .unwrap();
    let shots = CinemaService::list_shots(&root, &scene.id).unwrap();
    drop(conn);
    let world_continuity =
        CinemaService::resolve_world_continuity(&root, &scene.world_asset_version_id).unwrap();

    CompiledScene {
        _temp: temp,
        root,
        scene,
        behavioral_locks,
        world_continuity,
        visual_locks,
        shots,
        characters,
    }
}
#[test]
fn compiles_8s_two_shot_with_behavior_and_world_continuity() {
    let setup = compilable_scene();

    let prompt = compiler::compile(
        &setup.scene,
        "comp-1",
        8.0,
        None,
        &setup.characters,
        &setup.behavioral_locks,
        &setup.world_continuity,
        &setup.visual_locks,
        &setup.shots,
        &[],
    )
    .unwrap();

    assert_eq!(prompt.total_duration_seconds, 8.0);
    assert_eq!(prompt.shots.len(), 2);
    let sum: f64 = prompt.shots.iter().map(|shot| shot.duration_seconds).sum();
    assert!((sum - 8.0).abs() < 1e-9);
    assert_eq!(prompt.time_budget, vec![4.0, 4.0]);

    assert_eq!(prompt.behavioral_locks.speech.as_deref(), Some("locked speech"));
    assert_eq!(prompt.behavioral_locks.movement.as_deref(), Some("locked movement"));
    assert_eq!(prompt.behavioral_locks.stillness.as_deref(), Some("locked stillness"));

    assert_eq!(
        prompt.world_continuity.plate_asset_version_id,
        setup.scene.world_asset_version_id
    );

    // Every shot carries the sorted visual locks and a continuity note that
    // references the canonical look and world plate.
    for (index, shot) in prompt.shots.iter().enumerate() {
        assert_eq!(shot.order, index);
        let keys: Vec<&str> = shot
            .subject_locks
            .iter()
            .map(|lock| lock.key.as_str())
            .collect();
        assert_eq!(keys, vec!["left_wrist_watch", "right_eyebrow_scar"]);
        let note = shot.continuity_note.as_deref().unwrap();
        assert!(note.contains("canonical look"));
        assert!(note.contains("world plate"));
    }

    let text = &prompt.provider_prompt;
    assert!(text.contains("locked speech"));
    assert!(text.contains("locked movement"));
    assert!(text.contains("locked stillness"));
    assert!(text.contains("Establish the ops room"));
    assert!(text.contains("Close on the console"));
    assert!(text.contains("Station interior"));
    assert!(text.contains("8s"));
    assert!(text.contains("comp-1"));
}

#[test]
fn compiles_deterministically_and_scrubs_open_tbd_topics() {
    let setup = compilable_scene();
    let compile = |shots: &[ShotRecord], topics: &[String]| {
        compiler::compile(
            &setup.scene,
            "comp-1",
            8.0,
            None,
            &setup.characters,
            &setup.behavioral_locks,
            &setup.world_continuity,
            &setup.visual_locks,
            shots,
            topics,
        )
        .unwrap()
    };

    let first = compile(&setup.shots, &[]);
    let second = compile(&setup.shots, &[]);
    assert_eq!(first.provider_prompt, second.provider_prompt);
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );

    // An open (unprotected) TBD topic that leaked into shot action text is
    // scrubbed from the prompt deterministically.
    let mut shots = setup.shots.clone();
    shots[0].action = Some("What is behind the red door? Mara glances over".into());
    let scrubbed = compile(&shots, &["What is behind the red door?".to_string()]);
    assert!(!scrubbed.provider_prompt.contains("What is behind the red door?"));
    assert!(scrubbed.provider_prompt.contains("Mara glances over"));
}

