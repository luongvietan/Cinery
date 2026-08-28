use super::context::ResolvedQaContext;
use super::errors::QaError;
use super::models::{QaCheckDefinition, QaCheckPlan, QaCheckSource, QaCheckType};
use crate::error::AppError;
use std::collections::BTreeSet;

pub struct QaCheckPlanner;

impl QaCheckPlanner {
    pub fn compile(context: &ResolvedQaContext) -> Result<QaCheckPlan, AppError> {
        let mut checks = Vec::new();
        let references = reference_ids(context);

        if let Some(face) = &context.canonical_face {
            checks.push(QaCheckDefinition {
                id: "reference:identity".into(),
                check_type: QaCheckType::IdentitySimilarity,
                source: QaCheckSource::CanonicalReference,
                key: "identity".into(),
                label: "Identity".into(),
                requirement: "Preserve identity from the exact canonical Face version.".into(),
                validator_hint: None,
                blocking: true,
                reference_asset_version_ids: vec![face.asset_version_id.clone()],
            });
        }

        if let Some(look_reference) = &context.canonical_look {
            checks.push(QaCheckDefinition {
                id: "reference:look".into(),
                check_type: QaCheckType::OutfitPiece,
                source: QaCheckSource::CanonicalReference,
                key: "approved_look".into(),
                label: "Approved look".into(),
                requirement: "Match the exact approved/canonical Character Look reference.".into(),
                validator_hint: None,
                blocking: true,
                reference_asset_version_ids: vec![look_reference.asset_version_id.clone()],
            });
        }

        for visual_lock in &context.visual_locks {
            checks.push(QaCheckDefinition {
                id: format!("lock:{}", stable_fragment(&visual_lock.key)?),
                check_type: lock_type(&visual_lock.key),
                source: QaCheckSource::VisualLock,
                key: visual_lock.key.clone(),
                label: humanize(&visual_lock.key),
                requirement: visual_lock.description.clone(),
                validator_hint: visual_lock.validator_hint.clone(),
                blocking: visual_lock.severity == "required",
                reference_asset_version_ids: references.clone(),
            });
        }

        for expectation in &context.expectations {
            if !matches!(
                expectation.expectation_type,
                QaCheckType::RequiredElement
                    | QaCheckType::ForbiddenElement
                    | QaCheckType::BackgroundRequirement
                    | QaCheckType::CompositionRequirement
                    | QaCheckType::OutfitPiece
                    | QaCheckType::AccessoryPlacement
            ) {
                return Err(QaError::InvalidData(format!(
                    "unsupported operation expectation type: {}",
                    expectation.expectation_type
                ))
                .into());
            }
            checks.push(QaCheckDefinition {
                id: format!("expectation:{}", stable_fragment(&expectation.id)?),
                check_type: expectation.expectation_type,
                source: QaCheckSource::OperationExpectation,
                key: expectation.id.clone(),
                label: humanize(&expectation.id),
                requirement: expectation.requirement.clone(),
                validator_hint: expectation.validator_hint.clone(),
                blocking: expectation.blocking,
                reference_asset_version_ids: references.clone(),
            });
        }

        checks.extend([
            QaCheckDefinition {
                id: "artifact:watermark".into(),
                check_type: QaCheckType::Watermark,
                source: QaCheckSource::ArtifactDetection,
                key: "watermark".into(),
                label: "Watermark".into(),
                requirement: "No watermark, signature, logo, or generated text is visible.".into(),
                validator_hint: None,
                blocking: true,
                reference_asset_version_ids: Vec::new(),
            },
            QaCheckDefinition {
                id: "artifact:unexpected".into(),
                check_type: QaCheckType::UnexpectedArtifact,
                source: QaCheckSource::ArtifactDetection,
                key: "unexpected_artifact".into(),
                label: "Unexpected artifact".into(),
                requirement: "No unexpected visual artifact is present.".into(),
                validator_hint: None,
                blocking: true,
                reference_asset_version_ids: Vec::new(),
            },
        ]);

        checks.sort_by(|left, right| left.id.cmp(&right.id));
        if checks.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(QaError::InvalidData("duplicate stable QA check id".into()).into());
        }

        Ok(QaCheckPlan {
            schema_version: 1,
            asset_id: context.target.asset_id.clone(),
            asset_version_id: context.target.asset_version_id.clone(),
            owner_entity_id: context.target.owner_entity_id.clone(),
            asset_type: context.target.asset_type.clone(),
            reference_asset_version_ids: references,
            checks,
            created_at: context.created_at.clone(),
        })
    }
}

fn reference_ids(context: &ResolvedQaContext) -> Vec<String> {
    let mut values = BTreeSet::new();
    if let Some(reference) = &context.canonical_face {
        values.insert(reference.asset_version_id.clone());
    }
    if let Some(reference) = &context.canonical_look {
        values.insert(reference.asset_version_id.clone());
    }
    values.into_iter().collect()
}

fn stable_fragment(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(QaError::InvalidData(format!("invalid stable id fragment: {value}")).into());
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn lock_type(key: &str) -> QaCheckType {
    let key = key.to_ascii_lowercase();
    if key.contains("hair") {
        QaCheckType::HairConsistency
    } else if key.contains("skin") {
        QaCheckType::SkinRegister
    } else {
        QaCheckType::PermanentVisualLock
    }
}

fn humanize(value: &str) -> String {
    let mut label = value.replace(['_', '-'], " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}
