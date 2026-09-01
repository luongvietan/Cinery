use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};

use crate::workflow::execution::ExecutionGenerationParameters;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = String;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    other => Err(format!("invalid {}: {other}", stringify!($name))),
                }
            }
        }
    };
}

string_enum!(QaRunStatus {
    Queued => "queued",
    Running => "running",
    Succeeded => "succeeded",
    Failed => "failed",
    Cancelled => "cancelled",
});

string_enum!(QaOverallStatus {
    Pass => "pass",
    Fail => "fail",
    NeedsReview => "needs_review",
});

string_enum!(QaMediaKind {
    Image => "image",
    Video => "video",
});

#[allow(clippy::derivable_impls)]
impl Default for QaMediaKind {
    fn default() -> Self {
        Self::Image
    }
}

string_enum!(QaCheckStatus {
    Pass => "pass",
    Fail => "fail",
    Uncertain => "uncertain",
    NotApplicable => "not_applicable",
});

string_enum!(QaCheckSource {
    VisualLock => "visual_lock",
    CanonicalReference => "canonical_reference",
    OperationExpectation => "operation_expectation",
    ArtifactDetection => "artifact_detection",
});

string_enum!(QaCheckType {
    IdentitySimilarity => "identity_similarity",
    PermanentVisualLock => "permanent_visual_lock",
    HairConsistency => "hair_consistency",
    SkinRegister => "skin_register",
    OutfitPiece => "outfit_piece",
    AccessoryPlacement => "accessory_placement",
    RequiredElement => "required_element",
    ForbiddenElement => "forbidden_element",
    BackgroundRequirement => "background_requirement",
    CompositionRequirement => "composition_requirement",
    Watermark => "watermark",
    UnexpectedArtifact => "unexpected_artifact",
    VideoIntegrity => "video_integrity",
    StartFrameContinuity => "start_frame_continuity",
    IdentityTemporalConsistency => "identity_temporal_consistency",
    ReferenceTemporalConsistency => "reference_temporal_consistency",
    MotionAdherence => "motion_adherence",
    CameraMotionAdherence => "camera_motion_adherence",
    TemporalCoherence => "temporal_coherence",
    UnexpectedCut => "unexpected_cut",
    Flicker => "flicker",
    DeformationOrWarping => "deformation_or_warping",
});

string_enum!(QaReviewStatus {
    Unreviewed => "unreviewed",
    Confirmed => "confirmed",
    OverriddenPass => "overridden_pass",
    OverriddenFail => "overridden_fail",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaCheckDefinition {
    pub id: String,
    pub check_type: QaCheckType,
    pub source: QaCheckSource,
    pub key: String,
    pub label: String,
    pub requirement: String,
    pub validator_hint: Option<String>,
    pub blocking: bool,
    pub reference_asset_version_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaCheckPlan {
    pub schema_version: u32,
    pub asset_id: String,
    pub asset_version_id: String,
    pub owner_entity_id: Option<String>,
    pub asset_type: String,
    pub reference_asset_version_ids: Vec<String>,
    pub checks: Vec<QaCheckDefinition>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct QaCheckResult {
    pub check_id: String,
    pub status: QaCheckStatus,
    pub confidence: Option<f64>,
    pub observed: String,
    pub reason: String,
    pub repair_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct VisualQaResult {
    pub schema_version: u32,
    pub checks: Vec<QaCheckResult>,
    pub model_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualQaMedia {
    pub asset_version_id: String,
    pub local_path: String,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualQaReference {
    pub asset_version_id: String,
    pub local_path: String,
    pub purpose: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualQaRequest {
    pub request_id: String,
    pub target: VisualQaMedia,
    pub references: Vec<VisualQaReference>,
    pub checks: Vec<QaCheckDefinition>,
    pub response_schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualExpectation {
    pub id: String,
    pub expectation_type: QaCheckType,
    pub requirement: String,
    pub blocking: bool,
    pub validator_hint: Option<String>,
}

/// Versioned input contract for Video QA. Credentials and mutable project
/// paths are intentionally excluded from persisted workflow input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunVideoQaInput {
    pub asset_version_id: String,
    pub adapter_id: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoQaContextRequest {
    pub project_id: String,
    pub asset_version_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoQaTargetContext {
    pub asset_id: String,
    pub asset_version_id: String,
    pub asset_type: String,
    pub file_path: String,
    pub mime_type: String,
    pub content_sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoQaReferenceContext {
    pub asset_id: String,
    pub asset_version_id: String,
    pub asset_type: String,
    pub file_path: String,
    pub mime_type: String,
    pub content_sha256: String,
    pub size_bytes: u64,
    pub purpose: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoGenerationOrigin {
    pub workflow_run_id: String,
    pub operation_id: String,
    pub provider_attempt_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub compiled_request_sha256: String,
    pub source_asset_version_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoGenerationIntent {
    pub prompt: String,
    pub generation_parameters: ExecutionGenerationParameters,
    pub expected_duration_seconds: Option<f32>,
    pub motion_requirement: Option<String>,
    pub camera_requirement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedVideoQaContext {
    pub schema_version: u32,
    pub target: VideoQaTargetContext,
    pub origin: VideoGenerationOrigin,
    pub source_keyframe: Option<VideoQaReferenceContext>,
    pub references: Vec<VideoQaReferenceContext>,
    pub generation_intent: VideoGenerationIntent,
    pub created_at: String,
}

impl RunVideoQaInput {
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("assetVersionId", Some(self.asset_version_id.as_str())),
            ("adapterId", Some(self.adapter_id.as_str())),
            ("providerId", self.provider_id.as_deref()),
            ("modelId", self.model_id.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(format!("{field} must be a non-empty string when provided"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaRunRecord {
    pub id: String,
    pub project_id: String,
    pub asset_id: String,
    pub asset_version_id: String,
    #[serde(default)]
    pub media_kind: QaMediaKind,
    pub workflow_run_id: Option<String>,
    pub status: QaRunStatus,
    pub overall_status: Option<QaOverallStatus>,
    pub adapter_id: Option<String>,
    pub adapter_version: Option<String>,
    pub model_id: Option<String>,
    pub execution_location: String,
    pub check_plan: Value,
    pub context_snapshot: Value,
    pub raw_response_metadata: Option<Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaCheckRecord {
    pub id: String,
    pub qa_run_id: String,
    pub check_id: String,
    pub check_type: QaCheckType,
    pub source: QaCheckSource,
    pub requirement: Value,
    pub status: QaCheckStatus,
    pub confidence: Option<f64>,
    pub observed: String,
    pub reason: String,
    pub repair_hint: Option<String>,
    pub review_status: QaReviewStatus,
    pub review_note: Option<String>,
    pub reviewed_at: Option<String>,
    pub created_at: String,
}

impl QaCheckRecord {
    pub fn effective_status(&self) -> QaCheckStatus {
        match self.review_status {
            QaReviewStatus::OverriddenPass => QaCheckStatus::Pass,
            QaReviewStatus::OverriddenFail => QaCheckStatus::Fail,
            QaReviewStatus::Unreviewed | QaReviewStatus::Confirmed => self.status,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaRunDetail {
    pub run: QaRunRecord,
    pub checks: Vec<QaCheckRecord>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_qa_run_json_defaults_media_kind_to_image() {
        let legacy = serde_json::json!({
            "id": "qa-1",
            "projectId": "project-1",
            "assetId": "asset-1",
            "assetVersionId": "version-1",
            "workflowRunId": null,
            "status": "succeeded",
            "overallStatus": "pass",
            "adapterId": "mock",
            "adapterVersion": "1",
            "modelId": "mock-vlm",
            "executionLocation": "local",
            "checkPlan": {},
            "contextSnapshot": {},
            "rawResponseMetadata": null,
            "errorCode": null,
            "errorMessage": null,
            "createdAt": "2026-09-01T00:00:00Z",
            "startedAt": null,
            "completedAt": null
        });

        let record: QaRunRecord = serde_json::from_value(legacy).unwrap();

        assert_eq!(record.media_kind, QaMediaKind::Image);
    }
}
