use std::io::Read;
use std::time::Duration;

/// One part of a `multipart/form-data` request body.
#[derive(Debug, Clone)]
pub struct MultipartPart {
    pub field_name: String,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// Request body variants supported by the provider transport.
#[derive(Debug, Clone)]
pub enum HttpBody {
    None,
    Json(serde_json::Value),
    Multipart(Vec<MultipartPart>),
    Raw {
        bytes: Vec<u8>,
        content_type: String,
    },
}

/// A fully resolved HTTP request for a provider call. Authentication and
/// mapping have already been applied; the transport performs no credential
/// logic of its own.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: HttpBody,
    pub max_response_bytes: usize,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: "GET".into(),
            url: url.into(),
            headers: Vec::new(),
            body: HttpBody::None,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn with_json_body(mut self, body: serde_json::Value) -> Self {
        self.body = HttpBody::Json(body);
        self
    }

    pub fn with_max_response_bytes(mut self, max: usize) -> Self {
        self.max_response_bytes = max;
        self
    }
}

/// A provider HTTP response. Non-2xx statuses are *not* transport failures:
/// provider error bodies are first-class data for error normalization.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: Option<String>,
    pub headers: Vec<(String, String)>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn json(&self) -> Result<serde_json::Value, String> {
        serde_json::from_slice(&self.body).map_err(|error| error.to_string())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

/// A transport-level failure (DNS, connect, timeout, response too large).
/// Message text must be safe for display; call sites treat it as untrusted
/// provider/environment text and redact before persisting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportFailure {
    pub message: String,
}

impl TransportFailure {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TransportFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 50 * 1024 * 1024;

pub trait HttpExecutor: Send + Sync {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportFailure>;
}

/// Builds a `multipart/form-data` body with a deterministic boundary.
pub fn encode_multipart(parts: &[MultipartPart]) -> Result<(Vec<u8>, String), String> {
    let boundary = format!("cinery-{}", ulid::Ulid::new().to_string());
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        if let Some(file_name) = &part.file_name {
            let content_type = part
                .content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".into());
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                    part.field_name, file_name
                )
                .as_bytes(),
            );
            body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        } else {
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                    part.field_name
                )
                .as_bytes(),
            );
        }
        body.extend_from_slice(&part.bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((body, boundary))
}

pub struct UreqExecutor {
    agent: ureq::Agent,
}

impl UreqExecutor {
    pub fn new(timeout: Duration) -> Self {
        Self {
            agent: ureq::AgentBuilder::new().timeout(timeout).build(),
        }
    }

    /// Validation probes must not follow redirects so user credentials can
    /// never be forwarded to a redirect target.
    pub fn without_redirects(timeout: Duration) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(timeout)
                .redirects(0)
                .build(),
        }
    }

    fn transport_error(error: ureq::Error) -> TransportFailure {
        match error {
            // Statuses are responses, not transport failures.
            ureq::Error::Status(_, _) => {
                unreachable!("status errors are handled by the caller")
            }
            ureq::Error::Transport(inner) => TransportFailure::new(inner.to_string()),
        }
    }
}

impl HttpExecutor for UreqExecutor {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportFailure> {
        let mut builder = self
            .agent
            .request(&request.method, &request.url)
            .timeout(Duration::from_secs(300));
        for (name, value) in &request.headers {
            builder = builder.set(name, value);
        }
        let response = match &request.body {
            HttpBody::None => builder.call(),
            HttpBody::Json(value) => builder
                .set("Content-Type", "application/json")
                .send_json(value.clone()),
            HttpBody::Multipart(parts) => {
                let (bytes, boundary) = encode_multipart(parts).map_err(TransportFailure::new)?;
                builder
                    .set(
                        "Content-Type",
                        &format!("multipart/form-data; boundary={boundary}"),
                    )
                    .send_bytes(&bytes)
            }
            HttpBody::Raw {
                bytes,
                content_type,
            } => builder.set("Content-Type", content_type).send_bytes(bytes),
        };
        match response {
            Ok(response) => read_response(response, request.max_response_bytes),
            Err(ureq::Error::Status(status, response)) => {
                read_response(response, request.max_response_bytes)
                    .map(|mut parsed| {
                        parsed.status = status;
                        parsed
                    })
                    .map_err(|_| {
                        TransportFailure::new(format!(
                            "HTTP {status} response body could not be read"
                        ))
                    })
            }
            Err(error) => Err(Self::transport_error(error)),
        }
    }
}

fn read_response(
    response: ureq::Response,
    max_bytes: usize,
) -> Result<HttpResponse, TransportFailure> {
    let status = response.status();
    let content_type = response
        .header("content-type")
        .map(|value| value.to_string());
    let headers: Vec<(String, String)> = response
        .headers_names()
        .into_iter()
        .filter_map(|name| {
            response
                .header(&name)
                .map(|value| (name.to_ascii_lowercase(), value.to_string()))
        })
        .collect();
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take((max_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| TransportFailure::new(error.to_string()))?;
    if bytes.len() > max_bytes {
        return Err(TransportFailure::new(format!(
            "response exceeds {max_bytes} byte limit"
        )));
    }
    Ok(HttpResponse {
        status,
        body: bytes,
        content_type,
        headers,
    })
}

/// Stateless download helper for provider output URLs (no credentials, 60 s
/// timeout). Used by artifact capture and ingestion for remote results.
pub fn download_bytes(url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    let executor = UreqExecutor::new(Duration::from_secs(60));
    let response = executor
        .execute(HttpRequest::get(url).with_max_response_bytes(max_bytes))
        .map_err(|failure| failure.message)?;
    if !response.is_success() {
        return Err(format!("HTTP {}", response.status));
    }
    Ok(response.body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_encoding_covers_text_and_file_parts() {
        let parts = vec![
            MultipartPart {
                field_name: "prompt".into(),
                file_name: None,
                content_type: None,
                bytes: b"hello".to_vec(),
            },
            MultipartPart {
                field_name: "image".into(),
                file_name: Some("in.png".into()),
                content_type: Some("image/png".into()),
                bytes: vec![1, 2, 3],
            },
        ];
        let (body, boundary) = encode_multipart(&parts).unwrap();
        let text = String::from_utf8(body).unwrap();
        assert!(text.contains("name=\"prompt\"\r\n\r\nhello"));
        assert!(text.contains("name=\"image\"; filename=\"in.png\""));
        assert!(text.contains("Content-Type: image/png"));
        assert!(text.contains(&format!("--{boundary}--")));
    }
}
