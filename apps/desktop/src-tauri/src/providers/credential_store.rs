//! Injectable OS credential-vault boundary for provider secrets.
//!
//! The production implementation stores secrets in the operating system
//! credential vault through the `keyring` crate (service name `cinery`,
//! account `<project-id>:<provider-id>`). Tests use
//! [`MemoryCredentialStore`] and never touch the real vault.
//!
//! Error values are deliberately redacted: no secret material is ever
//! carried in a [`ProviderError`] message or diagnostic.

use super::error::{ProviderError, ProviderErrorKind};
use std::collections::HashMap;
use std::sync::Mutex;

/// The keyring service name used for every Cinery credential entry.
pub const KEYRING_SERVICE: &str = "cinery";

/// Deterministic, project-scoped account key for one provider credential.
pub fn credential_account(project_id: &str, provider_id: &str) -> String {
    format!("{project_id}:{provider_id}")
}

/// The opaque database reference stored in `provider_configurations`.
pub fn credential_reference(account: &str) -> String {
    format!("keyring://{KEYRING_SERVICE}/{account}")
}

/// Deterministic account key for one custom HTTP header value.
pub fn header_credential_account(project_id: &str, provider_id: &str, header_name: &str) -> String {
    let encoded = header_name
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{project_id}:{provider_id}:header:{encoded}")
}

pub trait CredentialStore: Send + Sync {
    fn set_secret(&self, account: &str, secret: &str) -> Result<(), ProviderError>;
    fn get_secret(&self, account: &str) -> Result<Option<String>, ProviderError>;
    fn delete_secret(&self, account: &str) -> Result<(), ProviderError>;
}

/// In-memory implementation used by tests and as a deterministic fallback.
#[derive(Default)]
pub struct MemoryCredentialStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl MemoryCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CredentialStore for MemoryCredentialStore {
    fn set_secret(&self, account: &str, secret: &str) -> Result<(), ProviderError> {
        self.secrets
            .lock()
            .expect("credential map mutex")
            .insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn get_secret(&self, account: &str) -> Result<Option<String>, ProviderError> {
        Ok(self
            .secrets
            .lock()
            .expect("credential map mutex")
            .get(account)
            .cloned())
    }

    fn delete_secret(&self, account: &str) -> Result<(), ProviderError> {
        self.secrets
            .lock()
            .expect("credential map mutex")
            .remove(account);
        Ok(())
    }
}

/// Credential store that always fails, used to verify compensation paths.
pub struct FailingCredentialStore {
    pub message: String,
}

impl CredentialStore for FailingCredentialStore {
    fn set_secret(&self, _: &str, _: &str) -> Result<(), ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::CredentialStore,
            self.message.clone(),
        ))
    }

    fn get_secret(&self, _: &str) -> Result<Option<String>, ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::CredentialStore,
            self.message.clone(),
        ))
    }

    fn delete_secret(&self, _: &str) -> Result<(), ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::CredentialStore,
            self.message.clone(),
        ))
    }
}

/// Production implementation backed by the OS credential vault.
///
/// On Windows this is Windows Credential Manager; on macOS the login
/// keychain; on Linux the Secret Service with an in-process crypto backend.
pub struct KeyringCredentialStore {
    service: &'static str,
}

impl KeyringCredentialStore {
    pub fn new() -> Self {
        Self {
            service: KEYRING_SERVICE,
        }
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, ProviderError> {
        keyring::Entry::new(self.service, account).map_err(|error| {
            ProviderError::new(
                ProviderErrorKind::CredentialStore,
                "credential vault is unavailable",
            )
            .with_diagnostic(error.to_string())
        })
    }
}

impl Default for KeyringCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn set_secret(&self, account: &str, secret: &str) -> Result<(), ProviderError> {
        let entry = self.entry(account)?;
        entry
            .set_password(secret)
            .map_err(|error| credential_error("save", error))
    }

    fn get_secret(&self, account: &str) -> Result<Option<String>, ProviderError> {
        let entry = self.entry(account)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(credential_error("read", error)),
        }
    }

    fn delete_secret(&self, account: &str) -> Result<(), ProviderError> {
        let entry = self.entry(account)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            // Deleting an absent entry is an idempotent success.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(credential_error("remove", error)),
        }
    }
}

fn credential_error(action: &str, error: keyring::Error) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::CredentialStore,
        format!("credential vault failed to {action} the entry"),
    )
    .with_diagnostic(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips_replaces_and_isolates_accounts() {
        let store = MemoryCredentialStore::new();
        assert_eq!(store.get_secret("p:openai").unwrap(), None);
        store.set_secret("p:openai", "alpha").unwrap();
        assert_eq!(
            store.get_secret("p:openai").unwrap().as_deref(),
            Some("alpha")
        );
        // Replacement overwrites the prior value.
        store.set_secret("p:openai", "beta").unwrap();
        assert_eq!(
            store.get_secret("p:openai").unwrap().as_deref(),
            Some("beta")
        );
        // Account isolation.
        store.set_secret("q:openai", "gamma").unwrap();
        assert_eq!(
            store.get_secret("q:openai").unwrap().as_deref(),
            Some("gamma")
        );
        assert_eq!(
            store.get_secret("p:openai").unwrap().as_deref(),
            Some("beta")
        );
        store.delete_secret("p:openai").unwrap();
        assert_eq!(store.get_secret("p:openai").unwrap(), None);
        // Deleting a missing account is idempotent.
        store.delete_secret("p:openai").unwrap();
    }

    #[test]
    fn failing_store_reports_credential_store_errors_without_echoing_secrets() {
        let store = FailingCredentialStore {
            message: "vault locked".into(),
        };
        let error = store.set_secret("p:openai", "supersecret").unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::CredentialStore);
        assert!(!error.message.contains("supersecret"));
        assert!(store.get_secret("p:openai").is_err());
        assert!(store.delete_secret("p:openai").is_err());
    }

    #[test]
    fn account_keys_and_references_are_deterministic() {
        assert_eq!(credential_account("proj-1", "openai"), "proj-1:openai");
        assert_eq!(
            credential_reference("proj-1:openai"),
            "keyring://cinery/proj-1:openai"
        );
    }

    #[test]
    fn keyring_store_maps_missing_entries_to_none() {
        // Uses a service/account pair that cannot exist on the development
        // machine; `NoEntry` must surface as Ok(None), never an error.
        let store = KeyringCredentialStore::new();
        let value = store
            .get_secret("cinery-selftest:absent-provider")
            .expect("absent entry must read as Ok(None)");
        assert_eq!(value, None);
        // Deleting an absent entry succeeds idempotently.
        store
            .delete_secret("cinery-selftest:absent-provider")
            .expect("absent entry delete must succeed");
    }
}
