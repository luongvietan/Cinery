use super::{RawVisualQaResponse, VisualQaAdapter, VisualQaAdapterError, VisualQaCapabilities};
use crate::qa::models::VisualQaRequest;
use serde_json::Value;

pub struct MockVisualQaAdapter {
    response: Value,
}

impl MockVisualQaAdapter {
    pub fn new(response: Value) -> Self {
        Self { response }
    }
}

impl VisualQaAdapter for MockVisualQaAdapter {
    fn id(&self) -> &'static str {
        "mock_visual_qa"
    }

    fn adapter_version(&self) -> u32 {
        1
    }

    fn capabilities(&self) -> VisualQaCapabilities {
        VisualQaCapabilities {
            supports_image_analysis: true,
            supports_multiple_references: true,
            max_media_inputs: 32,
        }
    }

    fn execution_location(&self) -> String {
        "local".into()
    }

    fn model_id(&self) -> &str {
        "mock-vlm"
    }

    fn analyze(
        &self,
        _request: &VisualQaRequest,
    ) -> Result<RawVisualQaResponse, VisualQaAdapterError> {
        Ok(RawVisualQaResponse {
            response_text: self.response.to_string(),
            metadata: serde_json::json!({"deterministic": true}),
        })
    }
}
