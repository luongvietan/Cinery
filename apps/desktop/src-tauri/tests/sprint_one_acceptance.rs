use std::path::{Path, PathBuf};

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;

/// Writes a solid-color PNG at `root/name` and returns its path. Used as a
/// stand-in for a user's original source file, distinct from anything the
/// project itself manages.
fn test_image(root: &Path, name: &str, pixel: [u8; 4]) -> PathBuf {
    let path = root.join(name);
    let image: image::RgbaImage = image::ImageBuffer::from_pixel(32, 32, image::Rgba(pixel));
    image.save(&path).unwrap();
    path
}

#[test]
fn sprint_one_project_and_asset_state_survives_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let project = ProjectService::create(root, "Red Door").unwrap();
    let asset = AssetService::create_asset(root, "face_lock", "MARA-FACE", None).unwrap();

    let first_source = test_image(root, "first.png", [10, 20, 30, 255]);
    let second_source = test_image(root, "second.png", [40, 50, 60, 255]);

    let first = AssetService::import_asset_version(root, &asset.id, &first_source, None).unwrap();
    let second = AssetService::import_asset_version(root, &asset.id, &second_source, None).unwrap();

    AssetService::promote_asset_version(root, &first.id).unwrap();
    AssetService::promote_asset_version(root, &second.id).unwrap();

    // Simulate the application closing: drop everything held from the
    // first "session" before reopening from scratch.
    drop(project);

    let reopened = ProjectService::open(root).unwrap();
    let reloaded = AssetService::get_asset_with_versions(root, &asset.id).unwrap();

    assert_eq!(reopened.name, "Red Door");
    assert_eq!(
        reloaded.asset.canonical_version_id.as_deref(),
        Some(second.id.as_str())
    );

    let v1 = reloaded.versions.iter().find(|v| v.id == first.id).unwrap();
    let v2 = reloaded
        .versions
        .iter()
        .find(|v| v.id == second.id)
        .unwrap();

    assert_eq!(v1.status, "superseded");
    assert_eq!(v2.status, "canonical");

    assert!(root.join(&v1.file_path).exists());
    assert!(root.join(&v2.file_path).exists());
    assert!(root.join(&v1.thumbnail_path).exists());
    assert!(root.join(&v2.thumbnail_path).exists());
    assert!(first_source.exists());
    assert!(second_source.exists());

    // Verify the canonical invariant directly against SQLite, independent
    // of the service layer's own reporting: exactly one canonical version
    // for this asset, and promotion must not have deleted any version row.
    let conn = db::open_connection(&root.join("project.db")).unwrap();

    let canonical_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM asset_versions WHERE asset_id = ?1 AND status = 'canonical'",
            rusqlite::params![asset.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(canonical_count, 1);

    let total_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM asset_versions WHERE asset_id = ?1",
            rusqlite::params![asset.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(total_count, 2);
}
