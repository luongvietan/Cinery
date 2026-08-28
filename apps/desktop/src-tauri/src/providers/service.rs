use super::error::{ProviderError, ProviderErrorKind};
use super::model::*;
use super::openai::OpenAiImageProvider;
use super::registry::ProviderRegistry;
use crate::error::AppError;
use crate::workflow::execution::ExecutionRequest;
use crate::db;
use crate::project::repository::read_project;
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigurationStatus {
    pub provider_id: String,
    pub enabled: bool,
    pub credential_configured: bool,
    pub credential_reference: Option<String>,
    pub default_model: Option<String>,
}

pub struct ProviderExecutionOutcome {
    pub provider_id: String,
    pub adapter_version: u32,
    pub submission: ProviderSubmission,
    pub status: ProviderJobStatus,
    pub result: ProviderResult,
}

pub struct ProviderService;

impl ProviderService {
    pub fn list_provider_ids() -> Vec<String> {
        let mut ids = ProviderRegistry::builtin().ids();
        if std::env::var_os("OPENAI_API_KEY").is_some() {
            ids.push("openai".into());
        }
        ids
    }

    pub fn configuration_status(
        project_root: &Path,
        provider_id: &str,
    ) -> Result<ProviderConfigurationStatus, AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        read_project(&conn)?;
        let config = super::repository::get_provider_config(&conn, provider_id)?;
        let credential_reference = config.as_ref().and_then(|record| record.credential_reference.clone());
        let credential_configured = credential_reference
            .as_deref()
            .and_then(|reference| std::env::var_os(reference))
            .is_some()
            || (provider_id == "mock" || provider_id == "dry_run");
        Ok(ProviderConfigurationStatus {
            provider_id: provider_id.into(),
            enabled: config.as_ref().map(|record| record.enabled).unwrap_or(true),
            credential_configured,
            credential_reference,
            default_model: config.and_then(|record| record.default_model),
        })
    }

    pub fn configure(
        project_root: &Path,
        config: &super::repository::ProviderConfigRecord,
    ) -> Result<ProviderConfigurationStatus, AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        read_project(&conn)?;
        super::repository::upsert_provider_config(&conn, config)?;
        Self::configuration_status(project_root, &config.provider_id)
    }

    pub fn remove_credential_reference(project_root: &Path, provider_id: &str) -> Result<(), AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        read_project(&conn)?;
        let mut config = super::repository::get_provider_config(&conn, provider_id)?.unwrap_or(super::repository::ProviderConfigRecord {
            provider_id: provider_id.into(), enabled: true, credential_reference: None, default_model: None, endpoint: None, request_timeout_seconds: 60, polling_interval_seconds: 3,
        });
        config.credential_reference = None;
        super::repository::upsert_provider_config(&conn, &config)
    }

    pub fn validate_configuration(provider_id: &str) -> Result<(), AppError> {
        if provider_id == "openai" && std::env::var_os("OPENAI_API_KEY").is_none() {
            return Err(AppError::ProviderConfiguration("OPENAI_API_KEY is not configured".into()));
        }
        ProviderRegistry::builtin().get(provider_id).map_err(provider_error)?;
        Ok(())
    }

    pub fn models(provider_id: &str) -> Result<Vec<String>, AppError> {
        if provider_id == "openai" {
            return Ok(vec!["gpt-image-1".into()]);
        }
        Ok(ProviderRegistry::builtin().get(provider_id).map_err(provider_error)?.capabilities().supported_models)
    }

    pub fn idempotency_key(run_id: &str, step_id: &str, attempt_number: i64) -> String {
        format!("{run_id}:{step_id}:{attempt_number}")
    }

    pub fn cancel_job(
        provider_id: &str,
        job: &ProviderJobRef,
    ) -> Result<ProviderCancellationResult, AppError> {
        let provider = ProviderRegistry::builtin().get(provider_id).map_err(provider_error)?;
        if !provider.capabilities().supports_cancel {
            return Ok(ProviderCancellationResult {
                provider_job_id: job.provider_job_id.clone(),
                lifecycle: ProviderLifecycle::Cancelled,
            });
        }
        provider.cancel(job).map_err(provider_error)
    }

    pub fn can_retry(error: &ProviderErrorKind) -> bool {
        error.retryable()
    }

    pub fn execute_compiled_request(
        request: &ExecutionRequest,
        step_id: &str,
        compiled_request_id: &str,
        provider_id: &str,
        model_id: &str,
        attempt_number: i64,
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
            &Self::idempotency_key(&request.provenance.workflow_run_id, step_id, attempt_number),
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
