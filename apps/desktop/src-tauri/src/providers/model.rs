use crate::skills::model::ExpectedOutputDefinition;
use crate::workflow::execution::{
    ExecutionConstraint, ExecutionMediaType, ExecutionReference, ExecutionRequest, ExecutionTask,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderModel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomProviderPurpose {
    Legacy,
    Llm,
    Image,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderHeader {
    pub name: String,
    #[serde(default, skip_serializing)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProviderDefinition {
    pub provider_id: String,
    pub display_name: String,
    pub base_url: String,
    pub purpose: CustomProviderPurpose,
    #[serde(default, skip_serializing)]
    pub api_key: Option<String>,
    /// Non-secret display hint (e.g. "sk-j9ml•••ray") derived from the vault
    /// secret on read paths. Never persisted in the database and never
    /// accepted back as a credential value.
    #[serde(default)]
    pub api_key_hint: Option<String>,
    pub models: Vec<CustomProviderModel>,
    pub headers: Vec<CustomProviderHeader>,
}

impl CustomProviderDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.provider_id.is_empty()
            || !self.provider_id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
            })
        {
            return Err(
                "provider ID must contain only lowercase letters, numbers, '-' or '_'".into(),
            );
        }
        if self.display_name.trim().is_empty() {
            return Err("display name must not be blank".into());
        }
        let base_url = url::Url::parse(&self.base_url)
            .map_err(|_| "base URL must be an absolute HTTP(S) URL")?;
        if !matches!(base_url.scheme(), "http" | "https") || base_url.host_str().is_none() {
            return Err("base URL must be an absolute HTTP(S) URL".into());
        }
        if !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            return Err("base URL must not contain credentials, a query, or a fragment".into());
        }
        if self
            .api_key
            .as_deref()
            .is_some_and(|value| value.contains(['\r', '\n']))
        {
            return Err("API key must not contain line breaks".into());
        }
        if self.models.is_empty() {
            return Err("at least one model is required".into());
        }
        let mut model_ids = HashSet::new();
        for model in &self.models {
            if model.id.trim().is_empty()
                || model.name.trim().is_empty()
                || !model_ids.insert(&model.id)
            {
                return Err("model IDs and names must be non-blank and model IDs unique".into());
            }
        }
        let mut header_names = HashSet::new();
        for header in &self.headers {
            if !is_http_header_name(&header.name)
                || !header_names.insert(header.name.to_ascii_lowercase())
            {
                return Err("header names must be valid, unique HTTP header names".into());
            }
            if matches!(
                header.name.to_ascii_lowercase().as_str(),
                "host" | "content-length" | "transfer-encoding" | "connection"
            ) {
                return Err("transport-controlled headers cannot be customized".into());
            }
            if header
                .value
                .as_deref()
                .is_some_and(|value| value.contains(['\r', '\n']))
            {
                return Err("header values must not contain line breaks".into());
            }
        }
        Ok(())
    }

    pub fn without_secrets(&self) -> Self {
        Self {
            api_key: None,
            headers: self
                .headers
                .iter()
                .map(|header| CustomProviderHeader {
                    name: header.name.clone(),
                    value: None,
                })
                .collect(),
            ..self.clone()
        }
    }
}

/// Builds a short, non-secret display hint such as `sk-j9ml•••ray` from a
/// stored secret. Only the first 7 and last 3 characters are exposed; shorter
/// secrets produce a length-only mask so no full value ever round-trips.
pub fn mask_secret(secret: &str) -> String {
    let trimmed = secret.trim();
    let char_count = trimmed.chars().count();
    if char_count < 12 {
        return "•".repeat(char_count.max(1));
    }
    let head: String = trimmed.chars().take(7).collect();
    let tail: String = trimmed.chars().skip(char_count - 3).collect();
    format!("{head}•••{tail}")
}

fn is_http_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMediaType {
    Image,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderLifecycle {
    Queued,
    Submitted,
    Running,
    Succeeded,
    Failed,
    CancellationRequested,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub media_types: Vec<ProviderMediaType>,
    pub supports_seed: bool,
    pub supports_negative_prompt: bool,
    pub supports_reference_image: bool,
    pub supports_image_edit: bool,
    pub supports_multiple_reference_images: bool,
    pub supports_image_to_video: bool,
    pub supports_cancel: bool,
    pub supports_progress: bool,
    pub supported_aspect_ratios: Vec<String>,
    pub supported_models: Vec<String>,
}

impl ProviderCapabilities {
    pub fn supports(&self, request: &ProviderExecutionRequest) -> Result<(), String> {
        let media_type = match request.media_type {
            ExecutionMediaType::Image => ProviderMediaType::Image,
        };
        if !self.media_types.contains(&media_type) {
            return Err(format!("media type {:?} is not supported", media_type));
        }
        if !request.references.is_empty() && !self.supports_reference_image {
            return Err("reference images are not supported".into());
        }
        if request.task == ExecutionTask::VisualRepair && !self.supports_image_edit {
            return Err("image editing is not supported".into());
        }
        if request.references.len() > 1 && !self.supports_multiple_reference_images {
            return Err("multiple reference images are not supported".into());
        }
        if !self.supported_models.is_empty()
            && !self
                .supported_models
                .iter()
                .any(|model| model == &request.selected_model)
        {
            return Err(format!("model {} is not supported", request.selected_model));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderGenerationParameters {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub aspect_ratio: Option<String>,
    pub duration_seconds: Option<f32>,
    pub fps: Option<u32>,
    pub seed: Option<u64>,
}

impl Default for ProviderGenerationParameters {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            aspect_ratio: None,
            duration_seconds: None,
            fps: None,
            seed: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderExecutionRequest {
    pub run_id: String,
    pub step_id: String,
    pub compiled_request_id: String,
    pub media_type: ExecutionMediaType,
    pub task: ExecutionTask,
    pub prompt: String,
    pub references: Vec<ExecutionReference>,
    pub constraints: Vec<ExecutionConstraint>,
    pub expected_output: ExpectedOutputDefinition,
    pub generation_parameters: ProviderGenerationParameters,
    pub selected_provider: String,
    pub selected_model: String,
    pub idempotency_key: String,
    /// Verified, ephemeral media attachments resolved immediately before
    /// submission. Skipped in serialization so bytes never leak into logs,
    /// diagnostics, or persisted snapshots.
    #[serde(skip)]
    pub reference_attachments: Vec<ProviderReferenceAttachment>,
}

/// One verified reference attachment ready for a multipart provider call.
/// Lives only for the duration of a submission; adapters never resolve or
/// persist these bytes themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderReferenceAttachment {
    pub asset_version_id: String,
    pub file_name: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

impl ProviderExecutionRequest {
    pub fn from_execution_request(
        run_id: &str,
        step_id: &str,
        compiled_request_id: &str,
        selected_provider: &str,
        selected_model: &str,
        idempotency_key: &str,
        request: &ExecutionRequest,
    ) -> Result<Self, String> {
        if selected_provider.trim().is_empty() {
            return Err("selected provider must not be blank".into());
        }
        if selected_model.trim().is_empty() {
            return Err("selected model must not be blank".into());
        }
        if idempotency_key.trim().is_empty() {
            return Err("idempotency key must not be blank".into());
        }
        Ok(Self {
            run_id: run_id.into(),
            step_id: step_id.into(),
            compiled_request_id: compiled_request_id.into(),
            media_type: request.media_type,
            task: request.task,
            prompt: request.prompt.clone(),
            references: request.references.clone(),
            constraints: request.constraints.clone(),
            expected_output: request.expected_output.clone(),
            generation_parameters: ProviderGenerationParameters::default(),
            selected_provider: selected_provider.into(),
            selected_model: selected_model.into(),
            idempotency_key: idempotency_key.into(),
            reference_attachments: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderJobRef {
    pub provider_id: String,
    pub provider_job_id: String,
    pub run_id: String,
    pub step_id: String,
    pub submission_id: String,
    pub submitted_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSubmission {
    pub job: ProviderJobRef,
    pub lifecycle: ProviderLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderJobStatus {
    pub lifecycle: ProviderLifecycle,
    pub progress_percent: Option<u8>,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCancellationResult {
    pub provider_job_id: String,
    pub lifecycle: ProviderLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOutput {
    pub uri: String,
    pub mime_type: String,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderResult {
    pub outputs: Vec<ProviderOutput>,
    pub provider_reported_model: Option<String>,
    pub metadata: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::execution::{ExecutionProvenance, ExecutionReferenceType, ExecutionTask};

    #[test]
    fn custom_provider_definition_validates_and_redacts_secrets() {
        let definition = CustomProviderDefinition {
            provider_id: "image_provider".into(),
            display_name: "Image Provider".into(),
            base_url: "https://images.example.test/v1".into(),
            purpose: CustomProviderPurpose::Image,
            api_key: Some("secret-api-key".into()),
            api_key_hint: None,
            models: vec![CustomProviderModel {
                id: "image-v1".into(),
                name: "Image V1".into(),
            }],
            headers: vec![CustomProviderHeader {
                name: "X-Workspace".into(),
                value: Some("secret-header".into()),
            }],
        };
        definition.validate().unwrap();
        let json = serde_json::to_string(&definition).unwrap();
        assert!(!json.contains("secret-api-key"));
        assert!(!json.contains("secret-header"));
    }

    #[test]
    fn mask_secret_exposes_only_edges_and_never_the_full_value() {
        assert_eq!(mask_secret("sk-j9mlQwErTyXzray"), "sk-j9ml•••ray");
        assert_eq!(mask_secret("  sk-j9mlQwErTyXzray  "), "sk-j9ml•••ray");
        assert_eq!(mask_secret("short"), "•••••");
        assert_eq!(mask_secret(""), "•");
        let masked = mask_secret("sk-j9mlQwErTyXzray");
        assert!(!masked.contains("QwErTyXz"));
    }

    #[test]
    fn custom_provider_definition_rejects_invalid_id_and_duplicates() {
        let definition = CustomProviderDefinition {
            provider_id: "Bad ID".into(),
            display_name: "x".into(),
            base_url: "http://x".into(),
            purpose: CustomProviderPurpose::Image,
            api_key: None,
            api_key_hint: None,
            models: vec![
                CustomProviderModel {
                    id: "same".into(),
                    name: "one".into(),
                },
                CustomProviderModel {
                    id: "same".into(),
                    name: "two".into(),
                },
            ],
            headers: vec![],
        };
        assert!(definition.validate().is_err());
    }

    #[test]
    fn custom_provider_definition_rejects_secret_bearing_urls_and_unsafe_headers() {
        let mut definition = CustomProviderDefinition {
            provider_id: "safe_id".into(),
            display_name: "Safe".into(),
            base_url: "https://user:secret@example.test/v1".into(),
            purpose: CustomProviderPurpose::Llm,
            api_key: None,
            api_key_hint: None,
            models: vec![CustomProviderModel {
                id: "m".into(),
                name: "M".into(),
            }],
            headers: vec![],
        };
        assert!(definition
            .validate()
            .unwrap_err()
            .contains("must not contain credentials"));

        definition.base_url = "https://example.test/v1".into();
        definition.headers = vec![CustomProviderHeader {
            name: "Host".into(),
            value: Some("redirect.example.test".into()),
        }];
        assert!(definition
            .validate()
            .unwrap_err()
            .contains("transport-controlled"));

        definition.headers[0] = CustomProviderHeader {
            name: "X-Api-Key\r\nInjected".into(),
            value: Some("secret".into()),
        };
        assert!(definition.validate().unwrap_err().contains("valid, unique"));
    }

    fn execution_request() -> ExecutionRequest {
        ExecutionRequest {
            request_version: 1,
            task: ExecutionTask::CharacterFaceLock,
            media_type: ExecutionMediaType::Image,
            prompt: "neutral face plate".into(),
            references: vec![ExecutionReference {
                reference_type: ExecutionReferenceType::CanonSnapshot,
                reference: "canon-1".into(),
                description: "locked visual spec".into(),
            }],
            constraints: vec![],
            expected_output: serde_json::from_value(serde_json::json!({
                "assetType": "face_lock",
                "mediaType": "image",
                "desiredStatus": "candidate",
                "ownerEntityInputRef": "characterEntityId"
            }))
            .unwrap(),
            provenance: ExecutionProvenance {
                workflow_run_id: "run-1".into(),
                skill_id: "character-builder".into(),
                skill_version: "1.0.0".into(),
                operation_id: "character.create_face_lock".into(),
            },
        }
    }

    #[test]
    fn provider_request_contains_execution_data_but_no_secret_or_canon_mutation() {
        let request = ProviderExecutionRequest::from_execution_request(
            "run-1",
            "execute",
            "compiled-1",
            "openai",
            "gpt-image-1",
            "run-1:execute:1",
            &execution_request(),
        )
        .unwrap();

        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["selectedProvider"], "openai");
        assert_eq!(value["selectedModel"], "gpt-image-1");
        assert!(value.get("apiKey").is_none());
        assert!(value.get("canon").is_none());
        assert!(value.get("skill").is_none());
    }

    #[test]
    fn capabilities_reject_unsupported_references_and_models_before_submission() {
        let request = ProviderExecutionRequest::from_execution_request(
            "run-1",
            "execute",
            "compiled-1",
            "openai",
            "unsupported-model",
            "run-1:execute:1",
            &execution_request(),
        )
        .unwrap();
        let capabilities = ProviderCapabilities {
            media_types: vec![ProviderMediaType::Image],
            supports_seed: false,
            supports_negative_prompt: true,
            supports_reference_image: false,
            supports_image_edit: false,
            supports_multiple_reference_images: false,
            supports_image_to_video: false,
            supports_cancel: false,
            supports_progress: false,
            supported_aspect_ratios: vec![],
            supported_models: vec!["gpt-image-1".into()],
        };

        let error = capabilities.supports(&request).unwrap_err();
        assert!(error.contains("reference images"));
    }

    #[test]
    fn lifecycle_serializes_as_stable_snake_case() {
        assert_eq!(
            serde_json::to_value(ProviderLifecycle::CancellationRequested).unwrap(),
            "cancellation_requested"
        );
    }
}
