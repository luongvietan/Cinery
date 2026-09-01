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

/// A small handle around the boxed provider error body.
///
/// The body carries seven fields and would exceed Clippy's
/// `result_large_err` threshold if returned inline from the many
/// `Result<_, ProviderError>` provider APIs; boxing keeps every such
/// `Result` two pointers wide while preserving the public shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderError(Box<ProviderErrorBody>);

/// The full provider failure payload. Serialized exactly as the previous
/// inline `ProviderError` struct (camelCase, optional fields omitted when
/// absent), so persisted records and frontend contracts are unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderErrorBody {
    pub kind: ProviderErrorKind,
    pub message: String,
    pub diagnostic: Option<String>,
    /// HTTP status when the failure came from a provider response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    /// The provider's own error text, extracted from its response body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_message: Option<String>,
    /// Provider request/job id when the response exposes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// The provider operation that failed (e.g. `image.generate`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
}

impl std::ops::Deref for ProviderError {
    type Target = ProviderErrorBody;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self(Box::new(ProviderErrorBody {
            kind,
            message: message.into(),
            diagnostic: None,
            status_code: None,
            provider_message: None,
            request_id: None,
            operation: None,
        }))
    }

    pub fn kind(&self) -> &ProviderErrorKind {
        &self.0.kind
    }

    pub fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.0.diagnostic = Some(redact_secret(&diagnostic.into()));
        self
    }

    pub fn with_status_code(mut self, status: u16) -> Self {
        self.0.status_code = Some(status);
        self
    }

    pub fn with_provider_message(mut self, message: impl Into<String>) -> Self {
        let message = message.into();
        let redacted = redact_secret(&message);
        if !redacted.trim().is_empty() {
            self.0.provider_message = Some(redacted);
        }
        self
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.0.request_id = Some(redact_secret(&request_id.into()));
        self
    }

    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.0.operation = Some(operation.into());
        self
    }

    /// Single human-readable line the UI can show verbatim. Never echoes
    /// secrets; provider-supplied text is redacted on the way in.
    pub fn display_text(&self) -> String {
        let mut text = String::new();
        if let Some(operation) = &self.0.operation {
            text.push_str(&format!("{operation}: "));
        }
        text.push_str(&self.0.message);
        if let Some(status) = self.0.status_code {
            text.push_str(&format!(" (HTTP {status})"));
        }
        if let Some(provider_message) = &self.0.provider_message {
            text.push_str(&format!(" — provider said: {provider_message}"));
        }
        if let Some(request_id) = &self.0.request_id {
            text.push_str(&format!(" [request {request_id}]"));
        }
        if let Some(diagnostic) = &self.0.diagnostic {
            text.push_str(&format!(" ({diagnostic})"));
        }
        text
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
    fn provider_error_is_small_enough_for_result_large_err() {
        assert!(std::mem::size_of::<ProviderError>() <= 16);
    }

    #[test]
    fn provider_error_serializes_with_the_legacy_inline_shape() {
        let error = ProviderError::new(ProviderErrorKind::RateLimited, "slow down")
            .with_status_code(429)
            .with_provider_message("quota")
            .with_request_id("req-9")
            .with_operation("image.generate");
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["kind"], "rate_limited");
        assert_eq!(json["message"], "slow down");
        assert_eq!(json["statusCode"], 429);
        assert_eq!(json["providerMessage"], "quota");
        assert_eq!(json["requestId"], "req-9");
        assert_eq!(json["operation"], "image.generate");
        let round_trip: ProviderError = serde_json::from_value(json).unwrap();
        assert_eq!(round_trip, error);
    }

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
        assert!(!error.diagnostic.clone().unwrap().contains("secret123"));
    }

    #[test]
    fn display_text_includes_status_provider_message_and_never_secrets() {
        let error = ProviderError::new(ProviderErrorKind::InvalidRequest, "validation failed")
            .with_status_code(400)
            .with_provider_message("prompt is required")
            .with_request_id("req-123")
            .with_operation("image.generate");
        let text = error.display_text();
        assert!(text.contains("image.generate"));
        assert!(text.contains("HTTP 400"));
        assert!(text.contains("prompt is required"));
        assert!(text.contains("req-123"));
        let secret_error = ProviderError::new(ProviderErrorKind::AuthenticationError, "rejected")
            .with_provider_message("bad key sk-j9mlQwErTyXzray");
        assert!(!secret_error.display_text().contains("sk-j9mlQwErTyXzray"));
    }

    #[test]
    fn provider_error_serializes_without_optional_noise() {
        let json = serde_json::to_value(
            ProviderError::new(ProviderErrorKind::NetworkError, "down")
                .with_operation("video.generate"),
        )
        .unwrap();
        assert!(json.get("statusCode").is_none());
        assert!(json.get("providerMessage").is_none());
        assert_eq!(json["operation"], "video.generate");
    }
}
