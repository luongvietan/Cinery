use super::errors::QaError;
use super::models::{QaCheckPlan, QaCheckResult, QaCheckStatus, QaOverallStatus, VisualQaResult};
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_RESULT_TEXT: usize = 4_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedQaResult {
    pub overall: QaOverallStatus,
    pub checks: Vec<QaCheckResult>,
    pub model_summary: Option<String>,
}

pub struct QaResponseNormalizer;

impl QaResponseNormalizer {
    pub fn normalize(
        plan: &QaCheckPlan,
        response_text: &str,
    ) -> Result<NormalizedQaResult, AppError> {
        if plan.schema_version != 1 {
            return Err(invalid("unsupported QA check-plan schema version"));
        }
        let response: VisualQaResult = serde_json::from_str(response_text)
            .map_err(|error| invalid(format!("malformed QA response: {error}")))?;
        if response.schema_version != 1 {
            return Err(invalid("unsupported QA response schema version"));
        }
        validate_text(
            response.model_summary.as_deref().unwrap_or_default(),
            "modelSummary",
        )?;

        let expected_ids = plan
            .checks
            .iter()
            .map(|check| check.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut results = BTreeMap::new();
        for result in response.checks {
            if !expected_ids.contains(result.check_id.as_str()) {
                return Err(invalid(format!("unknown QA check id: {}", result.check_id)));
            }
            if results.insert(result.check_id.clone(), result).is_some() {
                return Err(invalid("duplicate QA check id"));
            }
        }
        if results.len() != expected_ids.len() {
            let missing = expected_ids
                .iter()
                .filter(|id| !results.contains_key(**id))
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            return Err(invalid(format!("missing QA check results: {missing}")));
        }

        let mut normalized = Vec::with_capacity(plan.checks.len());
        for definition in &plan.checks {
            let result = results
                .remove(&definition.id)
                .ok_or_else(|| invalid("missing QA check result"))?;
            if let Some(confidence) = result.confidence {
                if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
                    return Err(invalid(format!(
                        "confidence for {} is outside [0,1]",
                        result.check_id
                    )));
                }
            }
            validate_text(&result.observed, "observed")?;
            validate_text(&result.reason, "reason")?;
            validate_text(
                result.repair_hint.as_deref().unwrap_or_default(),
                "repairHint",
            )?;
            normalized.push(result);
        }

        let overall = compute_overall(plan, &normalized)?;
        Ok(NormalizedQaResult {
            overall,
            checks: normalized,
            model_summary: response.model_summary,
        })
    }
}

pub fn compute_overall(
    plan: &QaCheckPlan,
    results: &[QaCheckResult],
) -> Result<QaOverallStatus, AppError> {
    if plan.checks.len() != results.len() {
        return Err(invalid("QA overall requires one result per planned check"));
    }
    let expected_ids = plan
        .checks
        .iter()
        .map(|definition| definition.id.as_str())
        .collect::<BTreeSet<_>>();
    if expected_ids.len() != plan.checks.len() {
        return Err(invalid("QA overall requires unique planned check ids"));
    }
    let mut by_id = BTreeMap::new();
    for result in results {
        if !expected_ids.contains(result.check_id.as_str()) {
            return Err(invalid(format!("unknown QA check id: {}", result.check_id)));
        }
        if by_id
            .insert(result.check_id.as_str(), result.status)
            .is_some()
        {
            return Err(invalid("duplicate QA check id"));
        }
    }
    if by_id.len() != expected_ids.len() {
        return Err(invalid("QA overall requires one result per planned check"));
    }

    if plan.checks.iter().any(|definition| {
        definition.blocking && by_id.get(definition.id.as_str()) == Some(&QaCheckStatus::Fail)
    }) {
        return Ok(QaOverallStatus::Fail);
    }

    let needs_review = plan.checks.iter().any(|definition| {
        matches!(
            by_id.get(definition.id.as_str()),
            Some(QaCheckStatus::Fail | QaCheckStatus::Uncertain)
        ) || (definition.blocking
            && by_id.get(definition.id.as_str()) == Some(&QaCheckStatus::NotApplicable))
    });
    Ok(if needs_review {
        QaOverallStatus::NeedsReview
    } else {
        QaOverallStatus::Pass
    })
}

fn validate_text(value: &str, field: &str) -> Result<(), AppError> {
    if value.chars().count() > MAX_RESULT_TEXT {
        return Err(invalid(format!(
            "QA field {field} exceeds {MAX_RESULT_TEXT} characters"
        )));
    }
    if value.contains('\0') {
        return Err(invalid(format!(
            "QA field {field} contains invalid control data"
        )));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> AppError {
    QaError::InvalidData(message.into()).into()
}
