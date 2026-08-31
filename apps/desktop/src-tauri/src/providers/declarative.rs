//! The declarative provider runtime: one implementation covers every
//! provider whose HTTP surface can be described by configuration.

use super::adapter::{GenerationProvider, PollingSpec};
use super::config::{
    self, AsyncJobConfig, AuthConfig, AuthMode, CanonicalValues, EndpointConfig,
    ProviderRuntimeConfig, ReferenceInput, RequestType, ResponseMapping, UrlContext,
    OPERATION_IMAGE_EDIT, OPERATION_IMAGE_GENERATE, OPERATION_VALIDATE, OPERATION_VIDEO_GENERATE,
    OPERATION_VIDEO_IMAGE_TO_VIDEO,
};
use super::error::{redact_secret, ProviderError, ProviderErrorKind};
use super::http::{HttpBody, HttpExecutor, HttpRequest, HttpResponse, MultipartPart};
use super::model::{
    CustomProviderModel, ProviderCancellationResult, ProviderCapabilities, ProviderExecutionRequest,
    ProviderJobRef, ProviderJobStatus, ProviderLifecycle, ProviderOutput, ProviderResult,
    ProviderSubmission,
};
use chrono::Utc;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Maximum size of a provider response body accepted for parsing (50 MiB).
const MAX_RESPONSE_BYTES: usize = 50 * 1024 * 1024;

pub struct DeclarativeProvider {
    provider_id: String,
    base_url: String,
    models: Vec<CustomProviderModel>,
    config: ProviderRuntimeConfig,
    /// Resolved vault secret for the configured auth mode. Never serialized.
    api_key: Option<String>,
    /// Resolved values for the config's static custom headers.
    header_values: BTreeMap<String, String>,
    transport: Arc<dyn HttpExecutor>,
    /// Sync results by provider job id (synchronous operations resolve on
    /// submit; async operations fetch from the API instead).
    results: Mutex<BTreeMap<String, ProviderResult>>,
    /// Which operation each async job id belongs to.
    job_operations: Mutex<BTreeMap<String, String>>,
}

impl DeclarativeProvider {
    pub fn new(
        provider_id: impl Into<String>,
        base_url: impl Into<String>,
        models: Vec<CustomProviderModel>,
        config: ProviderRuntimeConfig,
        api_key: Option<String>,
        header_values: BTreeMap<String, String>,
        transport: Arc<dyn HttpExecutor>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            base_url: base_url.into(),
            models,
            config,
            api_key,
            header_values,
            transport,
            results: Mutex::new(BTreeMap::new()),
            job_operations: Mutex::new(BTreeMap::new()),
        }
    }

    fn error(
        &self,
        kind: ProviderErrorKind,
        message: impl Into<String>,
        diagnostic: impl Into<String>,
    ) -> ProviderError {
        ProviderError::new(kind, message).with_diagnostic(redact_secret(&diagnostic.into()))
    }

    /// Chooses the operation for a canonical request. Editing is never
    /// assumed: image edit runs only when the provider defines the operation
    /// and references are present (or repair was requested).
    fn select_operation(
        &self,
        request: &ProviderExecutionRequest,
    ) -> Result<String, ProviderError> {
        let has_references = !request.references.is_empty();
        let candidate = match request.media_type {
            crate::workflow::execution::ExecutionMediaType::Image => {
                if has_references
                    || request.task == crate::workflow::execution::ExecutionTask::VisualRepair
                {
                    OPERATION_IMAGE_EDIT
                } else {
                    OPERATION_IMAGE_GENERATE
                }
            }
            crate::workflow::execution::ExecutionMediaType::Video => {
                if has_references {
                    OPERATION_VIDEO_IMAGE_TO_VIDEO
                } else {
                    OPERATION_VIDEO_GENERATE
                }
            }
        };
        self.operation_for(candidate)
    }

    fn operation_for(&self, operation: &str) -> Result<String, ProviderError> {
        if self.config.operations.contains_key(operation) {
            return Ok(operation.to_string());
        }
        // A video edit without imageToVideo falls back to plain generation.
        if operation == OPERATION_VIDEO_IMAGE_TO_VIDEO
            && self.config.operations.contains_key(OPERATION_VIDEO_GENERATE)
        {
            return Ok(OPERATION_VIDEO_GENERATE.to_string());
        }
        Err(self.error(
            ProviderErrorKind::UnsupportedCapability,
            format!(
                "this AI service does not define the required operation ({operation})"
            ),
            "operation lookup",
        ))
    }

    fn model_supports(&self, model_id: &str, operation: &str) -> Result<(), ProviderError> {
        let Some(model) = self.models.iter().find(|model| model.id == model_id) else {
            return Err(self.error(
                ProviderErrorKind::UnsupportedCapability,
                format!("model {model_id} is not configured for this AI service"),
                "model lookup",
            ));
        };
        if model.capabilities.is_empty() || model.capabilities.iter().any(|cap| cap == operation) {
            return Ok(());
        }
        Err(self.error(
            ProviderErrorKind::UnsupportedCapability,
            format!("model {model_id} does not support {operation}"),
            "model capabilities",
        ))
    }

    fn canonical_values(&self, request: &ProviderExecutionRequest) -> CanonicalValues {
        CanonicalValues {
            prompt: request.prompt.clone(),
            negative_prompt: None,
            model: request.selected_model.clone(),
            width: request.generation_parameters.width,
            height: request.generation_parameters.height,
            aspect_ratio: request.generation_parameters.aspect_ratio.clone(),
            seed: request.generation_parameters.seed,
            steps: None,
            quality: None,
            duration_seconds: request.generation_parameters.duration_seconds,
            fps: request.generation_parameters.fps,
            strength: None,
        }
    }

    fn reference_inputs(
        &self,
        request: &ProviderExecutionRequest,
    ) -> Vec<ReferenceInput> {
        request
            .reference_attachments
            .iter()
            .map(|attachment| ReferenceInput {
                file_name: attachment.file_name.clone(),
                media_type: attachment.media_type.clone(),
                bytes: attachment.bytes.clone(),
            })
            .collect()
    }

    /// Applies the configured authentication to a compiled request.
    fn apply_auth(
        &self,
        operation: &str,
        mut url: String,
        mut headers: Vec<(String, String)>,
    ) -> Result<(String, Vec<(String, String)>), ProviderError> {
        let auth = &self.config.auth;
        match auth.mode {
            AuthMode::None => {}
            AuthMode::Bearer => {
                let key = self.require_api_key(operation)?;
                headers.push(("Authorization".into(), format!("Bearer {key}")));
            }
            AuthMode::Header => {
                let name = auth.credential_name.as_deref().unwrap_or_default();
                if name.is_empty() {
                    return Err(self.error(
                        ProviderErrorKind::InvalidRequest,
                        "header authentication is missing its credential name",
                        "auth config",
                    )
                    .with_operation(operation));
                }
                let key = self.require_api_key(operation)?;
                headers.push((name.to_string(), key));
            }
            AuthMode::Query => {
                let name = auth.credential_name.as_deref().unwrap_or_default();
                if name.is_empty() {
                    return Err(self.error(
                        ProviderErrorKind::InvalidRequest,
                        "query authentication is missing its credential name",
                        "auth config",
                    )
                    .with_operation(operation));
                }
                let key = self.require_api_key(operation)?;
                url = config::with_query_parameter(&url, name, &key);
            }
        }
        Ok((url, headers))
    }

    fn require_api_key(&self, operation: &str) -> Result<String, ProviderError> {
        self.api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                self.error(
                    ProviderErrorKind::AuthenticationError,
                    "the API credential is not configured for this AI service",
                    "missing credential",
                )
                .with_operation(operation)
            })
    }

    fn url_context(
        &self,
        operation: &str,
        values: &CanonicalValues,
        job_id: Option<&str>,
    ) -> UrlContext {
        UrlContext {
            values: values.clone(),
            provider_id: self.provider_id.clone(),
            account_id: self.config.account_id.clone(),
            operation: operation.to_string(),
            job_id: job_id.map(str::to_string),
        }
    }

    /// Compiles an HTTP request from explicit parts. Used for operation
    /// endpoints and for status/fetch requests alike.
    #[allow(clippy::too_many_arguments)]
    fn compile_http(
        &self,
        operation: &str,
        method: &str,
        path_template: &str,
        body: HttpBody,
        values: &CanonicalValues,
        references: &[ReferenceInput],
        job_id: Option<&str>,
        extra_headers: Vec<(String, String)>,
    ) -> Result<HttpRequest, ProviderError> {
        let context = self.url_context(operation, values, job_id);
        let base = config::compile_url_template(&self.base_url, &context);
        let url = if path_template.is_empty() {
            base
        } else {
            format!(
                "{}{}",
                base.trim_end_matches('/'),
                config::compile_url_template(path_template, &context)
            )
        };

        let mut headers: Vec<(String, String)> = Vec::new();
        for (name, value) in &self.config.headers {
            if let Some(resolved) = self.header_values.get(name) {
                headers.push((name.clone(), resolved.clone()));
            } else {
                headers.push((name.clone(), value.clone()));
            }
        }
        headers.extend(extra_headers);

        let (url, headers) = self.apply_auth(operation, url, headers)?;
        Ok(HttpRequest {
            method: method.to_ascii_uppercase(),
            url,
            headers,
            body,
            max_response_bytes: MAX_RESPONSE_BYTES,
        })
    }

    /// Compiles the request body for one operation endpoint.
    fn compile_body(
        &self,
        operation: &str,
        endpoint: &EndpointConfig,
        values: &CanonicalValues,
        references: &[ReferenceInput],
    ) -> Result<HttpBody, ProviderError> {
        match endpoint.request_type {
            RequestType::Json => {
                let template = endpoint.request_mapping.as_ref().ok_or_else(|| {
                    self.error(
                        ProviderErrorKind::InvalidRequest,
                        format!("operation {operation} has no request mapping"),
                        "request compile",
                    )
                    .with_operation(operation)
                })?;
                Ok(HttpBody::Json(config::compile_json_template(
                    template, values, references,
                )))
            }
            RequestType::FormUrlEncoded => {
                let template = endpoint.request_mapping.as_ref().ok_or_else(|| {
                    self.error(
                        ProviderErrorKind::InvalidRequest,
                        format!("operation {operation} has no request mapping"),
                        "request compile",
                    )
                    .with_operation(operation)
                })?;
                let bytes = config::compile_form_urlencoded(template, values, references)
                    .map_err(|message| {
                        self.error(
                            ProviderErrorKind::InvalidRequest,
                            message,
                            "request compile",
                        )
                        .with_operation(operation)
                    })?;
                Ok(HttpBody::Raw {
                    bytes,
                    content_type: "application/x-www-form-urlencoded".into(),
                })
            }
            RequestType::Multipart => {
                let parts: Vec<MultipartPart> =
                    config::compile_multipart(&endpoint.multipart_fields, values, references)
                        .map_err(|message| {
                            self.error(
                                ProviderErrorKind::InvalidRequest,
                                message,
                                "request compile",
                            )
                            .with_operation(operation)
                        })?;
                Ok(HttpBody::Multipart(parts))
            }
        }
    }

    /// Compiles the full HTTP request for one operation endpoint against
    /// canonical input. GET/HEAD/DELETE requests never carry a body, so the
    /// request mapping on such endpoints documents the shape but is not sent.
    fn compile_call(
        &self,
        operation: &str,
        endpoint: &EndpointConfig,
        values: &CanonicalValues,
        references: &[ReferenceInput],
        job_id: Option<&str>,
    ) -> Result<HttpRequest, ProviderError> {
        let method = endpoint.method.to_ascii_uppercase();
        let body: HttpBody = if matches!(method.as_str(), "GET" | "HEAD" | "DELETE") {
            HttpBody::None
        } else {
            self.compile_body(operation, endpoint, values, references)?
        };
        let extra_headers: Vec<(String, String)> = endpoint
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        self.compile_http(
            operation,
            &method,
            &endpoint.path_template,
            body,
            values,
            references,
            job_id,
            extra_headers,
        )
    }

    /// Status/fetch request for an async job (GET-style, no body).
    #[allow(clippy::too_many_arguments)]
    fn compile_job_request(
        &self,
        operation: &str,
        method: &str,
        path_template: &str,
        extra_headers: Vec<(String, String)>,
        job_id: &str,
    ) -> Result<HttpRequest, ProviderError> {
        let values = CanonicalValues::default();
        let request = self.compile_http(
            operation,
            method,
            path_template,
            HttpBody::None,
            &values,
            &[],
            Some(job_id),
            extra_headers,
        )?;
        Ok(request)
    }

    fn execute_call(
        &self,
        operation: &str,
        request: HttpRequest,
    ) -> Result<HttpResponse, ProviderError> {
        self.transport.execute(request).map_err(|failure| {
            self.error(
                ProviderErrorKind::NetworkError,
                format!("the request to the AI service failed during {operation}"),
                failure.message,
            )
            .with_operation(operation)
        })
    }

    /// Normalizes a non-success response into a rich provider error.
    fn error_from_response(&self, operation: &str, response: &HttpResponse) -> ProviderError {
        let document = response.json().ok();
        let mapping = self.config.error_mapping.clone().unwrap_or_default();
        let (provider_message, _code, request_id) = match &document {
            Some(document) => mapping.extract(document),
            None => (None, None, None),
        };
        let request_id = request_id.or_else(|| {
            response
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("x-request-id"))
                .map(|(_, value)| value.clone())
        });
        let kind = match response.status {
            401 => ProviderErrorKind::AuthenticationError,
            403 => ProviderErrorKind::AuthorizationError,
            408 => ProviderErrorKind::Timeout,
            429 => ProviderErrorKind::RateLimited,
            500..=599 => ProviderErrorKind::ProviderUnavailable,
            _ => ProviderErrorKind::InvalidRequest,
        };
        let message = match kind {
            ProviderErrorKind::AuthenticationError => {
                "the AI service rejected the credential".to_string()
            }
            ProviderErrorKind::AuthorizationError => {
                "the credential is not authorized for this AI service".to_string()
            }
            ProviderErrorKind::RateLimited => {
                "the AI service rate-limited the request".to_string()
            }
            ProviderErrorKind::ProviderUnavailable => {
                "the AI service reported a server-side failure".to_string()
            }
            _ => format!("the AI service rejected the {operation} request"),
        };
        let mut error = ProviderError::new(kind, message)
            .with_diagnostic(format!(
                "HTTP {}: {}",
                response.status,
                redact_secret(&response.text())
            ))
            .with_status_code(response.status)
            .with_operation(operation);
        if let Some(message) = provider_message {
            error = error.with_provider_message(message);
        }
        if let Some(request_id) = request_id {
            error = error.with_request_id(request_id);
        }
        error
    }

    fn extract_outputs(
        &self,
        operation: &str,
        mapping: &ResponseMapping,
        document: &serde_json::Value,
    ) -> Result<(Vec<ProviderOutput>, Option<String>), ProviderError> {
        mapping
            .extract_outputs(document, operation)
            .map(|(outputs, request_id)| {
                (
                    outputs
                        .into_iter()
                        .map(|output| ProviderOutput {
                            uri: output.uri,
                            mime_type: output.mime_type,
                            filename: Some(output.filename),
                        })
                        .collect(),
                    request_id,
                )
            })
            .map_err(|message| {
                self.error(
                    ProviderErrorKind::MalformedProviderResponse,
                    "the AI service response could not be read at the configured output path",
                    format!("{message}; body: {}", redact_secret(&document.to_string())),
                )
                .with_operation(operation)
            })
    }

    fn sync_job_id(&self, request: &ProviderExecutionRequest) -> String {
        format!("{}:{}", self.provider_id, request.idempotency_key)
    }

    /// Performs one status request for an async job and maps it to a
    /// lifecycle. Unrecognized statuses are in-progress, not terminal.
    fn poll_async(
        &self,
        operation: &str,
        endpoint: &EndpointConfig,
        job_config: &AsyncJobConfig,
        job: &ProviderJobRef,
    ) -> Result<ProviderJobStatus, ProviderError> {
        let request = self.compile_job_request(
            operation,
            &job_config.status.method,
            &job_config.status.path_template,
            endpoint
                .headers
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            &job.provider_job_id,
        )?;
        let response = self.execute_call(operation, request)?;
        if !response.is_success() {
            return Err(self.error_from_response(operation, &response));
        }
        let document = response.json().map_err(|error| {
            self.error(
                ProviderErrorKind::MalformedProviderResponse,
                "the AI service returned a non-JSON job status",
                error,
            )
            .with_operation(operation)
        })?;
        let status = config::resolve_json_path(&document, &job_config.status.status_path)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let normalized = status.to_ascii_lowercase();
        let lifecycle = if job_config
            .status
            .completed_values
            .iter()
            .any(|value| value.eq_ignore_ascii_case(&normalized))
        {
            ProviderLifecycle::Succeeded
        } else if job_config
            .status
            .failed_values
            .iter()
            .any(|value| value.eq_ignore_ascii_case(&normalized))
        {
            ProviderLifecycle::Failed
        } else {
            ProviderLifecycle::Running
        };
        let diagnostic = if lifecycle == ProviderLifecycle::Failed {
            job_config
                .status
                .error_message_path
                .as_deref()
                .and_then(|path| config::resolve_json_path(&document, path))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    self.config
                        .error_mapping
                        .clone()
                        .unwrap_or_default()
                        .extract(&document)
                        .0
                })
        } else {
            None
        };
        let progress_percent = job_config
            .status
            .progress_path
            .as_deref()
            .and_then(|path| config::resolve_json_path(&document, path))
            .and_then(|value| {
                value
                    .as_f64()
                    .or_else(|| {
                        value
                            .as_str()
                            .and_then(|text| text.trim().parse::<f64>().ok())
                    })
                    .map(|value| value.clamp(0.0, 100.0) as u8)
            });
        Ok(ProviderJobStatus {
            lifecycle,
            progress_percent,
            diagnostic,
        })
    }

    fn fetch_async_result(
        &self,
        operation: &str,
        endpoint: &EndpointConfig,
        job_config: &AsyncJobConfig,
        job: &ProviderJobRef,
    ) -> Result<ProviderResult, ProviderError> {
        let binary_fetch = job_config.output.fetch_path_template.is_some()
            && job_config.output.response.binary_response;
        let (method, path_template, mapping) = match &job_config.output.fetch_path_template {
            Some(template) => (
                job_config.output.fetch_method.clone(),
                template.clone(),
                job_config.output.response.clone(),
            ),
            None => (
                job_config.status.method.clone(),
                job_config.status.path_template.clone(),
                job_config.output.response.clone(),
            ),
        };
        let request = self.compile_job_request(
            operation,
            &method,
            &path_template,
            endpoint
                .headers
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            &job.provider_job_id,
        )?;
        let response = self.execute_call(operation, request)?;
        if !response.is_success() {
            return Err(self.error_from_response(operation, &response));
        }
        if binary_fetch {
            // A configured binary fetch endpoint returns the asset bytes
            // directly (e.g. /videos/{jobId}/content).
            let output = mapping.extract_binary_output(
                &response.body,
                response.content_type.as_deref(),
                operation,
            );
            return Ok(ProviderResult {
                outputs: vec![ProviderOutput {
                    uri: output.uri,
                    mime_type: output.mime_type,
                    filename: Some(output.filename),
                }],
                provider_reported_model: None,
                metadata: serde_json::json!({"response": "normalized"}),
            });
        }
        let document = response.json().map_err(|error| {
            self.error(
                ProviderErrorKind::MalformedProviderResponse,
                "the AI service returned a non-JSON job result",
                error,
            )
            .with_operation(operation)
        })?;
        let (outputs, _request_id) =
            self.extract_outputs(operation, &mapping, &document)?;
        if outputs.is_empty() {
            return Err(self.error(
                ProviderErrorKind::MalformedProviderResponse,
                "the AI service returned no outputs for the completed job",
                redact_secret(&document.to_string()),
            )
            .with_operation(operation));
        }
        Ok(ProviderResult {
            outputs,
            provider_reported_model: None,
            // Metadata stays free of raw response bodies so inline bytes can
            // never leak into logs or diagnostics.
            metadata: serde_json::json!({"response": "normalized"}),
        })
    }

    /// Validation used by "Test connection": runs the configured validate
    /// operation (if any) and returns the outcome without exposing secrets.
    pub fn run_validation(&self, model_id: Option<&str>) -> Result<ValidationOutcome, ProviderError> {
        let Some(endpoint) = self.config.operations.get(OPERATION_VALIDATE) else {
            return Ok(ValidationOutcome {
                performed_network_check: false,
                status_code: None,
                provider_message: None,
            });
        };
        let model = model_id
            .map(str::to_string)
            .or_else(|| self.models.first().map(|model| model.id.clone()))
            .unwrap_or_default();
        let values = CanonicalValues {
            model,
            ..Default::default()
        };
        let request = self.compile_call(OPERATION_VALIDATE, endpoint, &values, &[], None)?;
        let response = self.execute_call(OPERATION_VALIDATE, request)?;
        if response.is_success() {
            return Ok(ValidationOutcome {
                performed_network_check: true,
                status_code: Some(response.status),
                provider_message: None,
            });
        }
        let error = self.error_from_response(OPERATION_VALIDATE, &response);
        Ok(ValidationOutcome {
            performed_network_check: true,
            status_code: Some(response.status),
            provider_message: error
                .provider_message
                .or_else(|| Some(redact_secret(&response.text()).chars().take(300).collect())),
        })
    }
}

/// Result of the configurable validation operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationOutcome {
    pub performed_network_check: bool,
    pub status_code: Option<u16>,
    pub provider_message: Option<String>,
}

/// Polling hints a provider can offer to the submission loop.
impl DeclarativeProvider {
    /// The async operations' configured polling behavior, if any.
    pub fn configured_polling_spec(&self) -> Option<PollingSpec> {
        self.config
            .operations
            .values()
            .filter_map(|endpoint| endpoint.job.as_ref().map(|job| job.polling.clone()))
            .max_by_key(|polling| polling.timeout_ms)
            .map(|polling| PollingSpec {
                interval: std::time::Duration::from_millis(polling.interval_ms.max(50)),
                timeout: std::time::Duration::from_millis(polling.timeout_ms),
            })
    }
}

impl GenerationProvider for DeclarativeProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }

    fn adapter_version(&self) -> u32 {
        1
    }

    fn polling_spec(&self) -> PollingSpec {
        self.configured_polling_spec().unwrap_or_default()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        let operations = &self.config.operations;
        let mut media_types = Vec::new();
        if operations.contains_key(OPERATION_IMAGE_GENERATE)
            || operations.contains_key(OPERATION_IMAGE_EDIT)
        {
            media_types.push(super::model::ProviderMediaType::Image);
        }
        if operations.contains_key(OPERATION_VIDEO_GENERATE) {
            media_types.push(super::model::ProviderMediaType::Video);
        }
        let all_endpoints: Vec<&EndpointConfig> = operations.values().collect();
        let supports_reference_image = all_endpoints
            .iter()
            .any(|endpoint| endpoint.accepts_reference_images());
        let supports_multiple = all_endpoints
            .iter()
            .any(|endpoint| endpoint.accepts_multiple_reference_images());
        ProviderCapabilities {
            media_types,
            supports_seed: false,
            supports_negative_prompt: false,
            supports_reference_image,
            supports_image_edit: operations.contains_key(OPERATION_IMAGE_EDIT),
            supports_multiple_reference_images: supports_multiple,
            supports_image_to_video: operations.contains_key(OPERATION_VIDEO_IMAGE_TO_VIDEO),
            supports_cancel: false,
            supports_progress: operations.values().any(|endpoint| {
                endpoint
                    .job
                    .as_ref()
                    .is_some_and(|job| job.status.progress_path.is_some())
            }),
            supported_aspect_ratios: Vec::new(),
            supported_models: self.models.iter().map(|model| model.id.clone()).collect(),
            max_reference_images: None,
        }
    }

    fn submit(
        &self,
        request: &ProviderExecutionRequest,
    ) -> Result<ProviderSubmission, ProviderError> {
        self.capabilities()
            .supports(request)
            .map_err(|reason| {
                self.error(
                    ProviderErrorKind::UnsupportedCapability,
                    reason,
                    "capability check",
                )
            })?;
        let operation = self.select_operation(request)?;
        self.model_supports(&request.selected_model, &operation)?;
        let values = self.canonical_values(request);
        let references = self.reference_inputs(request);
        let endpoint = self.config.operations.get(&operation).ok_or_else(|| {
            self.error(
                ProviderErrorKind::UnsupportedCapability,
                format!("operation {operation} is not configured"),
                "operation lookup",
            )
        })?;
        let http_request = self.compile_call(&operation, endpoint, &values, &references, None)?;
        let response = self.execute_call(&operation, http_request)?;
        if !response.is_success() {
            return Err(self.error_from_response(&operation, &response));
        }

        match &endpoint.job {
            Some(job_config) => {
                let document = response.json().map_err(|error| {
                    self.error(
                        ProviderErrorKind::MalformedProviderResponse,
                        "the AI service returned a non-JSON submission response",
                        error,
                    )
                    .with_operation(&operation)
                })?;
                let job_id =
                    config::resolve_json_path(&document, &job_config.job_id_path)
                        .and_then(|value| {
                            value
                                .as_str()
                                .map(str::to_string)
                                .or_else(|| value.as_str().is_none().then(|| value.to_string()))
                        })
                        .map(|value| value.trim_matches('"').to_string())
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            self.error(
                                ProviderErrorKind::MalformedProviderResponse,
                                "the AI service response did not contain a job id",
                                redact_secret(&document.to_string()),
                            )
                            .with_operation(&operation)
                        })?;
                self.job_operations
                    .lock()
                    .unwrap()
                    .insert(job_id.clone(), operation.clone());
                Ok(ProviderSubmission {
                    job: ProviderJobRef {
                        provider_id: self.provider_id.clone(),
                        provider_job_id: job_id,
                        run_id: request.run_id.clone(),
                        step_id: request.step_id.clone(),
                        submission_id: request.idempotency_key.clone(),
                        submitted_at: Utc::now().to_rfc3339(),
                        // Durable operation identity: a rehydrated adapter
                        // (background runner after a restart) polls without
                        // its in-memory job→operation map.
                        operation: Some(operation.clone()),
                    },
                    lifecycle: ProviderLifecycle::Submitted,
                })
            }
            None if endpoint.response.binary_response => {
                let output = endpoint.response.extract_binary_output(
                    &response.body,
                    response.content_type.as_deref(),
                    &operation,
                );
                let job_id = self.sync_job_id(request);
                self.results.lock().unwrap().insert(
                    job_id.clone(),
                    ProviderResult {
                        outputs: vec![ProviderOutput {
                            uri: output.uri,
                            mime_type: output.mime_type,
                            filename: Some(output.filename),
                        }],
                        provider_reported_model: Some(request.selected_model.clone()),
                        metadata: serde_json::json!({"response": "normalized"}),
                    },
                );
                Ok(ProviderSubmission {
                    job: ProviderJobRef {
                        provider_id: self.provider_id.clone(),
                        provider_job_id: job_id,
                        run_id: request.run_id.clone(),
                        step_id: request.step_id.clone(),
                        submission_id: request.idempotency_key.clone(),
                        submitted_at: Utc::now().to_rfc3339(),
                        operation: None,
                    },
                    lifecycle: ProviderLifecycle::Submitted,
                })
            }
            None => {
                let document = response.json().map_err(|error| {
                    self.error(
                        ProviderErrorKind::MalformedProviderResponse,
                        "the AI service returned a non-JSON generation response",
                        error,
                    )
                    .with_operation(&operation)
                })?;
                let (outputs, _request_id) =
                    self.extract_outputs(&operation, &endpoint.response, &document)?;
                if outputs.is_empty() {
                    return Err(self.error(
                        ProviderErrorKind::MalformedProviderResponse,
                        "the AI service returned no outputs",
                        redact_secret(&document.to_string()),
                    )
                    .with_operation(&operation));
                }
                let job_id = self.sync_job_id(request);
                self.results.lock().unwrap().insert(
                    job_id.clone(),
                    ProviderResult {
                        outputs,
                        provider_reported_model: Some(request.selected_model.clone()),
                        metadata: serde_json::json!({"response": "normalized"}),
                    },
                );
                Ok(ProviderSubmission {
                    job: ProviderJobRef {
                        provider_id: self.provider_id.clone(),
                        provider_job_id: job_id,
                        run_id: request.run_id.clone(),
                        step_id: request.step_id.clone(),
                        submission_id: request.idempotency_key.clone(),
                        submitted_at: Utc::now().to_rfc3339(),
                        operation: None,
                    },
                    lifecycle: ProviderLifecycle::Submitted,
                })
            }
        }
    }

    fn poll(&self, job: &ProviderJobRef) -> Result<ProviderJobStatus, ProviderError> {
        // The operation comes from the durable job ref first (a rehydrated
        // adapter has no in-memory job→operation map), then from the map
        // populated by this instance's own submissions.
        let operation = job
            .operation
            .clone()
            .or_else(|| {
                self.job_operations
                    .lock()
                    .unwrap()
                    .get(&job.provider_job_id)
                    .cloned()
            });
        if let Some(operation) = &operation {
            let endpoint = self.config.operations.get(operation).ok_or_else(|| {
                self.error(
                    ProviderErrorKind::UnknownProviderError,
                    "the job's operation configuration is missing",
                    operation,
                )
            })?;
            let job_config = endpoint.job.clone().ok_or_else(|| {
                self.error(
                    ProviderErrorKind::UnknownProviderError,
                    "the job's operation is not asynchronous",
                    operation,
                )
            })?;
            return self.poll_async(operation, endpoint, &job_config, job);
        }
        let succeeded = self
            .results
            .lock()
            .unwrap()
            .contains_key(&job.provider_job_id);
        Ok(ProviderJobStatus {
            lifecycle: if succeeded {
                ProviderLifecycle::Succeeded
            } else {
                ProviderLifecycle::Unknown
            },
            progress_percent: succeeded.then_some(100),
            diagnostic: None,
        })
    }

    fn cancel(&self, job: &ProviderJobRef) -> Result<ProviderCancellationResult, ProviderError> {
        Ok(ProviderCancellationResult {
            provider_job_id: job.provider_job_id.clone(),
            lifecycle: ProviderLifecycle::Cancelled,
        })
    }

    fn fetch_result(&self, job: &ProviderJobRef) -> Result<ProviderResult, ProviderError> {
        // Same durable-first operation resolution as `poll`.
        let operation = job
            .operation
            .clone()
            .or_else(|| {
                self.job_operations
                    .lock()
                    .unwrap()
                    .get(&job.provider_job_id)
                    .cloned()
            });
        if let Some(operation) = &operation {
            let endpoint = self.config.operations.get(operation).ok_or_else(|| {
                self.error(
                    ProviderErrorKind::UnknownProviderError,
                    "the job's operation configuration is missing",
                    operation,
                )
            })?;
            let job_config = endpoint.job.clone().ok_or_else(|| {
                self.error(
                    ProviderErrorKind::UnknownProviderError,
                    "the job's operation is not asynchronous",
                    operation,
                )
            })?;
            return self.fetch_async_result(operation, endpoint, &job_config, job);
        }
        self.results
            .lock()
            .unwrap()
            .get(&job.provider_job_id)
            .cloned()
            .ok_or_else(|| {
                self.error(
                    ProviderErrorKind::RemoteJobNotFound,
                    "the generation result was not found",
                    &job.provider_job_id,
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::http::{HttpResponse, TransportFailure};
    use crate::workflow::execution::{ExecutionMediaType, ExecutionTask};
    use std::sync::Mutex;

    /// Deterministic in-memory transport: maps URL fragments to scripted
    /// responses and records every request for assertions.
    #[derive(Clone, Default)]
    pub(crate) struct FakeTransport {
        pub(crate) requests: Arc<Mutex<Vec<RecordedRequest>>>,
        pub(crate) responses: Arc<Mutex<Vec<FakeResponse>>>,
        pub(crate) failures: Arc<Mutex<Vec<String>>>,
    }

    #[derive(Clone, Debug)]
    pub(crate) struct RecordedRequest {
        pub(crate) method: String,
        pub(crate) url: String,
        pub(crate) headers: Vec<(String, String)>,
        pub(crate) body: HttpBody,
    }

    impl RecordedRequest {
        pub(crate) fn json_body(&self) -> Option<serde_json::Value> {
            match &self.body {
                HttpBody::Json(value) => Some(value.clone()),
                _ => None,
            }
        }

        pub(crate) fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        }
    }

    #[derive(Clone, Debug)]
    pub(crate) struct FakeResponse {
        pub(crate) url_contains: String,
        pub(crate) status: u16,
        pub(crate) body: String,
    }

    impl FakeTransport {
        pub(crate) fn with_responses(responses: Vec<FakeResponse>) -> Self {
            Self {
                requests: Arc::new(Mutex::new(Vec::new())),
                responses: Arc::new(Mutex::new(responses)),
                failures: Arc::new(Mutex::new(Vec::new())),
            }
        }

        pub(crate) fn last_request(&self) -> RecordedRequest {
            self.requests.lock().unwrap().last().unwrap().clone()
        }
    }

    impl HttpExecutor for FakeTransport {
        fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportFailure> {
            self.requests.lock().unwrap().push(RecordedRequest {
                method: request.method.clone(),
                url: request.url.clone(),
                headers: request.headers.clone(),
                body: request.body.clone(),
            });
            if let Some(message) = self.failures.lock().unwrap().pop() {
                return Err(TransportFailure { message });
            }
            let mut responses = self.responses.lock().unwrap();
            let index = responses
                .iter()
                .position(|response| request.url.contains(&response.url_contains));
            let response = match index {
                Some(index) => responses.remove(index),
                None => FakeResponse {
                    url_contains: String::new(),
                    status: 500,
                    body: "{\"error\":\"no scripted response\"}".into(),
                },
            };
            Ok(HttpResponse {
                status: response.status,
                body: response.body.into_bytes(),
                content_type: Some("application/json".into()),
                headers: vec![("x-request-id".into(), "req-77".into())],
            })
        }
    }

    pub(crate) fn provider_request(
        media_type: ExecutionMediaType,
        model: &str,
    ) -> ProviderExecutionRequest {
        ProviderExecutionRequest {
            run_id: "run-1".into(),
            step_id: "execute".into(),
            compiled_request_id: "compiled-1".into(),
            media_type,
            task: ExecutionTask::CharacterFaceLock,
            prompt: "neutral face plate".into(),
            references: vec![],
            constraints: vec![],
            expected_output: serde_json::from_value(serde_json::json!({
                "assetType": "face_lock", "mediaType": "image",
                "desiredStatus": "candidate", "ownerEntityInputRef": "characterEntityId"
            }))
            .unwrap(),
            generation_parameters: Default::default(),
            selected_provider: "test".into(),
            selected_model: model.into(),
            idempotency_key: "run-1:execute:1".into(),
            reference_attachments: Vec::new(),
        }
    }

    pub(crate) fn openai_style_config() -> ProviderRuntimeConfig {
        let mut operations = BTreeMap::new();
        operations.insert(
            OPERATION_IMAGE_GENERATE.to_string(),
            EndpointConfig {
                method: "POST".into(),
                path_template: "/images/generations".into(),
                request_type: RequestType::Json,
                request_mapping: Some(serde_json::json!({
                    "model": "{{model}}",
                    "prompt": "{{prompt}}",
                    "size": "1024x1024"
                })),
                response: ResponseMapping {
                    outputs_path: Some("data".into()),
                    url_path: Some("url".into()),
                    base64_path: Some("b64_json".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        operations.insert(
            OPERATION_IMAGE_EDIT.to_string(),
            EndpointConfig {
                method: "POST".into(),
                path_template: "/images/edits".into(),
                request_type: RequestType::Multipart,
                multipart_fields: vec![
                    config::MultipartFieldConfig {
                        name: "prompt".into(),
                        kind: config::MultipartFieldKind::Text,
                        value: Some("{{prompt}}".into()),
                        source: None,
                    },
                    config::MultipartFieldConfig {
                        name: "image[]".into(),
                        kind: config::MultipartFieldKind::File,
                        value: None,
                        source: Some("images".into()),
                    },
                ],
                response: ResponseMapping {
                    outputs_path: Some("data".into()),
                    url_path: Some("url".into()),
                    base64_path: Some("b64_json".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        operations.insert(
            OPERATION_VALIDATE.to_string(),
            EndpointConfig {
                method: "GET".into(),
                path_template: "/models".into(),
                response: ResponseMapping {
                    binary_response: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        ProviderRuntimeConfig {
            auth: AuthConfig {
                mode: AuthMode::Bearer,
                credential_name: None,
            },
            operations,
            ..Default::default()
        }
    }

    pub(crate) fn provider_with(
        config: ProviderRuntimeConfig,
        transport: FakeTransport,
    ) -> DeclarativeProvider {
        DeclarativeProvider::new(
            "test-provider",
            "https://api.example.test/v1",
            vec![CustomProviderModel {
                id: "test-model".into(),
                name: "Test Model".into(),
                capabilities: Vec::new(),
            }],
            config,
            Some("sk-test-secret".into()),
            BTreeMap::new(),
            Arc::new(transport),
        )
    }

    // -------------------------------------------------------------------
    // Spec test matrix: the abstraction, not one provider.
    // -------------------------------------------------------------------

    use crate::providers::cancellation;
    use crate::providers::model::ProviderReferenceAttachment;
    use std::time::Duration;

    fn cloudflare_config() -> ProviderRuntimeConfig {
        let mut operations = BTreeMap::new();
        operations.insert(
            OPERATION_IMAGE_GENERATE.to_string(),
            EndpointConfig {
                method: "POST".into(),
                path_template: "/{model}".into(),
                request_type: RequestType::Json,
                request_mapping: Some(serde_json::json!({
                    "prompt": "{{prompt}}",
                    "steps": "{{steps}}"
                })),
                response: ResponseMapping {
                    base64_path: Some("result.image".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        operations.insert(
            OPERATION_VALIDATE.to_string(),
            EndpointConfig {
                method: "POST".into(),
                path_template: "/{model}".into(),
                request_type: RequestType::Json,
                request_mapping: Some(serde_json::json!({
                    "prompt": "simple provider validation test",
                    "steps": 1
                })),
                response: ResponseMapping {
                    binary_response: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        ProviderRuntimeConfig {
            auth: AuthConfig {
                mode: AuthMode::Bearer,
                credential_name: None,
            },
            account_id: Some("acc-9".into()),
            operations,
            ..Default::default()
        }
    }

    fn cloudflare_provider(transport: FakeTransport) -> DeclarativeProvider {
        DeclarativeProvider::new(
            "cloudflare",
            "https://api.cloudflare.com/client/v4/accounts/{accountId}/ai/run",
            vec![CustomProviderModel {
                id: "@cf/black-forest-labs/flux-1-schnell".into(),
                name: "FLUX.1 Schnell".into(),
                capabilities: Vec::new(),
            }],
            cloudflare_config(),
            Some("cf-token".into()),
            BTreeMap::new(),
            Arc::new(transport),
        )
    }

    fn png_attachment() -> ProviderReferenceAttachment {
        ProviderReferenceAttachment {
            asset_version_id: "v-1".into(),
            file_name: "face.png".into(),
            media_type: "image/png".into(),
            bytes: vec![137, 80, 78, 71],
            sha256: "a".repeat(64),
        }
    }

    #[test]
    fn openai_compatible_sync_image_generation_with_url_response() {
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/images/generations".into(),
            status: 200,
            body: r#"{"data":[{"url":"https://files.example/out.png"}],"model":"test-model"}"#
                .into(),
        }]);
        let provider = provider_with(openai_style_config(), transport.clone());
        let submission = provider.submit(&provider_request(ExecutionMediaType::Image, "test-model")).unwrap();
        assert_eq!(submission.lifecycle, ProviderLifecycle::Submitted);
        let result = provider.fetch_result(&submission.job).unwrap();
        assert_eq!(result.outputs[0].uri, "https://files.example/out.png");
        let request = transport.last_request();
        assert_eq!(request.url, "https://api.example.test/v1/images/generations");
        assert_eq!(request.header("Authorization"), Some("Bearer sk-test-secret"));
        assert_eq!(request.json_body().unwrap()["model"], "test-model");
        assert_eq!(request.json_body().unwrap()["prompt"], "neutral face plate");
    }

    #[test]
    fn cloudflare_url_auth_body_and_base64_mapping() {
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/ai/run".into(),
            status: 200,
            body: r#"{"result":{"image":"QUJD"}}"#.into(),
        }]);
        let provider = cloudflare_provider(transport.clone());
        let submission = provider
            .submit(&provider_request(
                ExecutionMediaType::Image,
                "@cf/black-forest-labs/flux-1-schnell",
            ))
            .unwrap();
        let result = provider.fetch_result(&submission.job).unwrap();
        // Base64 output normalizes into a data URI; callers never see the
        // provider's encoding.
        assert_eq!(result.outputs[0].uri, "data:image/png;base64,QUJD");
        assert_eq!(result.outputs[0].mime_type, "image/png");
        let request = transport.last_request();
        assert_eq!(
            request.url,
            "https://api.cloudflare.com/client/v4/accounts/acc-9/ai/run/@cf/black-forest-labs/flux-1-schnell"
        );
        assert_eq!(request.header("Authorization"), Some("Bearer cf-token"));
        let body = request.json_body().unwrap();
        assert_eq!(body["prompt"], "neutral face plate");
        // Unset canonical fields are omitted (steps stays unset at runtime).
        assert!(body.get("steps").is_none());
    }

    #[test]
    fn api_key_header_authentication() {
        let mut operations = BTreeMap::new();
        operations.insert(
            OPERATION_IMAGE_GENERATE.to_string(),
            EndpointConfig {
                path_template: "/generate".into(),
                request_mapping: Some(serde_json::json!({"prompt": "{{prompt}}"})),
                response: ResponseMapping {
                    url_path: Some("url".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let config = ProviderRuntimeConfig {
            auth: AuthConfig {
                mode: AuthMode::Header,
                credential_name: Some("x-api-key".into()),
            },
            operations,
            ..Default::default()
        };
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/generate".into(),
            status: 200,
            body: r#"{"url":"https://x/out.png"}"#.into(),
        }]);
        let provider = provider_with(config, transport.clone());
        provider
            .submit(&provider_request(ExecutionMediaType::Image, "test-model"))
            .unwrap();
        assert_eq!(
            transport.last_request().header("x-api-key"),
            Some("sk-test-secret")
        );
    }

    #[test]
    fn query_param_authentication() {
        let mut operations = BTreeMap::new();
        operations.insert(
            OPERATION_IMAGE_GENERATE.to_string(),
            EndpointConfig {
                path_template: "/generate".into(),
                request_mapping: Some(serde_json::json!({"prompt": "{{prompt}}"})),
                response: ResponseMapping {
                    url_path: Some("url".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let config = ProviderRuntimeConfig {
            auth: AuthConfig {
                mode: AuthMode::Query,
                credential_name: Some("key".into()),
            },
            operations,
            ..Default::default()
        };
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/generate".into(),
            status: 200,
            body: r#"{"url":"https://x/out.png"}"#.into(),
        }]);
        let provider = provider_with(config, transport.clone());
        provider
            .submit(&provider_request(ExecutionMediaType::Image, "test-model"))
            .unwrap();
        let request = transport.last_request();
        assert!(request.url.contains("key=sk-test-secret"), "{}", request.url);
        assert!(request.header("Authorization").is_none());
    }

    #[test]
    fn missing_credentials_fail_before_any_request() {
        let transport = FakeTransport::with_responses(vec![]);
        let provider = DeclarativeProvider::new(
            "test",
            "https://api.example.test",
            vec![CustomProviderModel {
                id: "m".into(),
                name: "M".into(),
                capabilities: Vec::new(),
            }],
            openai_style_config(),
            None,
            BTreeMap::new(),
            Arc::new(transport.clone()),
        );
        let error = provider
            .submit(&provider_request(ExecutionMediaType::Image, "m"))
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::AuthenticationError);
        assert!(transport.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn custom_headers_and_static_operation_headers_are_sent() {
        let mut operations = BTreeMap::new();
        let mut op_headers = BTreeMap::new();
        op_headers.insert("X-Op-Level".to_string(), "op-value".to_string());
        operations.insert(
            OPERATION_IMAGE_GENERATE.to_string(),
            EndpointConfig {
                path_template: "/generate".into(),
                request_mapping: Some(serde_json::json!({"prompt": "{{prompt}}"})),
                headers: op_headers,
                response: ResponseMapping {
                    url_path: Some("url".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let mut config_headers = BTreeMap::new();
        config_headers.insert("X-Workspace".to_string(), "ws-secret".to_string());
        let config = ProviderRuntimeConfig {
            auth: AuthConfig::default(),
            headers: config_headers,
            operations,
            ..Default::default()
        };
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/generate".into(),
            status: 200,
            body: r#"{"url":"https://x/out.png"}"#.into(),
        }]);
        let mut header_values = BTreeMap::new();
        header_values.insert("X-Workspace".to_string(), "resolved-ws".to_string());
        let provider = DeclarativeProvider::new(
            "test",
            "https://api.example.test",
            vec![CustomProviderModel {
                id: "test-model".into(),
                name: "M".into(),
                capabilities: Vec::new(),
            }],
            config,
            None,
            header_values,
            Arc::new(transport.clone()),
        );
        provider
            .submit(&provider_request(ExecutionMediaType::Image, "test-model"))
            .unwrap();
        let request = transport.last_request();
        assert_eq!(request.header("X-Workspace"), Some("resolved-ws"));
        assert_eq!(request.header("X-Op-Level"), Some("op-value"));
    }

    #[test]
    fn nested_json_request_mapping_and_response_extraction() {
        let mut operations = BTreeMap::new();
        operations.insert(
            OPERATION_IMAGE_GENERATE.to_string(),
            EndpointConfig {
                path_template: "/v1/predict".into(),
                request_mapping: Some(serde_json::json!({
                    "input": {
                        "text": "{{prompt}}",
                        "image_url": "{{image}}"
                    },
                    "options": {"seed": "{{seed}}"}
                })),
                response: ResponseMapping {
                    url_path: Some("prediction.urls.0".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let config = ProviderRuntimeConfig {
            auth: AuthConfig::default(),
            operations,
            ..Default::default()
        };
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/v1/predict".into(),
            status: 200,
            body: r#"{"prediction":{"urls":["https://x/deep.png"]}}"#.into(),
        }]);
        let mut request = provider_request(ExecutionMediaType::Image, "test-model");
        // Attachments ride along even for reference-free generation (the
        // mapping may inline them as data URIs).
        request.reference_attachments = vec![png_attachment()];
        let provider = provider_with(config, transport.clone());
        let submission = provider.submit(&request).unwrap();
        let result = provider.fetch_result(&submission.job).unwrap();
        assert_eq!(result.outputs[0].uri, "https://x/deep.png");
        let body = transport.last_request().json_body().unwrap();
        assert_eq!(body["input"]["text"], "neutral face plate");
        assert!(body["input"]["image_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn multipart_image_edit_uploads_file_parts() {
        let mut operations = BTreeMap::new();
        operations.insert(
            OPERATION_IMAGE_GENERATE.to_string(),
            EndpointConfig {
                path_template: "/generate".into(),
                request_type: RequestType::Json,
                request_mapping: Some(serde_json::json!({"prompt": "{{prompt}}"})),
                response: ResponseMapping {
                    url_path: Some("url".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        operations.insert(
            OPERATION_IMAGE_EDIT.to_string(),
            EndpointConfig {
                path_template: "/edit".into(),
                request_type: RequestType::Multipart,
                multipart_fields: vec![
                    config::MultipartFieldConfig {
                        name: "prompt".into(),
                        kind: config::MultipartFieldKind::Text,
                        value: Some("{{prompt}}".into()),
                        source: None,
                    },
                    config::MultipartFieldConfig {
                        name: "image[]".into(),
                        kind: config::MultipartFieldKind::File,
                        value: None,
                        source: Some("images".into()),
                    },
                ],
                response: ResponseMapping {
                    url_path: Some("url".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let config = ProviderRuntimeConfig {
            auth: AuthConfig::default(),
            operations,
            ..Default::default()
        };
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/edit".into(),
            status: 200,
            body: r#"{"url":"https://x/edited.png"}"#.into(),
        }]);
        let mut request = provider_request(ExecutionMediaType::Image, "test-model");
        request.task = crate::workflow::execution::ExecutionTask::VisualRepair;
        request.reference_attachments = vec![png_attachment()];
        request.references = vec![crate::workflow::execution::ExecutionReference {
            reference_type: crate::workflow::execution::ExecutionReferenceType::AssetVersion,
            reference: "v-1".into(),
            description: "face".into(),
            role: None,
        }];
        let provider = provider_with(config, transport.clone());
        let submission = provider.submit(&request).unwrap();
        let result = provider.fetch_result(&submission.job).unwrap();
        assert_eq!(result.outputs[0].uri, "https://x/edited.png");
        let request_record = transport.last_request();
        assert_eq!(request_record.url, "https://api.example.test/v1/edit");
        match &request_record.body {
            HttpBody::Multipart(parts) => {
                let files: Vec<&crate::providers::http::MultipartPart> = parts
                    .iter()
                    .filter(|part| part.file_name.is_some())
                    .collect();
                assert_eq!(files.len(), 1);
                assert_eq!(files[0].file_name.as_deref(), Some("face.png"));
                let prompt = parts
                    .iter()
                    .find(|part| part.field_name == "prompt")
                    .unwrap();
                assert_eq!(prompt.bytes, b"neutral face plate".to_vec());
            }
            other => panic!("expected multipart body, got {other:?}"),
        }
    }

    fn async_video_config(interval_ms: u64, timeout_ms: u64) -> ProviderRuntimeConfig {
        let mut operations = BTreeMap::new();
        operations.insert(
            OPERATION_VIDEO_GENERATE.to_string(),
            EndpointConfig {
                path_template: "/submit".into(),
                request_mapping: Some(serde_json::json!({"prompt": "{{prompt}}"})),
                response: ResponseMapping::default(),
                job: Some(config::AsyncJobConfig {
                    job_id_path: "result.task_id".into(),
                    status: config::StatusEndpointConfig {
                        method: "GET".into(),
                        path_template: "/tasks/{jobId}".into(),
                        status_path: "result.status".into(),
                        completed_values: vec![
                            "completed".into(),
                            "success".into(),
                            "succeeded".into(),
                        ],
                        failed_values: vec!["failed".into(), "error".into(), "cancelled".into()],
                        progress_path: Some("result.percent".into()),
                        error_message_path: Some("result.message".into()),
                    },
                    output: config::FinalOutputConfig {
                        fetch_path_template: None,
                        fetch_method: "GET".into(),
                        response: ResponseMapping {
                            url_path: Some("result.video_url".into()),
                            ..Default::default()
                        },
                    },
                    polling: config::PollingConfig {
                        interval_ms,
                        timeout_ms,
                    },
                }),
                ..Default::default()
            },
        );
        ProviderRuntimeConfig {
            auth: AuthConfig::default(),
            operations,
            ..Default::default()
        }
    }

    fn async_video_provider(config: ProviderRuntimeConfig, transport: FakeTransport) -> DeclarativeProvider {
        DeclarativeProvider::new(
            "async-video",
            "https://video.example.test/api",
            vec![CustomProviderModel {
                id: "vid-1".into(),
                name: "Vid".into(),
                capabilities: Vec::new(),
            }],
            config,
            None,
            BTreeMap::new(),
            Arc::new(transport),
        )
    }

    fn video_request() -> crate::providers::model::ProviderExecutionRequest {
        let mut request = provider_request(ExecutionMediaType::Video, "vid-1");
        request.prompt = "a slow pan".into();
        request
    }

    #[test]
    fn async_job_submits_polls_and_completes_with_video_url() {
        let transport = FakeTransport::with_responses(vec![
            FakeResponse {
                url_contains: "/submit".into(),
                status: 200,
                body: r#"{"result":{"task_id":"task-7"}}"#.into(),
            },
            FakeResponse {
                url_contains: "/tasks/task-7".into(),
                status: 200,
                body: r#"{"result":{"status":"running","percent":40}}"#.into(),
            },
            FakeResponse {
                url_contains: "/tasks/task-7".into(),
                status: 200,
                body: r#"{"result":{"status":"completed","video_url":"https://cdn.example/v.mp4"}}"#.into(),
            },
            FakeResponse {
                url_contains: "/tasks/task-7".into(),
                status: 200,
                body: r#"{"result":{"status":"completed","video_url":"https://cdn.example/v.mp4"}}"#.into(),
            },
        ]);
        let provider = async_video_provider(async_video_config(1, 10_000), transport.clone());
        let submission = provider.submit(&video_request()).unwrap();
        assert_eq!(submission.job.provider_job_id, "task-7");

        let running = provider.poll(&submission.job).unwrap();
        assert_eq!(running.lifecycle, ProviderLifecycle::Running);
        assert_eq!(running.progress_percent, Some(40));

        let done = provider.poll(&submission.job).unwrap();
        assert_eq!(done.lifecycle, ProviderLifecycle::Succeeded);
        let result = provider.fetch_result(&submission.job).unwrap();
        assert_eq!(result.outputs[0].uri, "https://cdn.example/v.mp4");
        assert_eq!(result.outputs[0].mime_type, "video/mp4");
        // One submit + two status polls + one final result fetch.
        let requests = transport.requests.lock().unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[1].method, "GET");
    }

    #[test]
    fn async_job_failure_preserves_the_provider_message() {
        let transport = FakeTransport::with_responses(vec![
            FakeResponse {
                url_contains: "/submit".into(),
                status: 200,
                body: r#"{"result":{"task_id":"task-8"}}"#.into(),
            },
            FakeResponse {
                url_contains: "/tasks/task-8".into(),
                status: 200,
                body: r#"{"result":{"status":"failed","message":"content policy violation"}}"#.into(),
            },
        ]);
        let provider = async_video_provider(async_video_config(1, 10_000), transport);
        let submission = provider.submit(&video_request()).unwrap();
        let status = provider.poll(&submission.job).unwrap();
        assert_eq!(status.lifecycle, ProviderLifecycle::Failed);
        assert_eq!(status.diagnostic.as_deref(), Some("content policy violation"));
    }

    /// P10.1 regression: the background runner rehydrates a *fresh*
    /// adapter instance after a restart. The submitted job carries its
    /// operation in the durable `ProviderJobRef` (persisted in the
    /// `provider_jobs` row), so the rehydrated instance must be able to
    /// poll and fetch the job without the in-memory job→operation map.
    #[test]
    fn rehydrated_adapter_polls_an_async_job_from_the_durable_ref() {
        let transport = FakeTransport::with_responses(vec![
            FakeResponse {
                url_contains: "/submit".into(),
                status: 200,
                body: r#"{"result":{"task_id":"task-rehydrated"}}"#.into(),
            },
            FakeResponse {
                url_contains: "/tasks/task-rehydrated".into(),
                status: 200,
                body: r#"{"result":{"status":"running","percent":25}}"#.into(),
            },
            FakeResponse {
                url_contains: "/tasks/task-rehydrated".into(),
                status: 200,
                body: r#"{"result":{"status":"completed","video_url":"https://cdn.example/r.mp4"}}"#.into(),
            },
            FakeResponse {
                url_contains: "/tasks/task-rehydrated".into(),
                status: 200,
                body: r#"{"result":{"status":"completed","video_url":"https://cdn.example/r.mp4"}}"#.into(),
            },
        ]);
        let submitting = async_video_provider(async_video_config(1, 10_000), transport.clone());
        let submission = submitting.submit(&video_request()).unwrap();
        assert_eq!(
            submission.job.operation.as_deref(),
            Some(OPERATION_VIDEO_GENERATE),
            "the durable job ref must carry the async operation"
        );

        // A brand-new adapter instance (same declarative config, empty
        // in-memory maps) — exactly what the runner rehydrates.
        let rehydrated = async_video_provider(async_video_config(1, 10_000), transport.clone());
        assert!(
            rehydrated.job_operations.lock().unwrap().is_empty(),
            "the rehydrated instance must start with no in-memory job map"
        );
        let running = rehydrated.poll(&submission.job).unwrap();
        assert_eq!(running.lifecycle, ProviderLifecycle::Running);
        assert_eq!(running.progress_percent, Some(25));
        let done = rehydrated.poll(&submission.job).unwrap();
        assert_eq!(done.lifecycle, ProviderLifecycle::Succeeded);
        let result = rehydrated.fetch_result(&submission.job).unwrap();
        assert_eq!(result.outputs[0].uri, "https://cdn.example/r.mp4");
    }

    /// P10.1 regression: a rehydrated adapter must not misinterpret a
    /// *synchronous* job (operation `None`) as an async poll — it falls
    /// back to the in-memory result map exactly as before.
    #[test]
    fn rehydrated_adapter_keeps_sync_jobs_on_the_result_path() {
        let mut operations = BTreeMap::new();
        operations.insert(
            OPERATION_IMAGE_GENERATE.to_string(),
            EndpointConfig {
                path_template: "/generate".into(),
                request_mapping: Some(serde_json::json!({"prompt": "{{prompt}}"})),
                response: ResponseMapping {
                    url_path: Some("data_url".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let config = ProviderRuntimeConfig {
            auth: AuthConfig::default(),
            operations,
            ..Default::default()
        };
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/generate".into(),
            status: 200,
            body: r#"{"data_url":"data:image/png;base64,aGVsbG8="}"#.into(),
        }]);
        let provider = async_video_provider(config, transport);
        let submission = provider.submit(&provider_request(ExecutionMediaType::Image, "vid-1")).unwrap();
        assert_eq!(submission.job.operation, None);
        // First poll sees the in-memory result → Succeeded (sync semantics).
        let status = provider.poll(&submission.job).unwrap();
        assert_eq!(status.lifecycle, ProviderLifecycle::Succeeded);
    }

    #[test]
    fn async_job_unrecognized_status_keeps_polling_not_terminal() {
        let transport = FakeTransport::with_responses(vec![
            FakeResponse {
                url_contains: "/submit".into(),
                status: 200,
                body: r#"{"result":{"task_id":"task-9"}}"#.into(),
            },
            FakeResponse {
                url_contains: "/tasks/task-9".into(),
                status: 200,
                body: r#"{"result":{"status":"queued_somewhere_else"}}"#.into(),
            },
        ]);
        let provider = async_video_provider(async_video_config(1, 10_000), transport);
        let submission = provider.submit(&video_request()).unwrap();
        let status = provider.poll(&submission.job).unwrap();
        assert_eq!(status.lifecycle, ProviderLifecycle::Running);
    }

    #[test]
    fn async_job_timeout_is_reported() {
        let transport = FakeTransport::with_responses(vec![
            FakeResponse {
                url_contains: "/submit".into(),
                status: 200,
                body: r#"{"result":{"task_id":"task-timeout"}}"#.into(),
            },
            FakeResponse {
                url_contains: "/tasks/task-timeout".into(),
                status: 200,
                body: r#"{"result":{"status":"running"}}"#.into(),
            },
        ]);
        let provider = async_video_provider(async_video_config(1, 40), transport);
        let submission = provider.submit(&video_request()).unwrap();
        let handle = crate::providers::service::ProviderSubmissionHandle {
            provider_id: "async-video".into(),
            adapter_version: provider.adapter_version(),
            provider: Arc::new(provider),
            submission: submission.clone(),
        };
        let error = crate::providers::service::ProviderService::finish_submission(&handle)
            .unwrap_err();
        assert!(error.to_string().contains("did not finish within"));
    }

    #[test]
    fn async_job_cancellation_aborts_the_wait() {
        let transport = FakeTransport::with_responses(vec![
            FakeResponse {
                url_contains: "/submit".into(),
                status: 200,
                body: r#"{"result":{"task_id":"task-cancel"}}"#.into(),
            },
            FakeResponse {
                url_contains: "/tasks/task-cancel".into(),
                status: 200,
                body: r#"{"result":{"status":"running"}}"#.into(),
            },
        ]);
        let provider = async_video_provider(async_video_config(1, 30_000), transport);
        let submission = provider.submit(&video_request()).unwrap();
        cancellation::register("async-video", &submission.job.provider_job_id);
        assert!(cancellation::signal("async-video", &submission.job.provider_job_id));
        let handle = crate::providers::service::ProviderSubmissionHandle {
            provider_id: "async-video".into(),
            adapter_version: provider.adapter_version(),
            provider: Arc::new(provider),
            submission: submission.clone(),
        };
        let error = crate::providers::service::ProviderService::finish_submission(&handle)
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        cancellation::unregister("async-video", &submission.job.provider_job_id);
    }

    #[test]
    fn http_400_rejection_preserves_status_and_provider_message() {
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/ai/run".into(),
            status: 400,
            body: r#"{"errors":[{"message":"prompt is required","code":7002}]}"#.into(),
        }]);
        let provider = cloudflare_provider(transport);
        let error = provider
            .submit(&provider_request(
                ExecutionMediaType::Image,
                "@cf/black-forest-labs/flux-1-schnell",
            ))
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
        assert_eq!(error.status_code, Some(400));
        assert_eq!(error.provider_message.as_deref(), Some("prompt is required"));
        assert_eq!(error.operation.as_deref(), Some(OPERATION_IMAGE_GENERATE));
        assert!(!error.display_text().contains("cf-token"));
    }

    #[test]
    fn http_401_and_403_map_to_credential_errors() {
        for (status, expected) in [
            (401u16, ProviderErrorKind::AuthenticationError),
            (403, ProviderErrorKind::AuthorizationError),
        ] {
            let transport = FakeTransport::with_responses(vec![FakeResponse {
                url_contains: "/images/generations".into(),
                status,
                body: r#"{"error":{"message":"nope"}}"#.into(),
            }]);
            let provider = provider_with(openai_style_config(), transport);
            let error = provider
                .submit(&provider_request(ExecutionMediaType::Image, "test-model"))
                .unwrap_err();
            assert_eq!(error.kind, expected);
            assert_eq!(error.status_code, Some(status));
        }
    }

    #[test]
    fn malformed_responses_fail_without_leaking_the_body_unredacted() {
        // Non-JSON body.
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/images/generations".into(),
            status: 200,
            body: "<html>gateway error</html>".into(),
        }]);
        let provider = provider_with(openai_style_config(), transport);
        let error = provider
            .submit(&provider_request(ExecutionMediaType::Image, "test-model"))
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::MalformedProviderResponse);

        // Empty data array.
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/images/generations".into(),
            status: 200,
            body: r#"{"data":[]}"#.into(),
        }]);
        let provider = provider_with(openai_style_config(), transport);
        let error = provider
            .submit(&provider_request(ExecutionMediaType::Image, "test-model"))
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::MalformedProviderResponse);

        // Path resolves to a non-string.
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/images/generations".into(),
            status: 200,
            body: r#"{"data":[{"url":{"nested":true}}]}"#.into(),
        }]);
        let provider = provider_with(openai_style_config(), transport);
        let error = provider
            .submit(&provider_request(ExecutionMediaType::Image, "test-model"))
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::MalformedProviderResponse);
    }

    #[test]
    fn model_capabilities_and_missing_operations_gate_submission() {
        // Model restricts itself to image.generate; an edit request fails.
        let transport = FakeTransport::with_responses(vec![]);
        let model = CustomProviderModel {
            id: "test-model".into(),
            name: "Test Model".into(),
            capabilities: vec![OPERATION_IMAGE_GENERATE.to_string()],
        };
        let provider = DeclarativeProvider::new(
            "test",
            "https://api.example.test/v1",
            vec![model],
            openai_style_config(),
            Some("sk-test-secret".into()),
            BTreeMap::new(),
            Arc::new(transport),
        );
        let mut request = provider_request(ExecutionMediaType::Image, "test-model");
        request.task = crate::workflow::execution::ExecutionTask::VisualRepair;
        request.references = vec![crate::workflow::execution::ExecutionReference {
            reference_type: crate::workflow::execution::ExecutionReferenceType::AssetVersion,
            reference: "v-1".into(),
            description: "face".into(),
            role: None,
        }];
        request.reference_attachments = vec![png_attachment()];
        // The provider defines image.edit, so the operation resolves, but the
        // model's capability list rejects it.
        let error = provider.submit(&request).unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::UnsupportedCapability);
        assert!(error.message.contains("does not support image.edit"));

        // A provider with no video operations rejects video requests at the
        // capability precheck before any request is compiled.
        let transport = FakeTransport::with_responses(vec![]);
        let provider = provider_with(openai_style_config(), transport);
        let error = provider
            .submit(&provider_request(ExecutionMediaType::Video, "test-model"))
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::UnsupportedCapability);
        assert!(error.message.contains("not supported"));

        // And one that declares only video operations rejects image edit.
        let provider = async_video_provider(async_video_config(1, 10_000), FakeTransport::with_responses(vec![]));
        let error = provider
            .submit(&provider_request(ExecutionMediaType::Image, "vid-1"))
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::UnsupportedCapability);
    }

    #[test]
    fn binary_responses_normalize_to_data_uris() {
        let mut operations = BTreeMap::new();
        operations.insert(
            OPERATION_IMAGE_GENERATE.to_string(),
            EndpointConfig {
                method: "GET".into(),
                path_template: "/prompt/{{prompt}}".into(),
                request_type: RequestType::Json,
                request_mapping: Some(serde_json::json!({})),
                response: ResponseMapping {
                    binary_response: true,
                    mime_type: "image/jpeg".into(),
                    filename: "generated.jpg".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let config = ProviderRuntimeConfig {
            auth: AuthConfig::default(),
            operations,
            ..Default::default()
        };
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/prompt/".into(),
            status: 200,
            body: "JPEGBYTES".into(),
        }]);
        let provider = provider_with(config, transport.clone());
        let submission = provider
            .submit(&provider_request(ExecutionMediaType::Image, "test-model"))
            .unwrap();
        let result = provider.fetch_result(&submission.job).unwrap();
        assert!(result.outputs[0].uri.starts_with("data:image/jpeg;base64,"));
        let request = transport.last_request();
        assert_eq!(request.method, "GET");
        assert!(request.url.contains("/prompt/neutral%20face%20plate"));
    }

    #[test]
    fn capability_derivation_follows_operations() {
        let capabilities = cloudflare_provider(FakeTransport::with_responses(vec![])).capabilities();
        assert!(capabilities.media_types.contains(&super::super::model::ProviderMediaType::Image));
        assert!(!capabilities.supports_image_edit);
        assert_eq!(
            capabilities.supported_models,
            vec!["@cf/black-forest-labs/flux-1-schnell".to_string()]
        );

        let capabilities = provider_with(openai_style_config(), FakeTransport::with_responses(vec![]))
            .capabilities();
        assert!(capabilities.supports_image_edit);
        assert!(capabilities.supports_reference_image);
        assert!(capabilities.supports_multiple_reference_images);
    }

    #[test]
    fn validation_operation_runs_without_generation() {
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/models".into(),
            status: 200,
            body: r#"{"data":[]}"#.into(),
        }]);
        let provider = provider_with(openai_style_config(), transport.clone());
        let outcome = provider.run_validation(Some("test-model")).unwrap();
        assert!(outcome.performed_network_check);
        assert_eq!(outcome.status_code, Some(200));

        // Cloudflare validates against the model endpoint with a tiny body.
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/ai/run".into(),
            status: 200,
            body: r#"{"result":{"image":"QUJD"}}"#.into(),
        }]);
        let provider = cloudflare_provider(transport.clone());
        let outcome = provider.run_validation(None).unwrap();
        assert!(outcome.performed_network_check);
        let request = transport.last_request();
        assert!(request.url.contains("/@cf/black-forest-labs/flux-1-schnell"));
        let body = request.json_body().unwrap();
        assert_eq!(body["prompt"], "simple provider validation test");
        assert_eq!(body["steps"], 1);
    }

    #[test]
    fn provider_error_display_is_human_readable_and_secret_free() {
        let transport = FakeTransport::with_responses(vec![FakeResponse {
            url_contains: "/ai/run".into(),
            status: 429,
            body: r#"{"errors":[{"message":"rate limit exceeded"}]}"#.into(),
        }]);
        let provider = cloudflare_provider(transport);
        let error = provider
            .submit(&provider_request(
                ExecutionMediaType::Image,
                "@cf/black-forest-labs/flux-1-schnell",
            ))
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
        let text = error.display_text();
        assert!(text.contains("image.generate"));
        assert!(text.contains("HTTP 429"));
        assert!(text.contains("rate limit exceeded"));
        assert!(!text.contains("cf-token"));
    }
}

