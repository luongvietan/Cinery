//! Shared scene fixtures for the cinema integration test binaries.
#![allow(dead_code)]

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::{CanonService, VisualLockDto};
use cinematic_desktop_lib::cinema::model::{
    BehavioralLocks, SceneCharacterRecord, SceneRecord, ShotRecord,
};
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

pub struct CompiledScene {
    pub _temp: TempDir,
    pub root: PathBuf,
    pub scene: SceneRecord,
    pub behavioral_locks: BehavioralLocks,
    pub world_continuity: cinematic_desktop_lib::cinema::model::WorldContinuity,
    pub visual_locks: Vec<VisualLockDto>,
    pub shots: Vec<ShotRecord>,
    pub characters: Vec<SceneCharacterRecord>,
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

/// Builds a compilable scene: locked-behavior + visual-locks character with
/// a canonical look, canonical world plate wired into the scene, and two
/// 4s shots (8s total).
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
        character_id: character.id,
        root,
        scene,
        behavioral_locks,
        world_continuity,
        visual_locks,
        shots,
        characters,
    }
}
pub mod command_harness;
