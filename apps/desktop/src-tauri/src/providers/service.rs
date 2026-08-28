use super::error::{ProviderError, ProviderErrorKind};
use super::model::*;
use super::openai::OpenAiImageProvider;
use super::registry::ProviderRegistry;
use crate::error::AppError;
use crate::workflow::execution::ExecutionRequest;

pub struct ProviderExecutionOutcome {
    pub provider_id: String,
    pub adapter_version: u32,
    pub submission: ProviderSubmission,
    pub status: ProviderJobStatus,
    pub result: ProviderResult,
}

pub struct ProviderService;

impl ProviderService {
    pub fn execute_compiled_request(
        request: &ExecutionRequest,
        step_id: &str,
        compiled_request_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ProviderExecutionOutcome, AppError> {
        let mut registry = ProviderRegistry::builtin();
        if provider_id == "openai" {
            let token = std::env::var("OPENAI_API_KEY").map_err(|_| {
                AppError::ProviderConfiguration("OPENAI_API_KEY is not configured".into())
            })?;
            registry.register(OpenAiImageProvider::new(
                "https://api.openai.com/v1",
                token,
            ));
        }
        let provider = registry.get(provider_id).map_err(provider_error)?;
        let provider_request = ProviderExecutionRequest::from_execution_request(
            &request.provenance.workflow_run_id,
            step_id,
            compiled_request_id,
            provider_id,
            model_id,
            &format!("{}:{step_id}:1", request.provenance.workflow_run_id),
            request,
        )
        .map_err(|message| AppError::ProviderExecution(message))?;
        provider
            .capabilities()
            .supports(&provider_request)
            .map_err(|message| {
                provider_error(ProviderError::new(
                    ProviderErrorKind::UnsupportedCapability,
                    message,
                ))
            })?;
        let submission = provider.submit(&provider_request).map_err(provider_error)?;
        let mut status = provider.poll(&submission.job).map_err(provider_error)?;
        for _ in 0..16 {
            if matches!(status.lifecycle, ProviderLifecycle::Succeeded | ProviderLifecycle::Failed | ProviderLifecycle::Cancelled | ProviderLifecycle::Unknown) {
                break;
            }
            status = provider.poll(&submission.job).map_err(provider_error)?;
        }
        if !matches!(status.lifecycle, ProviderLifecycle::Succeeded) {
            return Err(AppError::ProviderExecution(format!(
                "provider {} ended in {:?}",
                provider_id, status.lifecycle
            )));
        }
        let result = provider.fetch_result(&submission.job).map_err(provider_error)?;
        Ok(ProviderExecutionOutcome {
            provider_id: provider_id.into(),
            adapter_version: provider.adapter_version(),
            submission,
            status,
            result,
        })
    }
}

fn provider_error(error: ProviderError) -> AppError {
    AppError::ProviderExecution(serde_json::to_string(&error).unwrap_or(error.message))
}
