use crate::error::AppError;
use image::ImageFormat;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum bounding box (in pixels) a generated thumbnail is fit into.
/// Aspect ratio is always preserved and images are never upscaled --
/// `DynamicImage::thumbnail` already guarantees both.
const THUMBNAIL_MAX_DIMENSION: u32 = 512;

/// Decodes the image at `source`, fits it within a
/// `THUMBNAIL_MAX_DIMENSION` x `THUMBNAIL_MAX_DIMENSION` box (preserving
/// aspect ratio, never upscaling), and writes the result to `destination`
/// as WebP.
///
/// The write is atomic with respect to `destination`: the thumbnail is
/// encoded to a temporary sibling file first, then renamed into place, so a
/// reader never observes a partially-written thumbnail.
pub fn generate_thumbnail(source: &Path, destination: &Path) -> Result<(), AppError> {
    let image = image::ImageReader::open(source)
        .map_err(|e| AppError::FileSystem(e.to_string()))?
        .with_guessed_format()
        .map_err(|e| AppError::FileSystem(e.to_string()))?
        .decode()
        .map_err(|e| AppError::ImageProcessing(e.to_string()))?;

    // `DynamicImage::thumbnail` scales an image to fit within the given box
    // on whichever axis is tightest, which means it will happily scale a
    // *smaller* image up to fill the box. Only shrink -- an image already
    // within bounds on both axes is used as-is, unmodified.
    let exceeds_bounds =
        image.width() > THUMBNAIL_MAX_DIMENSION || image.height() > THUMBNAIL_MAX_DIMENSION;
    let thumbnail = if exceeds_bounds {
        image.thumbnail(THUMBNAIL_MAX_DIMENSION, THUMBNAIL_MAX_DIMENSION)
    } else {
        image
    };

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::FileSystem(e.to_string()))?;
    }

    let tmp_path = with_tmp_suffix(destination);
    let save_result = thumbnail.save_with_format(&tmp_path, ImageFormat::WebP);
    if let Err(e) = save_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(AppError::ImageProcessing(e.to_string()));
    }

    fs::rename(&tmp_path, destination).map_err(|e| AppError::FileSystem(e.to_string()))?;

    Ok(())
}

fn with_tmp_suffix(path: &Path) -> PathBuf {
    let mut os_string: OsString = path.as_os_str().to_os_string();
    os_string.push(".tmp");
    PathBuf::from(os_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use tempfile::tempdir;

    fn write_png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
        let path = dir.join(name);
        let image: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(width, height, Rgba([50, 60, 70, 255]));
        image.save(&path).unwrap();
        path
    }

    #[test]
    fn downscales_large_image_preserving_aspect_ratio() {
        let temp = tempdir().unwrap();
        let source = write_png(temp.path(), "large.png", 1024, 512);
        let destination = temp.path().join("thumb.webp");

        generate_thumbnail(&source, &destination).unwrap();

        let decoded = image::ImageReader::open(&destination)
            .unwrap()
            .with_guessed_format()
            .unwrap();
        assert_eq!(decoded.format(), Some(ImageFormat::WebP));
        let decoded = decoded.decode().unwrap();
        assert_eq!(decoded.width(), 512);
        assert_eq!(decoded.height(), 256);
    }

    #[test]
    fn never_upscales_a_small_image() {
        let temp = tempdir().unwrap();
        let source = write_png(temp.path(), "small.png", 200, 100);
        let destination = temp.path().join("thumb.webp");

        generate_thumbnail(&source, &destination).unwrap();

        let decoded = image::ImageReader::open(&destination)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(decoded.width(), 200);
        assert_eq!(decoded.height(), 100);
    }

    #[test]
    fn leaves_no_temporary_file_behind_on_success() {
        let temp = tempdir().unwrap();
        let source = write_png(temp.path(), "clean.png", 64, 64);
        let destination = temp.path().join("thumb.webp");

        generate_thumbnail(&source, &destination).unwrap();

        assert!(destination.exists());
        assert!(!with_tmp_suffix(&destination).exists());
    }
}
