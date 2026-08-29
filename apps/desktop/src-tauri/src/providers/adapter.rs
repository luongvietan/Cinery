use super::error::ProviderError;
use super::model::{
    ProviderCancellationResult, ProviderCapabilities, ProviderExecutionRequest, ProviderJobRef,
    ProviderJobStatus, ProviderResult, ProviderSubmission,
};

/// Polling behavior a provider suggests for its submission loop. Async
/// providers override this with their configured interval/timeout; the
/// default applies to synchronous adapters (whose polls resolve instantly).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollingSpec {
    pub interval: std::time::Duration,
    pub timeout: std::time::Duration,
}

impl Default for PollingSpec {
    fn default() -> Self {
        Self {
            interval: std::time::Duration::from_secs(2),
            timeout: std::time::Duration::from_secs(600),
        }
    }
}

pub trait GenerationProvider: Send + Sync {
    fn id(&self) -> &str;
    fn adapter_version(&self) -> u32;
    fn capabilities(&self) -> ProviderCapabilities;
    fn submit(
        &self,
        request: &ProviderExecutionRequest,
    ) -> Result<ProviderSubmission, ProviderError>;
    fn poll(&self, job: &ProviderJobRef) -> Result<ProviderJobStatus, ProviderError>;
    fn cancel(&self, job: &ProviderJobRef) -> Result<ProviderCancellationResult, ProviderError>;
    fn fetch_result(&self, job: &ProviderJobRef) -> Result<ProviderResult, ProviderError>;
    /// Suggested polling cadence for this provider's jobs.
    fn polling_spec(&self) -> PollingSpec {
        PollingSpec::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::error::{ProviderError, ProviderErrorKind};
    use crate::providers::model::{ProviderLifecycle, ProviderMediaType};

    struct FakeProvider;

    impl GenerationProvider for FakeProvider {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn adapter_version(&self) -> u32 {
            1
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                media_types: vec![ProviderMediaType::Image],
                supports_seed: false,
                supports_negative_prompt: false,
                supports_reference_image: false,
                supports_image_edit: false,
                supports_multiple_reference_images: false,
                supports_image_to_video: false,
                supports_cancel: false,
                supports_progress: false,
                supported_aspect_ratios: vec![],
                supported_models: vec![],
                max_reference_images: None,
            }
        }
        fn submit(
            &self,
            _: &ProviderExecutionRequest,
        ) -> Result<ProviderSubmission, ProviderError> {
            Err(ProviderError::new(
                ProviderErrorKind::InvalidRequest,
                "fixture",
            ))
        }
        fn poll(&self, _: &ProviderJobRef) -> Result<ProviderJobStatus, ProviderError> {
            Ok(ProviderJobStatus {
                lifecycle: ProviderLifecycle::Unknown,
                progress_percent: None,
                diagnostic: None,
            })
        }
        fn cancel(
            &self,
            job: &ProviderJobRef,
        ) -> Result<ProviderCancellationResult, ProviderError> {
            Ok(ProviderCancellationResult {
                provider_job_id: job.provider_job_id.clone(),
                lifecycle: ProviderLifecycle::Cancelled,
            })
        }
        fn fetch_result(&self, _: &ProviderJobRef) -> Result<ProviderResult, ProviderError> {
            Ok(ProviderResult {
                outputs: vec![],
                provider_reported_model: None,
                metadata: serde_json::Value::Null,
            })
        }
    }

    #[test]
    fn contract_exposes_versioned_adapter_and_unknown_states() {
        let provider = FakeProvider;
        assert_eq!(provider.id(), "fake");
        assert_eq!(provider.adapter_version(), 1);
        assert_eq!(
            provider
                .poll(&ProviderJobRef {
                    provider_id: "fake".into(),
                    provider_job_id: "job".into(),
                    run_id: "run".into(),
                    step_id: "step".into(),
                    submission_id: "submission".into(),
                    submitted_at: "now".into(),
                })
                .unwrap()
                .lifecycle,
            ProviderLifecycle::Unknown
        );
    }
}
