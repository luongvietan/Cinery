#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::adapter::GenerationProvider;
    use crate::providers::model::{ProviderExecutionRequest, ProviderLifecycle};
    use crate::providers::http::HttpTransport;
    use std::sync::{Arc, Mutex};

    struct FixtureTransport {
        requests: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl HttpTransport for FixtureTransport {
        fn post_json(&self, endpoint: &str, _: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
            self.requests.lock().unwrap().push((endpoint.into(), body.clone()));
            Ok(serde_json::json!({"created": 1, "data": [{"url": "https://files.example/result.png"}], "model": "gpt-image-1"}))
        }
        fn get_json(&self, _: &str, _: &str) -> Result<serde_json::Value, String> { Ok(serde_json::json!({})) }
        fn get_bytes(&self, _: &str, _: &str, _: usize) -> Result<Vec<u8>, String> { Ok(vec![137, 80, 78, 71]) }
    }

    fn request() -> ProviderExecutionRequest {
        ProviderExecutionRequest {
            run_id: "run-1".into(), step_id: "execute".into(), compiled_request_id: "compiled-1".into(),
            media_type: crate::workflow::execution::ExecutionMediaType::Image,
            task: crate::workflow::execution::ExecutionTask::CharacterFaceLock,
            prompt: "neutral face plate".into(), references: vec![], constraints: vec![],
            expected_output: serde_json::from_value(serde_json::json!({"assetType":"face_lock","mediaType":"image","desiredStatus":"candidate","ownerEntityInputRef":"characterEntityId"})).unwrap(),
            generation_parameters: Default::default(), selected_provider: "openai".into(), selected_model: "gpt-image-2".into(), idempotency_key: "run-1:execute:1".into(),
            reference_attachments: Vec::new(),
        }
    }

    #[test]
    fn reference_adapter_translates_request_and_normalizes_immediate_completion() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = OpenAiImageProvider::with_transport("https://api.example/v1", "secret", FixtureTransport { requests: requests.clone() });
        let submission = provider.submit(&request()).unwrap();
        assert_eq!(submission.lifecycle, ProviderLifecycle::Submitted);
        let body = &requests.lock().unwrap()[0].1;
        assert_eq!(body["prompt"], "neutral face plate");
        assert_eq!(body["model"], "gpt-image-2");
        assert!(body.get("apiKey").is_none());
        assert_eq!(provider.poll(&submission.job).unwrap().lifecycle, ProviderLifecycle::Succeeded);
        assert_eq!(provider.fetch_result(&submission.job).unwrap().outputs[0].uri, "https://files.example/result.png");
    }

    #[test]
    fn provider_errors_never_echo_bearer_token() {
        struct ErrorTransport;
        impl HttpTransport for ErrorTransport {
            fn post_json(&self, _: &str, _: &str, _: &serde_json::Value) -> Result<serde_json::Value, String> { Err("Authorization: Bearer secret".into()) }
            fn get_json(&self, _: &str, _: &str) -> Result<serde_json::Value, String> { Err("bad".into()) }
            fn get_bytes(&self, _: &str, _: &str, _: usize) -> Result<Vec<u8>, String> { Err("bad".into()) }
        }
        let provider = OpenAiImageProvider::with_transport("https://api.example/v1", "secret", ErrorTransport);
        let error = provider.submit(&request()).unwrap_err();
        assert!(!error.message.contains("secret"));
        assert!(!error.diagnostic.unwrap_or_default().contains("secret"));
    }
}
use super::adapter::GenerationProvider;
use super::error::{redact_secret, ProviderError, ProviderErrorKind};
use super::http::{HttpTransport, MultipartHttpRequest, MultipartPart, UreqTransport};
use super::model::*;
use chrono::Utc;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Maximum size of a provider response body accepted for parsing (50 MiB).
const MAX_RESPONSE_BYTES: usize = 50 * 1024 * 1024;

pub struct OpenAiImageProvider {
    endpoint: String,
    bearer_token: String,
    transport: Box<dyn HttpTransport>,
    results: Arc<Mutex<BTreeMap<String, ProviderResult>>>,
}

impl OpenAiImageProvider {
    pub fn new(endpoint: impl Into<String>, bearer_token: impl Into<String>) -> Self {
        Self::with_transport(endpoint, bearer_token, UreqTransport::new(std::time::Duration::from_secs(60)))
    }

    pub fn with_transport<T: HttpTransport + 'static>(
        endpoint: impl Into<String>,
        bearer_token: impl Into<String>,
        transport: T,
    ) -> Self {
        Self {
            endpoint: endpoint.into().trim_end_matches('/').into(),
            bearer_token: bearer_token.into(),
            transport: Box::new(transport),
            results: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    fn generations_endpoint(&self) -> String {
        format!("{}/images/generations", self.endpoint)
    }

    fn edits_endpoint(&self) -> String {
        format!("{}/images/edits", self.endpoint)
    }

    fn error(kind: ProviderErrorKind, message: impl Into<String>, diagnostic: impl Into<String>) -> ProviderError {
        ProviderError::new(kind, message).with_diagnostic(redact_secret(&diagnostic.into()))
    }
}

/// Normalizes one response `data` item into a provider output. Remote URLs
/// and inline base64 images are both supported; raw base64 bytes never land
/// in metadata.
fn normalize_output(item: &serde_json::Value) -> Option<ProviderOutput> {
    if let Some(url) = item.get("url").and_then(serde_json::Value::as_str) {
        if !url.is_empty() {
            return Some(ProviderOutput {
                uri: url.to_string(),
                mime_type: "image/png".into(),
                filename: Some("generated.png".into()),
            });
        }
    }
    if let Some(b64) = item.get("b64_json").and_then(serde_json::Value::as_str) {
        if base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64).is_ok() {
            return Some(ProviderOutput {
                uri: format!("data:image/png;base64,{b64}"),
                mime_type: "image/png".into(),
                filename: Some("generated.png".into()),
            });
        }
        return None;
    }
    None
}

impl GenerationProvider for OpenAiImageProvider {
    fn id(&self) -> &'static str { "openai" }
    fn adapter_version(&self) -> u32 { 2 }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            media_types: vec![ProviderMediaType::Image],
            supports_seed: false,
            supports_negative_prompt: false,
            supports_reference_image: true,
            supports_image_edit: true,
            supports_multiple_reference_images: true,
            supports_image_to_video: false,
            supports_cancel: false,
            supports_progress: false,
            supported_aspect_ratios: vec!["square".into()],
            supported_models: vec![super::super::providers::service::OPENAI_DEFAULT_MODEL.into()],
            max_reference_images: Some(1),
        }
    }
    fn submit(&self, request: &ProviderExecutionRequest) -> Result<ProviderSubmission, ProviderError> {
        self.capabilities().supports(request).map_err(|reason| Self::error(ProviderErrorKind::UnsupportedCapability, reason, "capability check"))?;
        if self.bearer_token.trim().is_empty() {
            return Err(Self::error(ProviderErrorKind::AuthenticationError, "OpenAI credential is not configured", "missing credential"));
        }
        // Dual-path: references select the multipart edits endpoint; the
        // JSON generations endpoint is used only for reference-free runs.
        let response = if request.reference_attachments.is_empty() {
            let body = serde_json::json!({
                "model": request.selected_model,
                "prompt": request.prompt,
                "size": "1024x1024",
            });
            self.transport
                .post_json(&self.generations_endpoint(), &self.bearer_token, &body)
                .map_err(|diagnostic| Self::error(ProviderErrorKind::NetworkError, "OpenAI image request failed", diagnostic))?
        } else {
            let mut parts = vec![
                MultipartPart { field_name: "model".into(), file_name: None, content_type: None, bytes: request.selected_model.clone().into_bytes() },
                MultipartPart { field_name: "prompt".into(), file_name: None, content_type: None, bytes: request.prompt.clone().into_bytes() },
                MultipartPart { field_name: "input_fidelity".into(), file_name: None, content_type: None, bytes: b"high".to_vec() },
            ];
            for attachment in &request.reference_attachments {
                parts.push(MultipartPart {
                    field_name: "image[]".into(),
                    file_name: Some(attachment.file_name.clone()),
                    content_type: Some(attachment.media_type.clone()),
                    bytes: attachment.bytes.clone(),
                });
            }
            let http_request = MultipartHttpRequest {
                endpoint: self.edits_endpoint(),
                bearer_token: self.bearer_token.clone(),
                parts,
                max_response_bytes: MAX_RESPONSE_BYTES,
            };
            self.transport
                .post_multipart(http_request)
                .map_err(|diagnostic| Self::error(ProviderErrorKind::NetworkError, "OpenAI image edit request failed", diagnostic))?
        };
        let items = response
            .get("data")
            .and_then(|data| data.as_array())
            .ok_or_else(|| {
                Self::error(ProviderErrorKind::MalformedProviderResponse, "OpenAI response did not contain a data array", response.to_string())
            })?;
        if items.is_empty() {
            return Err(Self::error(ProviderErrorKind::MalformedProviderResponse, "OpenAI response contained no generated images", response.to_string()));
        }
        let mut outputs = Vec::with_capacity(items.len());
        for item in items {
            let output = normalize_output(item).ok_or_else(|| {
                Self::error(ProviderErrorKind::MalformedProviderResponse, "OpenAI response contained an unreadable image payload", response.to_string())
            })?;
            outputs.push(output);
        }
        let job_id = format!("openai:{}", request.idempotency_key);
        self.results.lock().unwrap().insert(job_id.clone(), ProviderResult {
            outputs,
            provider_reported_model: response.get("model").and_then(|value| value.as_str()).map(str::to_string),
            // Metadata stays free of raw response bodies so inline image
            // bytes can never leak into logs or diagnostics.
            metadata: serde_json::json!({"response": "normalized"}),
        });
        Ok(ProviderSubmission {
            job: ProviderJobRef { provider_id: self.id().into(), provider_job_id: job_id, run_id: request.run_id.clone(), step_id: request.step_id.clone(), submission_id: request.idempotency_key.clone(), submitted_at: Utc::now().to_rfc3339() },
            lifecycle: ProviderLifecycle::Submitted,
        })
    }
    fn poll(&self, job: &ProviderJobRef) -> Result<ProviderJobStatus, ProviderError> {
        let succeeded = self.results.lock().unwrap().contains_key(&job.provider_job_id);
        Ok(ProviderJobStatus { lifecycle: if succeeded { ProviderLifecycle::Succeeded } else { ProviderLifecycle::Unknown }, progress_percent: succeeded.then_some(100), diagnostic: None })
    }
    fn cancel(&self, job: &ProviderJobRef) -> Result<ProviderCancellationResult, ProviderError> {
        Ok(ProviderCancellationResult { provider_job_id: job.provider_job_id.clone(), lifecycle: ProviderLifecycle::Cancelled })
    }
    fn fetch_result(&self, job: &ProviderJobRef) -> Result<ProviderResult, ProviderError> {
        self.results.lock().unwrap().get(&job.provider_job_id).cloned().ok_or_else(|| Self::error(ProviderErrorKind::RemoteJobNotFound, "OpenAI image result was not found", &job.provider_job_id))
    }
}


#[cfg(test)]
mod gpt_image_2_tests {
    use super::*;
    use crate::providers::adapter::GenerationProvider;
    use crate::providers::http::{HttpTransport, MultipartHttpRequest, MultipartPart};
    use crate::providers::model::ProviderReferenceAttachment;
    use crate::workflow::execution::{ExecutionMediaType, ExecutionTask};
    use std::sync::{Arc, Mutex};

    type JsonRequestLog = Arc<Mutex<Vec<(String, String, serde_json::Value)>>>;
    type MultipartRequestLog = Arc<Mutex<Vec<(String, String, Vec<MultipartPart>)>>>;

    #[derive(Clone, Default)]
    struct RecordingTransport {
        json_requests: JsonRequestLog,
        multipart_requests: MultipartRequestLog,
        response: Arc<Mutex<Option<serde_json::Value>>>,
    }

    impl RecordingTransport {
        fn with_response(response: serde_json::Value) -> Self {
            Self { response: Arc::new(Mutex::new(Some(response))), ..Default::default() }
        }
    }

    impl HttpTransport for RecordingTransport {
        fn post_json(&self, endpoint: &str, bearer: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
            self.json_requests.lock().unwrap().push((endpoint.into(), bearer.into(), body.clone()));
            Ok(self.response.lock().unwrap().clone().unwrap_or_else(|| serde_json::json!({"data": [{"url": "https://files.example/out.png"}], "model": "gpt-image-2"})))
        }
        fn post_multipart(&self, request: MultipartHttpRequest) -> Result<serde_json::Value, String> {
            self.multipart_requests.lock().unwrap().push((request.endpoint, request.bearer_token, request.parts));
            Ok(self.response.lock().unwrap().clone().unwrap_or_else(|| serde_json::json!({"data": [{"url": "https://files.example/out.png"}], "model": "gpt-image-2"})))
        }
        fn get_json(&self, _: &str, _: &str) -> Result<serde_json::Value, String> { Ok(serde_json::json!({})) }
        fn get_bytes(&self, _: &str, _: &str, _: usize) -> Result<Vec<u8>, String> { Ok(vec![137, 80, 78, 71]) }
    }

    fn base_request() -> ProviderExecutionRequest {
        ProviderExecutionRequest {
            run_id: "run-1".into(), step_id: "execute".into(), compiled_request_id: "compiled-1".into(),
            media_type: ExecutionMediaType::Image, task: ExecutionTask::CharacterFaceLock,
            prompt: "neutral face plate".into(), references: vec![], constraints: vec![],
            expected_output: serde_json::from_value(serde_json::json!({"assetType":"face_lock","mediaType":"image","desiredStatus":"candidate","ownerEntityInputRef":"characterEntityId"})).unwrap(),
            generation_parameters: Default::default(), selected_provider: "openai".into(), selected_model: "gpt-image-2".into(), idempotency_key: "run-1:execute:1".into(),
            reference_attachments: vec![],
        }
    }

    fn png() -> Vec<u8> {
        let image: image::RgbaImage = image::ImageBuffer::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]));
        let mut cursor = std::io::Cursor::new(Vec::new());
        image.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
        cursor.into_inner()
    }

    #[test]
    fn without_references_posts_json_generations() {
        let transport = RecordingTransport::default();
        let json_requests = transport.json_requests.clone();
        let multipart_requests = transport.multipart_requests.clone();
        let provider = OpenAiImageProvider::with_transport("https://api.example/v1", "secret", transport);
        let request = base_request();
        provider.submit(&request).unwrap();
        let json_requests = json_requests.lock().unwrap();
        assert_eq!(json_requests.len(), 1);
        assert!(json_requests[0].0.ends_with("/images/generations"));
        assert_eq!(json_requests[0].2["model"], "gpt-image-2");
        assert_eq!(json_requests[0].2["prompt"], "neutral face plate");
        assert_eq!(multipart_requests.lock().unwrap().len(), 0);
    }

    #[test]
    fn with_references_posts_multipart_edits_with_high_fidelity() {
        let transport = RecordingTransport::default();
        let json_requests = transport.json_requests.clone();
        let multipart_requests = transport.multipart_requests.clone();
        let provider = OpenAiImageProvider::with_transport("https://api.example/v1", "secret", transport);
        let mut request = base_request();
        request.task = ExecutionTask::CharacterOutfit;
        request.reference_attachments.push(ProviderReferenceAttachment {
            asset_version_id: "v-1".into(), file_name: "face.png".into(), media_type: "image/png".into(), bytes: png(), sha256: "a".repeat(64),
        });
        request.reference_attachments.push(ProviderReferenceAttachment {
            asset_version_id: "v-2".into(), file_name: "sheet.png".into(), media_type: "image/png".into(), bytes: png(), sha256: "b".repeat(64),
        });
        provider.submit(&request).unwrap();
        let multipart = multipart_requests.lock().unwrap();
        assert_eq!(multipart.len(), 1);
        assert!(multipart[0].0.ends_with("/images/edits"));
        let parts = &multipart[0].2;
        let image_parts: Vec<&MultipartPart> = parts.iter().filter(|p| p.field_name == "image[]").collect();
        assert_eq!(image_parts.len(), 2, "one image[] part per verified attachment");
        assert_eq!(image_parts[0].file_name.as_deref(), Some("face.png"));
        assert_eq!(image_parts[1].file_name.as_deref(), Some("sheet.png"));
        let model = parts.iter().find(|p| p.field_name == "model").unwrap();
        assert_eq!(model.bytes, b"gpt-image-2".to_vec());
        let fidelity = parts.iter().find(|p| p.field_name == "input_fidelity").unwrap();
        assert_eq!(fidelity.bytes, b"high".to_vec());
        let prompt = parts.iter().find(|p| p.field_name == "prompt").unwrap();
        assert_eq!(prompt.bytes, b"neutral face plate".to_vec());
        assert_eq!(json_requests.lock().unwrap().len(), 0, "references must never use the JSON generations endpoint");
    }

    #[test]
    fn base64_output_is_normalized_without_logging_bytes() {
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png());
        let transport = RecordingTransport::with_response(serde_json::json!({
            "created": 1, "model": "gpt-image-2",
            "data": [{"b64_json": b64}]
        }));
        let provider = OpenAiImageProvider::with_transport("https://api.example/v1", "secret", transport);
        let submission = provider.submit(&base_request()).unwrap();
        let result = provider.fetch_result(&submission.job).unwrap();
        assert_eq!(result.outputs.len(), 1);
        // The data URI carries the image but metadata must not embed raw bytes.
        let serialized = serde_json::to_string(&result.metadata).unwrap();
        assert!(!serialized.contains(&b64[..32.min(b64.len())]));
    }

    #[test]
    fn malformed_base64_and_empty_data_fail_without_submission_artifacts() {
        // Malformed base64 fails during response normalization at submit
        // time, before any artifact can be persisted.
        let transport = RecordingTransport::with_response(serde_json::json!({"data": [{"b64_json": "not-base64!!!"}]}));
        let provider = OpenAiImageProvider::with_transport("https://api.example/v1", "secret", transport);
        let error = provider.submit(&base_request()).unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::MalformedProviderResponse);

        let transport = RecordingTransport::with_response(serde_json::json!({"data": []}));
        let provider = OpenAiImageProvider::with_transport("https://api.example/v1", "secret", transport);
        let error = provider.submit(&base_request()).unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::MalformedProviderResponse);
    }

    #[test]
    fn unsupported_output_media_type_fails_validation() {
        let transport = RecordingTransport::with_response(serde_json::json!({
            "data": [{"url": "https://files.example/out.png"}], "model": "gpt-image-2", "output_format": "gif"
        }));
        let provider = OpenAiImageProvider::with_transport("https://api.example/v1", "secret", transport);
        let submission = provider.submit(&base_request()).unwrap();
        // fetch_result itself only reports the stored outputs; MIME validation
        // happens during capture, so the URI normalization keeps image/png.
        let result = provider.fetch_result(&submission.job).unwrap();
        assert_eq!(result.outputs[0].mime_type, "image/png");
    }

    #[test]
    fn capabilities_declare_gpt_image_2_with_reference_edit_support() {
        let provider = OpenAiImageProvider::with_transport("https://api.example/v1", "secret", RecordingTransport::default());
        let capabilities = provider.capabilities();
        assert!(capabilities.supported_models.contains(&"gpt-image-2".to_string()));
        assert!(capabilities.supports_reference_image);
        assert!(capabilities.supports_image_edit);
        assert!(capabilities.supports_multiple_reference_images);
    }
}
