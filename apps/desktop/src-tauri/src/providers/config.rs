//! Declarative provider configuration: endpoint definitions, request/response
//! mappings, authentication modes, and async job lifecycles.
//!
//! Providers are described by data. The engine in this file compiles a
//! canonical generation request into a concrete HTTP call and extracts a
//! canonical result from the response — deterministically, with no user
//! script execution anywhere.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const OPERATION_IMAGE_GENERATE: &str = "image.generate";
pub const OPERATION_IMAGE_EDIT: &str = "image.edit";
pub const OPERATION_VIDEO_GENERATE: &str = "video.generate";
pub const OPERATION_VIDEO_IMAGE_TO_VIDEO: &str = "video.imageToVideo";
pub const OPERATION_VALIDATE: &str = "validate";

/// The operations a provider (or model) can implement. A provider implements
/// any subset; no operation implies another.
pub const ALL_OPERATIONS: &[&str] = &[
    OPERATION_IMAGE_GENERATE,
    OPERATION_IMAGE_EDIT,
    OPERATION_VIDEO_GENERATE,
    OPERATION_VIDEO_IMAGE_TO_VIDEO,
    OPERATION_VALIDATE,
];

pub fn is_known_operation(name: &str) -> bool {
    ALL_OPERATIONS.contains(&name)
}

/// Operations that produce images or video (i.e. generation, not validation).
pub fn operation_output_kind(operation: &str) -> Option<&'static str> {
    match operation {
        OPERATION_IMAGE_GENERATE | OPERATION_IMAGE_EDIT => Some("image"),
        OPERATION_VIDEO_GENERATE | OPERATION_VIDEO_IMAGE_TO_VIDEO => Some("video"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    #[default]
    None,
    /// `Authorization: Bearer {apiKey}`
    Bearer,
    /// `{credentialName}: {apiKey}`
    Header,
    /// `?{credentialName}={apiKey}`
    Query,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    #[serde(default)]
    pub mode: AuthMode,
    /// Header or query parameter name carrying the API key. Required for
    /// header/query modes; ignored otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_name: Option<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: AuthMode::None,
            credential_name: None,
        }
    }
}

impl AuthConfig {
    pub fn requires_credential(&self) -> bool {
        !matches!(self.mode, AuthMode::None)
    }

    pub fn validate(&self) -> Result<(), String> {
        match self.mode {
            AuthMode::None => Ok(()),
            AuthMode::Bearer => Ok(()),
            AuthMode::Header | AuthMode::Query => {
                let name = self.credential_name.as_deref().unwrap_or_default();
                if name.is_empty() {
                    return Err("header/query authentication requires a credential name".into());
                }
                if name.contains(['\r', '\n', ' ', ':', '=', '&', '?']) {
                    return Err("credential name must be a bare header or query name".into());
                }
                Ok(())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Request compilation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequestType {
    #[default]
    Json,
    Multipart,
    FormUrlEncoded,
}

/// A canonical generation input, provider-agnostic. Field names mirror the
/// canonical set the product uses across image and video operations.
#[derive(Debug, Clone, Default)]
pub struct CanonicalValues {
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub model: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub aspect_ratio: Option<String>,
    pub seed: Option<u64>,
    pub steps: Option<u32>,
    pub quality: Option<String>,
    pub duration_seconds: Option<f32>,
    pub fps: Option<u32>,
    pub strength: Option<f32>,
}

/// One verified reference attachment, rendered into mappings as a `data:` URI.
#[derive(Debug, Clone)]
pub struct ReferenceInput {
    pub file_name: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl ReferenceInput {
    pub fn as_data_uri(&self) -> String {
        format!(
            "data:{};base64,{}",
            self.media_type,
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &self.bytes)
        )
    }
}

const CANONICAL_NAMES: &[&str] = &[
    "prompt",
    "negativePrompt",
    "model",
    "width",
    "height",
    "aspectRatio",
    "seed",
    "steps",
    "quality",
    "duration",
    "fps",
    "strength",
    "size",
    "image",
    "images",
    "referenceImages",
];

pub fn is_known_canonical_name(name: &str) -> bool {
    CANONICAL_NAMES.contains(&name)
}

/// Look up a `{{name}}` placeholder in the canonical values.
/// Returns `None` when the canonical field is unset (the mapped field is
/// then omitted rather than sent empty).
pub fn canonical_lookup(
    values: &CanonicalValues,
    name: &str,
    references: &[ReferenceInput],
) -> Option<serde_json::Value> {
    let rendered = match name {
        "prompt" => serde_json::Value::String(values.prompt.clone()),
        "negativePrompt" => values
            .negative_prompt
            .clone()
            .map(serde_json::Value::String)?,
        "model" => serde_json::Value::String(values.model.clone()),
        "width" => values.width.map(|v| serde_json::json!(v))?,
        "height" => values.height.map(|v| serde_json::json!(v))?,
        "aspectRatio" => values.aspect_ratio.clone().map(serde_json::Value::String)?,
        "seed" => values.seed.map(|v| serde_json::json!(v))?,
        "steps" => values.steps.map(|v| serde_json::json!(v))?,
        "quality" => values.quality.clone().map(serde_json::Value::String)?,
        "duration" => values.duration_seconds.map(|v| serde_json::json!(v))?,
        "fps" => values.fps.map(|v| serde_json::json!(v))?,
        "strength" => values.strength.map(|v| serde_json::json!(v))?,
        // Convenience helpers derived from canonical fields.
        "size" => {
            let width = values.width?;
            let height = values.height?;
            serde_json::Value::String(format!("{width}x{height}"))
        }
        "image" => serde_json::Value::String(references.first()?.as_data_uri()),
        "images" | "referenceImages" => {
            if references.is_empty() {
                return None;
            }
            serde_json::Value::Array(
                references
                    .iter()
                    .map(|reference| serde_json::Value::String(reference.as_data_uri()))
                    .collect(),
            )
        }
        _ => return None,
    };
    Some(rendered)
}

/// Compiles a JSON request mapping template against canonical values.
///
/// Substitution rules (deterministic, no script execution):
/// - a string leaf that is exactly `{{name}}` becomes the typed canonical
///   value (numbers stay numbers, arrays stay arrays);
/// - a string leaf containing `{{name}}` among other text gets a string
///   substitution;
/// - a leaf referencing an unset canonical field is omitted entirely;
/// - unknown placeholder names are left untouched (they may be literal).
pub fn compile_json_template(
    template: &serde_json::Value,
    values: &CanonicalValues,
    references: &[ReferenceInput],
) -> serde_json::Value {
    match template {
        serde_json::Value::String(text) => compile_string_leaf(text, values, references),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| compile_json_template(item, values, references))
                .collect(),
        ),
        serde_json::Value::Object(map) => {
            let mut compiled = serde_json::Map::new();
            for (key, value) in map {
                let compiled_value = compile_json_template(value, values, references);
                // A `{{missing}}` whole-string leaf reports omitted; drop the key.
                if is_omitted_marker(&compiled_value) {
                    continue;
                }
                compiled.insert(key.clone(), compiled_value);
            }
            serde_json::Value::Object(compiled)
        }
        other => other.clone(),
    }
}

/// Internal marker for "this leaf references an unset canonical value".
/// A NUL-prefixed string can never originate from user configuration.
const OMITTED_MARKER: &str = "\u{0}__cinery_omitted__";

fn is_omitted_marker(value: &serde_json::Value) -> bool {
    matches!(value, serde_json::Value::String(text) if text == OMITTED_MARKER)
}

fn compile_string_leaf(
    text: &str,
    values: &CanonicalValues,
    references: &[ReferenceInput],
) -> serde_json::Value {
    let trimmed = text.trim();
    if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
        let name = &trimmed[2..trimmed.len() - 2];
        // Only known canonical names participate in omission; unknown
        // placeholders are left untouched (they may be literal text).
        if is_known_canonical_name(name.trim()) {
            return match canonical_lookup(values, name.trim(), references) {
                Some(value) => value,
                None => serde_json::Value::String(OMITTED_MARKER.into()),
            };
        }
    }
    // Embedded substitution: replace each known placeholder with its string
    // rendering; unknown placeholders are left as literal text.
    let mut result = text.to_string();
    let mut search = 0;
    while let Some(start) = result[search..].find("{{") {
        let absolute_start = search + start;
        let Some(relative_end) = result[absolute_start..].find("}}") else {
            break;
        };
        let absolute_end = absolute_start + relative_end + 2;
        let name = &result[absolute_start + 2..absolute_end - 2];
        if let Some(value) = canonical_lookup(values, name.trim(), references) {
            let rendered = match value {
                serde_json::Value::String(text) => text,
                serde_json::Value::Number(number) => number.to_string(),
                serde_json::Value::Bool(flag) => flag.to_string(),
                serde_json::Value::Array(items) => items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
                    .join(","),
                _ => String::new(),
            };
            result.replace_range(absolute_start..absolute_end, &rendered);
            search = absolute_start + rendered.len();
        } else {
            search = absolute_end;
        }
    }
    serde_json::Value::String(result)
}

/// Compiles a `application/x-www-form-urlencoded` body from an object
/// mapping template.
pub fn compile_form_urlencoded(
    template: &serde_json::Value,
    values: &CanonicalValues,
    references: &[ReferenceInput],
) -> Result<Vec<u8>, String> {
    let compiled = compile_json_template(template, values, references);
    let map = compiled
        .as_object()
        .ok_or_else(|| "form-urlencoded request mapping must be a JSON object".to_string())?;
    let mut encoded = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in map {
        let rendered = match value {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Number(number) => number.to_string(),
            serde_json::Value::Bool(flag) => flag.to_string(),
            _ => return Err(format!("form-urlencoded field {key} must be a scalar")),
        };
        encoded.append_pair(key, &rendered);
    }
    Ok(encoded.finish().into_bytes())
}

/// Compiles multipart parts from field specs: text fields use template
/// substitution, file fields expand the verified reference attachments
/// (one part per file, repeated under the same field name).
pub fn compile_multipart(
    fields: &[MultipartFieldConfig],
    values: &CanonicalValues,
    references: &[ReferenceInput],
) -> Result<Vec<crate::providers::http::MultipartPart>, String> {
    use crate::providers::http::MultipartPart;
    let mut parts = Vec::new();
    for field in fields {
        match field.kind {
            MultipartFieldKind::Text => {
                let template = field.value.as_deref().ok_or_else(|| {
                    format!(
                        "multipart text field {} requires a value template",
                        field.name
                    )
                })?;
                let compiled = compile_string_leaf(template, values, references);
                if is_omitted_marker(&compiled) {
                    continue;
                }
                let text = compiled.as_str().ok_or_else(|| {
                    format!("multipart text field {} must resolve to text", field.name)
                })?;
                parts.push(MultipartPart {
                    field_name: field.name.clone(),
                    file_name: None,
                    content_type: None,
                    bytes: text.as_bytes().to_vec(),
                });
            }
            MultipartFieldKind::File => {
                let source = field.source.as_deref().unwrap_or("image");
                let selected: Vec<&ReferenceInput> = match source {
                    "image" => references.first().into_iter().collect(),
                    "images" | "referenceImages" => references.iter().collect(),
                    other => {
                        return Err(format!(
                            "multipart file field {} references unknown source {other}",
                            field.name
                        ))
                    }
                };
                if selected.is_empty() {
                    return Err(format!(
                        "multipart file field {} requires at least one reference image",
                        field.name
                    ));
                }
                for reference in selected {
                    parts.push(MultipartPart {
                        field_name: field.name.clone(),
                        file_name: Some(reference.file_name.clone()),
                        content_type: Some(reference.media_type.clone()),
                        bytes: reference.bytes.clone(),
                    });
                }
            }
        }
    }
    Ok(parts)
}

// ---------------------------------------------------------------------------
// URL templates
// ---------------------------------------------------------------------------

/// Interpolates single-brace context variables (`{model}`, `{accountId}`,
/// `{providerId}`, `{operation}`, `{jobId}`) and double-brace canonical
/// scalars (`{{prompt}}` — URL-encoded — plus numeric fields) into a URL
/// path template. Model ids are inserted verbatim so path-separated ids
/// (e.g. `@cf/black-forest-labs/flux-1-schnell`) stay intact.
pub fn compile_url_template(template: &str, context: &UrlContext) -> String {
    let mut result = template.to_string();
    // Double-brace canonical scalars first (URL-encoded).
    for name in [
        "prompt",
        "negativePrompt",
        "aspectRatio",
        "quality",
        "width",
        "height",
        "seed",
        "steps",
        "duration",
        "fps",
    ] {
        let placeholder = format!("{{{{{name}}}}}");
        if !result.contains(&placeholder) {
            continue;
        }
        let rendered = canonical_lookup(&context.values, name, &[])
            .map(|value| match value {
                serde_json::Value::String(text) => text,
                other => other.to_string(),
            })
            .unwrap_or_default();
        result = result.replace(&placeholder, &url_encode(&rendered));
    }
    let single = [
        ("{model}", context.values.model.clone()),
        (
            "{accountId}",
            context.account_id.clone().unwrap_or_default(),
        ),
        ("{providerId}", context.provider_id.clone()),
        ("{operation}", context.operation.clone()),
        ("{jobId}", context.job_id.clone().unwrap_or_default()),
    ];
    for (placeholder, value) in single {
        result = result.replace(placeholder, &value);
    }
    result
}

#[derive(Debug, Clone, Default)]
pub struct UrlContext {
    pub values: CanonicalValues,
    pub provider_id: String,
    pub account_id: Option<String>,
    pub operation: String,
    pub job_id: Option<String>,
}

/// Percent-encodes everything except RFC 3986 unreserved characters.
pub fn url_encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

/// Appends or merges a query parameter into a URL string.
pub fn with_query_parameter(url: &str, name: &str, value: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}{name}={}", url_encode(value))
}

// ---------------------------------------------------------------------------
// Response extraction
// ---------------------------------------------------------------------------

/// Resolves a dotted JSON path (`data.0.url`, `result.image`) against a
/// document. Numeric segments index arrays; other segments index objects.
pub fn resolve_json_path<'a>(
    document: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = document;
    for segment in path.split('.') {
        if segment.is_empty() {
            return None;
        }
        current = match current {
            serde_json::Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            serde_json::Value::Object(map) => map.get(segment)?,
            _ => return None,
        };
    }
    Some(current)
}

fn path_to_string(document: &serde_json::Value, path: &str) -> Option<String> {
    match resolve_json_path(document, path) {
        Some(serde_json::Value::String(text)) => Some(text.clone()),
        Some(serde_json::Value::Number(number)) => Some(number.to_string()),
        _ => None,
    }
}

/// Default locations probed (in order) for a provider's human-readable error
/// message when no explicit error mapping is configured.
const DEFAULT_ERROR_MESSAGE_PATHS: &[&str] = &[
    "error.message",
    "message",
    "errors.0.message",
    "errors.0",
    "error",
    "detail",
    "title",
    "result.error.message",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ErrorMapping {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id_path: Option<String>,
}

impl ErrorMapping {
    /// Extracts a normalized provider error from a failed response body.
    pub fn extract(
        &self,
        body: &serde_json::Value,
    ) -> (Option<String>, Option<String>, Option<String>) {
        let message = self
            .message_path
            .as_deref()
            .and_then(|path| path_to_string(body, path))
            .or_else(|| {
                DEFAULT_ERROR_MESSAGE_PATHS
                    .iter()
                    .find_map(|path| path_to_string(body, path))
            })
            .filter(|text| !text.trim().is_empty());
        let code = self
            .code_path
            .as_deref()
            .and_then(|path| path_to_string(body, path));
        let request_id = self
            .request_id_path
            .as_deref()
            .and_then(|path| path_to_string(body, path));
        (message, code, request_id)
    }
}

/// How to read generation outputs from a JSON response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseMapping {
    /// Optional path to an array of output items (e.g. `data`, `images`).
    /// When absent, the mapping paths are evaluated against the response root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs_path: Option<String>,
    /// Path to an output URL (absolute, or relative to each item when
    /// `outputs_path` is set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_path: Option<String>,
    /// Path to an inline base64 payload (same semantics).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base64_path: Option<String>,
    /// The whole response body is the asset (e.g. a raw image download).
    #[serde(default)]
    pub binary_response: bool,
    #[serde(default = "default_output_mime_type")]
    pub mime_type: String,
    #[serde(default = "default_output_filename")]
    pub filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_request_id_path: Option<String>,
}

fn default_output_mime_type() -> String {
    "image/png".into()
}

fn default_output_filename() -> String {
    "generated.png".into()
}

impl Default for ResponseMapping {
    fn default() -> Self {
        Self {
            outputs_path: None,
            url_path: None,
            base64_path: None,
            binary_response: false,
            mime_type: default_output_mime_type(),
            filename: default_output_filename(),
            provider_request_id_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedOutput {
    pub uri: String,
    pub mime_type: String,
    pub filename: String,
}

impl ResponseMapping {
    /// Extracts canonical outputs from a JSON response document.
    pub fn extract_outputs(
        &self,
        document: &serde_json::Value,
        operation: &str,
    ) -> Result<(Vec<ExtractedOutput>, Option<String>), String> {
        let default_mime = match operation {
            OPERATION_VIDEO_GENERATE | OPERATION_VIDEO_IMAGE_TO_VIDEO => "video/mp4",
            _ => "image/png",
        };
        let mut outputs = Vec::new();
        let items: Vec<&serde_json::Value> = match &self.outputs_path {
            Some(path) => resolve_json_path(document, path)
                .and_then(|value| value.as_array())
                .ok_or_else(|| format!("response path {path} did not resolve to an array"))?
                .iter()
                .collect(),
            None => vec![document],
        };
        for item in items {
            let uri = if let Some(path) = &self.url_path {
                path_to_string(item, path).filter(|url| !url.trim().is_empty())
            } else {
                None
            }
            .or_else(|| {
                self.base64_path
                    .as_deref()
                    .and_then(|path| path_to_string(item, path))
                    .filter(|payload| !payload.trim().is_empty())
                    .map(|payload| {
                        let mime = if self.mime_type == default_output_mime_type() {
                            default_mime
                        } else {
                            &self.mime_type
                        };
                        format!("data:{mime};base64,{payload}")
                    })
            })
            .ok_or_else(|| {
                "response did not contain a readable output at the configured path".to_string()
            })?;
            outputs.push(ExtractedOutput {
                uri,
                mime_type: if self.mime_type == default_output_mime_type() {
                    default_mime.into()
                } else {
                    self.mime_type.clone()
                },
                filename: self.filename.clone(),
            });
        }
        let request_id = self
            .provider_request_id_path
            .as_deref()
            .and_then(|path| path_to_string(document, path));
        Ok((outputs, request_id))
    }

    /// Extracts a single output from a raw binary response body.
    pub fn extract_binary_output(
        &self,
        bytes: &[u8],
        content_type: Option<&str>,
        operation: &str,
    ) -> ExtractedOutput {
        let default_mime = match operation {
            OPERATION_VIDEO_GENERATE | OPERATION_VIDEO_IMAGE_TO_VIDEO => "video/mp4",
            _ => "image/png",
        };
        let mime_type = if self.mime_type == default_output_mime_type() {
            content_type
                .map(|value| value.split(';').next().unwrap_or(default_mime).to_string())
                .filter(|value| value.starts_with("image/") || value.starts_with("video/"))
                .unwrap_or_else(|| default_mime.to_string())
        } else {
            self.mime_type.clone()
        };
        let extension = mime_type.rsplit('/').next().unwrap_or("bin").to_string();
        ExtractedOutput {
            uri: format!(
                "data:{mime_type};base64,{}",
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
            ),
            mime_type: mime_type.clone(),
            filename: self
                .filename
                .rsplit_once('.')
                .map(|(stem, _)| format!("{stem}.{extension}"))
                .unwrap_or_else(|| self.filename.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Async job lifecycle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEndpointConfig {
    #[serde(default = "default_get_method")]
    pub method: String,
    /// Path template relative to the base URL; `{jobId}` is interpolated.
    pub path_template: String,
    /// Path to the status string (e.g. `result.status`).
    pub status_path: String,
    #[serde(default)]
    pub completed_values: Vec<String>,
    #[serde(default)]
    pub failed_values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message_path: Option<String>,
}

fn default_get_method() -> String {
    "GET".into()
}

impl Default for StatusEndpointConfig {
    fn default() -> Self {
        Self {
            method: default_get_method(),
            path_template: String::new(),
            status_path: String::new(),
            completed_values: Vec::new(),
            failed_values: Vec::new(),
            progress_path: None,
            error_message_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalOutputConfig {
    /// When set, the completed job's outputs are fetched from this path
    /// template instead of being read from the status response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetch_path_template: Option<String>,
    #[serde(default = "default_get_method")]
    pub fetch_method: String,
    #[serde(default)]
    pub response: ResponseMapping,
}

impl Default for FinalOutputConfig {
    fn default() -> Self {
        Self {
            fetch_path_template: None,
            fetch_method: default_get_method(),
            response: ResponseMapping::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollingConfig {
    #[serde(default = "default_polling_interval_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_polling_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_polling_interval_ms() -> u64 {
    3000
}

fn default_polling_timeout_ms() -> u64 {
    600_000
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_polling_interval_ms(),
            timeout_ms: default_polling_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AsyncJobConfig {
    /// Path to the job/task id in the submit response.
    pub job_id_path: String,
    #[serde(default)]
    pub status: StatusEndpointConfig,
    #[serde(default)]
    pub output: FinalOutputConfig,
    #[serde(default)]
    pub polling: PollingConfig,
}

// ---------------------------------------------------------------------------
// Operations and endpoints
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MultipartFieldKind {
    #[default]
    Text,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultipartFieldConfig {
    pub name: String,
    #[serde(default)]
    pub kind: MultipartFieldKind,
    /// Template for text fields (`{{prompt}}` etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// For file fields: `image`, `images`, or `referenceImages`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// One configurable endpoint for an operation. The final URL is
/// `base_url (interpolated) + path_template (interpolated)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointConfig {
    #[serde(default = "default_post_method")]
    pub method: String,
    #[serde(default)]
    pub path_template: String,
    #[serde(default)]
    pub request_type: RequestType,
    /// JSON (or form) template with `{{canonical}}` placeholders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_mapping: Option<serde_json::Value>,
    /// Multipart field specs (required when request_type is multipart).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub multipart_fields: Vec<MultipartFieldConfig>,
    /// Extra static headers for this operation only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub response: ResponseMapping,
    /// Present when the operation is asynchronous (submit → poll → fetch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<AsyncJobConfig>,
}

fn default_post_method() -> String {
    "POST".into()
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            method: default_post_method(),
            path_template: String::new(),
            request_type: RequestType::Json,
            request_mapping: None,
            multipart_fields: Vec::new(),
            headers: BTreeMap::new(),
            response: ResponseMapping::default(),
            job: None,
        }
    }
}

impl EndpointConfig {
    /// True when the request mapping (or multipart fields) reference image
    /// input — used to derive reference-image capabilities.
    pub fn accepts_reference_images(&self) -> bool {
        if self
            .multipart_fields
            .iter()
            .any(|field| matches!(field.kind, MultipartFieldKind::File))
        {
            return true;
        }
        let Some(mapping) = &self.request_mapping else {
            return false;
        };
        mapping_references(mapping, &["{{image", "{{images", "{{referenceImages"])
    }

    pub fn accepts_multiple_reference_images(&self) -> bool {
        self.multipart_fields.iter().any(|field| {
            matches!(field.kind, MultipartFieldKind::File)
                && matches!(
                    field.source.as_deref(),
                    Some("images") | Some("referenceImages")
                )
        }) || self
            .request_mapping
            .as_ref()
            .is_some_and(|mapping| mapping_references(mapping, &["{{images", "{{referenceImages"]))
    }

    pub fn validate(&self, operation: &str) -> Result<(), String> {
        let method = self.method.to_ascii_uppercase();
        if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
            return Err(format!(
                "operation {operation} has an unsupported HTTP method"
            ));
        }
        if !self.path_template.is_empty() && !self.path_template.starts_with('/') {
            return Err(format!(
                "operation {operation} path template must start with '/'"
            ));
        }
        if operation == OPERATION_VALIDATE && !matches!(method.as_str(), "GET" | "POST") {
            return Err("validate operation must use GET or POST".into());
        }
        for name in self.headers.keys() {
            if name.eq_ignore_ascii_case("host")
                || name.eq_ignore_ascii_case("content-length")
                || name.eq_ignore_ascii_case("transfer-encoding")
                || name.eq_ignore_ascii_case("connection")
                || name.eq_ignore_ascii_case("authorization")
            {
                return Err(format!(
                    "operation {operation} customizes transport-controlled header {name}"
                ));
            }
        }
        let body_allowed = !matches!(
            self.method.to_ascii_uppercase().as_str(),
            "GET" | "HEAD" | "DELETE"
        );
        match self.request_type {
            RequestType::Json => {
                if body_allowed && self.request_mapping.is_none() {
                    return Err(format!(
                        "operation {operation} uses JSON requests but has no request mapping"
                    ));
                }
            }
            RequestType::FormUrlEncoded => {
                if self.request_mapping.is_none() {
                    return Err(format!(
                        "operation {operation} uses form requests but has no request mapping"
                    ));
                }
            }
            RequestType::Multipart => {
                if self.multipart_fields.is_empty() {
                    return Err(format!(
                        "operation {operation} uses multipart requests but has no fields"
                    ));
                }
            }
        }
        // Async operations read outputs from the job lifecycle, not from the
        // submit response, so the submit response mapping is optional.
        if self.job.is_none() {
            self.response.validate(operation)?;
        }
        if let Some(job) = &self.job {
            job.validate(operation)?;
        }
        Ok(())
    }
}

fn mapping_references(value: &serde_json::Value, needles: &[&str]) -> bool {
    match value {
        serde_json::Value::String(text) => needles.iter().any(|needle| text.contains(needle)),
        serde_json::Value::Array(items) => {
            items.iter().any(|item| mapping_references(item, needles))
        }
        serde_json::Value::Object(map) => {
            map.values().any(|item| mapping_references(item, needles))
        }
        _ => false,
    }
}

impl ResponseMapping {
    fn validate(&self, operation: &str) -> Result<(), String> {
        if self.binary_response {
            return Ok(());
        }
        if self.url_path.is_none() && self.base64_path.is_none() {
            return Err(format!(
                "operation {operation} response mapping needs urlPath, base64Path, or binaryResponse"
            ));
        }
        Ok(())
    }
}

impl AsyncJobConfig {
    fn validate(&self, operation: &str) -> Result<(), String> {
        if self.job_id_path.trim().is_empty() {
            return Err(format!(
                "operation {operation} async config requires jobIdPath"
            ));
        }
        if self.status.path_template.is_empty() {
            return Err(format!(
                "operation {operation} async config requires a status path template"
            ));
        }
        if self.status.status_path.trim().is_empty() {
            return Err(format!(
                "operation {operation} async config requires statusPath"
            ));
        }
        if self.status.completed_values.is_empty() {
            return Err(format!(
                "operation {operation} async config requires completedValues"
            ));
        }
        if self.output.response.binary_response && self.output.fetch_path_template.is_none() {
            return Err(format!(
                "operation {operation} async output cannot use binaryResponse without fetchPathTemplate"
            ));
        }
        if self.output.response.url_path.is_none()
            && self.output.response.base64_path.is_none()
            && self.output.fetch_path_template.is_none()
        {
            return Err(format!(
                "operation {operation} async output needs urlPath, base64Path, or fetchPathTemplate"
            ));
        }
        if self.polling.interval_ms == 0 || self.polling.timeout_ms == 0 {
            return Err(format!(
                "operation {operation} async polling interval and timeout must be positive"
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Full runtime configuration
// ---------------------------------------------------------------------------

/// The compiled, provider-agnostic runtime configuration for one AI service.
/// Presets generate this; advanced users can author it directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRuntimeConfig {
    #[serde(default)]
    pub auth: AuthConfig,
    /// Non-secret contextual value interpolated as `{accountId}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// Static custom headers. Secret values are resolved from the vault at
    /// execution time (keys here are names only).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// Operation definitions keyed by operation name (`image.generate`, ...).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub operations: BTreeMap<String, EndpointConfig>,
    /// Optional provider-wide error extraction overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_mapping: Option<ErrorMapping>,
}

impl ProviderRuntimeConfig {
    pub fn validate(&self) -> Result<(), String> {
        self.auth.validate()?;
        if let Some(account_id) = &self.account_id {
            if account_id.contains(['\r', '\n', ' ', '/']) {
                return Err("account ID must not contain whitespace or slashes".into());
            }
        }
        let mut header_names = BTreeMap::new();
        for name in self.headers.keys() {
            if !crate::providers::model::is_http_header_name(name)
                || header_names.insert(name.to_ascii_lowercase(), ()).is_some()
            {
                return Err("custom header names must be valid and unique".into());
            }
        }
        if self.operations.is_empty() {
            return Err("at least one operation must be configured".into());
        }
        for (name, endpoint) in &self.operations {
            if !is_known_operation(name) {
                return Err(format!("unknown operation {name}"));
            }
            endpoint.validate(name)?;
        }
        if self.operations.contains_key(OPERATION_IMAGE_EDIT)
            && !self.operations.contains_key(OPERATION_IMAGE_GENERATE)
            && self.operations.contains_key(OPERATION_VIDEO_GENERATE)
        {
            // Editing without any image or video context is suspicious but
            // legal; generation-only providers commonly include edit too. No
            // extra constraint needed here.
        }
        Ok(())
    }

    /// Derives the legacy purpose label from configured operations so older
    /// surfaces (provider picker, LLM suggestions) keep working.
    pub fn derived_purpose(&self) -> crate::providers::model::CustomProviderPurpose {
        if self.operations.contains_key(OPERATION_VIDEO_GENERATE)
            || self.operations.contains_key(OPERATION_VIDEO_IMAGE_TO_VIDEO)
        {
            crate::providers::model::CustomProviderPurpose::Video
        } else if self.operations.contains_key(OPERATION_IMAGE_GENERATE)
            || self.operations.contains_key(OPERATION_IMAGE_EDIT)
        {
            crate::providers::model::CustomProviderPurpose::Image
        } else {
            crate::providers::model::CustomProviderPurpose::Llm
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn values() -> CanonicalValues {
        CanonicalValues {
            prompt: "a calm lake".into(),
            negative_prompt: Some("fog".into()),
            model: "test-model".into(),
            width: Some(1024),
            height: Some(768),
            seed: Some(42),
            steps: Some(4),
            ..Default::default()
        }
    }

    #[test]
    fn json_template_substitutes_typed_values_and_omits_missing() {
        let template = json!({
            "model": "{{model}}",
            "prompt": "{{prompt}}",
            "steps": "{{steps}}",
            "seed": "{{seed}}",
            "size": "{{size}}",
            "negative": "{{negativePrompt}}",
            "missing": "{{quality}}",
            "nested": {"inner": {"text": "prompt: {{prompt}} v2"}},
            "unknown_left_alone": "{{notAField}}"
        });

        let compiled = compile_json_template(&template, &values(), &[]);
        assert_eq!(compiled["model"], "test-model");
        assert_eq!(compiled["prompt"], "a calm lake");
        assert_eq!(compiled["steps"], 4);
        assert_eq!(compiled["seed"], 42);
        assert_eq!(compiled["size"], "1024x768");
        assert_eq!(compiled["negative"], "fog");
        assert_eq!(
            compiled["nested"]["inner"]["text"],
            "prompt: a calm lake v2"
        );
        assert_eq!(compiled["unknown_left_alone"], "{{notAField}}");
        // Unset canonical fields are omitted, never sent empty.
        assert!(compiled.get("missing").is_none());
    }

    #[test]
    fn json_template_substitutes_reference_data_uris() {
        let references = vec![ReferenceInput {
            file_name: "face.png".into(),
            media_type: "image/png".into(),
            bytes: vec![1, 2, 3],
        }];
        let template = json!({
            "image": "{{image}}",
            "many": "{{images}}",
            "also": "{{referenceImages}}",
            "absent": "{{image}}"
        });
        let compiled = compile_json_template(&template, &values(), &references);
        let uri = compiled["image"].as_str().unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
        assert_eq!(compiled["many"].as_array().unwrap().len(), 1);
        assert_eq!(compiled["also"].as_array().unwrap().len(), 1);
        // Without references the fields are omitted entirely.
        let compiled_empty = compile_json_template(&template, &values(), &[]);
        assert!(compiled_empty.get("image").is_none());
        assert!(compiled_empty.get("absent").is_none());
    }

    #[test]
    fn form_urlencoded_compiles_scalars_only() {
        let template = json!({"prompt": "{{prompt}}", "steps": "{{steps}}"});
        let body = compile_form_urlencoded(&template, &values(), &[]).unwrap();
        let text = String::from_utf8(body).unwrap();
        assert!(text.contains("prompt=a+calm+lake") || text.contains("prompt=a%20calm%20lake"));
        assert!(text.contains("steps=4"));
    }

    #[test]
    fn url_templates_interpolate_context_and_encode_prompt() {
        let context = UrlContext {
            values: values(),
            provider_id: "cf".into(),
            account_id: Some("acc-1".into()),
            operation: OPERATION_IMAGE_GENERATE.into(),
            job_id: None,
        };
        let url = compile_url_template(
            "https://api.cloudflare.com/client/v4/accounts/{accountId}/ai/run/{model}",
            &context,
        );
        assert_eq!(
            url,
            "https://api.cloudflare.com/client/v4/accounts/acc-1/ai/run/test-model"
        );
        let with_prompt = compile_url_template(
            "https://image.example/prompt/{{prompt}}?seed={{seed}}",
            &context,
        );
        assert_eq!(
            with_prompt,
            "https://image.example/prompt/a%20calm%20lake?seed=42"
        );
    }

    #[test]
    fn json_paths_resolve_nested_objects_and_arrays() {
        let document = json!({"data": [{"url": "https://x/y.png"}], "result": {"task_id": "t-1"}});
        assert_eq!(
            resolve_json_path(&document, "data.0.url").and_then(|v| v.as_str()),
            Some("https://x/y.png")
        );
        assert_eq!(
            resolve_json_path(&document, "result.task_id").and_then(|v| v.as_str()),
            Some("t-1")
        );
        assert!(resolve_json_path(&document, "data.5.url").is_none());
        assert!(resolve_json_path(&document, "").is_none());
    }

    #[test]
    fn response_mapping_extracts_single_and_array_outputs() {
        let document = json!({"data": [{"url": "https://x/1.png"}, {"b64_json": "AAAA"}]});
        let mapping = ResponseMapping {
            outputs_path: Some("data".into()),
            url_path: Some("url".into()),
            base64_path: Some("b64_json".into()),
            ..Default::default()
        };
        let (outputs, _) = mapping
            .extract_outputs(&document, OPERATION_IMAGE_GENERATE)
            .unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].uri, "https://x/1.png");
        assert!(outputs[1].uri.starts_with("data:image/png;base64,AAAA"));

        let absolute = ResponseMapping {
            url_path: Some("result.images.0.url".into()),
            ..Default::default()
        };
        let doc = json!({"result": {"images": [{"url": "https://x/final.png"}]}});
        let (outputs, _) = absolute
            .extract_outputs(&doc, OPERATION_IMAGE_GENERATE)
            .unwrap();
        assert_eq!(outputs[0].uri, "https://x/final.png");
    }

    #[test]
    fn video_operations_default_to_video_mime() {
        let mapping = ResponseMapping {
            url_path: Some("output.0".into()),
            ..Default::default()
        };
        let doc = json!({"output": ["https://x/clip.mp4"]});
        let (outputs, _) = mapping
            .extract_outputs(&doc, OPERATION_VIDEO_GENERATE)
            .unwrap();
        assert_eq!(outputs[0].mime_type, "video/mp4");
    }

    #[test]
    fn error_mapping_extracts_provider_messages() {
        let body = json!({"errors": [{"message": "prompt is required", "code": 7002}]});
        let (message, _, _) = ErrorMapping::default().extract(&body);
        assert_eq!(message.as_deref(), Some("prompt is required"));
        let explicit = ErrorMapping {
            message_path: Some("errors.0.message".into()),
            code_path: Some("errors.0.code".into()),
            request_id_path: None,
        };
        let (message, code, _) = explicit.extract(&body);
        assert_eq!(message.as_deref(), Some("prompt is required"));
        assert_eq!(code.as_deref(), Some("7002"));
    }

    #[test]
    fn runtime_config_validation_rejects_unknown_operations_and_bad_auth() {
        let mut config = ProviderRuntimeConfig {
            auth: AuthConfig {
                mode: AuthMode::Header,
                credential_name: None,
            },
            ..Default::default()
        };
        assert!(config.validate().is_err());
        config.auth = AuthConfig {
            mode: AuthMode::Header,
            credential_name: Some("x-api-key".into()),
        };
        assert!(config.validate().is_err(), "no operations configured");
        let mut operations = BTreeMap::new();
        operations.insert(
            OPERATION_IMAGE_GENERATE.into(),
            EndpointConfig {
                request_mapping: Some(json!({"prompt": "{{prompt}}"})),
                response: ResponseMapping {
                    url_path: Some("data.0.url".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        config.operations = operations;
        config.validate().unwrap();

        let mut operations = BTreeMap::new();
        operations.insert("made.up".into(), EndpointConfig::default());
        config.operations = operations;
        assert!(config.validate().unwrap_err().contains("unknown operation"));
    }

    #[test]
    fn endpoint_validation_requires_output_paths_and_methods() {
        let endpoint = EndpointConfig {
            request_mapping: Some(json!({"prompt": "{{prompt}}"})),
            response: ResponseMapping::default(),
            ..Default::default()
        };
        let error = endpoint.validate(OPERATION_IMAGE_GENERATE).unwrap_err();
        assert!(error.contains("urlPath, base64Path, or binaryResponse"));

        let endpoint = EndpointConfig {
            method: "BREW".into(),
            ..endpoint
        };
        assert!(endpoint.validate(OPERATION_IMAGE_GENERATE).is_err());
    }

    #[test]
    fn async_config_validation_requires_the_full_lifecycle() {
        let endpoint = EndpointConfig {
            request_mapping: Some(json!({"prompt": "{{prompt}}"})),
            response: ResponseMapping::default(),
            job: Some(AsyncJobConfig {
                job_id_path: "id".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let error = endpoint.validate(OPERATION_VIDEO_GENERATE).unwrap_err();
        assert!(error.contains("status path template"));
    }

    #[test]
    fn multipart_compilation_expands_files_and_text() {
        use crate::providers::http::MultipartPart;
        let fields = vec![
            MultipartFieldConfig {
                name: "prompt".into(),
                kind: MultipartFieldKind::Text,
                value: Some("{{prompt}}".into()),
                source: None,
            },
            MultipartFieldConfig {
                name: "model".into(),
                kind: MultipartFieldKind::Text,
                value: Some("{{model}}".into()),
                source: None,
            },
            MultipartFieldConfig {
                name: "image[]".into(),
                kind: MultipartFieldKind::File,
                value: None,
                source: Some("images".into()),
            },
        ];
        let references = vec![ReferenceInput {
            file_name: "a.png".into(),
            media_type: "image/png".into(),
            bytes: vec![9, 9],
        }];
        let parts = compile_multipart(&fields, &values(), &references).unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].field_name, "prompt");
        assert_eq!(parts[0].bytes, b"a calm lake".to_vec());
        let files: Vec<&MultipartPart> = parts.iter().filter(|p| p.file_name.is_some()).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name.as_deref(), Some("a.png"));
    }

    #[test]
    fn binary_response_extraction_uses_content_type() {
        let mapping = ResponseMapping {
            binary_response: true,
            ..Default::default()
        };
        let output =
            mapping.extract_binary_output(b"bytes", Some("image/jpeg"), OPERATION_IMAGE_GENERATE);
        assert_eq!(output.mime_type, "image/jpeg");
        assert!(output.uri.starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn derived_purpose_follows_operations() {
        let mut config = ProviderRuntimeConfig::default();
        let mut operations = BTreeMap::new();
        operations.insert(OPERATION_VIDEO_GENERATE.into(), EndpointConfig::default());
        config.operations = operations;
        assert_eq!(
            config.derived_purpose(),
            crate::providers::model::CustomProviderPurpose::Video
        );
    }
}
