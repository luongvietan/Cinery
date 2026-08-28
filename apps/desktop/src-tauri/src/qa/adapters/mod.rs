mod mock;
mod multimodal;

pub use mock::MockVisualQaAdapter;
pub use multimodal::OpenAiCompatibleVisualQaAdapter;

use super::models::VisualQaRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualQaCapabilities {
    pub supports_image_analysis: bool,
    pub supports_multiple_references: bool,
    pub max_media_inputs: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawVisualQaResponse {
    pub response_text: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualQaAdapterErrorKind {
    UnsupportedCapability,
    InvalidRequest,
    Authentication,
    Network,
    MalformedResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct VisualQaAdapterError {
    pub kind: VisualQaAdapterErrorKind,
    pub message: String,
    pub diagnostic: Option<String>,
}

impl VisualQaAdapterError {
    pub fn new(kind: VisualQaAdapterErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            diagnostic: None,
        }
    }

    pub fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.diagnostic = Some(diagnostic.into());
        self
    }
}

pub trait VisualQaAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn adapter_version(&self) -> u32;
    fn capabilities(&self) -> VisualQaCapabilities;
    fn execution_location(&self) -> String;
    fn model_id(&self) -> &str;
    fn analyze(
        &self,
        request: &VisualQaRequest,
    ) -> Result<RawVisualQaResponse, VisualQaAdapterError>;
}
