use std::io::Read;
use std::time::Duration;

/// One part of a multipart/form-data request.
#[derive(Debug, Clone)]
pub struct MultipartPart {
    pub field_name: String,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// A multipart/form-data HTTP request (used for provider edit endpoints).
#[derive(Debug, Clone)]
pub struct MultipartHttpRequest {
    pub endpoint: String,
    pub bearer_token: String,
    pub parts: Vec<MultipartPart>,
    pub max_response_bytes: usize,
}

/// Builds a `multipart/form-data` body with a deterministic boundary.
pub fn encode_multipart(parts: &[MultipartPart]) -> Result<(Vec<u8>, String), String> {
    let boundary = format!("cinery-{}", ulid::Ulid::new().to_string());
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        if let Some(file_name) = &part.file_name {
            let content_type = part.content_type.clone().unwrap_or_else(|| "application/octet-stream".into());
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n", part.field_name, file_name).as_bytes(),
            );
            body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        } else {
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", part.field_name).as_bytes(),
            );
        }
        body.extend_from_slice(&part.bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((body, boundary))
}

pub trait HttpTransport: Send + Sync {
    fn post_json(&self, endpoint: &str, bearer_token: &str, body: &serde_json::Value) -> Result<serde_json::Value, String>;
    fn post_multipart(&self, request: MultipartHttpRequest) -> Result<serde_json::Value, String> {
        // Default implementation performs a real multipart POST through ureq.
        let (body, boundary) = encode_multipart(&request.parts)?;
        let response = ureq::post(&request.endpoint)
            .set("Authorization", &format!("Bearer {}", request.bearer_token))
            .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .send_bytes(&body)
            .map_err(multipart_request_error)?;
        read_json_response(response.into_reader(), request.max_response_bytes)
    }
    fn get_json(&self, endpoint: &str, bearer_token: &str) -> Result<serde_json::Value, String>;
    fn get_bytes(&self, endpoint: &str, bearer_token: &str, max_bytes: usize) -> Result<Vec<u8>, String>;
}

fn multipart_request_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let body = response.into_string().unwrap_or_default();
            format!("HTTP {code}: {body}")
        }
        ureq::Error::Transport(error) => error.to_string(),
    }
}

fn read_json_response(reader: impl Read, max_bytes: usize) -> Result<serde_json::Value, String> {
    let mut limited = reader.take((max_bytes + 1) as u64);
    let mut buffer = Vec::new();
    limited.read_to_end(&mut buffer).map_err(|error| error.to_string())?;
    if buffer.len() > max_bytes {
        return Err(format!("response exceeds {max_bytes} byte limit"));
    }
    serde_json::from_slice(&buffer).map_err(|error| error.to_string())
}

pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new(timeout: Duration) -> Self {
        Self {
            agent: ureq::AgentBuilder::new().timeout(timeout).build(),
        }
    }

    const DEFAULT_MAX_RESPONSE_BYTES: usize = 50 * 1024 * 1024;

    fn request_error(error: ureq::Error) -> String {
        match error {
            ureq::Error::Status(code, response) => {
                let body = response.into_string().unwrap_or_default();
                format!("HTTP {code}: {body}")
            }
            ureq::Error::Transport(error) => error.to_string(),
        }
    }
}

impl HttpTransport for UreqTransport {
    fn post_json(
        &self,
        endpoint: &str,
        bearer_token: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.agent
            .post(endpoint)
            .set("Authorization", &format!("Bearer {bearer_token}"))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(Self::request_error)?
            .into_json()
            .map_err(|error| error.to_string())
    }

    fn post_multipart(&self, request: MultipartHttpRequest) -> Result<serde_json::Value, String> {
        let max_bytes = if request.max_response_bytes == 0 { Self::DEFAULT_MAX_RESPONSE_BYTES } else { request.max_response_bytes };
        let (body, boundary) = encode_multipart(&request.parts)?;
        let response = self
            .agent
            .post(&request.endpoint)
            .set("Authorization", &format!("Bearer {}", request.bearer_token))
            .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .send_bytes(&body)
            .map_err(Self::request_error)?;
        read_json_response(response.into_reader(), max_bytes)
    }

    fn get_json(&self, endpoint: &str, bearer_token: &str) -> Result<serde_json::Value, String> {
        self.agent
            .get(endpoint)
            .set("Authorization", &format!("Bearer {bearer_token}"))
            .call()
            .map_err(Self::request_error)?
            .into_json()
            .map_err(|error| error.to_string())
    }

    fn get_bytes(
        &self,
        endpoint: &str,
        bearer_token: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>, String> {
        let response = self
            .agent
            .get(endpoint)
            .set("Authorization", &format!("Bearer {bearer_token}"))
            .call()
            .map_err(Self::request_error)?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take((max_bytes + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > max_bytes {
            return Err(format!("response exceeds {max_bytes} byte limit"));
        }
        Ok(bytes)
    }
}
