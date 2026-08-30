use super::adapter::GenerationProvider;
use super::cancellation;
use super::credential_store::{
    credential_account, credential_reference, header_credential_account,
    legacy_header_credential_account, CredentialStore, KeyringCredentialStore,
};
use super::config::OPERATION_VALIDATE;
use super::declarative::DeclarativeProvider;
use super::error::{ProviderError, ProviderErrorKind};
use super::http::{HttpExecutor, HttpRequest, HttpResponse, UreqExecutor};
use super::model::*;
use super::model::CustomProviderPurpose;
use super::presets::preset_by_id;
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
            .map(|definition| {
                Self::attach_api_key_hint(&conn, credentials, &project_id, definition)
            })
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

    /// Connection testing is operation-aware: the provider's configured
    /// `validate` operation runs (with its own URL, method, and auth); when
    /// no validate operation exists, configuration-only validation applies.
    /// No OpenAI-shaped request is ever assumed.
    pub fn test_connection<S: CredentialStore + ?Sized, T: HttpExecutor + 'static>(
        project_root: &Path,
        credentials: &S,
        transport: T,
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
        let account = credential_account(&project_id, provider_id);
        let mut api_key: Option<String> = None;
        if let Some(config) = super::repository::get_provider_config(&conn, provider_id)? {
            if config.credential_reference.as_deref()
                == Some(credential_reference(&account).as_str())
            {
                api_key = resolve_configured_secret_impl(credentials, &account)?;
            }
        }
        // Legacy compatibility: an Authorization header secret can act as
        // the bearer credential when no API key account exists.
        if api_key.is_none() && definition.runtime.auth.requires_credential() {
            for header in &definition.headers {
                if header.name.eq_ignore_ascii_case("authorization") {
                    api_key =
                        resolve_header_secret(credentials, &project_id, provider_id, &header.name)?;
                    break;
                }
            }
        }
        let mut header_values = BTreeMap::new();
        for header in &definition.headers {
            if let Some(value) =
                resolve_header_secret(credentials, &project_id, provider_id, &header.name)?
            {
                header_values.insert(header.name.clone(), value);
            }
        }
        let credential_configured = !definition.runtime.auth.requires_credential()
            || api_key.as_deref().is_some_and(|key| !key.trim().is_empty())
            || !header_values.is_empty();
        if !credential_configured {
            return Err(AppError::ProviderConfiguration(format!(
                "provider {provider_id} has no API key or authentication header configured"
            )));
        }
        let adapter = DeclarativeProvider::new(
            provider_id,
            definition.base_url.clone(),
            definition.models.clone(),
            definition.runtime.clone(),
            api_key,
            header_values,
            Arc::new(transport),
        );
        let outcome = adapter
            .run_validation(definition.models.first().map(|model| model.id.as_str()))
            .map_err(provider_error)?;
        let endpoint = Self::validation_endpoint_display(&definition);
        if !outcome.performed_network_check {
            return Ok(ProviderConnectionTestResult {
                provider_id: provider_id.into(),
                endpoint,
                connected: true,
                status_code: None,
                message: "Configuration is valid and a credential is saved. This service has                           no test endpoint configured, so no network request was sent."
                    .into(),
            });
        }
        let provider_note = outcome
            .provider_message
            .as_deref()
            .map(|message| format!(" Provider message: {message}"))
            .unwrap_or_default();
        let (connected, status_code, message) = match outcome.status_code {
            Some(code) if (200..300).contains(&code) => (
                true,
                Some(code),
                "Endpoint reachable and credentials were not rejected; no inference was run."
                    .into(),
            ),
            Some(401) => (
                false,
                Some(401),
                format!("The API key was rejected (HTTP 401).{provider_note}"),
            ),
            Some(403) => (
                false,
                Some(403),
                format!("The credential is not authorized for this provider (HTTP 403).{provider_note}"),
            ),
            Some(code) => (
                false,
                Some(code),
                format!("The provider returned HTTP {code} from its validation endpoint.{provider_note}"),
            ),
            None => (
                false,
                None,
                "The validation request could not be completed.".into(),
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

    /// Human-readable validation endpoint for display (never secret-bearing).
    fn validation_endpoint_display(definition: &CustomProviderDefinition) -> String {
        let model = definition
            .models
            .first()
            .map(|model| model.id.clone())
            .unwrap_or_default();
        match definition.runtime.operations.get(OPERATION_VALIDATE) {
            Some(endpoint) => {
                let base = definition.base_url.trim_end_matches('/');
                let mut url = format!("{base}{}", endpoint.path_template);
                for (placeholder, value) in [
                    ("{model}", model),
                    (
                        "{accountId}",
                        definition.runtime.account_id.clone().unwrap_or_default(),
                    ),
                    ("{providerId}", definition.provider_id.clone()),
                ] {
                    url = url.replace(placeholder, &value);
                }
                url
            }
            None => definition.base_url.clone(),
        }
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
        let authority_changed = existing_definition.as_ref().is_some_and(|existing| {
            provider_authority(&existing.base_url) != provider_authority(&definition.base_url)
        });
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
                if authority_changed
                    || !current_header_names.contains(&header.name.to_ascii_lowercase())
                {
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
                    None => existing_config
                        .as_ref()
                        .and_then(|record| record.credential_reference.clone()),
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
            let tx = conn
                .transaction()
                .map_err(|error| AppError::Database(error.to_string()))?;
            super::repository::delete_custom_provider(&tx, provider_id)?;
            super::repository::delete_provider_config(&tx, provider_id)?;
            tx.commit()
                .map_err(|error| AppError::Database(error.to_string()))?;
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
        project_root: Option<&Path>,
        step_id: &str,
        compiled_request_id: &str,
        provider_id: &str,
        model_id: &str,
        attempt_number: i64,
    ) -> Result<ProviderExecutionOutcome, AppError> {
        let handle = Self::submit_compiled_request(
            request,
            project_root,
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
        project_root: Option<&Path>,
        step_id: &str,
        compiled_request_id: &str,
        provider_id: &str,
        model_id: &str,
        attempt_number: i64,
    ) -> Result<ProviderSubmissionHandle, AppError> {
        Self::submit_prepared_request(
            request,
            project_root,
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
        project_root: Option<&Path>,
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
            project_root,
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
        project_root: Option<&Path>,
        credentials: Option<&dyn CredentialStore>,
        step_id: &str,
        compiled_request_id: &str,
        provider_id: &str,
        model_id: &str,
        attempt_number: i64,
    ) -> Result<ProviderSubmissionHandle, AppError> {
        let mut registry = ProviderRegistry::builtin();
        match provider_id {
            // Local test/diagnostic providers need no external credential.
            // `fake_async_video` is the deterministic async video provider
            // used by the offline video golden path and provider tests.
            "mock" | "dry_run" | "fake_async_video" => {}
            "openai" => {
                let token = Self::resolve_openai_token(project_root, credentials)?;
                registry.register_arc(Self::openai_builtin_adapter(token));
            }
            _ => {
                // A user-defined AI service: build a real HTTP adapter from
                // the stored definition and its vault credential.
                let root = project_root.ok_or_else(|| {
                    AppError::ProviderConfiguration(
                        "project context is unavailable for provider execution".into(),
                    )
                })?;
                let adapter = Self::custom_execution_adapter(root, credentials, provider_id, true)?;
                registry.register_arc(adapter);
            }
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
        cancellation::register(provider_id, &submission.job.provider_job_id);
        Ok(ProviderSubmissionHandle {
            provider_id: provider_id.into(),
            adapter_version: provider.adapter_version(),
            provider,
            submission,
        })
    }

    /// Built-in "openai" credential resolution: the project's OS-keyring
    /// secret first, then the developer-time OPENAI_API_KEY environment
    /// variable. Raw secrets never leave this function.
    fn resolve_openai_token(
        project_root: Option<&Path>,
        credentials: Option<&dyn CredentialStore>,
    ) -> Result<String, AppError> {
        let store_owned;
        let store: &dyn CredentialStore = match credentials {
            Some(store) => store,
            None => {
                store_owned = KeyringCredentialStore::new();
                &store_owned
            }
        };
        if let Some(root) = project_root {
            let conn = db::open_existing_connection(&root.join("project.db"))?;
            let project_id = read_project(&conn)?.id;
            let account = credential_account(&project_id, "openai");
            if let Some(secret) = resolve_configured_secret_impl(store, &account)? {
                if !secret.trim().is_empty() {
                    return Ok(secret);
                }
            }
        }
        std::env::var("OPENAI_API_KEY").map_err(|_| {
            AppError::ProviderConfiguration(
                "the OpenAI credential is not configured for this project".into(),
            )
        })
    }

    /// The builtin `openai` provider, compiled from the openai-compatible
    /// preset like any other declarative provider.
    pub fn openai_builtin_adapter(token: String) -> Arc<dyn GenerationProvider> {
        let preset = preset_by_id("openai-compatible")
            .expect("the openai-compatible preset ships with the binary");
        Arc::new(DeclarativeProvider::new(
            "openai",
            "https://api.openai.com/v1",
            vec![CustomProviderModel {
                id: OPENAI_DEFAULT_MODEL.into(),
                name: "GPT Image 2".into(),
                capabilities: Vec::new(),
            }],
            preset.runtime.clone(),
            Some(token),
            BTreeMap::new(),
            Arc::new(UreqExecutor::new(Duration::from_secs(60))),
        ))
    }

    /// Builds a real HTTP execution adapter for a user-defined AI service
    /// from its stored declarative definition. `require_credential` is false
    /// when only capabilities are needed.
    fn custom_execution_adapter(
        project_root: &Path,
        credentials: Option<&dyn CredentialStore>,
        provider_id: &str,
        require_credential: bool,
    ) -> Result<Arc<dyn GenerationProvider>, AppError> {
        let definition = Self::load_custom_definition(project_root, provider_id)?;
        let definition = definition.ok_or_else(|| {
            AppError::ProviderConfiguration(format!(
                "provider {provider_id} is not a configured AI service"
            ))
        })?;
        let mut api_key = String::new();
        let mut header_values: BTreeMap<String, String> = BTreeMap::new();
        if require_credential {
            let store_owned;
            let store: &dyn CredentialStore = match credentials {
                Some(store) => store,
                None => {
                    store_owned = KeyringCredentialStore::new();
                    &store_owned
                }
            };
            let conn = db::open_existing_connection(&project_root.join("project.db"))?;
            let project_id = read_project(&conn)?.id;
            drop(conn);
            let account = credential_account(&project_id, provider_id);
            if let Some(secret) = resolve_configured_secret_impl(store, &account)? {
                if !secret.trim().is_empty() {
                    api_key = secret;
                }
            }
            if api_key.is_empty() {
                for header in &definition.headers {
                    if header.name.eq_ignore_ascii_case("authorization") {
                        if let Some(value) =
                            resolve_header_secret(store, &project_id, provider_id, &header.name)?
                        {
                            api_key = value;
                            break;
                        }
                    }
                }
            }
            if api_key.is_empty() && definition.runtime.auth.requires_credential() {
                return Err(AppError::ProviderConfiguration(format!(
                    "provider {provider_id} has no API key configured. Add the key in AI Services, then run this step again."
                )));
            }
            for header in &definition.headers {
                if let Some(value) =
                    resolve_header_secret(store, &project_id, provider_id, &header.name)?
                {
                    header_values.insert(header.name.clone(), value);
                }
            }
        }
        let transport = Arc::new(UreqExecutor::new(Duration::from_secs(120)));
        Ok(Arc::new(DeclarativeProvider::new(
            provider_id,
            definition.base_url.clone(),
            definition.models.clone(),
            definition.runtime.clone(),
            (!api_key.is_empty()).then_some(api_key),
            header_values,
            transport,
        )))
    }

    fn load_custom_definition(
        project_root: &Path,
        provider_id: &str,
    ) -> Result<Option<CustomProviderDefinition>, AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        super::repository::get_custom_provider(&conn, provider_id).map_err(Into::into)
    }

    /// Capability lookup that also resolves user-defined AI services. Used by
    /// the capability precheck in the workflow runtime and the provider picker.
    pub fn capabilities_for(
        project_root: &Path,
        provider_id: &str,
    ) -> Result<ProviderCapabilities, AppError> {
        match ProviderRegistry::builtin().get(provider_id) {
            Ok(provider) => Ok(provider.capabilities()),
            Err(_) => {
                let adapter =
                    Self::custom_execution_adapter(project_root, None, provider_id, false)?;
                Ok(adapter.capabilities())
            }
        }
    }

    /// Default model for a provider, from its config record or its custom
    /// definition's first model. No credential access is required.
    pub fn default_model_for(
        project_root: &Path,
        provider_id: &str,
    ) -> Result<Option<String>, AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let config = super::repository::get_provider_config(&conn, provider_id)?;
        let custom = super::repository::get_custom_provider(&conn, provider_id)?;
        Ok(config
            .and_then(|record| record.default_model)
            .or_else(|| custom.and_then(|definition| definition.models.first().map(|model| model.id.clone()))))
    }

    /// Waits for a submission to finish: polls with the provider's suggested
    /// interval, honors a wall-clock timeout, tolerates transient network
    /// failures, surfaces progress, and aborts promptly on cancellation.
    pub fn finish_submission(
        handle: &ProviderSubmissionHandle,
    ) -> Result<(ProviderJobStatus, ProviderResult), AppError> {
        let job = &handle.submission.job;
        let cancelled = || cancellation::is_cancelled(&job.provider_id, &job.provider_job_id);
        let options = FinishOptions {
            cancelled: Some(&cancelled),
            on_progress: None,
        };
        Self::finish_submission_with_options(handle, &options)
    }

    pub fn finish_submission_with_options(
        handle: &ProviderSubmissionHandle,
        options: &FinishOptions<'_>,
    ) -> Result<(ProviderJobStatus, ProviderResult), AppError> {
        let provider = &handle.provider;
        let submission = &handle.submission;
        let spec = provider.polling_spec();
        let deadline = std::time::Instant::now() + spec.timeout;
        let mut status = provider.poll(&submission.job).map_err(provider_error)?;
        let mut saw_progress = matches!(
            status.lifecycle,
            ProviderLifecycle::Queued
                | ProviderLifecycle::Submitted
                | ProviderLifecycle::Running
        );
        loop {
            if options.cancelled.is_some_and(|check| check()) {
                return Err(AppError::ProviderExecution(
                    "generation was cancelled".into(),
                ));
            }
            match status.lifecycle {
                ProviderLifecycle::Succeeded => break,
                ProviderLifecycle::Failed => {
                    let diagnostic = status
                        .diagnostic
                        .as_deref()
                        .map(|text| format!(": {}", crate::providers::redact_secret(text)))
                        .unwrap_or_default();
                    return Err(AppError::ProviderExecution(format!(
                        "the AI service reported the job failed{diagnostic}"
                    )));
                }
                ProviderLifecycle::Cancelled | ProviderLifecycle::CancellationRequested => {
                    return Err(AppError::ProviderExecution(
                        "generation was cancelled".into(),
                    ));
                }
                // One unreadable status before any progress is a hard failure
                // (a sync adapter that lost its result); after progress has
                // been observed, unknown statuses keep polling until timeout.
                ProviderLifecycle::Unknown if !saw_progress => {
                    return Err(AppError::ProviderExecution(format!(
                        "provider {} ended in an unknown state",
                        handle.provider_id
                    )));
                }
                _ => {}
            }
            if let Some(on_progress) = options.on_progress {
                on_progress(&status);
            }
            saw_progress = true;
            if std::time::Instant::now() + spec.interval >= deadline {
                return Err(AppError::ProviderExecution(format!(
                    "the AI service did not finish within {} seconds",
                    spec.timeout.as_secs()
                )));
            }
            std::thread::sleep(spec.interval);
            status = match provider.poll(&submission.job) {
                Ok(status) => status,
                Err(error) if error.kind.retryable() => continue,
                Err(error) => return Err(provider_error(error)),
            };
        }
        let result = provider
            .fetch_result(&submission.job)
            .map_err(provider_error)?;
        cancellation::unregister(&handle.provider_id, &submission.job.provider_job_id);
        Ok((status, result))
    }
}

/// Options for observing and interrupting the submission wait loop.
pub struct FinishOptions<'a> {
    /// Checked between polls; returning true aborts the wait.
    pub cancelled: Option<&'a dyn Fn() -> bool>,
    /// Invoked with every non-terminal status observation.
    pub on_progress: Option<&'a dyn Fn(&ProviderJobStatus)>,
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
    use crate::providers::http::TransportFailure;
    use crate::providers::presets::preset_by_id;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    /// Deterministic executor: records requests, returns a scripted status.
    #[derive(Default, Clone)]
    struct RecordingExecutor {
        requests: std::sync::Arc<Mutex<Vec<(String, String, Vec<(String, String)>)>>>,
        status: u16,
        body: String,
    }

    impl RecordingExecutor {
        fn with_status(status: u16) -> Self {
            Self {
                status,
                ..Default::default()
            }
        }

        fn with_status_and_body(status: u16, body: &str) -> Self {
            Self {
                status,
                body: body.into(),
                ..Default::default()
            }
        }
    }

    impl HttpExecutor for RecordingExecutor {
        fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportFailure> {
            self.requests.lock().unwrap().push((
                request.method.clone(),
                request.url.clone(),
                request.headers.clone(),
            ));
            Ok(HttpResponse {
                status: self.status,
                body: self.body.clone().into_bytes(),
                content_type: Some("application/json".into()),
                headers: vec![],
            })
        }
    }

    fn openai_compatible_definition(provider_id: &str) -> CustomProviderDefinition {
        let preset = preset_by_id("openai-compatible").unwrap();
        CustomProviderDefinition {
            provider_id: provider_id.into(),
            display_name: "Image Service".into(),
            base_url: "https://api.example.test/v1/".into(),
            purpose: CustomProviderPurpose::Image,
            preset_id: Some(preset.id.to_string()),
            runtime: preset.runtime.clone(),
            api_key: Some("sk-test-secret".into()),
            api_key_hint: None,
            models: vec![CustomProviderModel {
                id: "model-v1".into(),
                name: "Model V1".into(),
                capabilities: Vec::new(),
            }],
            headers: vec![],
        }
    }

    fn cloudflare_definition(provider_id: &str) -> CustomProviderDefinition {
        let preset = preset_by_id("cloudflare-workers-ai").unwrap();
        let mut runtime = preset.runtime.clone();
        runtime.account_id = Some("acc-123".into());
        CustomProviderDefinition {
            provider_id: provider_id.into(),
            display_name: "Cloudflare".into(),
            base_url: preset.default_base_url.into(),
            purpose: CustomProviderPurpose::Image,
            preset_id: Some(preset.id.to_string()),
            runtime,
            api_key: Some("cf-token".into()),
            api_key_hint: None,
            models: vec![CustomProviderModel {
                id: "@cf/black-forest-labs/flux-1-schnell".into(),
                name: "FLUX.1 Schnell".into(),
                capabilities: Vec::new(),
            }],
            headers: vec![],
        }
    }

    #[test]
    fn connection_test_uses_validate_operation_with_configured_auth() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Probe").unwrap();
        let credentials = MemoryCredentialStore::new();
        ProviderService::upsert_custom_provider(
            &root,
            &credentials,
            &openai_compatible_definition("image_provider"),
        )
        .unwrap();
        let transport = RecordingExecutor::with_status(200);

        let result =
            ProviderService::test_connection(&root, &credentials, transport.clone(), "image_provider")
                .unwrap();

        assert!(result.connected);
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.endpoint, "https://api.example.test/v1/models");
        let (method, url, headers) = transport.requests.lock().unwrap().remove(0);
        assert_eq!(method, "GET");
        assert_eq!(url, "https://api.example.test/v1/models");
        assert!(headers.contains(&("Authorization".into(), "Bearer sk-test-secret".into())));
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("sk-test-secret"));
    }

    #[test]
    fn cloudflare_validation_uses_tiny_generation_not_models_endpoint() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Cloudflare").unwrap();
        let credentials = MemoryCredentialStore::new();
        ProviderService::upsert_custom_provider(
            &root,
            &credentials,
            &cloudflare_definition("cloudflare"),
        )
        .unwrap();
        let transport = RecordingExecutor::with_status(200);

        let result =
            ProviderService::test_connection(&root, &credentials, transport.clone(), "cloudflare").unwrap();

        assert!(result.connected);
        let (method, url, headers) = transport.requests.lock().unwrap().remove(0);
        assert_eq!(method, "POST", "Cloudflare validates via a tiny generation");
        assert_eq!(
            url,
            "https://api.cloudflare.com/client/v4/accounts/acc-123/ai/run/@cf/black-forest-labs/flux-1-schnell"
        );
        assert!(headers.contains(&("Authorization".into(), "Bearer cf-token".into())));
    }

    #[test]
    fn connection_test_surfaces_provider_error_messages() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Error body").unwrap();
        let credentials = MemoryCredentialStore::new();
        ProviderService::upsert_custom_provider(
            &root,
            &credentials,
            &cloudflare_definition("cloudflare_err"),
        )
        .unwrap();
        let transport = RecordingExecutor::with_status_and_body(
            400,
            r#"{"errors":[{"message":"prompt is required","code":7002}]}"#,
        );

        let result =
            ProviderService::test_connection(&root, &credentials, transport.clone(), "cloudflare_err")
                .unwrap();

        assert!(!result.connected);
        assert_eq!(result.status_code, Some(400));
        assert!(result.message.contains("prompt is required"));
        assert!(result.message.contains("HTTP 400"));
    }

    #[test]
    fn auth_errors_are_reported_specifically() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Auth errors").unwrap();
        let credentials = MemoryCredentialStore::new();
        ProviderService::upsert_custom_provider(
            &root,
            &credentials,
            &openai_compatible_definition("auth_provider"),
        )
        .unwrap();
        for status in [401u16, 403] {
            let transport = RecordingExecutor::with_status(status);
            let result =
                ProviderService::test_connection(&root, &credentials, transport.clone(), "auth_provider")
                    .unwrap();
            assert!(!result.connected);
            assert_eq!(result.status_code, Some(status));
        }
    }

    #[test]
    fn connection_test_rejects_provider_without_any_auth_before_network_io() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "No auth").unwrap();
        let credentials = MemoryCredentialStore::new();
        let mut definition = openai_compatible_definition("no_auth");
        definition.api_key = None;
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();
        let transport = RecordingExecutor::default();

        let error = ProviderService::test_connection(&root, &credentials, transport.clone(), "no_auth")
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("no API key or authentication header"));
        assert!(transport.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn providers_without_validation_get_configuration_only_checks() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Config only").unwrap();
        let credentials = MemoryCredentialStore::new();
        let mut definition = openai_compatible_definition("config_only");
        definition
            .runtime
            .operations
            .remove(crate::providers::config::OPERATION_VALIDATE);
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();
        let transport = RecordingExecutor::default();

        let result =
            ProviderService::test_connection(&root, &credentials, transport.clone(), "config_only")
                .unwrap();

        assert!(result.connected);
        assert_eq!(result.status_code, None);
        assert!(transport.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn provider_definitions_expose_masked_hint_without_the_secret() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Hint").unwrap();
        let credentials = MemoryCredentialStore::new();
        let mut definition = openai_compatible_definition("hinted");
        definition.api_key = Some("sk-j9mlQwErTyXzray".into());
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
    }

    #[test]
    fn removing_custom_header_metadata_removes_its_vault_secret() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Header cleanup").unwrap();
        let credentials = MemoryCredentialStore::new();
        let mut definition = openai_compatible_definition("header_provider");
        definition.headers = vec![CustomProviderHeader {
            name: "X-API-Key".into(),
            value: Some("header-secret".into()),
        }];
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
    fn connection_test_revalidates_persisted_metadata_before_resolving_secrets() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Tampered metadata").unwrap();
        let credentials = MemoryCredentialStore::new();
        ProviderService::upsert_custom_provider(
            &root,
            &credentials,
            &openai_compatible_definition("tampered"),
        )
        .unwrap();
        let conn = ProviderService::open_project_conn(&root).unwrap();
        conn.execute(
            "UPDATE custom_provider_definitions SET base_url = 'https://user:secret@evil.example.test/v1' WHERE provider_id = 'tampered'",
            [],
        )
        .unwrap();
        let transport = RecordingExecutor::default();

        let error = ProviderService::test_connection(&root, &credentials, transport.clone(), "tampered")
            .unwrap_err();

        assert!(error.to_string().contains("must not contain credentials"));
        assert!(transport.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn changing_endpoint_authority_requires_explicit_credential_reentry() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Authority change").unwrap();
        let credentials = MemoryCredentialStore::new();
        let mut definition = openai_compatible_definition("moving");
        definition.headers = vec![CustomProviderHeader {
            name: "X-API-Key".into(),
            value: Some("header-old".into()),
        }];
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();
        definition.base_url = "https://second.example.test/v1".into();
        definition.api_key = None;
        definition.headers[0].value = None;

        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();

        let conn = ProviderService::open_project_conn(&root).unwrap();
        let project_id = ProviderService::project_id(&conn).unwrap();
        assert!(credentials
            .get_secret(&credential_account(&project_id, "moving"))
            .unwrap()
            .is_none());
        assert!(credentials
            .get_secret(&header_credential_account(
                &project_id,
                "moving",
                "X-API-Key"
            ))
            .unwrap()
            .is_none());
        let transport = RecordingExecutor::default();
        assert!(ProviderService::test_connection(&root, &credentials, transport.clone(), "moving").is_err());
    }

    #[test]
    fn deleting_custom_provider_removes_config_and_all_vault_entries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Delete cleanup").unwrap();
        let credentials = MemoryCredentialStore::new();
        let mut definition = openai_compatible_definition("delete_me");
        definition.headers = vec![CustomProviderHeader {
            name: "X-Key".into(),
            value: Some("header-delete".into()),
        }];
        ProviderService::upsert_custom_provider(&root, &credentials, &definition).unwrap();
        let conn = ProviderService::open_project_conn(&root).unwrap();
        let project_id = ProviderService::project_id(&conn).unwrap();
        drop(conn);

        ProviderService::delete_custom_provider(&root, &credentials, "delete_me").unwrap();

        let conn = ProviderService::open_project_conn(&root).unwrap();
        assert!(
            super::super::repository::get_custom_provider(&conn, "delete_me")
                .unwrap()
                .is_none()
        );
        assert!(
            super::super::repository::get_provider_config(&conn, "delete_me")
                .unwrap()
                .is_none()
        );
        assert!(credentials
            .get_secret(&credential_account(&project_id, "delete_me"))
            .unwrap()
            .is_none());
        assert!(credentials
            .get_secret(&header_credential_account(
                &project_id,
                "delete_me",
                "X-Key"
            ))
            .unwrap()
            .is_none());
    }

    #[test]
    fn legacy_purpose_rows_synthesize_openai_compatible_operations() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Legacy").unwrap();
        let credentials = MemoryCredentialStore::new();
        // Insert a pre-refactor row exactly as the old code wrote it: no
        // definition_json, purpose 'image', models without capabilities.
        let conn = ProviderService::open_project_conn(&root).unwrap();
        conn.execute(
            "INSERT INTO custom_provider_definitions
             (provider_id, display_name, base_url, purpose, models_json, headers_json, created_at, updated_at)
             VALUES ('legacy_img', 'Legacy', 'https://old.example.test/v1', 'image',
                     '[{\"id\":\"old-model\",\"name\":\"Old Model\"}]', '[]', 'now', 'now')",
            [],
        )
        .unwrap();
        drop(conn);

        let definition = super::super::repository::get_custom_provider(
            &ProviderService::open_project_conn(&root).unwrap(),
            "legacy_img",
        )
        .unwrap()
        .unwrap();
        assert!(definition
            .runtime
            .operations
            .contains_key(crate::providers::config::OPERATION_IMAGE_GENERATE));
        assert!(definition
            .runtime
            .operations
            .contains_key(crate::providers::config::OPERATION_IMAGE_EDIT));

        // The synthesized provider validates exactly like the old adapter:
        // a GET /models probe with the resolved Bearer credential.
        ProviderService::save_credential(
            &root,
            &credentials,
            "legacy_img",
            "sk-legacy",
            Some("old-model"),
        )
        .unwrap();
        let transport = RecordingExecutor::with_status(200);
        let result = ProviderService::test_connection(&root, &credentials, transport.clone(), "legacy_img")
            .unwrap();
        assert!(result.connected, "legacy rows validate through /models");
        assert_eq!(result.endpoint, "https://old.example.test/v1/models");
    }

    #[test]
    fn validation_probe_never_follows_redirects() {
        let redirect_target = TcpListener::bind("127.0.0.1:0").unwrap();
        redirect_target.set_nonblocking(true).unwrap();
        let target_address = redirect_target.local_addr().unwrap();
        let redirect_source = TcpListener::bind("127.0.0.1:0").unwrap();
        let source_address = redirect_source.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = redirect_source.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: http://{target_address}/models\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });
        let transport = UreqExecutor::without_redirects(Duration::from_secs(2));

        let response = transport
            .execute(
                HttpRequest::get(format!("http://{source_address}/models"))
                    .with_header("X-Api-Key", "must-not-follow"),
            )
            .unwrap();

        server.join().unwrap();
        assert_eq!(response.status, 302);
        assert!(
            matches!(redirect_target.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}

fn provider_error(error: ProviderError) -> AppError {
    // Human-readable, secret-free text the UI can show verbatim.
    AppError::ProviderExecution(error.display_text())
}
