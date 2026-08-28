use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::CanonService;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::error::AppError;
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

/// Creates an outfit asset with one imported, promoted (canonical) version
/// and returns the canonical version id.
fn canonical_version(root: &Path, asset_type: &str, pixel: [u8; 4]) -> String {
    let asset =
        AssetService::create_asset(root, asset_type, "Mara Look", None).unwrap();
    let source = test_image(root, "look.png", pixel);
    let version =
        AssetService::import_asset_version(root, &asset.id, &source, None).unwrap();
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
fn add_character_requires_canonical_current_look_version() {
    let (_temp, root) = project("Red Door");
    let character = locked_character(&root, &["speech", "movement", "stillness"]);
    let scene = CinemaService::create_scene(&root, "Scene 001", None, None).unwrap();

    let asset = AssetService::create_asset(&root, "outfit", "Mara Look", None).unwrap();
    let source = test_image(&root, "look.png", [10, 20, 30, 255]);
    let draft = AssetService::import_asset_version(&root, &asset.id, &source, None).unwrap();

    // Draft (not canonical) version must be rejected.
    let error = CinemaService::add_character_to_scene(
        &root, &scene.id, &character, &draft.id, None,
    )
    .unwrap_err();
    assert!(matches!(error, AppError::WorkflowPrerequisiteFailed(_)));

    AssetService::promote_asset_version(&root, &draft.id).unwrap();
    CinemaService::add_character_to_scene(&root, &scene.id, &character, &draft.id, None)
        .unwrap();
}

#[test]
fn create_scene_and_shots_validate_and_auto_order() {
    let (_temp, root) = project("Red Door");

    // Blank titles are rejected.
    assert!(matches!(
        CinemaService::create_scene(&root, "   ", None, None),
        Err(AppError::InvalidSceneTitle)
    ));

    let scene = CinemaService::create_scene(&root, "Scene 001", None, None).unwrap();

    let first = CinemaService::create_shot(
        &root, &scene.id, None, 4.0, "Establish the ops room", None, Some("wide".into()),
    )
    .unwrap();
    let second = CinemaService::create_shot(
        &root, &scene.id, None, 4.0, "Close on the console", Some("Mara leans in".into()), None,
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
fn resolves_world_continuity_from_canonical_world_plate() {
    let (_temp, root) = project("Red Door");
    let version = canonical_version(&root, "world_plate", [1, 2, 3, 255]);

    // No world plate configured -> continuity is simply empty.
    let empty = CinemaService::resolve_world_continuity(&root, &None).unwrap();
    assert!(empty.plate_asset_version_id.is_none());

    let continuity =
        CinemaService::resolve_world_continuity(&root, &Some(version.clone())).unwrap();
    assert_eq!(
        continuity.plate_asset_version_id.as_deref(),
        Some(version.as_str())
    );
    assert!(continuity.plate_id.is_some());

    // A non-world-plate version is rejected.
    let outfit = canonical_version(&root, "outfit", [4, 5, 6, 255]);
    assert!(CinemaService::resolve_world_continuity(&root, &Some(outfit)).is_err());
}

#[test]
fn validate_scene_for_compilation_requires_characters_and_shots() {
    let (_temp, root) = project("Red Door");
    let character = locked_character(&root, &["speech", "movement", "stillness"]);
    let look = canonical_version(&root, "outfit", [10, 10, 10, 255]);
    let scene = CinemaService::create_scene(&root, "Scene 001", None, None).unwrap();

    // No characters/shots yet.
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let project_id: String = conn
        .query_row("SELECT id FROM projects LIMIT 1", [], |row| row.get(0))
        .unwrap();
    assert!(CinemaService::validate_scene_for_compilation(&conn, &project_id, &scene.id).is_err());
    drop(conn);

    CinemaService::add_character_to_scene(&root, &scene.id, &character, &look, None).unwrap();
    CinemaService::create_shot(&root, &scene.id, None, 4.0, "Establish", None, None).unwrap();

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let validated =
        CinemaService::validate_scene_for_compilation(&conn, &project_id, &scene.id).unwrap();
    assert_eq!(validated.id, scene.id);
}

