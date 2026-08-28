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
            generation_parameters: Default::default(), selected_provider: "openai".into(), selected_model: "gpt-image-1".into(), idempotency_key: "run-1:execute:1".into(),
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
        assert_eq!(body["model"], "gpt-image-1");
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
use super::http::{HttpTransport, UreqTransport};
use super::model::*;
use chrono::Utc;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

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

    fn images_endpoint(&self) -> String {
        format!("{}/images/generations", self.endpoint)
    }

    fn error(kind: ProviderErrorKind, message: impl Into<String>, diagnostic: impl Into<String>) -> ProviderError {
        ProviderError::new(kind, message).with_diagnostic(redact_secret(&diagnostic.into()))
    }
}

impl GenerationProvider for OpenAiImageProvider {
    fn id(&self) -> &'static str { "openai" }
    fn adapter_version(&self) -> u32 { 1 }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            media_types: vec![ProviderMediaType::Image],
            supports_seed: false,
            supports_negative_prompt: false,
            supports_reference_image: false,
            supports_image_edit: false,
            supports_multiple_reference_images: false,
            supports_image_to_video: false,
            supports_cancel: false,
            supports_progress: false,
            supported_aspect_ratios: vec!["square".into()],
            supported_models: vec!["gpt-image-1".into()],
            max_reference_images: Some(1),
        }
    }
    fn submit(&self, request: &ProviderExecutionRequest) -> Result<ProviderSubmission, ProviderError> {
        self.capabilities().supports(request).map_err(|reason| Self::error(ProviderErrorKind::UnsupportedCapability, reason, "capability check"))?;
        if self.bearer_token.trim().is_empty() {
            return Err(Self::error(ProviderErrorKind::AuthenticationError, "OpenAI credential is not configured", "missing credential"));
        }
        let body = serde_json::json!({
            "model": request.selected_model,
            "prompt": request.prompt,
            "size": "1024x1024",
        });
        let response = self.transport.post_json(&self.images_endpoint(), &self.bearer_token, &body).map_err(|diagnostic| {
            Self::error(ProviderErrorKind::NetworkError, "OpenAI image request failed", diagnostic)
        })?;
        let output = response.get("data").and_then(|data| data.as_array()).and_then(|data| data.first()).and_then(|item| item.get("url")).and_then(|url| url.as_str()).ok_or_else(|| {
            Self::error(ProviderErrorKind::MalformedProviderResponse, "OpenAI response did not contain an image URL", response.to_string())
        })?;
        let job_id = format!("openai:{}", request.idempotency_key);
        self.results.lock().unwrap().insert(job_id.clone(), ProviderResult {
            outputs: vec![ProviderOutput { uri: output.into(), mime_type: "image/png".into(), filename: Some("generated.png".into()) }],
            provider_reported_model: response.get("model").and_then(|value| value.as_str()).map(str::to_string),
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
