use crate::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum QaError {
    #[error("QA run was not found")]
    RunNotFound,
    #[error("QA check was not found")]
    CheckNotFound,
    #[error("Visual QA data is invalid: {0}")]
    InvalidData(String),
}

impl From<QaError> for AppError {
    fn from(error: QaError) -> Self {
        match error {
            QaError::RunNotFound => AppError::QaRunNotFound,
            QaError::CheckNotFound => AppError::QaCheckNotFound,
            QaError::InvalidData(message) => AppError::InvalidQaData(message),
        }
    }
}
