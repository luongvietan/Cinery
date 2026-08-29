use super::{
    RawVisualQaResponse, VisualQaAdapter, VisualQaAdapterError, VisualQaAdapterErrorKind,
    VisualQaCapabilities,
};
use crate::providers::http::{HttpBody, HttpExecutor, HttpRequest, UreqExecutor};
use crate::qa::models::{VisualQaReference, VisualQaRequest};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

const MAX_MEDIA_BYTES: u64 = 20 * 1024 * 1024;

pub struct OpenAiCompatibleVisualQaAdapter {
    endpoint: String,
    bearer_token: String,
    model_id: String,
    execution_location: String,
    transport: Box<dyn HttpExecutor>,
}

impl OpenAiCompatibleVisualQaAdapter {
    pub fn new(
        endpoint: impl Into<String>,
        bearer_token: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Result<Self, VisualQaAdapterError> {
        Self::with_transport(
            endpoint,
            bearer_token,
            model_id,
            UreqExecutor::new(Duration::from_secs(120)),
        )
    }

    pub fn with_transport<T: HttpExecutor + 'static>(
        endpoint: impl Into<String>,
        bearer_token: impl Into<String>,
        model_id: impl Into<String>,
        transport: T,
    ) -> Result<Self, VisualQaAdapterError> {
        let endpoint = endpoint.into().trim_end_matches('/').to_string();
        let bearer_token = bearer_token.into();
        let model_id = model_id.into();
        if endpoint.trim().is_empty() {
            return Err(VisualQaAdapterError::new(
                VisualQaAdapterErrorKind::InvalidRequest,
                "Visual QA endpoint is required",
            ));
        }
        if !is_likely_multimodal_model(&model_id) {
            return Err(VisualQaAdapterError::new(
                VisualQaAdapterErrorKind::UnsupportedCapability,
                "QA blocked: selected model is not declared image-analysis capable",
            ));
        }
        let execution_location = endpoint_location(&endpoint);
        Ok(Self {
            endpoint,
            bearer_token,
            model_id,
            execution_location,
            transport: Box::new(transport),
        })
    }

    fn chat_endpoint(&self) -> String {
        format!("{}/chat/completions", self.endpoint)
    }

    fn request_body(&self, request: &VisualQaRequest) -> Result<Value, VisualQaAdapterError> {
        if request.target.media_type != "image" || request.response_schema_version != 1 {
            return Err(VisualQaAdapterError::new(
                VisualQaAdapterErrorKind::UnsupportedCapability,
                "Visual QA supports image input and response schema version 1 only",
            ));
        }
        let media_count = 1 + request.references.len();
        if media_count > self.capabilities().max_media_inputs {
            return Err(VisualQaAdapterError::new(
                VisualQaAdapterErrorKind::UnsupportedCapability,
                "Visual QA request contains too many declared media inputs",
            ));
        }

        let checks = serde_json::to_string(&request.checks).map_err(|error| {
            VisualQaAdapterError::new(
                VisualQaAdapterErrorKind::InvalidRequest,
                "Visual QA checks could not be serialized",
            )
            .with_diagnostic(error.to_string())
        })?;
        let mut content = vec![json!({
            "type": "text",
            "text": format!(
                "Target asset version: {}. Evaluate every supplied check exactly once. Do not invent requirements or check IDs. Use uncertain when evidence is insufficient. Return JSON only. Checks: {}",
                request.target.asset_version_id, checks
            )
        })];
        content.push(image_content(
            Path::new(&request.target.local_path),
            "target",
        )?);
        for reference in &request.references {
            content.push(reference_text(reference));
            content.push(image_content(
                Path::new(&reference.local_path),
                "reference",
            )?);
        }

        Ok(json!({
            "model": self.model_id,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a strict visual QA evaluator. Evaluate only the predefined checks, preserve character-left/right semantics, and output structured JSON only."
                },
                {"role": "user", "content": content}
            ],
            "response_format": {"type": "json_object"},
            "temperature": 0
        }))
    }
}

impl VisualQaAdapter for OpenAiCompatibleVisualQaAdapter {
    fn id(&self) -> &'static str {
        "openai_compatible_visual_qa"
    }

    fn adapter_version(&self) -> u32 {
        1
    }

    fn capabilities(&self) -> VisualQaCapabilities {
        VisualQaCapabilities {
            supports_image_analysis: true,
            supports_multiple_references: true,
            max_media_inputs: 32,
        }
    }

    fn execution_location(&self) -> String {
        self.execution_location.clone()
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn analyze(
        &self,
        request: &VisualQaRequest,
    ) -> Result<RawVisualQaResponse, VisualQaAdapterError> {
        if self.bearer_token.trim().is_empty() && self.execution_location != "local" {
            return Err(VisualQaAdapterError::new(
                VisualQaAdapterErrorKind::Authentication,
                "Visual QA credential is not configured",
            ));
        }
        let body = self.request_body(request)?;
        let request = HttpRequest {
            method: "POST".into(),
            url: self.chat_endpoint(),
            headers: vec![("Authorization".into(), format!("Bearer {}", self.bearer_token))],
            body: HttpBody::Json(body),
            max_response_bytes: 50 * 1024 * 1024,
        };
        let response = self
            .transport
            .execute(request)
            .map_err(|failure| {
                VisualQaAdapterError::new(
                    VisualQaAdapterErrorKind::Network,
                    "Visual QA request failed",
                )
                .with_diagnostic(redact(&failure.message, &self.bearer_token))
            })?;
        if !response.is_success() {
            return Err(VisualQaAdapterError::new(
                VisualQaAdapterErrorKind::Network,
                "Visual QA request was rejected",
            )
            .with_diagnostic(redact(&response.text(), &self.bearer_token)));
        }
        let document = response.json().map_err(|error| {
            VisualQaAdapterError::new(
                VisualQaAdapterErrorKind::MalformedResponse,
                "Visual QA response was not valid JSON",
            )
            .with_diagnostic(redact(&error, &self.bearer_token))
        })?;
        let response_text = document
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                VisualQaAdapterError::new(
                    VisualQaAdapterErrorKind::MalformedResponse,
                    "Visual QA response did not contain structured message content",
                )
            })?
            .to_string();
        Ok(RawVisualQaResponse {
            response_text,
            metadata: json!({
                "model": document.get("model"),
                "usage": document.get("usage"),
            }),
        })
    }
}

fn image_content(path: &Path, role: &str) -> Result<Value, VisualQaAdapterError> {
    let metadata = std::fs::metadata(path).map_err(|_| {
        VisualQaAdapterError::new(
            VisualQaAdapterErrorKind::InvalidRequest,
            format!("Declared {role} image is unavailable"),
        )
    })?;
    if metadata.len() > MAX_MEDIA_BYTES {
        return Err(VisualQaAdapterError::new(
            VisualQaAdapterErrorKind::InvalidRequest,
            format!("Declared {role} image exceeds 20 MiB"),
        ));
    }
    let bytes = std::fs::read(path).map_err(|_| {
        VisualQaAdapterError::new(
            VisualQaAdapterErrorKind::InvalidRequest,
            format!("Declared {role} image could not be read"),
        )
    })?;
    let mime_type = mime_guess::from_path(path)
        .first_raw()
        .filter(|mime| mime.starts_with("image/"))
        .unwrap_or("image/png");
    Ok(json!({
        "type": "image_url",
        "image_url": {"url": format!("data:{mime_type};base64,{}", STANDARD.encode(bytes))}
    }))
}

fn reference_text(reference: &VisualQaReference) -> Value {
    json!({
        "type": "text",
        "text": format!(
            "Reference asset version {} purpose: {}",
            reference.asset_version_id, reference.purpose
        )
    })
}

fn endpoint_location(endpoint: &str) -> String {
    let authority = endpoint
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(endpoint)
        .split('/')
        .next()
        .unwrap_or_default();
    let host = authority
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(authority)
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    if is_local_host(&host) {
        "local".into()
    } else {
        format!("cloud:{host}")
    }
}

fn is_local_host(host: &str) -> bool {
    if matches!(host, "localhost" | "::1")
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.ends_with(".local")
    {
        return true;
    }
    let octets = host
        .split('.')
        .map(str::parse::<u8>)
        .collect::<Result<Vec<_>, _>>();
    matches!(octets.as_deref(), Ok([172, second, _, _]) if (16..=31).contains(second))
}

fn is_likely_multimodal_model(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    [
        "gpt-4o", "gpt-4.1", "gpt-5", "vision", "vl", "llava", "pixtral", "gemma-3", "claude-3",
        "claude-4", "mock-vlm",
    ]
    .iter()
    .any(|marker| model.contains(marker))
}

fn redact(diagnostic: &str, secret: &str) -> String {
    if secret.is_empty() {
        diagnostic.to_string()
    } else {
        diagnostic.replace(secret, "[REDACTED]")
    }
}
