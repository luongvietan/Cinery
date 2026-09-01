use cinematic_desktop_lib::qa::{
    models::{
        QaCheckDefinition, QaCheckPlan, QaCheckRecord, QaCheckSource, QaCheckStatus, QaCheckType,
        QaMediaKind, QaOverallStatus, QaReviewStatus, QaRunDetail, QaRunRecord, QaRunStatus,
    },
    repair::RepairCompiler,
};
use serde_json::json;

fn definition(
    id: &str,
    check_type: QaCheckType,
    source: QaCheckSource,
    key: &str,
    requirement: &str,
) -> QaCheckDefinition {
    QaCheckDefinition {
        id: id.into(),
        check_type,
        source,
        key: key.into(),
        label: key.replace('_', " "),
        requirement: requirement.into(),
        validator_hint: if requirement.contains("character-") {
            Some("Character-relative direction is authoritative; do not mirror it.".into())
        } else {
            None
        },
        blocking: true,
        reference_asset_version_ids: vec!["face-v1".into()],
    }
}

fn check(
    id: &str,
    check_type: QaCheckType,
    source: QaCheckSource,
    status: QaCheckStatus,
    repair_hint: Option<&str>,
) -> QaCheckRecord {
    QaCheckRecord {
        id: format!("row-{id}"),
        qa_run_id: "qa-1".into(),
        check_id: id.into(),
        check_type,
        source,
        requirement: json!({}),
        status,
        confidence: Some(0.95),
        observed: "fixture observation".into(),
        reason: "fixture reason".into(),
        repair_hint: repair_hint.map(str::to_owned),
        review_status: QaReviewStatus::Unreviewed,
        review_note: None,
        reviewed_at: None,
        created_at: "2026-08-28T00:00:00Z".into(),
    }
}

fn detail() -> QaRunDetail {
    let definitions = vec![
        definition(
            "identity",
            QaCheckType::IdentitySimilarity,
            QaCheckSource::CanonicalReference,
            "character_identity",
            "Match the canonical character identity.",
        ),
        definition(
            "lock:right_eyebrow_scar",
            QaCheckType::PermanentVisualLock,
            QaCheckSource::VisualLock,
            "right_eyebrow_scar",
            "Scar is on the character-right eyebrow.",
        ),
        definition(
            "lock:watch_left_wrist",
            QaCheckType::AccessoryPlacement,
            QaCheckSource::VisualLock,
            "watch_left_wrist",
            "Watch remains on the character-left wrist.",
        ),
        definition(
            "expectation:wardrobe",
            QaCheckType::OutfitPiece,
            QaCheckSource::OperationExpectation,
            "wardrobe",
            "Keep the approved charcoal wardrobe.",
        ),
        definition(
            "expectation:framing",
            QaCheckType::CompositionRequirement,
            QaCheckSource::OperationExpectation,
            "framing",
            "Keep the approved medium portrait framing and pose.",
        ),
        definition(
            "artifact:unexpected",
            QaCheckType::UnexpectedArtifact,
            QaCheckSource::ArtifactDetection,
            "unexpected_artifact",
            "No unexpected marks or rendering artifacts.",
        ),
    ];
    let plan = QaCheckPlan {
        schema_version: 1,
        asset_id: "asset-1".into(),
        asset_version_id: "version-1".into(),
        owner_entity_id: Some("character-1".into()),
        asset_type: "character_face".into(),
        reference_asset_version_ids: vec!["face-v1".into()],
        checks: definitions,
        created_at: "2026-08-28T00:00:00Z".into(),
    };
    QaRunDetail {
        run: QaRunRecord {
            id: "qa-1".into(),
            project_id: "project-1".into(),
            asset_id: "asset-1".into(),
            asset_version_id: "version-1".into(),
            media_kind: QaMediaKind::Image,
            workflow_run_id: Some("workflow-qa-1".into()),
            status: QaRunStatus::Succeeded,
            overall_status: Some(QaOverallStatus::Fail),
            adapter_id: Some("mock-vlm".into()),
            adapter_version: Some("1".into()),
            model_id: Some("mock-vision".into()),
            execution_location: "local".into(),
            check_plan: serde_json::to_value(&plan).unwrap(),
            context_snapshot: json!({}),
            raw_response_metadata: None,
            error_code: None,
            error_message: None,
            created_at: "2026-08-28T00:00:00Z".into(),
            started_at: Some("2026-08-28T00:00:01Z".into()),
            completed_at: Some("2026-08-28T00:00:02Z".into()),
        },
        checks: vec![
            check(
                "identity",
                QaCheckType::IdentitySimilarity,
                QaCheckSource::CanonicalReference,
                QaCheckStatus::Pass,
                None,
            ),
            check(
                "lock:right_eyebrow_scar",
                QaCheckType::PermanentVisualLock,
                QaCheckSource::VisualLock,
                QaCheckStatus::Fail,
                Some("Move the scar to the character-right eyebrow."),
            ),
            check(
                "lock:watch_left_wrist",
                QaCheckType::AccessoryPlacement,
                QaCheckSource::VisualLock,
                QaCheckStatus::Pass,
                None,
            ),
            check(
                "expectation:wardrobe",
                QaCheckType::OutfitPiece,
                QaCheckSource::OperationExpectation,
                QaCheckStatus::Pass,
                None,
            ),
            check(
                "expectation:framing",
                QaCheckType::CompositionRequirement,
                QaCheckSource::OperationExpectation,
                QaCheckStatus::Pass,
                None,
            ),
            check(
                "artifact:unexpected",
                QaCheckType::UnexpectedArtifact,
                QaCheckSource::ArtifactDetection,
                QaCheckStatus::Fail,
                Some("Remove the unexpected mark in the lower-right corner."),
            ),
        ],
    }
}

#[test]
fn compiles_only_failed_changes_and_preserves_passed_high_value_traits() {
    let first = RepairCompiler::compile(&detail()).unwrap();
    let second = RepairCompiler::compile(&detail()).unwrap();

    assert_eq!(first, second, "repair compilation must be deterministic");
    assert_eq!(
        first.plan.failed_check_ids,
        vec!["artifact:unexpected", "lock:right_eyebrow_scar"]
    );
    assert_eq!(first.plan.changes.len(), 2);
    let prompt = first.request.prompt;
    assert!(prompt.contains("character-right eyebrow"));
    assert!(prompt.contains("lower-right corner"));
    assert!(prompt.contains("character identity"));
    assert!(prompt.contains("character-left wrist"));
    assert!(prompt.contains("charcoal wardrobe"));
    assert!(prompt.contains("framing and pose"));
    assert!(!prompt.contains("new outfit"));
    assert!(!prompt.contains("new pose"));
    assert!(!prompt.contains("new face"));
    assert!(!prompt.contains("new background"));
    assert_eq!(first.snapshot, serde_json::to_value(&first.plan).unwrap());
}

#[test]
fn rejects_no_failures_and_unresolved_uncertain_checks() {
    let mut no_failures = detail();
    for check in &mut no_failures.checks {
        check.status = QaCheckStatus::Pass;
        check.repair_hint = None;
    }
    assert!(RepairCompiler::compile(&no_failures)
        .unwrap_err()
        .to_string()
        .contains("no effective failed checks"));

    let mut uncertain = detail();
    uncertain.checks[2].status = QaCheckStatus::Uncertain;
    assert!(RepairCompiler::compile(&uncertain)
        .unwrap_err()
        .to_string()
        .contains("unresolved uncertain"));

    uncertain.checks[2].review_status = QaReviewStatus::OverriddenFail;
    uncertain.checks[2].repair_hint = Some("Confirm the watch on the character-left wrist.".into());
    assert!(RepairCompiler::compile(&uncertain).is_ok());
}
