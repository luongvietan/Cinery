use super::adapter::GenerationProvider;
use super::credential_store::{
    credential_account, credential_reference, header_credential_account,
    legacy_header_credential_account, CredentialStore, KeyringCredentialStore,
};
use super::error::{ProviderError, ProviderErrorKind};
use super::model::*;
use super::openai::OpenAiImageProvider;
use super::registry::ProviderRegistry;
use crate::db;
use crate::error::AppError;
use crate::project::repository::read_project;
use crate::workflow::execution::ExecutionRequest;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

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

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnectionTestResult {
    pub provider_id: String,
    pub endpoint: String,
    pub connected: bool,
    pub status_code: Option<u16>,
    pub message: String,
}

pub trait ConnectionProbeTransport: Send + Sync {
    fn get_status(&self, endpoint: &str, headers: &[(String, String)]) -> Result<u16, String>;
}

pub struct UreqConnectionProbe {
    agent: ureq::Agent,
}

impl UreqConnectionProbe {
    pub fn new(timeout: Duration) -> Self {
        // Do not forward user-supplied authentication headers to a redirect
        // target. The validation endpoint must be the exact configured host.
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(timeout)
                .redirects(0)
                .build(),
        }
    }
}

impl ConnectionProbeTransport for UreqConnectionProbe {
    fn get_status(&self, endpoint: &str, headers: &[(String, String)]) -> Result<u16, String> {
        let mut request = self.agent.get(endpoint);
        for (name, value) in headers {
            request = request.set(name, value);
        }
        match request.call() {
            Ok(response) => Ok(response.status()),
            Err(ureq::Error::Status(code, _)) => Ok(code),
            Err(ureq::Error::Transport(error)) => Err(super::redact_secret(&error.to_string())),
        }
    }
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

    pub fn list_custom_providers<S: CredentialStore + ?Sized>(
        project_root: &Path,
        credentials: &S,
    ) -> Result<Vec<CustomProviderDefinition>, AppError> {
        let conn = Self::open_project_conn(project_root)?;
        let project_id = Self::project_id(&conn)?;
        let definitions = super::repository::list_custom_providers(&conn)?;
        definitions
            .into_iter()
            .map(|definition| Self::attach_api_key_hint(&conn, credentials, &project_id, definition))
            .collect()
    }

    /// Fills the non-secret `api_key_hint` from the credential vault for a
    /// stored definition. The vault secret itself never leaves this function.
    fn attach_api_key_hint<S: CredentialStore + ?Sized>(
        conn: &rusqlite::Connection,
        credentials: &S,
        project_id: &str,
        mut definition: CustomProviderDefinition,
    ) -> Result<CustomProviderDefinition, AppError> {
        let account = credential_account(project_id, &definition.provider_id);
        if super::repository::get_provider_config(conn, &definition.provider_id)?
            .and_then(|config| config.credential_reference)
            .map(|reference| reference == credential_reference(&account))
            .unwrap_or(false)
        {
            if let Some(secret) = resolve_configured_secret_impl(credentials, &account)? {
                definition.api_key_hint = Some(crate::providers::model::mask_secret(&secret));
            }
        }
        Ok(definition)
    }

    pub fn test_connection<S: CredentialStore + ?Sized, T: ConnectionProbeTransport + ?Sized>(
        project_root: &Path,
        credentials: &S,
        transport: &T,
        provider_id: &str,
    ) -> Result<ProviderConnectionTestResult, AppError> {
        let conn = Self::open_project_conn(project_root)?;
        let project_id = Self::project_id(&conn)?;
        let definition =
            super::repository::get_custom_provider(&conn, provider_id)?.ok_or_else(|| {
                AppError::ProviderConfiguration(format!(
                    "custom provider {provider_id} does not exist"
                ))
            })?;
        definition
            .validate()
            .map_err(AppError::ProviderConfiguration)?;
        let mut headers = Vec::new();
        let account = credential_account(&project_id, provider_id);
        if let Some(config) = super::repository::get_provider_config(&conn, provider_id)? {
            if config.credential_reference.as_deref()
                == Some(credential_reference(&account).as_str())
            {
                if let Some(secret) = resolve_configured_secret_impl(credentials, &account)? {
                    headers.push(("Authorization".into(), format!("Bearer {secret}")));
                }
            }
        }
        for header in &definition.headers {
            if let Some(value) =
                resolve_header_secret(credentials, &project_id, provider_id, &header.name)?
            {
                headers.push((header.name.clone(), value));
            }
        }
        if headers.is_empty() {
            return Err(AppError::ProviderConfiguration(format!(
                "provider {provider_id} has no API key or authentication header configured"
            )));
        }
        let mut endpoint_url = url::Url::parse(&definition.base_url)
            .map_err(|_| AppError::ProviderConfiguration("provider base URL is invalid".into()))?;
        let mut path = endpoint_url.path().trim_end_matches('/').to_string();
        path.push_str("/models");
        endpoint_url.set_path(&path);
        let endpoint = endpoint_url.to_string();
        let (connected, status_code, message) = match transport.get_status(&endpoint, &headers) {
            Ok(code) if (200..300).contains(&code) => (
                true,
                Some(code),
                "Endpoint reachable and credentials were not rejected; no inference was run."
                    .into(),
            ),
            Ok(401) => (
                false,
                Some(401),
                "The API key was rejected (HTTP 401).".into(),
            ),
            Ok(403) => (
                false,
                Some(403),
                "The credential is not authorized for this provider (HTTP 403).".into(),
            ),
            Ok(code) => (
                false,
                Some(code),
                format!("The provider returned HTTP {code} from the validation endpoint."),
            ),
            Err(error) => (
                false,
                None,
                format!("Connection failed: {}", super::redact_secret(&error)),
            ),
        };
        Ok(ProviderConnectionTestResult {
            provider_id: provider_id.into(),
            endpoint,
            connected,
            status_code,
            message,
        })
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
        let mut conn = Self::open_project_conn(project_root)?;
        let project_id = Self::project_id(&conn)?;
        let existing_definition =
            super::repository::get_custom_provider(&conn, &definition.provider_id)?;
        let existing_config =
            super::repository::get_provider_config(&conn, &definition.provider_id)?;
        let authority_changed = existing_definition
            .as_ref()
            .is_some_and(|existing| provider_authority(&existing.base_url) != provider_authority(&definition.base_url));
        let account = credential_account(&project_id, &definition.provider_id);
        let mut desired_secrets = BTreeMap::<String, Option<String>>::new();
        if let Some(api_key) = definition.api_key.as_deref() {
            if api_key.trim().is_empty() {
                desired_secrets.insert(account.clone(), None);
            } else {
                desired_secrets.insert(account.clone(), Some(api_key.trim().into()));
            }
        } else if authority_changed {
            desired_secrets.insert(account.clone(), None);
        }
        let current_header_names = definition
            .headers
            .iter()
            .map(|header| header.name.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        if let Some(existing) = existing_definition.as_ref() {
            for header in &existing.headers {
                let canonical =
                    header_credential_account(&project_id, &definition.provider_id, &header.name);
                let legacy = legacy_header_credential_account(
                    &project_id,
                    &definition.provider_id,
                    &header.name,
                );
                if authority_changed || !current_header_names.contains(&header.name.to_ascii_lowercase()) {
                    desired_secrets.insert(canonical, None);
                    desired_secrets.insert(legacy, None);
                } else if legacy != canonical {
                    if let Some(secret) = resolve_configured_secret_impl(credentials, &legacy)? {
                        desired_secrets.entry(canonical).or_insert(Some(secret));
                        desired_secrets.insert(legacy, None);
                    }
                }
            }
        }
        for header in &definition.headers {
            let header_account =
                header_credential_account(&project_id, &definition.provider_id, &header.name);
            if let Some(value) = header.value.as_deref() {
                if value.trim().is_empty() {
                    desired_secrets.insert(header_account, None);
                } else {
                    desired_secrets.insert(header_account, Some(value.trim().into()));
                }
            }
        }
        let mut previous_secrets = Vec::new();
        for (secret_account, desired) in &desired_secrets {
            let previous = resolve_configured_secret_impl(credentials, secret_account)?;
            if previous.as_ref() == desired.as_ref() {
                continue;
            }
            let change = match desired {
                Some(secret) => credentials.set_secret(secret_account, secret),
                None => credentials.delete_secret(secret_account),
            };
            if change.is_err() {
                restore_secret_states(credentials, &previous_secrets)?;
                return Err(credential_store_error("updating provider credentials"));
            }
            previous_secrets.push((secret_account.clone(), previous));
        }

        let db_result = (|| -> Result<(), AppError> {
            let tx = conn
                .transaction()
                .map_err(|error| AppError::Database(error.to_string()))?;
            super::repository::upsert_custom_provider(&tx, definition)?;
            if definition.api_key.is_some() || existing_config.is_some() {
                let credential_reference = match definition.api_key.as_deref() {
                    Some(value) if value.trim().is_empty() => None,
                    Some(_) => Some(credential_reference(&account)),
                    None if authority_changed => None,
                    None => existing_config.as_ref().and_then(|record| record.credential_reference.clone()),
                };
                super::repository::upsert_provider_config(
                    &tx,
                    &super::repository::ProviderConfigRecord {
                        provider_id: definition.provider_id.clone(),
                        enabled: true,
                        credential_reference,
                        default_model: existing_config
                            .as_ref()
                            .and_then(|record| record.default_model.clone())
                            .or_else(|| definition.models.first().map(|model| model.id.clone())),
                        endpoint: Some(definition.base_url.clone()),
                        request_timeout_seconds: 60,
                        polling_interval_seconds: 3,
                    },
                )?;
            }
            tx.commit()
                .map_err(|error| AppError::Database(error.to_string()))?;
            Ok(())
        })();
        if let Err(error) = db_result {
            restore_secret_states(credentials, &previous_secrets)?;
            return Err(error);
        }
        let mut saved = definition.without_secrets();
        if saved.api_key_hint.is_none() {
            if let Some(secret) = resolve_configured_secret_impl(credentials, &account)? {
                saved.api_key_hint = Some(crate::providers::model::mask_secret(&secret));
            }
        }
        Ok(saved)
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
        let mut conn = Self::open_project_conn(project_root)?;
        let project_id = Self::project_id(&conn)?;
        let mut accounts = BTreeSet::from([credential_account(&project_id, provider_id)]);
        if let Some(definition) = super::repository::get_custom_provider(&conn, provider_id)? {
            for header in definition.headers {
                let canonical = header_credential_account(&project_id, provider_id, &header.name);
                let legacy =
                    legacy_header_credential_account(&project_id, provider_id, &header.name);
                accounts.insert(canonical);
                accounts.insert(legacy);
            }
        }
        let mut previous_secrets = Vec::new();
        for account in accounts {
            let previous = resolve_configured_secret_impl(credentials, &account)?;
            if credentials.delete_secret(&account).is_err() {
                restore_secret_states(credentials, &previous_secrets)?;
                return Err(credential_store_error("deleting provider credentials"));
            }
            previous_secrets.push((account, previous));
        }
        let db_result = (|| -> Result<(), AppError> {
            let tx = conn.transaction().map_err(|error| AppError::Database(error.to_string()))?;
            super::repository::delete_custom_provider(&tx, provider_id)?;
            super::repository::delete_provider_config(&tx, provider_id)?;
            tx.commit().map_err(|error| AppError::Database(error.to_string()))?;
            Ok(())
        })();
        if let Err(error) = db_result {
            restore_secret_states(credentials, &previous_secrets)?;
            return Err(error);
        }
        Ok(())
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
                    resolve_header_secret(credentials, &project_id, provider_id, &header.name)
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

fn provider_authority(base_url: &str) -> Option<(String, String, Option<u16>)> {
    let parsed = url::Url::parse(base_url).ok()?;
    Some((
        parsed.scheme().to_ascii_lowercase(),
        parsed.host_str()?.to_ascii_lowercase(),
        parsed.port_or_known_default(),
    ))
}

fn resolve_header_secret<S: CredentialStore + ?Sized>(
    credentials: &S,
    project_id: &str,
    provider_id: &str,
    header_name: &str,
) -> Result<Option<String>, AppError> {
    let canonical = header_credential_account(project_id, provider_id, header_name);
    if let Some(secret) = resolve_configured_secret_impl(credentials, &canonical)? {
        return Ok(Some(secret));
    }
    let legacy = legacy_header_credential_account(project_id, provider_id, header_name);
    if legacy == canonical {
        return Ok(None);
    }
    resolve_configured_secret_impl(credentials, &legacy)
}

fn restore_secret_states<S: CredentialStore + ?Sized>(
    credentials: &S,
    previous: &[(String, Option<String>)],
) -> Result<(), AppError> {
    for (account, secret) in previous.iter().rev() {
        let result = match secret {
            Some(value) => credentials.set_secret(account, value),
            None => credentials.delete_secret(account),
        };
        if result.is_err() {
            return Err(credential_store_error("rolling back provider credentials"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod connection_tests {
    use super::*;
    use crate::project::service::ProjectService;
    use crate::providers::credential_store::MemoryCredentialStore;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingProbe {
        request: Mutex<Option<(String, Vec<(String, String)>)>>,
        status: u16,
    }

    impl ConnectionProbeTransport for RecordingProbe {
        fn get_status(&self, endpoint: &str, headers: &[(String, String)]) -> Result<u16, String> {
            *self.request.lock().unwrap() = Some((endpoint.into(), headers.to_vec()));
            Ok(self.status)
        }
    }

    #[test]
    fn connection_test_uses_models_endpoint_and_never_returns_secrets() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Probe").unwrap();
        let credentials = MemoryCredentialStore::new();
        let definition = CustomProviderDefinition {
            provider_id: "llm_provider".into(),
            display_name: "LLM".into(),
            base_url: "https://api.example.test/v1/".into(),
            purpose: CustomProviderPurpose::Llm,
            api_key: Some("sk-test-secret".into()),
            api_key_hint: None,
            models: vec![CustomProviderModel {
                id: "model-v1".into(),
                name: "Model V1".into(),
            }],
            headers: vec![CustomProviderHeader {
                name: "X-Workspace".into(),
                value: Some("workspace-secret".into()),
            }],
        };
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();
        let probe = RecordingProbe {
            status: 200,
            ..Default::default()
        };

        let result =
            ProviderService::test_connection(&root, &credentials, &probe, "llm_provider").unwrap();

        assert!(result.connected);
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.endpoint, "https://api.example.test/v1/models");
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("sk-test-secret"));
        assert!(!serialized.contains("workspace-secret"));
        let request = probe.request.lock().unwrap().clone().unwrap();
        assert!(request
            .1
            .contains(&("Authorization".into(), "Bearer sk-test-secret".into())));
        assert!(request
            .1
            .contains(&("X-Workspace".into(), "workspace-secret".into())));

        let rejected = RecordingProbe {
            status: 401,
            ..Default::default()
        };
        let result =
            ProviderService::test_connection(&root, &credentials, &rejected, "llm_provider")
                .unwrap();
        assert!(!result.connected);
        assert_eq!(result.status_code, Some(401));
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("sk-test-secret"));
    }

    #[test]
    fn provider_definitions_expose_masked_hint_without_the_secret() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Hint").unwrap();
        let credentials = MemoryCredentialStore::new();
        let definition = CustomProviderDefinition {
            provider_id: "hinted".into(),
            display_name: "Hinted".into(),
            base_url: "https://api.example.test/v1".into(),
            purpose: CustomProviderPurpose::Llm,
            api_key: Some("sk-j9mlQwErTyXzray".into()),
            api_key_hint: None,
            models: vec![CustomProviderModel {
                id: "m".into(),
                name: "M".into(),
            }],
            headers: vec![],
        };
        let saved = ProviderService::upsert_custom_provider(&root, &credentials, &definition)
            .unwrap();
        assert_eq!(saved.api_key, None);
        assert_eq!(saved.api_key_hint.as_deref(), Some("sk-j9ml•••ray"));

        let listed = ProviderService::list_custom_providers(&root, &credentials).unwrap();
        let listed = listed
            .iter()
            .find(|provider| provider.provider_id == "hinted")
            .unwrap();
        assert_eq!(listed.api_key_hint.as_deref(), Some("sk-j9ml•••ray"));
        let serialized = serde_json::to_string(&listed).unwrap();
        assert!(!serialized.contains("sk-j9mlQwErTyXzray"));

        // Saving again without a key preserves the existing vault secret and hint.
        let update = CustomProviderDefinition {
            api_key: None,
            ..definition.clone()
        };
        let saved_again =
            ProviderService::upsert_custom_provider(&root, &credentials, &update).unwrap();
        assert_eq!(saved_again.api_key_hint.as_deref(), Some("sk-j9ml•••ray"));
        assert_eq!(
            credentials
                .get_secret(&credential_account(
                    &ProviderService::project_id(&ProviderService::open_project_conn(&root).unwrap())
                        .unwrap(),
                    "hinted"
                ))
                .unwrap()
                .as_deref(),
            Some("sk-j9mlQwErTyXzray")
        );
    }

    #[test]
    fn production_probe_does_not_follow_redirects_with_custom_auth_headers() {
        let redirect_target = TcpListener::bind("127.0.0.1:0").unwrap();
        redirect_target.set_nonblocking(true).unwrap();
        let target_address = redirect_target.local_addr().unwrap();
        let redirect_source = TcpListener::bind("127.0.0.1:0").unwrap();
        let source_address = redirect_source.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = redirect_source.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            write!(stream, "HTTP/1.1 302 Found\r\nLocation: http://{target_address}/models\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let probe = UreqConnectionProbe::new(Duration::from_secs(2));

        let status = probe
            .get_status(
                &format!("http://{source_address}/models"),
                &[("X-Api-Key".into(), "must-not-follow".into())],
            )
            .unwrap();

        server.join().unwrap();
        assert_eq!(status, 302);
        assert!(
            matches!(redirect_target.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }

    #[test]
    fn removing_custom_header_metadata_removes_its_vault_secret() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Header cleanup").unwrap();
        let credentials = MemoryCredentialStore::new();
        let mut definition = CustomProviderDefinition {
            provider_id: "header_provider".into(),
            display_name: "Header".into(),
            base_url: "https://api.example.test/v1".into(),
            purpose: CustomProviderPurpose::Llm,
            api_key: None,
            api_key_hint: None,
            models: vec![CustomProviderModel {
                id: "m".into(),
                name: "M".into(),
            }],
            headers: vec![CustomProviderHeader {
                name: "X-API-Key".into(),
                value: Some("header-secret".into()),
            }],
        };
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();
        let conn = ProviderService::open_project_conn(&root).unwrap();
        let project_id = ProviderService::project_id(&conn).unwrap();
        let account = header_credential_account(&project_id, "header_provider", "x-api-key");
        assert_eq!(
            credentials.get_secret(&account).unwrap().as_deref(),
            Some("header-secret")
        );

        definition.headers.clear();
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();

        assert!(credentials.get_secret(&account).unwrap().is_none());
    }

    #[test]
    fn connection_test_rejects_provider_without_any_auth_before_network_io() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "No auth").unwrap();
        let credentials = MemoryCredentialStore::new();
        let definition = CustomProviderDefinition {
            provider_id: "no_auth".into(),
            display_name: "No auth".into(),
            base_url: "https://api.example.test/v1".into(),
            purpose: CustomProviderPurpose::Llm,
            api_key: None,
            api_key_hint: None,
            models: vec![CustomProviderModel {
                id: "m".into(),
                name: "M".into(),
            }],
            headers: vec![],
        };
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();
        let probe = RecordingProbe::default();

        let error =
            ProviderService::test_connection(&root, &credentials, &probe, "no_auth").unwrap_err();

        assert!(error
            .to_string()
            .contains("no API key or authentication header"));
        assert!(probe.request.lock().unwrap().is_none());
    }

    #[test]
    fn connection_test_revalidates_persisted_metadata_before_resolving_secrets() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Tampered metadata").unwrap();
        let credentials = MemoryCredentialStore::new();
        let definition = CustomProviderDefinition {
            provider_id: "tampered".into(), display_name: "Tampered".into(),
            base_url: "https://safe.example.test/v1".into(), purpose: CustomProviderPurpose::Llm,
            api_key: Some("sk-secret".into()), api_key_hint: None, models: vec![CustomProviderModel { id: "m".into(), name: "M".into() }], headers: vec![],
        };
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();
        let conn = ProviderService::open_project_conn(&root).unwrap();
        conn.execute(
            "UPDATE custom_provider_definitions SET base_url = 'https://user:secret@evil.example.test/v1' WHERE provider_id = 'tampered'",
            [],
        ).unwrap();
        let probe = RecordingProbe::default();

        let error = ProviderService::test_connection(&root, &credentials, &probe, "tampered").unwrap_err();

        assert!(error.to_string().contains("must not contain credentials"));
        assert!(probe.request.lock().unwrap().is_none());
    }

    #[test]
    fn changing_endpoint_authority_requires_explicit_credential_reentry() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Authority change").unwrap();
        let credentials = MemoryCredentialStore::new();
        let mut definition = CustomProviderDefinition {
            provider_id: "moving".into(), display_name: "Moving".into(),
            base_url: "https://first.example.test/v1".into(), purpose: CustomProviderPurpose::Llm,
            api_key: Some("sk-old".into()), api_key_hint: None, models: vec![CustomProviderModel { id: "m".into(), name: "M".into() }],
            headers: vec![CustomProviderHeader { name: "X-API-Key".into(), value: Some("header-old".into()) }],
        };
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();
        definition.base_url = "https://second.example.test/v1".into();
        definition.api_key = None;
        definition.headers[0].value = None;

        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();

        let conn = ProviderService::open_project_conn(&root).unwrap();
        let project_id = ProviderService::project_id(&conn).unwrap();
        assert!(credentials.get_secret(&credential_account(&project_id, "moving")).unwrap().is_none());
        assert!(credentials.get_secret(&header_credential_account(&project_id, "moving", "X-API-Key")).unwrap().is_none());
        assert!(super::super::repository::get_provider_config(&conn, "moving").unwrap().unwrap().credential_reference.is_none());
        let probe = RecordingProbe::default();
        assert!(ProviderService::test_connection(&root, &credentials, &probe, "moving").is_err());
        assert!(probe.request.lock().unwrap().is_none());
    }

    #[test]
    fn deleting_custom_provider_removes_config_and_all_vault_entries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Delete cleanup").unwrap();
        let credentials = MemoryCredentialStore::new();
        let definition = CustomProviderDefinition {
            provider_id: "delete_me".into(), display_name: "Delete".into(),
            base_url: "https://api.example.test/v1".into(), purpose: CustomProviderPurpose::Llm,
            api_key: Some("sk-delete".into()), api_key_hint: None, models: vec![CustomProviderModel { id: "m".into(), name: "M".into() }],
            headers: vec![CustomProviderHeader { name: "X-Key".into(), value: Some("header-delete".into()) }],
        };
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();
        let conn = ProviderService::open_project_conn(&root).unwrap();
        let project_id = ProviderService::project_id(&conn).unwrap();
        drop(conn);

        ProviderService::delete_custom_provider(&root, &credentials, "delete_me").unwrap();

        let conn = ProviderService::open_project_conn(&root).unwrap();
        assert!(super::super::repository::get_custom_provider(&conn, "delete_me").unwrap().is_none());
        assert!(super::super::repository::get_provider_config(&conn, "delete_me").unwrap().is_none());
        assert!(credentials.get_secret(&credential_account(&project_id, "delete_me")).unwrap().is_none());
        assert!(credentials.get_secret(&header_credential_account(&project_id, "delete_me", "X-Key")).unwrap().is_none());
    }
}

fn provider_error(error: ProviderError) -> AppError {
    AppError::ProviderExecution(serde_json::to_string(&error).unwrap_or(error.message))
}
