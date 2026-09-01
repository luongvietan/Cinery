use super::{RawVideoQaResponse, VideoQaAdapter, VideoQaAdapterError, VideoQaCapabilities};
use crate::qa::models::VideoQaRequest;
use crate::video_qa::evidence::EvidenceMode;
use serde_json::Value;

/// Deterministic Video QA adapter for workflow and normalization tests.
pub struct MockVideoQaAdapter {
    response: Value,
    evidence_mode: EvidenceMode,
}

impl MockVideoQaAdapter {
    pub fn new(response: Value) -> Self {
        Self {
            response,
            evidence_mode: EvidenceMode::DirectVideo,
        }
    }

    pub fn with_evidence_mode(response: Value, evidence_mode: EvidenceMode) -> Self {
        Self {
            response,
            evidence_mode,
        }
    }
}

impl VideoQaAdapter for MockVideoQaAdapter {
    fn id(&self) -> &'static str {
        "mock_video_qa"
    }

    fn adapter_version(&self) -> u32 {
        1
    }

    fn capabilities(&self) -> VideoQaCapabilities {
        VideoQaCapabilities {
            supports_direct_video: true,
            supports_sampled_frames: true,
            supports_multiple_references: true,
            max_media_inputs: 32,
        }
    }

    fn evidence_mode(&self) -> EvidenceMode {
        self.evidence_mode
    }

    fn execution_location(&self) -> String {
        "local".into()
    }

    fn model_id(&self) -> &str {
        "mock-video-qa"
    }

    fn analyze(
        &self,
        _request: &VideoQaRequest,
    ) -> Result<RawVideoQaResponse, VideoQaAdapterError> {
        Ok(RawVideoQaResponse {
            response_text: self.response.to_string(),
            metadata: serde_json::json!({"deterministic": true}),
        })
    }
}
