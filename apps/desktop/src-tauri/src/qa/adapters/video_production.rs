use super::{
    RawVideoQaResponse, VideoQaAdapter, VideoQaAdapterError, VideoQaAdapterErrorKind,
    VideoQaCapabilities,
};
use crate::providers::http::{HttpBody, HttpExecutor, HttpRequest, MultipartPart, UreqExecutor};
use crate::qa::models::VideoQaRequest;
use crate::video_qa::evidence::{
    prepare_packaged_evidence, EvidenceMode, EvidencePathError, PreparedEvidence,
    TemporalDecoderAvailability,
};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

/// Production Video QA adapter. Direct-video transfer is always prepared by
/// the packaged evidence boundary before bytes can leave the device.
pub struct OpenAiCompatibleVideoQaAdapter {
    endpoint: String,
    bearer_token: String,
    model_id: String,
    evidence_mode: EvidenceMode,
    decoder: TemporalDecoderAvailability,
    execution_location: String,
    transport: Box<dyn HttpExecutor>,
}

impl OpenAiCompatibleVideoQaAdapter {
    pub fn new(
        endpoint: impl Into<String>,
        bearer_token: impl Into<String>,
        model_id: impl Into<String>,
        evidence_mode: EvidenceMode,
        decoder: TemporalDecoderAvailability,
    ) -> Result<Self, VideoQaAdapterError> {
        Self::with_transport(
            endpoint,
            bearer_token,
            model_id,
            evidence_mode,
            decoder,
            UreqExecutor::new(Duration::from_secs(120)),
        )
    }

    pub fn with_transport<T: HttpExecutor + 'static>(
        endpoint: impl Into<String>,
        bearer_token: impl Into<String>,
        model_id: impl Into<String>,
        evidence_mode: EvidenceMode,
        decoder: TemporalDecoderAvailability,
        transport: T,
    ) -> Result<Self, VideoQaAdapterError> {
        let endpoint = endpoint.into().trim_end_matches('/').to_string();
        let model_id = model_id.into();
        if endpoint.trim().is_empty() {
            return Err(VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::InvalidRequest,
                "Video QA endpoint is required",
            ));
        }
        if model_id.trim().is_empty() {
            return Err(VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::InvalidRequest,
                "Video QA model is required",
            ));
        }
        Ok(Self {
            execution_location: endpoint_location(&endpoint),
            endpoint,
            bearer_token: bearer_token.into(),
            model_id,
            evidence_mode,
            decoder,
            transport: Box::new(transport),
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/video/qa", self.endpoint)
    }

    fn request_parts(
        &self,
        request: &VideoQaRequest,
    ) -> Result<Vec<MultipartPart>, VideoQaAdapterError> {
        if request.response_schema_version != 1 {
            return Err(VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::UnsupportedCapability,
                "Video QA supports response schema version 1 only",
            ));
        }
        if request.target.mime_type != "video/mp4" {
            return Err(VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::UnsupportedCapability,
                "Video QA supports video/mp4 target evidence only",
            ));
        }
        if request.references.len() + 1 > self.capabilities().max_media_inputs {
            return Err(VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::UnsupportedCapability,
                "Video QA request contains too many declared media inputs",
            ));
        }

        let prepared = prepare_packaged_evidence(
            Path::new(&request.target.local_path),
            self.evidence_mode,
            self.decoder,
        )
        .map_err(evidence_error)?;
        let PreparedEvidence::DirectVideo(binding) = prepared;
        if binding.source_content_sha256 != request.target.content_sha256
            || binding.size_bytes != request.target.size_bytes
            || binding.mime_type != request.target.mime_type
        {
            return Err(VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::InvalidRequest,
                "Declared video evidence does not match the packaged evidence binding",
            ));
        }

        // Re-read immediately before transfer and require the same binding. This
        // closes a time-of-check/time-of-transfer swap without invoking PATH
        // tools or attempting a host decoder fallback.
        let bytes = std::fs::read(&request.target.local_path).map_err(|_| {
            VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::InvalidRequest,
                "Declared video evidence could not be read",
            )
        })?;
        let transfer_sha256 = format!("{:x}", Sha256::digest(&bytes));
        if transfer_sha256 != binding.source_content_sha256
            || bytes.len() as u64 != binding.size_bytes
        {
            return Err(VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::InvalidRequest,
                "Video evidence changed after packaged evidence preparation",
            ));
        }

        let descriptor = serde_json::json!({
            "requestId": request.request_id,
            "schemaVersion": request.response_schema_version,
            "evidenceMode": evidence_mode_name(self.evidence_mode),
            "target": {
                "assetVersionId": request.target.asset_version_id,
                "mimeType": binding.mime_type,
                "contentSha256": binding.source_content_sha256,
                "sizeBytes": binding.size_bytes,
            },
            "references": request.references,
            "checks": request.checks,
        });
        let descriptor = serde_json::to_vec(&descriptor).map_err(|error| {
            VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::InvalidRequest,
                "Video QA request could not be serialized",
            )
            .with_diagnostic(error.to_string())
        })?;
        Ok(vec![
            MultipartPart {
                field_name: "request".into(),
                file_name: None,
                content_type: Some("application/json".into()),
                bytes: descriptor,
            },
            MultipartPart {
                field_name: "video".into(),
                file_name: Some(format!("{}.mp4", request.target.asset_version_id)),
                content_type: Some(binding.mime_type.into()),
                bytes,
            },
        ])
    }
}

impl VideoQaAdapter for OpenAiCompatibleVideoQaAdapter {
    fn id(&self) -> &'static str {
        "openai_compatible_video_qa"
    }

    fn adapter_version(&self) -> u32 {
        1
    }

    fn capabilities(&self) -> VideoQaCapabilities {
        VideoQaCapabilities {
            supports_direct_video: true,
            supports_sampled_frames: false,
            supports_multiple_references: true,
            max_media_inputs: 32,
        }
    }

    fn evidence_mode(&self) -> EvidenceMode {
        self.evidence_mode
    }

    fn execution_location(&self) -> String {
        self.execution_location.clone()
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn analyze(&self, request: &VideoQaRequest) -> Result<RawVideoQaResponse, VideoQaAdapterError> {
        if self.bearer_token.trim().is_empty() && self.execution_location != "local" {
            return Err(VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::Authentication,
                "Video QA credential is not configured",
            ));
        }
        let parts = self.request_parts(request)?;
        let response = self
            .transport
            .execute(HttpRequest {
                method: "POST".into(),
                url: self.endpoint(),
                headers: vec![(
                    "Authorization".into(),
                    format!("Bearer {}", self.bearer_token),
                )],
                body: HttpBody::Multipart(parts),
                max_response_bytes: 50 * 1024 * 1024,
            })
            .map_err(|failure| {
                VideoQaAdapterError::new(
                    VideoQaAdapterErrorKind::Network,
                    "Video QA request failed",
                )
                .with_diagnostic(redact(&failure.message, &self.bearer_token))
            })?;
        if !response.is_success() {
            return Err(VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::Network,
                "Video QA request was rejected",
            )
            .with_diagnostic(redact(&response.text(), &self.bearer_token)));
        }
        let document = response.json().map_err(|error| {
            VideoQaAdapterError::new(
                VideoQaAdapterErrorKind::MalformedResponse,
                "Video QA response was not valid JSON",
            )
            .with_diagnostic(redact(&error, &self.bearer_token))
        })?;
        let response_text = document
            .pointer("/choices/0/message/content")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                VideoQaAdapterError::new(
                    VideoQaAdapterErrorKind::MalformedResponse,
                    "Video QA response did not contain structured message content",
                )
            })?
            .to_string();
        Ok(RawVideoQaResponse {
            response_text,
            metadata: serde_json::json!({
                "model": document.get("model"),
                "usage": document.get("usage"),
                "evidenceMode": evidence_mode_name(self.evidence_mode),
            }),
        })
    }
}

fn evidence_error(error: EvidencePathError) -> VideoQaAdapterError {
    match error {
        EvidencePathError::EvidenceUnsupported { code, reason, .. } => {
            VideoQaAdapterError::new(VideoQaAdapterErrorKind::UnsupportedCapability, code)
                .with_diagnostic(reason)
        }
        EvidencePathError::Unreadable(_) => VideoQaAdapterError::new(
            VideoQaAdapterErrorKind::InvalidRequest,
            "Declared video evidence is unavailable",
        ),
        EvidencePathError::InvalidMp4 => VideoQaAdapterError::new(
            VideoQaAdapterErrorKind::InvalidRequest,
            "Declared video evidence is not a valid MP4",
        ),
    }
}

fn evidence_mode_name(mode: EvidenceMode) -> &'static str {
    match mode {
        EvidenceMode::DirectVideo => "direct_video",
        EvidenceMode::SampledFrames => "sampled_frames",
    }
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

fn redact(diagnostic: &str, secret: &str) -> String {
    if secret.is_empty() {
        diagnostic.to_string()
    } else {
        diagnostic.replace(secret, "[REDACTED]")
    }
}
