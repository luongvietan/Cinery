use super::adapter::GenerationProvider;
use super::error::{redact_secret, ProviderError, ProviderErrorKind};
use super::http::{HttpTransport, UreqTransport};
use super::model::*;
use chrono::Utc;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Maximum size of a downloaded video accepted for parsing (200 MiB).
const MAX_VIDEO_BYTES: usize = 200 * 1024 * 1024;

/// OpenAI-compatible video generation (the `/videos` job API): submit a job,
/// poll it, then download the rendered video bytes. Used for custom providers
/// whose purpose is `video` and for any future built-in video surface.
pub struct OpenAiVideoProvider {
    provider_id: String,
    endpoint: String,
    bearer_token: String,
    supported_models: Vec<String>,
    transport: Box<dyn HttpTransport>,
    jobs: Arc<Mutex<BTreeMap<String, String>>>,
}

impl OpenAiVideoProvider {
    pub fn new(endpoint: impl Into<String>, bearer_token: impl Into<String>) -> Self {
        Self::with_transport(
            endpoint,
            bearer_token,
            UreqTransport::new(std::time::Duration::from_secs(120)),
        )
    }

    pub fn with_transport<T: HttpTransport + 'static>(
        endpoint: impl Into<String>,
        bearer_token: impl Into<String>,
        transport: T,
    ) -> Self {
        Self {
            provider_id: "openai-video".into(),
            endpoint: endpoint.into().trim_end_matches('/').into(),
            bearer_token: bearer_token.into(),
            supported_models: Vec::new(),
            transport: Box::new(transport),
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn with_provider_id(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = provider_id.into();
        self
    }

    pub fn with_models(mut self, models: Vec<String>) -> Self {
        if !models.is_empty() {
            self.supported_models = models;
        }
        self
    }

    fn jobs_endpoint(&self) -> String {
        format!("{}/videos", self.endpoint)
    }

    fn job_endpoint(&self, job_id: &str) -> String {
        format!("{}/{}", self.jobs_endpoint(), job_id)
    }

    fn content_endpoint(&self, job_id: &str) -> String {
        format!("{}/content", self.job_endpoint(job_id))
    }

    fn error(
        kind: ProviderErrorKind,
        message: impl Into<String>,
        diagnostic: impl Into<String>,
    ) -> ProviderError {
        ProviderError::new(kind, message).with_diagnostic(redact_secret(&diagnostic.into()))
    }
}

fn lifecycle_for(status: &str) -> ProviderLifecycle {
    match status {
        "completed" => ProviderLifecycle::Succeeded,
        "failed" => ProviderLifecycle::Failed,
        "cancelled" => ProviderLifecycle::Cancelled,
        "queued" | "in_progress" => ProviderLifecycle::Running,
        _ => ProviderLifecycle::Unknown,
    }
}

impl GenerationProvider for OpenAiVideoProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }
    fn adapter_version(&self) -> u32 {
        1
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            media_types: vec![ProviderMediaType::Video],
            supports_seed: false,
            supports_negative_prompt: false,
            supports_reference_image: false,
            supports_image_edit: false,
            supports_multiple_reference_images: false,
            supports_image_to_video: false,
            supports_cancel: false,
            supports_progress: true,
            supported_aspect_ratios: vec![],
            supported_models: self.supported_models.clone(),
            max_reference_images: None,
        }
    }
    fn submit(
        &self,
        request: &ProviderExecutionRequest,
    ) -> Result<ProviderSubmission, ProviderError> {
        self.capabilities().supports(request).map_err(|reason| {
            Self::error(
                ProviderErrorKind::UnsupportedCapability,
                reason,
                "capability check",
            )
        })?;
        if self.bearer_token.trim().is_empty() {
            return Err(Self::error(
                ProviderErrorKind::AuthenticationError,
                "video credential is not configured",
                "missing credential",
            ));
        }
        let body = serde_json::json!({
            "model": request.selected_model,
            "prompt": request.prompt,
        });
        let response = self
            .transport
            .post_json(&self.jobs_endpoint(), &self.bearer_token, &body)
            .map_err(|diagnostic| {
                Self::error(
                    ProviderErrorKind::NetworkError,
                    "video job submission failed",
                    diagnostic,
                )
            })?;
        let job_id = response
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                Self::error(
                    ProviderErrorKind::MalformedProviderResponse,
                    "video service response did not contain a job id",
                    response.to_string(),
                )
            })?
            .to_string();
        self.jobs
            .lock()
            .unwrap()
            .insert(job_id.clone(), request.selected_model.clone());
        Ok(ProviderSubmission {
            job: ProviderJobRef {
                provider_id: self.provider_id.clone(),
                provider_job_id: job_id,
                run_id: request.run_id.clone(),
                step_id: request.step_id.clone(),
                submission_id: request.idempotency_key.clone(),
                submitted_at: Utc::now().to_rfc3339(),
            },
            lifecycle: ProviderLifecycle::Submitted,
        })
    }
    fn poll(&self, job: &ProviderJobRef) -> Result<ProviderJobStatus, ProviderError> {
        let response = self
            .transport
            .get_json(&self.job_endpoint(&job.provider_job_id), &self.bearer_token)
            .map_err(|diagnostic| {
                Self::error(
                    ProviderErrorKind::NetworkError,
                    "video job status check failed",
                    diagnostic,
                )
            })?;
        let status = response
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let progress = response
            .get("progress")
            .and_then(serde_json::Value::as_f64)
            .map(|value| value.clamp(0.0, 100.0) as u8);
        Ok(ProviderJobStatus {
            lifecycle: lifecycle_for(status),
            progress_percent: progress,
            diagnostic: response
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        })
    }
    fn cancel(&self, job: &ProviderJobRef) -> Result<ProviderCancellationResult, ProviderError> {
        Ok(ProviderCancellationResult {
            provider_job_id: job.provider_job_id.clone(),
            lifecycle: ProviderLifecycle::Cancelled,
        })
    }
    fn fetch_result(&self, job: &ProviderJobRef) -> Result<ProviderResult, ProviderError> {
        let response = self
            .transport
            .get_json(&self.job_endpoint(&job.provider_job_id), &self.bearer_token)
            .map_err(|diagnostic| {
                Self::error(
                    ProviderErrorKind::NetworkError,
                    "video job result check failed",
                    diagnostic,
                )
            })?;
        let status = response
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if lifecycle_for(status) != ProviderLifecycle::Succeeded {
            return Err(Self::error(
                ProviderErrorKind::RemoteJobNotFound,
                "video job is not complete yet",
                status,
            ));
        }
        // Prefer a provider-supplied download URL; otherwise fetch the raw
        // content bytes from the conventional content endpoint.
        let uri = match response.get("url").and_then(serde_json::Value::as_str) {
            Some(url) if !url.is_empty() => url.to_string(),
            _ => {
                let bytes = self
                    .transport
                    .get_bytes(
                        &self.content_endpoint(&job.provider_job_id),
                        &self.bearer_token,
                        MAX_VIDEO_BYTES,
                    )
                    .map_err(|diagnostic| {
                        Self::error(
                            ProviderErrorKind::NetworkError,
                            "video download failed",
                            diagnostic,
                        )
                    })?;
                format!(
                    "data:video/mp4;base64,{}",
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
                )
            }
        };
        Ok(ProviderResult {
            outputs: vec![ProviderOutput {
                uri,
                mime_type: "video/mp4".into(),
                filename: Some("generated.mp4".into()),
            }],
            provider_reported_model: self
                .jobs
                .lock()
                .unwrap()
                .get(&job.provider_job_id)
                .cloned(),
            metadata: serde_json::json!({"response": "normalized"}),
        })
    }
}
