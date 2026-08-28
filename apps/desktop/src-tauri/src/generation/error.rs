use crate::error::AppError;

pub fn unavailable(path: &str) -> AppError {
    AppError::GenerationArtifactUnavailable(path.to_string())
}
