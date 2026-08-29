use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::CanonService;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::error::AppError;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::scenes::service::SceneService;
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

/// Creates an asset with one imported, promoted (canonical) version and
/// returns the canonical version id.
fn canonical_version(root: &Path, asset_type: &str, pixel: [u8; 4]) -> String {
    let asset = AssetService::create_asset(root, asset_type, "Mara Look", None).unwrap();
    let source = test_image(root, "look.png", pixel);
    let version = AssetService::import_asset_version(root, &asset.id, &source, None).unwrap();
    AssetService::promote_asset_version(root, &version.id).unwrap();
    version.id
}

fn locked_character(root: &PathBuf, keys: &[&str]) -> String {
    let character =
        CanonService::create_entity(root, CanonEntityType::Character, "Mara Keene").unwrap();
    for key in keys {
        let section = CanonService::upsert_section(
            root,
            &character.id,
            key,
            serde_json::json!({ "text": format!("locked {key}") }),
            None,
        )
        .unwrap();
        CanonService::lock_section(root, &section.id, None).unwrap();
    }
    character.id
}

#[test]
fn resolves_speech_movement_stillness_from_locked_sections() {
    let (_temp, root) = project("Red Door");
    let character = locked_character(&root, &["speech", "movement", "stillness"]);
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();

    let locks = CinemaService::resolve_behavioral_locks(&conn, &character).unwrap();
    assert_eq!(locks.speech.as_deref(), Some("locked speech"));
    assert_eq!(locks.movement.as_deref(), Some("locked movement"));
    assert_eq!(locks.stillness.as_deref(), Some("locked stillness"));
}

#[test]
fn blocks_behavior_resolution_when_any_section_is_unlocked() {
    let (_temp, root) = project("Red Door");
    let character = locked_character(&root, &["speech", "movement"]);
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();

    let error = CinemaService::resolve_behavioral_locks(&conn, &character).unwrap_err();
    assert!(matches!(error, AppError::WorkflowPrerequisiteFailed(_)));
    assert!(error.to_string().contains("stillness"));
}

#[test]
fn create_shots_validate_and_auto_order() {
    let (_temp, root) = project("Red Door");
    let scene = SceneService::create_scene(&root, "Scene 001", "A test scene").unwrap();

    let first = CinemaService::create_shot(
        &root,
        &scene.id,
        None,
        4.0,
        "Establish the ops room",
        None,
        Some("wide".into()),
    )
    .unwrap();
    let second = CinemaService::create_shot(
        &root,
        &scene.id,
        None,
        4.0,
        "Close on the console",
        Some("Mara leans in".into()),
        None,
    )
    .unwrap();
    assert_eq!(first.ordering, 0);
    assert_eq!(second.ordering, 1);

    // Blank intents and invalid durations are rejected.
    assert!(matches!(
        CinemaService::create_shot(&root, &scene.id, None, 4.0, "  ", None, None),
        Err(AppError::InvalidShotIntent)
    ));
    assert!(matches!(
        CinemaService::create_shot(&root, &scene.id, None, 0.0, "Look", None, None),
        Err(AppError::InvalidCinemaDuration(_))
    ));
}

#[test]
fn compile_scene_requires_characters_and_shots() {
    let (_temp, root) = project("Red Door");
    let character = locked_character(&root, &["speech", "movement", "stillness"]);
    let look = canonical_version(&root, "outfit", [10, 10, 10, 255]);
    let scene = SceneService::create_scene(&root, "Scene 001", "A test scene").unwrap();

    // Empty compilation input is rejected (duration validation runs first,
    // then scene validation rejects the empty scene).
    let error = CinemaService::compile_scene(
        &root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap_err();
    assert!(matches!(error, AppError::WorkflowPrerequisiteFailed(_)));

    // Re-import the look under an asset owned by the character (P7 checks
    // look ownership when casting).
    let look_asset =
        AssetService::create_asset(&root, "outfit", "Owned Look", Some(character.clone())).unwrap();
    let owned_source = test_image(&root, "owned-look.png", [11, 12, 13, 255]);
    let owned = AssetService::import_asset_version(&root, &look_asset.id, &owned_source, None)
        .unwrap();
    AssetService::promote_asset_version(&root, &owned.id).unwrap();
    SceneService::add_scene_character(&root, &scene.id, &character, &owned.id, None, None)
        .unwrap();
    CinemaService::create_shot(&root, &scene.id, None, 4.0, "Establish", None, None).unwrap();

    let compilation = CinemaService::compile_scene(
        &root,
        cinematic_desktop_lib::cinema::model::CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();
    assert_eq!(compilation.scene_id, scene.id);
}
