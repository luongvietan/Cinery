use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::CanonService;
use cinematic_desktop_lib::canon::tbd;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::cinema::tbd_guard;
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

/// Full compilable scene setup: locked-behavior character with a canonical
/// look, one shot. Returns (root, scene_id, character_id).
fn scene_with_character(keys: &[&str]) -> (TempDir, PathBuf, String, String) {
    let (temp, root) = project("Red Door");
    let character =
        CanonService::create_entity(&root, CanonEntityType::Character, "Mara Keene").unwrap();
    for key in keys {
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
    // Look asset must be owned by the character (P7 reference checks).
    let look_asset =
        AssetService::create_asset(&root, "outfit", "Mara Look", Some(character.id.clone()))
            .unwrap();
    let look_source = test_image(&root, "look.png", [4, 5, 6, 255]);
    let look_version =
        AssetService::import_asset_version(&root, &look_asset.id, &look_source, None).unwrap();
    AssetService::promote_asset_version(&root, &look_version.id).unwrap();
    let look = look_version.id;
    let scene = cinematic_desktop_lib::scenes::service::SceneService::create_scene(
        &root,
        "Scene 001",
        "A test scene",
    )
    .unwrap();
    cinematic_desktop_lib::scenes::service::SceneService::add_scene_character(
        &root,
        &scene.id,
        &character.id,
        &look,
        None,
        None,
    )
    .unwrap();
    CinemaService::create_shot(&root, &scene.id, None, 4.0, "Establish", None, None).unwrap();
    (temp, root, scene.id, character.id)
}

fn project_id(root: &Path) -> String {
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    conn.query_row("SELECT id FROM projects LIMIT 1", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn blocks_compilation_when_protected_tbd_open_for_scene_character() {
    let (_temp, root, scene_id, character_id) =
        scene_with_character(&["speech", "movement", "stillness"]);
    tbd::create(
        &root,
        Some(&character_id),
        None,
        "What does Mara hide?",
        None,
        true,
    )
    .unwrap();

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let error = tbd_guard::check_tbd_firewall(&conn, &project_id(&root), &scene_id).unwrap_err();
    assert!(matches!(error, AppError::WorkflowBlockedByProtectedTbd(_)));
    assert!(error.to_string().contains("What does Mara hide?"));
}

#[test]
fn blocks_on_project_scoped_protected_tbd() {
    let (_temp, root, scene_id, _character) =
        scene_with_character(&["speech", "movement", "stillness"]);
    tbd::create(
        &root,
        None,
        None,
        "What is behind the red door?",
        None,
        true,
    )
    .unwrap();

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let error = tbd_guard::check_tbd_firewall(&conn, &project_id(&root), &scene_id).unwrap_err();
    assert!(matches!(error, AppError::WorkflowBlockedByProtectedTbd(_)));
}

#[test]
fn allows_compilation_when_no_protected_tbd_open() {
    let (_temp, root, scene_id, _character) =
        scene_with_character(&["speech", "movement", "stillness"]);

    // Unprotected open TBDs and resolved protected TBDs do not block.
    tbd::create(&root, None, None, "Unprotected question", None, false).unwrap();
    let resolved =
        tbd::create(&root, None, None, "Resolved protected question", None, true).unwrap();
    tbd::resolve(&root, &resolved.id, "Answered in the story bible.").unwrap();

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    assert!(tbd_guard::check_tbd_firewall(&conn, &project_id(&root), &scene_id).is_ok());
}

#[test]
fn tbd_on_unrelated_character_does_not_block() {
    let (_temp, root, scene_id, _character) =
        scene_with_character(&["speech", "movement", "stillness"]);
    let other = CanonService::create_entity(&root, CanonEntityType::Character, "Other").unwrap();
    tbd::create(
        &root,
        Some(&other.id),
        None,
        "Other arc question",
        None,
        true,
    )
    .unwrap();

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    assert!(tbd_guard::check_tbd_firewall(&conn, &project_id(&root), &scene_id).is_ok());
}
