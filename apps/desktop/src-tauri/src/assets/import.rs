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
    pub width: u32,
    pub height: u32,
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
    let decoded = reader
        .decode()
        .map_err(|_| AppError::UnsupportedImageFormat)?;

    let sha256 = hex_sha256(&bytes);

    Ok(InspectedImage {
        mime_type,
        extension,
        byte_size: bytes.len() as u64,
        width: decoded.width(),
        height: decoded.height(),
        sha256,
    })
}

/// Inspects a candidate video file before it is imported as an asset
/// version. Videos are validated by container signature only (the "ftyp"
/// brand box of an ISO-BMFF/MP4 file); frames are never decoded here.
pub fn inspect_video(source: &Path) -> Result<InspectedImage, AppError> {
    let bytes = fs::read(source).map_err(|e| AppError::FileSystem(e.to_string()))?;
    if bytes.len() <= 12 || &bytes[4..8] != b"ftyp" {
        return Err(AppError::UnsupportedVideoFormat);
    }
    let sha256 = hex_sha256(&bytes);
    Ok(InspectedImage {
        mime_type: "video/mp4",
        extension: "mp4",
        byte_size: bytes.len() as u64,
        width: 0,
        height: 0,
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
        image.save_with_format(&path, ImageFormat::Png).unwrap();
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
        assert_eq!(inspected.width, 8);
        assert_eq!(inspected.height, 8);
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
    fn recognizes_a_minimal_mp4_by_container_signature() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("clip.mp4");
        // Minimal ISO-BMFF header: 32-bit size, "ftyp" brand, minor version,
        // compatible brand.
        let header: [u8; 16] = [
            0, 0, 0, 24, b'f', b't', b'y', b'p', 0, 0, 0, 0, b'i', b's', b'o', b'm',
        ];
        fs::write(&source, header).unwrap();

        let inspected = inspect_video(&source).unwrap();

        assert_eq!(inspected.mime_type, "video/mp4");
        assert_eq!(inspected.extension, "mp4");
        assert_eq!(inspected.width, 0);
        assert_eq!(inspected.height, 0);
        assert_eq!(inspected.sha256.len(), 64);
    }

    #[test]
    fn rejects_non_video_content_with_a_video_specific_error() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("fake.mp4");
        fs::write(&source, b"not a video at all").unwrap();

        let error = inspect_video(&source).unwrap_err();

        assert!(matches!(error, AppError::UnsupportedVideoFormat));
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
