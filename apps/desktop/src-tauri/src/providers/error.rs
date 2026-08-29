use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    AuthenticationError,
    AuthorizationError,
    InvalidRequest,
    UnsupportedCapability,
    RateLimited,
    QuotaExceeded,
    ProviderUnavailable,
    NetworkError,
    Timeout,
    RemoteJobFailed,
    RemoteJobNotFound,
    MalformedProviderResponse,
    ArtifactDownloadFailed,
    ArtifactValidationFailed,
    Cancelled,
    UnknownProviderError,
    CredentialStore,
}

impl ProviderErrorKind {
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited
                | Self::ProviderUnavailable
                | Self::NetworkError
                | Self::Timeout
                | Self::ArtifactDownloadFailed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: String,
    pub diagnostic: Option<String>,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            diagnostic: None,
        }
    }

    pub fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.diagnostic = Some(redact_secret(&diagnostic.into()));
        self
    }
}

pub fn redact_secret(value: &str) -> String {
    let mut redacted = value.to_string();
    // List of patterns that precede secrets
    let patterns = [
        "Authorization",
        "authorization",
        "api_key",
        "apiKey",
        "API_KEY",
        "token",
        "Token",
        "TOKEN",
        "x-api-key",
        "X-Api-Key",
        "X-API-KEY",
        "secret",
        "Secret",
        "SECRET",
        "credential",
        "Credential",
        "CREDENTIAL",
        "password",
        "Password",
        "PASSWORD",
    ];

    for pattern in &patterns {
        let mut search_start = 0;
        while let Some(start) = redacted[search_start..].find(pattern) {
            let actual_start = search_start + start;
            if let Some(separator) = redacted[actual_start..].find([':', '=']) {
                let mut value_start = actual_start + separator + 1;
                // Skip whitespace
                while redacted[value_start..]
                    .chars()
                    .next()
                    .map(|character| character.is_whitespace())
                    .unwrap_or(false)
                {
                    value_start += redacted[value_start..].chars().next().unwrap().len_utf8();
                }

                // Check if the value is quoted (starts with ")
                let is_quoted = redacted[value_start..].starts_with('"');
                if is_quoted {
                    value_start += 1; // Skip opening quote
                }

                let credential_start = if is_quoted {
                    value_start - 1 // Include the opening quote in redaction
                } else {
                    value_start
                };

                // Handle Bearer prefix
                if redacted[value_start..].starts_with("Bearer ") {
                    value_start += "Bearer ".len();
                }

                // Find the end of the secret value
                let value_end = if is_quoted {
                    // For quoted strings, look for the closing quote
                    redacted[value_start..]
                        .find('"')
                        .map(|offset| value_start + offset + 1) // Include closing quote
                        .unwrap_or(redacted.len())
                } else {
                    // For unquoted strings, look for delimiters
                    redacted[value_start..]
                        .find([',', ' ', '&', '\n', '"', '}'])
                        .map(|offset| value_start + offset)
                        .unwrap_or(redacted.len())
                };

                // Don't redact if the value is empty or just a placeholder
                if value_start < value_end || is_quoted {
                    redacted.replace_range(credential_start..value_end, "[REDACTED]");
                    // Start searching after the replacement to avoid re-processing
                    search_start = actual_start + "[REDACTED]".len();
                } else {
                    search_start = actual_start + pattern.len();
                }
            } else {
                search_start = actual_start + pattern.len();
            }
        }
    }

    // Second pass: redact common secret prefixes (like "sk-", "sk_")
    let secret_prefixes = ["sk-", "sk_", "sk.", "api-", "api_", "secret-"];
    for prefix in &secret_prefixes {
        let mut search_start = 0;
        while let Some(start) = redacted[search_start..].find(prefix) {
            let actual_start = search_start + start;
            // Find the end of the secret - look for quote, space, newline, comma, or other delimiters
            let value_end = redacted[actual_start..]
                .find(['"', ' ', '\n', ',', '}', ']', '&'])
                .map(|offset| actual_start + offset)
                .unwrap_or(redacted.len());

            if value_end > actual_start + prefix.len() {
                redacted.replace_range(actual_start..value_end, "[REDACTED]");
                search_start = actual_start + "[REDACTED]".len();
            } else {
                search_start = actual_start + prefix.len();
            }
        }
    }

    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_is_stable() {
        assert!(ProviderErrorKind::NetworkError.retryable());
        assert!(!ProviderErrorKind::InvalidRequest.retryable());
        assert!(!ProviderErrorKind::AuthenticationError.retryable());
    }

    #[test]
    fn diagnostics_redact_credentials() {
        let error = ProviderError::new(ProviderErrorKind::NetworkError, "request failed")
            .with_diagnostic("Authorization: Bearer secret123");
        assert!(!error.diagnostic.unwrap().contains("secret123"));
    }
}
