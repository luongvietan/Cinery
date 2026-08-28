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
    for key in ["Authorization", "authorization", "api_key", "apiKey", "token"] {
        if let Some(start) = redacted.find(key) {
            if let Some(separator) = redacted[start..].find([':', '=']) {
                let mut value_start = start + separator + 1;
                while redacted[value_start..]
                    .chars()
                    .next()
                    .map(|character| character.is_whitespace())
                    .unwrap_or(false)
                {
                    value_start += redacted[value_start..]
                        .chars()
                        .next()
                        .unwrap()
                        .len_utf8();
                }
                let credential_start = value_start;
                if redacted[value_start..].starts_with("Bearer ") {
                    value_start += "Bearer ".len();
                }
                let value_end = redacted[value_start..]
                    .find([',', ' ', '&', '\n'])
                    .map(|offset| value_start + offset)
                    .unwrap_or(redacted.len());
                let replace_start = if credential_start < value_start {
                    credential_start
                } else {
                    value_start
                };
                redacted.replace_range(replace_start..value_end, "[REDACTED]");
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
