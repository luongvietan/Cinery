use cinematic_desktop_lib::db;
use cinematic_desktop_lib::generation::recovery::quarantine_orphan_generated_files;
use cinematic_desktop_lib::generation::storage::materialize_image;
use cinematic_desktop_lib::project::service::ProjectService;
use image::{ImageBuffer, ImageFormat, Rgba};
use std::io::Cursor;
use tempfile::tempdir;

#[test]
fn finalized_file_without_artifact_row_is_quarantined_safely() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("recovery-project");
    ProjectService::create(&root, "Recovery Project").unwrap();
    let _conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let mut bytes = Cursor::new(Vec::new());
    ImageBuffer::from_pixel(2, 2, Rgba([10u8, 20u8, 30u8, 255u8]))
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();

    let materialized = materialize_image(&root, "run-1", "attempt-1", 1, &bytes.into_inner()).unwrap();
    assert!(root.join(&materialized.storage_path).exists());

    assert_eq!(quarantine_orphan_generated_files(&root).unwrap(), 1);
    assert!(!root.join(&materialized.storage_path).exists());
    assert!(root.join("generated/quarantine/run-1/attempt-1/0001.png").exists());
}
