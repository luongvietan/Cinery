use crate::skills::model::ExpectedOutputDefinition;
use crate::workflow::execution::{
    ExecutionConstraint, ExecutionMediaType, ExecutionReference, ExecutionRequest, ExecutionTask,
};
use serde::{Deserialize, Serialize};

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
    use crate::workflow::execution::{
        ExecutionProvenance, ExecutionReferenceType, ExecutionTask,
    };

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
