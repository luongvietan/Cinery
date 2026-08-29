use cinematic_desktop_lib::generation::storage::{
    materialize_image, read_and_verify, MaterializedArtifact,
};
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::io::Cursor;
use tempfile::tempdir;

fn png_bytes() -> Vec<u8> {
    let image =
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 3, image::Rgba([24, 24, 24, 255])));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn materializes_project_relative_png_and_verifies_hash_and_dimensions() {
    let root = tempdir().unwrap();
    let artifact: MaterializedArtifact =
        materialize_image(root.path(), "run-1", "attempt-1", 1, &png_bytes()).unwrap();

    assert_eq!(artifact.storage_path, "generated/run-1/attempt-1/0001.png");
    assert_eq!(artifact.mime_type, "image/png");
    assert_eq!(artifact.width, Some(2));
    assert_eq!(artifact.height, Some(3));
    assert_eq!(
        read_and_verify(root.path(), &artifact.storage_path, &artifact.sha256).unwrap(),
        png_bytes()
    );
}

#[test]
fn verification_reports_missing_and_corrupt_artifacts() {
    let root = tempdir().unwrap();
    let artifact = materialize_image(root.path(), "run-1", "attempt-1", 1, &png_bytes()).unwrap();

    std::fs::remove_file(root.path().join(&artifact.storage_path)).unwrap();
    let missing =
        read_and_verify(root.path(), &artifact.storage_path, &artifact.sha256).unwrap_err();
    assert_eq!(missing.code(), "GENERATION_ARTIFACT_UNAVAILABLE");

    let artifact = materialize_image(root.path(), "run-1", "attempt-2", 1, &png_bytes()).unwrap();
    std::fs::write(root.path().join(&artifact.storage_path), b"not-an-image").unwrap();
    let corrupt =
        read_and_verify(root.path(), &artifact.storage_path, &artifact.sha256).unwrap_err();
    assert_eq!(corrupt.code(), "GENERATION_ARTIFACT_INTEGRITY_MISMATCH");
}
