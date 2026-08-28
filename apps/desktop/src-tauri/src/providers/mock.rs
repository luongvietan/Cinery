use super::adapter::GenerationProvider;
use super::error::{ProviderError, ProviderErrorKind};
use super::model::*;
use chrono::Utc;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct MockImageProvider {
    statuses: Arc<Mutex<Vec<ProviderJobStatus>>>,
    result: ProviderResult,
    submitted: Arc<Mutex<Vec<ProviderExecutionRequest>>>,
    cancelled: Arc<Mutex<Vec<String>>>,
}

impl MockImageProvider {
    pub fn new(statuses: Vec<ProviderJobStatus>) -> Self {
        Self {
            statuses: Arc::new(Mutex::new(statuses)),
            result: ProviderResult {
                outputs: (1..=4).map(|ordinal| ProviderOutput {
                    uri: format!("mock://face-lock-{ordinal}.png"),
                    mime_type: "image/png".into(),
                    filename: Some(format!("face-lock-{ordinal}.png")),
                }).collect(),
                provider_reported_model: Some("mock-image-v1".into()),
                metadata: serde_json::json!({"fixture": true}),
            },
            submitted: Arc::new(Mutex::new(Vec::new())),
            cancelled: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn submitted_requests(&self) -> Vec<ProviderExecutionRequest> {
        self.submitted.lock().unwrap().clone()
    }

    pub fn cancelled_jobs(&self) -> Vec<String> {
        self.cancelled.lock().unwrap().clone()
    }
}

impl Default for MockImageProvider {
    fn default() -> Self {
        Self::new(vec![ProviderJobStatus { lifecycle: ProviderLifecycle::Succeeded, progress_percent: Some(100), diagnostic: None }])
    }
}

impl GenerationProvider for MockImageProvider {
    fn id(&self) -> &'static str { "mock" }
    fn adapter_version(&self) -> u32 { 1 }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            media_types: vec![ProviderMediaType::Image],
            supports_seed: true,
            supports_negative_prompt: false,
            supports_reference_image: true,
            supports_image_edit: true,
            supports_multiple_reference_images: true,
            supports_image_to_video: false,
            supports_cancel: true,
            supports_progress: true,
            supported_aspect_ratios: vec![],
            supported_models: vec!["mock-image-v1".into()],
        }
    }
    fn submit(&self, request: &ProviderExecutionRequest) -> Result<ProviderSubmission, ProviderError> {
        self.capabilities().supports(request).map_err(|reason| ProviderError::new(ProviderErrorKind::UnsupportedCapability, reason))?;
        self.submitted.lock().unwrap().push(request.clone());
        Ok(ProviderSubmission {
            job: ProviderJobRef {
                provider_id: self.id().into(),
                provider_job_id: format!("mock:{}", request.idempotency_key),
                run_id: request.run_id.clone(),
                step_id: request.step_id.clone(),
                submission_id: request.idempotency_key.clone(),
                submitted_at: Utc::now().to_rfc3339(),
            },
            lifecycle: ProviderLifecycle::Submitted,
        })
    }
    fn poll(&self, _: &ProviderJobRef) -> Result<ProviderJobStatus, ProviderError> {
        Ok(self.statuses.lock().unwrap().pop().unwrap_or(ProviderJobStatus { lifecycle: ProviderLifecycle::Succeeded, progress_percent: Some(100), diagnostic: None }))
    }
    fn cancel(&self, job: &ProviderJobRef) -> Result<ProviderCancellationResult, ProviderError> {
        self.cancelled.lock().unwrap().push(job.provider_job_id.clone());
        Ok(ProviderCancellationResult { provider_job_id: job.provider_job_id.clone(), lifecycle: ProviderLifecycle::Cancelled })
    }
    fn fetch_result(&self, _: &ProviderJobRef) -> Result<ProviderResult, ProviderError> {
        Ok(self.result.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::execution::{ExecutionMediaType, ExecutionTask};

    fn request() -> ProviderExecutionRequest {
        ProviderExecutionRequest {
            run_id: "run-1".into(), step_id: "execute".into(), compiled_request_id: "compiled-1".into(),
            media_type: ExecutionMediaType::Image, task: ExecutionTask::CharacterFaceLock,
            prompt: "face lock".into(), references: vec![], constraints: vec![],
            expected_output: serde_json::from_value(serde_json::json!({
                "assetType":"face_lock","mediaType":"image","desiredStatus":"candidate","ownerEntityInputRef":"characterEntityId"
            })).unwrap(),
            generation_parameters: ProviderGenerationParameters::default(),
            selected_provider: "mock".into(), selected_model: "mock-image-v1".into(), idempotency_key: "run-1:execute:1".into(),
        }
    }

    #[test]
    fn mock_provider_contract_is_deterministic_and_cancellable() {
        let provider = MockImageProvider::default();
        let submission = provider.submit(&request()).unwrap();
        assert_eq!(submission.lifecycle, ProviderLifecycle::Submitted);
        assert_eq!(provider.poll(&submission.job).unwrap().lifecycle, ProviderLifecycle::Succeeded);
        let result = provider.fetch_result(&submission.job).unwrap();
        assert_eq!(result.outputs.len(), 4);
        assert_eq!(result.outputs[0].uri, "mock://face-lock-1.png");
        assert_eq!(provider.cancel(&submission.job).unwrap().lifecycle, ProviderLifecycle::Cancelled);
        assert_eq!(provider.cancelled_jobs(), vec!["mock:run-1:execute:1"]);
    }
}
