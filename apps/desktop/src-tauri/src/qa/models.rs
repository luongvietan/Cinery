use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaRunRecord {
    pub id: String,
    pub project_id: String,
    pub asset_id: String,
    pub asset_version_id: String,
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
