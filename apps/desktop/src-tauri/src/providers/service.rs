use super::adapter::GenerationProvider;
use super::credential_store::{
    credential_account, credential_reference, header_credential_account, CredentialStore,
    KeyringCredentialStore,
};
use super::error::{ProviderError, ProviderErrorKind};
use super::model::*;
use super::openai::OpenAiImageProvider;
use super::registry::ProviderRegistry;
use crate::db;
use crate::error::AppError;
use crate::project::repository::read_project;
use crate::workflow::execution::ExecutionRequest;
use std::path::Path;
use std::sync::Arc;

/// Default OpenAI image model per the provider keychain spec.
pub const OPENAI_DEFAULT_MODEL: &str = "gpt-image-2";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigurationStatus {
    pub provider_id: String,
    pub enabled: bool,
    pub credential_configured: bool,
    pub default_model: Option<String>,
    pub models: Vec<String>,
}

/// A resolved, ephemeral secret handed to a provider adapter at execution
/// time. Never serialized, logged, or persisted.
#[derive(Clone)]
pub struct ResolvedProviderCredential {
    pub provider_id: String,
    pub account: String,
    pub secret: String,
}

/// Legacy environment-reference prefix used by pre-keychain configurations.
const LEGACY_ENV_PREFIX: &str = "env://";

fn credential_store_error(action: &str) -> AppError {
    AppError::ProviderConfiguration(format!(
        "the operating system credential vault failed during {action}; no credential was changed"
    ))
}

pub struct ProviderExecutionOutcome {
    pub provider_id: String,
    pub adapter_version: u32,
    pub submission: ProviderSubmission,
    pub status: ProviderJobStatus,
    pub result: ProviderResult,
}

pub struct ProviderSubmissionHandle {
    pub provider_id: String,
    pub adapter_version: u32,
    pub provider: Arc<dyn GenerationProvider>,
    pub submission: ProviderSubmission,
}

pub struct ProviderService;

impl ProviderService {
    /// Shared credential store used by the production command surface.
    pub fn default_credential_store() -> Arc<KeyringCredentialStore> {
        Arc::new(KeyringCredentialStore::new())
    }

    pub fn list_provider_ids() -> Vec<String> {
        ProviderRegistry::builtin().ids()
    }

    pub fn list_custom_providers(
        project_root: &Path,
    ) -> Result<Vec<CustomProviderDefinition>, AppError> {
        let conn = Self::open_project_conn(project_root)?;
        super::repository::list_custom_providers(&conn)
    }

    pub fn upsert_custom_provider<S: CredentialStore + ?Sized>(
        project_root: &Path,
        credentials: &S,
        definition: &CustomProviderDefinition,
    ) -> Result<CustomProviderDefinition, AppError> {
        definition
            .validate()
            .map_err(AppError::ProviderConfiguration)?;
        if ProviderRegistry::builtin()
            .get(&definition.provider_id)
            .is_ok()
        {
            return Err(AppError::ProviderConfiguration(
                "custom provider ID conflicts with a built-in provider".into(),
            ));
        }
        let conn = Self::open_project_conn(project_root)?;
        let project_id = Self::project_id(&conn)?;
        let account = credential_account(&project_id, &definition.provider_id);
        if let Some(api_key) = definition.api_key.as_deref() {
            if api_key.trim().is_empty() {
                credentials
                    .delete_secret(&account)
                    .map_err(|_| credential_store_error("removing the API key"))?;
            } else {
                credentials
                    .set_secret(&account, api_key.trim())
                    .map_err(|_| credential_store_error("saving the API key"))?;
            }
        }
        for header in &definition.headers {
            let header_account =
                header_credential_account(&project_id, &definition.provider_id, &header.name);
            if let Some(value) = header.value.as_deref() {
                if value.trim().is_empty() {
                    credentials
                        .delete_secret(&header_account)
                        .map_err(|_| credential_store_error("removing the header credential"))?;
                } else {
                    credentials
                        .set_secret(&header_account, value.trim())
                        .map_err(|_| credential_store_error("saving the header credential"))?;
                }
            }
        }
        super::repository::upsert_custom_provider(&conn, definition)?;
        let existing = super::repository::get_provider_config(&conn, &definition.provider_id)?;
        let api_key_configured = definition
            .api_key
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        if api_key_configured {
            super::repository::upsert_provider_config(
                &conn,
                &super::repository::ProviderConfigRecord {
                    provider_id: definition.provider_id.clone(),
                    enabled: true,
                    credential_reference: Some(credential_reference(&account)),
                    default_model: existing
                        .as_ref()
                        .and_then(|record| record.default_model.clone())
                        .or_else(|| definition.models.first().map(|model| model.id.clone())),
                    endpoint: Some(definition.base_url.clone()),
                    request_timeout_seconds: 60,
                    polling_interval_seconds: 3,
                },
            )?;
        }
        Ok(definition.without_secrets())
    }

    pub fn delete_custom_provider<S: CredentialStore + ?Sized>(
        project_root: &Path,
        credentials: &S,
        provider_id: &str,
    ) -> Result<(), AppError> {
        if ProviderRegistry::builtin().get(provider_id).is_ok() {
            return Err(AppError::ProviderConfiguration(
                "built-in providers cannot be deleted".into(),
            ));
        }
        let conn = Self::open_project_conn(project_root)?;
        let project_id = Self::project_id(&conn)?;
        if let Some(definition) = super::repository::get_custom_provider(&conn, provider_id)? {
            let _ = credentials.delete_secret(&credential_account(&project_id, provider_id));
            for header in definition.headers {
                let _ = credentials.delete_secret(&header_credential_account(
                    &project_id,
                    provider_id,
                    &header.name,
                ));
            }
        }
        super::repository::delete_custom_provider(&conn, provider_id)
    }

    fn open_project_conn(project_root: &Path) -> Result<rusqlite::Connection, AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        read_project(&conn)?;
        Ok(conn)
    }

    fn project_id(conn: &rusqlite::Connection) -> Result<String, AppError> {
        Ok(read_project(conn)?.id)
    }

    /// Reads the effective credential configuration for one provider:
    /// - `keyring://` references are verified against the vault;
    /// - `env://` legacy references migrate into the vault when the variable
    ///   exists and are reported unconfigured otherwise;
    /// - always-local providers (mock, dry_run) are configured by default.
    pub fn configuration_status<S: CredentialStore + ?Sized>(
        project_root: &Path,
        credentials: &S,
        provider_id: &str,
    ) -> Result<ProviderConfigurationStatus, AppError> {
        let conn = Self::open_project_conn(project_root)?;
        let project_id = Self::project_id(&conn)?;
        let config = super::repository::get_provider_config(&conn, provider_id)?;
        let custom_definition = super::repository::get_custom_provider(&conn, provider_id)?;
        let models = if let Some(custom) = custom_definition.as_ref() {
            custom.models.iter().map(|model| model.id.clone()).collect()
        } else {
            Self::models(provider_id)?
        };

        let always_configured = provider_id == "mock" || provider_id == "dry_run";
        let mut credential_configured = always_configured;
        let mut default_model = config
            .as_ref()
            .and_then(|record| record.default_model.clone());

        if let Some(record) = &config {
            match record.credential_reference.as_deref() {
                Some(reference) if reference.starts_with("keyring://") => {
                    let account = credential_account(&project_id, provider_id);
                    if reference == credential_reference(&account) {
                        let secret = resolve_configured_secret_impl(credentials, &account)?;
                        credential_configured = secret.is_some();
                    }
                }
                Some(reference) if reference.starts_with(LEGACY_ENV_PREFIX) => {
                    let variable = reference.trim_start_matches(LEGACY_ENV_PREFIX);
                    if let Ok(secret) = std::env::var(variable) {
                        if !secret.trim().is_empty() {
                            // Migrate into the vault and replace the stored
                            // reference. The environment variable itself is
                            // never persisted anywhere.
                            let account = credential_account(&project_id, provider_id);
                            credentials.set_secret(&account, &secret).map_err(|_| {
                                credential_store_error("migrating a legacy credential")
                            })?;
                            super::repository::upsert_provider_config(
                                &conn,
                                &super::repository::ProviderConfigRecord {
                                    provider_id: provider_id.into(),
                                    enabled: record.enabled,
                                    credential_reference: Some(credential_reference(&account)),
                                    default_model: record.default_model.clone(),
                                    endpoint: record.endpoint.clone(),
                                    request_timeout_seconds: record.request_timeout_seconds,
                                    polling_interval_seconds: record.polling_interval_seconds,
                                },
                            )?;
                            credential_configured = true;
                            if default_model.is_none() && provider_id == "openai" {
                                default_model = Some(OPENAI_DEFAULT_MODEL.into());
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if credential_configured && default_model.is_none() && provider_id == "openai" {
            default_model = Some(OPENAI_DEFAULT_MODEL.into());
        }

        if let Some(custom) = custom_definition {
            if !credential_configured {
                credential_configured = custom.headers.iter().any(|header| {
                    credentials
                        .get_secret(&header_credential_account(
                            &project_id,
                            provider_id,
                            &header.name,
                        ))
                        .ok()
                        .flatten()
                        .is_some()
                });
            }
            if default_model.is_none() {
                default_model = custom.models.first().map(|model| model.id.clone());
            }
        }

        Ok(ProviderConfigurationStatus {
            provider_id: provider_id.into(),
            enabled: config.as_ref().map(|record| record.enabled).unwrap_or(true),
            credential_configured,
            default_model,
            models,
        })
    }

    /// Saves a secret at the command boundary: vault first, database second,
    /// with compensating vault cleanup when the database write fails.
    pub fn save_credential<S: CredentialStore + ?Sized>(
        project_root: &Path,
        credentials: &S,
        provider_id: &str,
        secret: &str,
        default_model: Option<&str>,
    ) -> Result<ProviderConfigurationStatus, AppError> {
        let secret = secret.trim();
        if secret.is_empty() {
            return Err(AppError::ProviderConfiguration(
                "the credential value must not be empty".into(),
            ));
        }
        let conn = Self::open_project_conn(project_root)?;
        let project_id = Self::project_id(&conn)?;
        let account = credential_account(&project_id, provider_id);

        let previous = credentials
            .get_secret(&account)
            .map_err(|_| credential_store_error("reading the prior credential"))?;
        credentials
            .set_secret(&account, secret)
            .map_err(|_| credential_store_error("saving the credential"))?;

        let reference = credential_reference(&account);
        let existing = super::repository::get_provider_config(&conn, provider_id)?;
        let record = super::repository::ProviderConfigRecord {
            provider_id: provider_id.into(),
            enabled: existing
                .as_ref()
                .map(|record| record.enabled)
                .unwrap_or(true),
            credential_reference: Some(reference),
            default_model: default_model
                .map(str::to_string)
                .or_else(|| {
                    existing
                        .as_ref()
                        .and_then(|record| record.default_model.clone())
                })
                .or_else(|| (provider_id == "openai").then(|| OPENAI_DEFAULT_MODEL.into())),
            endpoint: existing.as_ref().and_then(|record| record.endpoint.clone()),
            request_timeout_seconds: existing
                .as_ref()
                .map(|record| record.request_timeout_seconds)
                .unwrap_or(60),
            polling_interval_seconds: existing
                .as_ref()
                .map(|record| record.polling_interval_seconds)
                .unwrap_or(3),
        };
        if let Err(db_error) = super::repository::upsert_provider_config(&conn, &record) {
            // Compensate: restore the prior vault value or delete the entry.
            let restore = previous.as_deref();
            let _ = match restore {
                Some(value) => credentials.set_secret(&account, value),
                None => credentials.delete_secret(&account),
            };
            return Err(db_error);
        }
        drop(conn);
        Self::configuration_status(project_root, credentials, provider_id)
    }

    /// Removes a credential: database reference first, vault second. A vault
    /// cleanup failure is surfaced as an orphaned-secret error while the
    /// provider stays disabled (no DB reference => not configured).
    pub fn remove_credential<S: CredentialStore + ?Sized>(
        project_root: &Path,
        credentials: &S,
        provider_id: &str,
    ) -> Result<(), AppError> {
        let conn = Self::open_project_conn(project_root)?;
        let project_id = Self::project_id(&conn)?;
        let account = credential_account(&project_id, provider_id);

        let existing = super::repository::get_provider_config(&conn, provider_id)?;
        if let Some(record) = existing {
            let cleared = super::repository::ProviderConfigRecord {
                credential_reference: None,
                ..record
            };
            super::repository::upsert_provider_config(&conn, &cleared)?;
        }
        drop(conn);

        credentials.delete_secret(&account).map_err(|_| {
            AppError::ProviderConfiguration(
                "the credential reference was removed but the vault entry could not be \
                     deleted; an orphaned secret remains in the credential vault and the \
                     provider stays disabled until it is removed"
                    .into(),
            )
        })
    }

    /// Resolves the ephemeral secret for execution. Fails as a configuration
    /// error (never a provider attempt) when the credential is unavailable.
    pub fn resolve_credential<S: CredentialStore + ?Sized>(
        project_root: &Path,
        credentials: &S,
        provider_id: &str,
    ) -> Result<ResolvedProviderCredential, AppError> {
        let conn = Self::open_project_conn(project_root)?;
        let project_id = Self::project_id(&conn)?;
        let account = credential_account(&project_id, provider_id);
        let secret = resolve_configured_secret_impl(credentials, &account)?.ok_or_else(|| {
            AppError::ProviderConfiguration(format!(
                "provider {provider_id} has no credential configured for this project"
            ))
        })?;
        Ok(ResolvedProviderCredential {
            provider_id: provider_id.into(),
            account,
            secret,
        })
    }

    pub fn configured_default(project_root: &Path) -> Result<Option<(String, String)>, AppError> {
        let conn = Self::open_project_conn(project_root)?;
        Ok(super::repository::list_provider_configs(&conn)?
            .into_iter()
            .find(|config| config.enabled && config.default_model.as_deref().is_some())
            .and_then(|config| {
                config
                    .default_model
                    .map(|model| (config.provider_id, model))
            }))
    }

    pub fn configure(
        project_root: &Path,
        config: &super::repository::ProviderConfigRecord,
    ) -> Result<ProviderConfigurationStatus, AppError> {
        let conn = Self::open_project_conn(project_root)?;
        // Guard: configuration updates must never (re)introduce a plaintext
        // environment reference as a credential path.
        let mut sanitized = config.clone();
        if let Some(reference) = &sanitized.credential_reference {
            if reference.starts_with(LEGACY_ENV_PREFIX) {
                sanitized.credential_reference = None;
            }
        }
        super::repository::upsert_provider_config(&conn, &sanitized)?;
        drop(conn);
        let models = Self::models(&sanitized.provider_id)?;
        Ok(ProviderConfigurationStatus {
            provider_id: sanitized.provider_id,
            enabled: sanitized.enabled,
            credential_configured: sanitized.credential_reference.is_some(),
            default_model: sanitized.default_model,
            models,
        })
    }

    pub fn remove_credential_reference(
        project_root: &Path,
        provider_id: &str,
    ) -> Result<(), AppError> {
        let conn = Self::open_project_conn(project_root)?;
        let mut config = super::repository::get_provider_config(&conn, provider_id)?.unwrap_or(
            super::repository::ProviderConfigRecord {
                provider_id: provider_id.into(),
                enabled: true,
                credential_reference: None,
                default_model: None,
                endpoint: None,
                request_timeout_seconds: 60,
                polling_interval_seconds: 3,
            },
        );
        config.credential_reference = None;
        super::repository::upsert_provider_config(&conn, &config)
    }

    pub fn validate_configuration(provider_id: &str) -> Result<(), AppError> {
        if provider_id == "openai" && std::env::var_os("OPENAI_API_KEY").is_none() {
            return Err(AppError::ProviderConfiguration(
                "OPENAI_API_KEY is not configured".into(),
            ));
        }
        ProviderRegistry::builtin()
            .get(provider_id)
            .map_err(provider_error)?;
        Ok(())
    }

    pub fn models(provider_id: &str) -> Result<Vec<String>, AppError> {
        if provider_id == "openai" {
            return Ok(vec![OPENAI_DEFAULT_MODEL.into()]);
        }
        Ok(ProviderRegistry::builtin()
            .get(provider_id)
            .map_err(provider_error)?
            .capabilities()
            .supported_models)
    }

    pub fn idempotency_key(run_id: &str, step_id: &str, attempt_number: i64) -> String {
        format!("{run_id}:{step_id}:{attempt_number}")
    }

    pub fn cancel_job(
        provider_id: &str,
        job: &ProviderJobRef,
    ) -> Result<ProviderCancellationResult, AppError> {
        let provider = ProviderRegistry::builtin()
            .get(provider_id)
            .map_err(provider_error)?;
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
        let handle = Self::submit_compiled_request(
            request,
            step_id,
            compiled_request_id,
            provider_id,
            model_id,
            attempt_number,
        )?;
        let (status, result) = Self::finish_submission(&handle)?;
        Ok(ProviderExecutionOutcome {
            provider_id: handle.provider_id,
            adapter_version: handle.adapter_version,
            submission: handle.submission,
            status,
            result,
        })
    }

    pub fn submit_compiled_request(
        request: &ExecutionRequest,
        step_id: &str,
        compiled_request_id: &str,
        provider_id: &str,
        model_id: &str,
        attempt_number: i64,
    ) -> Result<ProviderSubmissionHandle, AppError> {
        Self::submit_prepared_request(
            request,
            None,
            step_id,
            compiled_request_id,
            provider_id,
            model_id,
            attempt_number,
        )
    }

    /// Submission with attachments already resolved by the caller (the
    /// workflow runtime owns project-root access). The secret is resolved
    /// from the vault when the caller supplies a credential store.
    pub fn submit_prepared_request(
        request: &ExecutionRequest,
        credentials: Option<&dyn CredentialStore>,
        step_id: &str,
        compiled_request_id: &str,
        provider_id: &str,
        model_id: &str,
        attempt_number: i64,
    ) -> Result<ProviderSubmissionHandle, AppError> {
        Self::submit_provider_request(
            request,
            Vec::new(),
            credentials,
            step_id,
            compiled_request_id,
            provider_id,
            model_id,
            attempt_number,
        )
    }

    /// Full submission path: caller-supplied verified attachments plus an
    /// optional credential store.
    pub fn submit_provider_request(
        request: &ExecutionRequest,
        reference_attachments: Vec<ProviderReferenceAttachment>,
        credentials: Option<&dyn CredentialStore>,
        step_id: &str,
        compiled_request_id: &str,
        provider_id: &str,
        model_id: &str,
        attempt_number: i64,
    ) -> Result<ProviderSubmissionHandle, AppError> {
        let mut registry = ProviderRegistry::builtin();
        if provider_id == "openai" {
            let token = match credentials {
                Some(store) => {
                    let project_root = std::env::var("CINERY_PROJECT_ROOT").map_err(|_| {
                        AppError::ProviderConfiguration(
                            "project context is unavailable for credential resolution".into(),
                        )
                    })?;
                    let conn =
                        db::open_existing_connection(&Path::new(&project_root).join("project.db"))?;
                    let project_id = read_project(&conn)?.id;
                    let account = credential_account(&project_id, provider_id);
                    store
                        .get_secret(&account)
                        .map_err(|_| credential_store_error("reading the credential"))?
                        .ok_or_else(|| {
                            AppError::ProviderConfiguration(format!(
                                "provider {provider_id} has no credential configured for this project"
                            ))
                        })?
                }
                None => std::env::var("OPENAI_API_KEY").map_err(|_| {
                    AppError::ProviderConfiguration("OPENAI_API_KEY is not configured".into())
                })?,
            };
            registry.register(OpenAiImageProvider::new("https://api.openai.com/v1", token));
        }
        let provider = registry.get(provider_id).map_err(provider_error)?;
        let mut provider_request = ProviderExecutionRequest::from_execution_request(
            &request.provenance.workflow_run_id,
            step_id,
            compiled_request_id,
            provider_id,
            model_id,
            &Self::idempotency_key(&request.provenance.workflow_run_id, step_id, attempt_number),
            request,
        )
        .map_err(|message| AppError::ProviderExecution(message))?;
        provider_request.reference_attachments = reference_attachments;
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
        Ok(ProviderSubmissionHandle {
            provider_id: provider_id.into(),
            adapter_version: provider.adapter_version(),
            provider,
            submission,
        })
    }

    pub fn finish_submission(
        handle: &ProviderSubmissionHandle,
    ) -> Result<(ProviderJobStatus, ProviderResult), AppError> {
        let provider = &handle.provider;
        let submission = &handle.submission;
        let mut status = provider.poll(&submission.job).map_err(provider_error)?;
        for _ in 0..16 {
            if matches!(
                status.lifecycle,
                ProviderLifecycle::Succeeded
                    | ProviderLifecycle::Failed
                    | ProviderLifecycle::Cancelled
                    | ProviderLifecycle::Unknown
            ) {
                break;
            }
            status = provider.poll(&submission.job).map_err(provider_error)?;
        }
        if !matches!(status.lifecycle, ProviderLifecycle::Succeeded) {
            return Err(AppError::ProviderExecution(format!(
                "provider {} ended in {:?}",
                handle.provider_id, status.lifecycle
            )));
        }
        let result = provider
            .fetch_result(&submission.job)
            .map_err(provider_error)?;
        Ok((status, result))
    }
}

fn resolve_configured_secret_impl<S: CredentialStore + ?Sized>(
    credentials: &S,
    account: &str,
) -> Result<Option<String>, AppError> {
    credentials
        .get_secret(account)
        .map_err(|_| credential_store_error("reading the credential"))
}

fn provider_error(error: ProviderError) -> AppError {
    AppError::ProviderExecution(serde_json::to_string(&error).unwrap_or(error.message))
}
