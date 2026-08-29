use crate::error::AppError;
use image::{guess_format, GenericImageView, ImageFormat};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedArtifact {
    pub storage_path: String,
    pub mime_type: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub byte_size: i64,
    pub sha256: String,
}

pub fn materialize_image(
    project_root: &Path,
    workflow_run_id: &str,
    provider_attempt_id: &str,
    ordinal: i64,
    bytes: &[u8],
) -> Result<MaterializedArtifact, AppError> {
    if ordinal < 1 {
        return Err(AppError::GenerationArtifactCaptureFailed(
            "artifact ordinal must be positive".into(),
        ));
    }
    validate_component(workflow_run_id)?;
    validate_component(provider_attempt_id)?;

    let format = guess_format(bytes).map_err(|error| {
        AppError::GenerationArtifactCaptureFailed(format!("unsupported image payload: {error}"))
    })?;
    let (mime_type, extension) = format_metadata(format)?;
    let decoded = image::load_from_memory(bytes).map_err(|error| {
        AppError::GenerationArtifactCaptureFailed(format!("image decode failed: {error}"))
    })?;
    let (width, height) = decoded.dimensions();

    let relative = PathBuf::from("generated")
        .join(workflow_run_id)
        .join(provider_attempt_id)
        .join(format!("{ordinal:04}.{extension}"));
    let destination = project_root.join(&relative);
    let parent = destination.parent().ok_or_else(|| {
        AppError::GenerationArtifactCaptureFailed("artifact destination has no parent".into())
    })?;
    fs::create_dir_all(parent).map_err(io_error)?;

    let temp = parent.join(format!(".{ordinal:04}.{}.tmp", ulid::Ulid::new()));
    let write_result = write_and_flush(&temp, bytes);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temp, &destination) {
        let _ = fs::remove_file(&temp);
        return Err(AppError::GenerationArtifactCaptureFailed(format!(
            "could not finalize artifact: {error}"
        )));
    }

    Ok(MaterializedArtifact {
        storage_path: to_forward_slash(&relative),
        mime_type: mime_type.into(),
        width: Some(width as i64),
        height: Some(height as i64),
        byte_size: bytes.len() as i64,
        sha256: sha256(bytes),
    })
}

pub fn read_and_verify(
    project_root: &Path,
    storage_path: &str,
    expected_sha256: &str,
) -> Result<Vec<u8>, AppError> {
    let relative = validate_relative_storage_path(storage_path)?;
    let absolute = project_root.join(&relative);
    let bytes = fs::read(&absolute).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::GenerationArtifactUnavailable(storage_path.into())
        } else {
            AppError::GenerationArtifactUnavailable(format!("{storage_path}: {error}"))
        }
    })?;
    let actual = sha256(&bytes);
    if actual != expected_sha256 {
        return Err(AppError::GenerationArtifactIntegrityMismatch(
            storage_path.into(),
        ));
    }
    Ok(bytes)
}

fn write_and_flush(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    Ok(())
}

fn format_metadata(format: ImageFormat) -> Result<(&'static str, &'static str), AppError> {
    match format {
        ImageFormat::Png => Ok(("image/png", "png")),
        ImageFormat::Jpeg => Ok(("image/jpeg", "jpg")),
        ImageFormat::WebP => Ok(("image/webp", "webp")),
        other => Err(AppError::GenerationArtifactCaptureFailed(format!(
            "unsupported image format: {other:?}"
        ))),
    }
}

fn validate_component(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(AppError::GenerationArtifactCaptureFailed(
            "artifact identity contains an unsafe path component".into(),
        ));
    }
    Ok(())
}

fn validate_relative_storage_path(value: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(AppError::GenerationArtifactUnavailable(value.into()));
    }
    Ok(path.to_path_buf())
}

fn sha256(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

fn to_forward_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::GenerationArtifactCaptureFailed(error.to_string())
}
