use crate::providers::error::redact_secret;
use serde_json::Value;

/// Central redaction layer for diagnostics and logs
pub struct DiagnosticsRedactor;

impl DiagnosticsRedactor {
    /// Redact all secrets from a diagnostic string
    pub fn redact_string(input: &str) -> String {
        redact_secret(input)
    }

    /// Redact all secrets from a JSON value
    pub fn redact_json(value: &Value) -> Value {
        match value {
            Value::Object(obj) => {
                let mut redacted = serde_json::Map::new();
                for (key, val) in obj {
                    let redacted_val = Self::redact_json(val);
                    redacted.insert(key.clone(), redacted_val);
                }
                Value::Object(redacted)
            }
            Value::Array(arr) => {
                let redacted: Vec<Value> = arr.iter().map(|v| Self::redact_json(v)).collect();
                Value::Array(redacted)
            }
            Value::String(s) => Value::String(Self::redact_string(s)),
            other => other.clone(),
        }
    }

    /// Redact secrets from error metadata
    pub fn redact_error_metadata(metadata: &str) -> String {
        Self::redact_string(metadata)
    }

    /// Redact secrets from HTTP headers
    pub fn redact_headers(headers: &str) -> String {
        Self::redact_string(headers)
    }

    /// Redact secrets from request/response bodies
    pub fn redact_body(body: &str) -> String {
        Self::redact_string(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_json_string_values_with_secrets() {
        let json_with_secret = json!({
            "Authorization": "Bearer sk-test-1234567890abcdefghij",
            "model": "gpt-4"
        });

        let redacted = DiagnosticsRedactor::redact_json(&json_with_secret);
        let redacted_str = redacted.to_string();

        assert!(!redacted_str.contains("sk-test-1234567890abcdefghij"));
        assert!(redacted_str.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_nested_json_secrets() {
        let nested = json!({
            "request": {
                "headers": {
                    "Authorization": "Bearer sk-test-secret",
                    "Content-Type": "application/json"
                },
                "body": {
                    "apiKey": "sk-test-secret"
                }
            }
        });

        let redacted = DiagnosticsRedactor::redact_json(&nested);
        let redacted_str = redacted.to_string();

        assert!(!redacted_str.contains("sk-test-secret"));
        assert!(redacted_str.matches("[REDACTED]").count() >= 2);
    }

    #[test]
    fn redacts_error_metadata() {
        let error_meta = "Error: HTTP 401 Unauthorized\nAuthorization: Bearer sk-test-secret";
        let redacted = DiagnosticsRedactor::redact_error_metadata(error_meta);

        assert!(!redacted.contains("sk-test-secret"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_http_headers() {
        let headers = "Authorization: Bearer sk-test-secret\nContent-Type: application/json";
        let redacted = DiagnosticsRedactor::redact_headers(headers);

        assert!(!redacted.contains("sk-test-secret"));
        assert!(redacted.contains("[REDACTED]"));
    }
}
