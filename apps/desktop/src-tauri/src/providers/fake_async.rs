//! A deterministic fake async video provider. It proves the full runtime
//! supports asynchronous submit → job id → poll → completed → video URL
//! without any live credentials. The "video" it returns is a minimal
//! ISO-BMFF (ftyp) payload so downstream media handling sees real MP4
//! framing.

use super::adapter::GenerationProvider;
use super::error::{ProviderError, ProviderErrorKind};
use super::model::{
    ProviderCancellationResult, ProviderCapabilities, ProviderExecutionRequest, ProviderJobRef,
    ProviderJobStatus, ProviderLifecycle, ProviderMediaType, ProviderOutput, ProviderResult,
    ProviderSubmission,
};
use chrono::Utc;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Minimal MP4 framing: a 24-byte `ftyp` box. Capture only requires valid
/// ISO-BMFF magic; these bytes are deterministic and tiny.
const FAKE_MP4_BYTES: &[u8] = &[
    0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'm', b'p', b'4', b'2', 0x00, 0x00, 0x00,
    0x00, b'm', b'p', b'4', b'2', b'i', b's', b'o', b'm',
];

pub const FAKE_ASYNC_VIDEO_PROVIDER_ID: &str = "fake_async_video";
pub const FAKE_ASYNC_VIDEO_MODEL: &str = "fake-video-v1";

pub struct FakeAsyncVideoProvider {
    /// Polls observed per job: the job completes on its second poll.
    polls: Mutex<BTreeMap<String, u32>>,
}

impl Default for FakeAsyncVideoProvider {
    fn default() -> Self {
        Self {
            polls: Mutex::new(BTreeMap::new()),
        }
    }
}

impl FakeAsyncVideoProvider {
    fn error(&self, kind: ProviderErrorKind, message: impl Into<String>) -> ProviderError {
        ProviderError::new(kind, message)
    }
}

impl GenerationProvider for FakeAsyncVideoProvider {
    fn id(&self) -> &str {
        FAKE_ASYNC_VIDEO_PROVIDER_ID
    }

    fn adapter_version(&self) -> u32 {
        1
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            media_types: vec![ProviderMediaType::Video],
            supports_seed: false,
            supports_negative_prompt: false,
            supports_reference_image: false,
            supports_image_edit: false,
            supports_multiple_reference_images: false,
            supports_image_to_video: false,
            supports_cancel: false,
            supports_progress: true,
            supported_aspect_ratios: vec![],
            supported_models: vec![FAKE_ASYNC_VIDEO_MODEL.into()],
            max_reference_images: None,
        }
    }

    fn submit(
        &self,
        request: &ProviderExecutionRequest,
    ) -> Result<ProviderSubmission, ProviderError> {
        self.capabilities()
            .supports(request)
            .map_err(|reason| self.error(ProviderErrorKind::UnsupportedCapability, reason))?;
        let job_id = format!("fake-job-{}", request.idempotency_key);
        self.polls.lock().unwrap().insert(job_id.clone(), 0);
        Ok(ProviderSubmission {
            job: ProviderJobRef {
                provider_id: self.id().to_string(),
                provider_job_id: job_id,
                run_id: request.run_id.clone(),
                step_id: request.step_id.clone(),
                submission_id: request.idempotency_key.clone(),
                submitted_at: Utc::now().to_rfc3339(),
            },
            lifecycle: ProviderLifecycle::Submitted,
        })
    }

    fn poll(&self, job: &ProviderJobRef) -> Result<ProviderJobStatus, ProviderError> {
        let mut polls = self.polls.lock().unwrap();
        let count = polls.entry(job.provider_job_id.clone()).or_insert(0);
        *count += 1;
        if *count >= 2 {
            Ok(ProviderJobStatus {
                lifecycle: ProviderLifecycle::Succeeded,
                progress_percent: Some(100),
                diagnostic: None,
            })
        } else {
            Ok(ProviderJobStatus {
                lifecycle: ProviderLifecycle::Running,
                progress_percent: Some(50),
                diagnostic: None,
            })
        }
    }

    fn cancel(&self, job: &ProviderJobRef) -> Result<ProviderCancellationResult, ProviderError> {
        Ok(ProviderCancellationResult {
            provider_job_id: job.provider_job_id.clone(),
            lifecycle: ProviderLifecycle::Cancelled,
        })
    }

    fn fetch_result(&self, job: &ProviderJobRef) -> Result<ProviderResult, ProviderError> {
        let polls = self.polls.lock().unwrap();
        let count = polls.get(&job.provider_job_id).copied().unwrap_or(0);
        if count < 2 {
            return Err(self.error(
                ProviderErrorKind::RemoteJobNotFound,
                "the fake video job is not complete yet",
            ));
        }
        Ok(ProviderResult {
            outputs: vec![ProviderOutput {
                uri: format!(
                    "data:video/mp4;base64,{}",
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, FAKE_MP4_BYTES)
                ),
                mime_type: "video/mp4".into(),
                filename: Some("fake-video.mp4".into()),
            }],
            provider_reported_model: Some(FAKE_ASYNC_VIDEO_MODEL.into()),
            metadata: serde_json::json!({"response": "normalized"}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::execution::{ExecutionMediaType, ExecutionTask};

    fn request() -> ProviderExecutionRequest {
        ProviderExecutionRequest {
            run_id: "run-1".into(),
            step_id: "execute".into(),
            compiled_request_id: "compiled-1".into(),
            media_type: ExecutionMediaType::Video,
            task: ExecutionTask::SceneVideo,
            prompt: "a slow pan".into(),
            references: vec![],
            constraints: vec![],
            expected_output: serde_json::from_value(serde_json::json!({
                "assetType": "video", "mediaType": "video",
                "desiredStatus": "candidate", "ownerEntityInputRef": "sceneId"
            }))
            .unwrap(),
            generation_parameters: Default::default(),
            selected_provider: FAKE_ASYNC_VIDEO_PROVIDER_ID.into(),
            selected_model: FAKE_ASYNC_VIDEO_MODEL.into(),
            idempotency_key: "run-1:execute:1".into(),
            reference_attachments: Vec::new(),
        }
    }

    #[test]
    fn submit_reports_a_job_and_polling_completes_it() {
        let provider = FakeAsyncVideoProvider::default();
        let submission = provider.submit(&request()).unwrap();
        assert_eq!(submission.lifecycle, ProviderLifecycle::Submitted);
        assert!(submission.job.provider_job_id.starts_with("fake-job-"));

        // First poll: still running with progress.
        let status = provider.poll(&submission.job).unwrap();
        assert_eq!(status.lifecycle, ProviderLifecycle::Running);
        assert_eq!(status.progress_percent, Some(50));

        // Second poll: completed; result carries an MP4 data URI.
        let status = provider.poll(&submission.job).unwrap();
        assert_eq!(status.lifecycle, ProviderLifecycle::Succeeded);
        let result = provider.fetch_result(&submission.job).unwrap();
        assert_eq!(result.outputs.len(), 1);
        assert!(result.outputs[0].uri.starts_with("data:video/mp4;base64,"));
    }
}
