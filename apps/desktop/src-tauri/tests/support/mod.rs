//! Shared fixtures for the cinema integration test binaries. Scenes are
//! created on the authoritative `world_scenes` aggregate (P7 pipeline); the
//! cinema layer contributes shots and compilations.
#![allow(dead_code)]

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::{CanonService, VisualLockDto};
use cinematic_desktop_lib::cinema::model::{BehavioralLocks, ShotRecord, WorldContinuity};
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::scenes::model::{Scene, SceneCharacterAssignment};
use cinematic_desktop_lib::scenes::service::SceneService;
use cinematic_desktop_lib::worlds::service::WorldService;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

pub struct CompiledScene {
    pub _temp: TempDir,
    pub root: PathBuf,
    pub scene: Scene,
    pub behavioral_locks: BehavioralLocks,
    pub world_continuity: WorldContinuity,
    pub visual_locks: Vec<VisualLockDto>,
    pub shots: Vec<ShotRecord>,
    pub cast: Vec<SceneCharacterAssignment>,
    pub character_id: String,
}

pub fn test_image(root: &Path, name: &str, pixel: [u8; 4]) -> PathBuf {
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

/// Builds a compilable scene on the authoritative aggregate: locked-behavior
/// + visual-locks character with a canonical look, a World whose plate is the
///   canonical world plate assigned to the scene, and two 4s shots (8s total).
pub fn compilable_scene() -> CompiledScene {
    let temp = tempdir().unwrap();
    let root = temp.path().join("red-door");
    ProjectService::create(&root, "Red Door").unwrap();

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

    // The look asset must be owned by the character (P7 reference checks).
    let look_asset =
        AssetService::create_asset(&root, "outfit", "Mara Look", Some(character.id.clone()))
            .unwrap();
    let look_source = test_image(&root, "look.png", [4, 5, 6, 255]);
    let look_version =
        AssetService::import_asset_version(&root, &look_asset.id, &look_source, None).unwrap();
    AssetService::promote_asset_version(&root, &look_version.id).unwrap();
    let look = look_version.id;
    let location =
        CanonService::create_entity(&root, CanonEntityType::Location, "The Station").unwrap();
    let world = WorldService::create_world(&root, &location.id).unwrap();
    // Give the World's plate asset its canonical version so the scene can
    // pin the exact version.
    let plate_source = test_image(&root, "world-plate.png", [10, 20, 30, 255]);
    let plate_version =
        AssetService::import_asset_version(&root, &world.world_plate_asset_id, &plate_source, None)
            .unwrap();
    AssetService::promote_asset_version(&root, &plate_version.id).unwrap();
    let scene =
        SceneService::create_scene(&root, "Scene 001", "Mara returns to the ops room").unwrap();
    SceneService::assign_scene_world(&root, &scene.id, &world.id).unwrap();
    // Re-fetch so the fixture carries the pinned world version.
    let scene = SceneService::get_scene(&root, &scene.id).unwrap();
    assert!(scene.world_asset_version_id.is_some());
    SceneService::add_scene_character(&root, &scene.id, &character.id, &look, None, None).unwrap();
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
    let behavioral_locks = CinemaService::resolve_scene_behavioral_locks(&conn, &scene.id).unwrap();
    let visual_locks =
        CanonService::get_locked_character_visual_locks(&root, &character.id).unwrap();
    let cast =
        cinematic_desktop_lib::cinema::repository::list_scene_cast(&conn, &scene.id).unwrap();
    let shots = CinemaService::list_shots(&root, &scene.id).unwrap();
    let world_continuity = match &scene.world_asset_version_id {
        Some(version_id) => {
            let canonical = cinematic_desktop_lib::cinema::service::ensure_canonical_version(
                &conn,
                &scene.project_id,
                version_id,
                &["world_plate"],
            )
            .unwrap();
            WorldContinuity {
                plate_id: Some(canonical.asset_id),
                plate_asset_version_id: Some(canonical.version_id),
                description: Some(canonical.label),
            }
        }
        None => WorldContinuity::default(),
    };
    drop(conn);

    CompiledScene {
        _temp: temp,
        character_id: character.id,
        root,
        scene,
        behavioral_locks,
        world_continuity,
        visual_locks,
        shots,
        cast,
    }
}
pub mod command_harness;
