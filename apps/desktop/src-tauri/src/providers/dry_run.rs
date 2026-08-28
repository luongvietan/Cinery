use super::adapter::GenerationProvider;
use super::error::{ProviderError, ProviderErrorKind};
use super::model::*;
use chrono::Utc;

pub struct DryRunProvider;

impl GenerationProvider for DryRunProvider {
    fn id(&self) -> &'static str { "dry_run" }
    fn adapter_version(&self) -> u32 { 1 }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            media_types: vec![ProviderMediaType::Image],
            supports_seed: true,
            supports_negative_prompt: true,
            supports_reference_image: true,
            supports_image_edit: true,
            supports_multiple_reference_images: true,
            supports_image_to_video: false,
            supports_cancel: true,
            supports_progress: true,
            supported_aspect_ratios: vec![],
            supported_models: vec!["dry-run-v1".into()],
        }
    }
    fn submit(&self, request: &ProviderExecutionRequest) -> Result<ProviderSubmission, ProviderError> {
        self.capabilities().supports(request).map_err(|reason| {
            ProviderError::new(ProviderErrorKind::UnsupportedCapability, reason)
        })?;
        Ok(ProviderSubmission {
            job: ProviderJobRef {
                provider_id: self.id().into(),
                provider_job_id: format!("dry-run:{}", request.idempotency_key),
                run_id: request.run_id.clone(),
                step_id: request.step_id.clone(),
                submission_id: request.idempotency_key.clone(),
                submitted_at: Utc::now().to_rfc3339(),
            },
            lifecycle: ProviderLifecycle::Submitted,
        })
    }
    fn poll(&self, _: &ProviderJobRef) -> Result<ProviderJobStatus, ProviderError> {
        Ok(ProviderJobStatus { lifecycle: ProviderLifecycle::Succeeded, progress_percent: Some(100), diagnostic: None })
    }
    fn cancel(&self, job: &ProviderJobRef) -> Result<ProviderCancellationResult, ProviderError> {
        Ok(ProviderCancellationResult { provider_job_id: job.provider_job_id.clone(), lifecycle: ProviderLifecycle::Cancelled })
    }
    fn fetch_result(&self, job: &ProviderJobRef) -> Result<ProviderResult, ProviderError> {
        Ok(ProviderResult {
            outputs: vec![ProviderOutput { uri: format!("dry-run://{}", job.provider_job_id), mime_type: "image/png".into(), filename: Some("dry-run.png".into()) }],
            provider_reported_model: Some("dry-run-v1".into()),
            metadata: serde_json::json!({"deterministic": true}),
        })
    }
}
