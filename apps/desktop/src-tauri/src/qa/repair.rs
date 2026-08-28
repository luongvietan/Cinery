use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;

use super::models::{QaCheckDefinition, QaCheckStatus, QaCheckType, QaRunDetail, QaRunStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairPreservationInstruction {
    pub check_id: String,
    pub key: String,
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairChangeInstruction {
    pub check_id: String,
    pub instruction: String,
    pub validator_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairPlan {
    pub schema_version: u32,
    pub source_asset_id: String,
    pub source_asset_version_id: String,
    pub source_qa_run_id: String,
    pub failed_check_ids: Vec<String>,
    pub preserve: Vec<RepairPreservationInstruction>,
    pub changes: Vec<RepairChangeInstruction>,
    pub reference_asset_version_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderNeutralEditRequest {
    pub schema_version: u32,
    pub request_id: String,
    pub source_asset_version_id: String,
    pub reference_asset_version_ids: Vec<String>,
    pub prompt: String,
    pub forbidden_changes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledRepair {
    pub plan: RepairPlan,
    pub request: ProviderNeutralEditRequest,
    pub snapshot: Value,
}

pub struct RepairCompiler;

impl RepairCompiler {
    pub fn compile(detail: &QaRunDetail) -> Result<CompiledRepair, AppError> {
        if detail.run.status != QaRunStatus::Succeeded {
            return Err(invalid("repair requires a succeeded QA run"));
        }

        let check_plan: super::models::QaCheckPlan =
            serde_json::from_value(detail.run.check_plan.clone())
                .map_err(|error| invalid(format!("invalid QA check plan: {error}")))?;
        if check_plan.asset_id != detail.run.asset_id
            || check_plan.asset_version_id != detail.run.asset_version_id
        {
            return Err(invalid(
                "QA check plan does not target the run's exact Asset Version",
            ));
        }

        let definitions = check_plan
            .checks
            .iter()
            .map(|definition| (definition.id.as_str(), definition))
            .collect::<BTreeMap<_, _>>();

        if detail
            .checks
            .iter()
            .any(|check| check.effective_status() == QaCheckStatus::Uncertain)
        {
            return Err(invalid(
                "repair cannot include unresolved uncertain checks; review them as pass or fail first",
            ));
        }

        let mut failed = detail
            .checks
            .iter()
            .filter(|check| check.effective_status() == QaCheckStatus::Fail)
            .collect::<Vec<_>>();
        failed.sort_by(|left, right| left.check_id.cmp(&right.check_id));
        if failed.is_empty() {
            return Err(invalid("repair has no effective failed checks"));
        }

        let mut passed = detail
            .checks
            .iter()
            .filter(|check| {
                check.effective_status() == QaCheckStatus::Pass && should_preserve(check.check_type)
            })
            .collect::<Vec<_>>();
        passed.sort_by(|left, right| left.check_id.cmp(&right.check_id));

        let preserve = passed
            .iter()
            .map(|check| {
                let definition = exact_definition(&definitions, &check.check_id)?;
                Ok(RepairPreservationInstruction {
                    check_id: check.check_id.clone(),
                    key: definition.key.clone(),
                    instruction: definition.requirement.clone(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;

        let changes = failed
            .iter()
            .map(|check| {
                let definition = exact_definition(&definitions, &check.check_id)?;
                let instruction = check
                    .repair_hint
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        format!("Correct only this condition: {}", definition.requirement)
                    });
                Ok(RepairChangeInstruction {
                    check_id: check.check_id.clone(),
                    instruction,
                    validator_hint: definition.validator_hint.clone(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;

        let mut reference_ids = check_plan
            .reference_asset_version_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for check in &failed {
            let definition = exact_definition(&definitions, &check.check_id)?;
            reference_ids.extend(definition.reference_asset_version_ids.iter().cloned());
        }

        let plan = RepairPlan {
            schema_version: 1,
            source_asset_id: detail.run.asset_id.clone(),
            source_asset_version_id: detail.run.asset_version_id.clone(),
            source_qa_run_id: detail.run.id.clone(),
            failed_check_ids: failed.iter().map(|check| check.check_id.clone()).collect(),
            preserve,
            changes,
            reference_asset_version_ids: reference_ids.into_iter().collect(),
        };
        let snapshot = serde_json::to_value(&plan)
            .map_err(|error| invalid(format!("cannot snapshot repair plan: {error}")))?;
        let request = ProviderNeutralEditRequest {
            schema_version: 1,
            request_id: format!(
                "repair:{}:{}",
                plan.source_qa_run_id, plan.source_asset_version_id
            ),
            source_asset_version_id: plan.source_asset_version_id.clone(),
            reference_asset_version_ids: plan.reference_asset_version_ids.clone(),
            prompt: compile_prompt(&plan),
            forbidden_changes: vec![
                "identity drift".into(),
                "wardrobe changes".into(),
                "pose changes".into(),
                "framing changes".into(),
                "background changes".into(),
                "accessory changes".into(),
                "unrequested text, marks, characters, or props".into(),
            ],
        };

        Ok(CompiledRepair {
            plan,
            request,
            snapshot,
        })
    }
}

fn exact_definition<'a>(
    definitions: &BTreeMap<&str, &'a QaCheckDefinition>,
    check_id: &str,
) -> Result<&'a QaCheckDefinition, AppError> {
    definitions.get(check_id).copied().ok_or_else(|| {
        invalid(format!(
            "QA check {check_id} is missing from its immutable plan"
        ))
    })
}

fn should_preserve(check_type: QaCheckType) -> bool {
    matches!(
        check_type,
        QaCheckType::IdentitySimilarity
            | QaCheckType::PermanentVisualLock
            | QaCheckType::HairConsistency
            | QaCheckType::SkinRegister
            | QaCheckType::OutfitPiece
            | QaCheckType::AccessoryPlacement
            | QaCheckType::RequiredElement
            | QaCheckType::BackgroundRequirement
            | QaCheckType::CompositionRequirement
    )
}

fn compile_prompt(plan: &RepairPlan) -> String {
    let mut lines = vec![
        "Edit the exact source image. Preserve every listed condition that already passed QA:"
            .to_owned(),
    ];
    for item in &plan.preserve {
        lines.push(format!("- {}", item.instruction));
    }
    lines.push(String::new());
    lines.push("Make only these corrections:".into());
    for (index, item) in plan.changes.iter().enumerate() {
        lines.push(format!("{}. {}", index + 1, item.instruction));
        if let Some(hint) = &item.validator_hint {
            lines.push(format!("   Directionality: {hint}"));
        }
    }
    lines.push(String::new());
    lines.push(
        "Do not alter any preserved trait or introduce any unrequested element, character, text, mark, prop, or composition change."
            .into(),
    );
    lines.join("\n")
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidQaData(message.into())
}
