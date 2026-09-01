//! Deterministic check planning from immutable generated-video provenance.

use super::errors::QaError;
use super::models::{
    QaCheckDefinition, QaCheckPlan, QaCheckSource, QaCheckType, ResolvedVideoQaContext,
    VideoQaReferenceContext,
};

pub struct VideoQaCheckPlanner;

impl VideoQaCheckPlanner {
    /// Compiles checks only from the resolved, immutable video QA context.
    pub fn compile(
        context: &ResolvedVideoQaContext,
    ) -> Result<QaCheckPlan, crate::error::AppError> {
        let references = relevant_references(context);
        let reference_ids = references
            .iter()
            .map(|reference| reference.asset_version_id.clone())
            .collect::<Vec<_>>();
        let mut checks = vec![
            technical_check(
                "video:integrity",
                QaCheckType::VideoIntegrity,
                "integrity",
                "Video integrity",
                "The exact video is readable, non-empty, and structurally evaluable.",
                true,
            ),
            technical_check(
                "video:temporal-coherence",
                QaCheckType::TemporalCoherence,
                "temporal_coherence",
                "Temporal coherence",
                "Visual geometry, objects, background, and lighting remain coherent through time.",
                true,
            ),
            technical_check(
                "video:unexpected-cut",
                QaCheckType::UnexpectedCut,
                "unexpected_cut",
                "Unexpected cut",
                "The video remains one continuous Shot with no unexpected cut or scene substitution.",
                true,
            ),
            technical_check(
                "video:flicker",
                QaCheckType::Flicker,
                "flicker",
                "Flicker",
                "No abnormal frame-to-frame luminance, texture, identity, or geometry instability is present.",
                false,
            ),
            technical_check(
                "video:deformation",
                QaCheckType::DeformationOrWarping,
                "deformation_or_warping",
                "Deformation or warping",
                "No generative temporal deformation, warping, duplication, or geometry collapse is present.",
                true,
            ),
            technical_check(
                "video:watermark",
                QaCheckType::Watermark,
                "watermark",
                "Watermark",
                "No unexpected watermark, provider mark, text overlay, or logo is visible.",
                true,
            ),
            technical_check(
                "video:unexpected-artifact",
                QaCheckType::UnexpectedArtifact,
                "unexpected_artifact",
                "Unexpected artifact",
                "No unexpected visual artifact is present.",
                true,
            ),
        ];

        if let Some(source) = &context.source_keyframe {
            checks.push(QaCheckDefinition {
                id: format!("video:start-frame:{}", source.asset_version_id),
                check_type: QaCheckType::StartFrameContinuity,
                source: QaCheckSource::CanonicalReference,
                key: source.asset_version_id.clone(),
                label: "Start-frame continuity".into(),
                requirement: "The opening frame preserves the exact source keyframe's identity, composition, wardrobe, and scene.".into(),
                validator_hint: None,
                blocking: true,
                reference_asset_version_ids: vec![source.asset_version_id.clone()],
            });
        }

        for identity in references
            .iter()
            .filter(|reference| is_identity_reference(reference))
        {
            checks.push(QaCheckDefinition {
                id: format!(
                    "character:{}:{}:temporal-identity",
                    identity.asset_id, identity.asset_version_id
                ),
                check_type: QaCheckType::IdentityTemporalConsistency,
                source: QaCheckSource::CanonicalReference,
                key: identity.asset_id.clone(),
                label: "Temporal identity consistency".into(),
                requirement: "Character identity remains recognizably consistent through the clip against the exact historical character reference.".into(),
                validator_hint: None,
                blocking: true,
                reference_asset_version_ids: vec![identity.asset_version_id.clone()],
            });
        }

        for reference in &references {
            checks.push(QaCheckDefinition {
                id: format!("video:reference:{}", reference.asset_version_id),
                check_type: QaCheckType::ReferenceTemporalConsistency,
                source: QaCheckSource::CanonicalReference,
                key: reference.asset_version_id.clone(),
                label: "Reference temporal consistency".into(),
                requirement: format!(
                    "The video remains consistent with the exact {} reference.",
                    reference.purpose.replace('_', " ")
                ),
                validator_hint: None,
                blocking: false,
                reference_asset_version_ids: vec![reference.asset_version_id.clone()],
            });
        }

        if let Some(requirement) = non_empty(&context.generation_intent.motion_requirement) {
            checks.push(intent_check(
                "video:motion-adherence",
                QaCheckType::MotionAdherence,
                "motion_adherence",
                "Motion adherence",
                requirement,
            ));
        }
        if let Some(requirement) = non_empty(&context.generation_intent.camera_requirement) {
            checks.push(intent_check(
                "video:camera-motion-adherence",
                QaCheckType::CameraMotionAdherence,
                "camera_motion_adherence",
                "Camera-motion adherence",
                requirement,
            ));
        }

        checks.sort_by(|left, right| left.id.cmp(&right.id));
        if checks.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(QaError::InvalidData("duplicate stable video QA check id".into()).into());
        }

        Ok(QaCheckPlan {
            schema_version: 1,
            asset_id: context.target.asset_id.clone(),
            asset_version_id: context.target.asset_version_id.clone(),
            owner_entity_id: None,
            asset_type: context.target.asset_type.clone(),
            reference_asset_version_ids: reference_ids,
            checks,
            created_at: context.created_at.clone(),
        })
    }
}

fn relevant_references(context: &ResolvedVideoQaContext) -> Vec<&VideoQaReferenceContext> {
    let source_id = context
        .source_keyframe
        .as_ref()
        .map(|reference| &reference.asset_version_id);
    let mut references = context
        .references
        .iter()
        .filter(|reference| Some(&reference.asset_version_id) != source_id)
        .filter(|reference| {
            matches!(
                reference.purpose.as_str(),
                "character_look_reference"
                    | "character_sheet_reference"
                    | "world_reference"
                    | "prop_reference"
            )
        })
        .collect::<Vec<_>>();
    references.sort_by(|left, right| left.asset_version_id.cmp(&right.asset_version_id));
    references
}

fn is_identity_reference(reference: &VideoQaReferenceContext) -> bool {
    matches!(
        reference.purpose.as_str(),
        "character_look_reference" | "character_sheet_reference"
    )
}

fn technical_check(
    id: &str,
    check_type: QaCheckType,
    key: &str,
    label: &str,
    requirement: &str,
    blocking: bool,
) -> QaCheckDefinition {
    QaCheckDefinition {
        id: id.into(),
        check_type,
        source: QaCheckSource::ArtifactDetection,
        key: key.into(),
        label: label.into(),
        requirement: requirement.into(),
        validator_hint: None,
        blocking,
        reference_asset_version_ids: Vec::new(),
    }
}

fn intent_check(
    id: &str,
    check_type: QaCheckType,
    key: &str,
    label: &str,
    requirement: &str,
) -> QaCheckDefinition {
    QaCheckDefinition {
        id: id.into(),
        check_type,
        source: QaCheckSource::OperationExpectation,
        key: key.into(),
        label: label.into(),
        requirement: requirement.into(),
        validator_hint: None,
        blocking: false,
        reference_asset_version_ids: Vec::new(),
    }
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|value| !value.trim().is_empty())
}
