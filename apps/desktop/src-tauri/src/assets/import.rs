use crate::error::AppError;
use image::ImageFormat;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Cursor;
use std::path::Path;

/// Result of inspecting a candidate source file before it is imported as an
/// asset version.
#[derive(Debug)]
pub struct InspectedImage {
    pub mime_type: &'static str,
    pub extension: &'static str,
    pub byte_size: u64,
    pub sha256: String,
}

/// Reads `source` from disk, identifies its real image format by decoding
/// its content (never by trusting the file extension), and hashes the
/// original bytes.
///
/// Only PNG, JPEG, and WebP are supported in Sprint 1. Anything else --
/// including a file whose extension claims to be one of these but whose
/// content does not actually decode as one -- is rejected with
/// `AppError::UnsupportedImageFormat`.
pub fn inspect_image(source: &Path) -> Result<InspectedImage, AppError> {
    let bytes = fs::read(source).map_err(|e| AppError::FileSystem(e.to_string()))?;

    let reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let (mime_type, extension) = match reader.format() {
        Some(ImageFormat::Png) => ("image/png", "png"),
        Some(ImageFormat::Jpeg) => ("image/jpeg", "jpg"),
        Some(ImageFormat::WebP) => ("image/webp", "webp"),
        _ => return Err(AppError::UnsupportedImageFormat),
    };

    // A correct-looking signature is not enough on its own -- make sure the
    // content actually decodes before accepting it.
    reader
        .decode()
        .map_err(|_| AppError::UnsupportedImageFormat)?;

    let sha256 = hex_sha256(&bytes);

    Ok(InspectedImage {
        mime_type,
        extension,
        byte_size: bytes.len() as u64,
        sha256,
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use tempfile::tempdir;

    fn write_png(dir: &Path, name: &str, width: u32, height: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        let image: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(width, height, Rgba([10, 20, 30, 255]));
        // Encode explicitly as PNG regardless of the file's extension --
        // some of these tests intentionally use a non-`.png` name to prove
        // that `inspect_image` identifies format by content, not name.
        image
            .save_with_format(&path, ImageFormat::Png)
            .unwrap();
        path
    }

    #[test]
    fn recognizes_a_real_png_regardless_of_extension() {
        let temp = tempdir().unwrap();
        let source = write_png(temp.path(), "picture.dat", 8, 8);

        let inspected = inspect_image(&source).unwrap();

        assert_eq!(inspected.mime_type, "image/png");
        assert_eq!(inspected.extension, "png");
        assert!(inspected.byte_size > 0);
        assert_eq!(inspected.sha256.len(), 64);
    }

    #[test]
    fn rejects_non_image_content() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("notes.txt");
        fs::write(&source, b"not an image").unwrap();

        let error = inspect_image(&source).unwrap_err();

        assert!(matches!(error, AppError::UnsupportedImageFormat));
    }

    #[test]
    fn hashes_the_original_bytes_deterministically() {
        let temp = tempdir().unwrap();
        let first = write_png(temp.path(), "first.png", 4, 4);
        let second = write_png(temp.path(), "second.png", 4, 4);

        let a = inspect_image(&first).unwrap();
        let b = inspect_image(&second).unwrap();

        assert_eq!(a.sha256, b.sha256);
    }
}
