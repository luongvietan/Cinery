use cinematic_desktop_lib::qa::models::{
    QaCheckDefinition, QaCheckPlan, QaCheckSource, QaCheckStatus, QaCheckType, QaOverallStatus,
};
use cinematic_desktop_lib::qa::normalizer::QaResponseNormalizer;
use serde_json::json;

fn plan() -> QaCheckPlan {
    QaCheckPlan {
        schema_version: 1,
        asset_id: "asset-1".into(),
        asset_version_id: "version-1".into(),
        owner_entity_id: None,
        asset_type: "image".into(),
        reference_asset_version_ids: vec![],
        checks: vec![
            QaCheckDefinition {
                id: "blocking".into(),
                check_type: QaCheckType::RequiredElement,
                source: QaCheckSource::OperationExpectation,
                key: "blocking".into(),
                label: "Blocking".into(),
                requirement: "Required".into(),
                validator_hint: None,
                blocking: true,
                reference_asset_version_ids: vec![],
            },
            QaCheckDefinition {
                id: "advisory".into(),
                check_type: QaCheckType::UnexpectedArtifact,
                source: QaCheckSource::ArtifactDetection,
                key: "advisory".into(),
                label: "Advisory".into(),
                requirement: "Inspect".into(),
                validator_hint: None,
                blocking: false,
                reference_asset_version_ids: vec![],
            },
        ],
        created_at: "now".into(),
    }
}

fn response(blocking: &str, advisory: &str) -> String {
    json!({
        "schemaVersion": 1,
        "checks": [
            {"checkId":"blocking","status":blocking,"confidence":0.8,"observed":"A","reason":"A","repairHint":null},
            {"checkId":"advisory","status":advisory,"confidence":null,"observed":"B","reason":"B","repairHint":null}
        ],
        "modelSummary": "summary"
    })
    .to_string()
}

#[test]
fn valid_response_is_normalized_and_overall_is_computed_locally() {
    let normalized = QaResponseNormalizer::normalize(&plan(), &response("pass", "pass")).unwrap();
    assert_eq!(normalized.overall, QaOverallStatus::Pass);
    assert_eq!(normalized.checks[0].check_id, "blocking");

    let failed = QaResponseNormalizer::normalize(&plan(), &response("fail", "pass")).unwrap();
    assert_eq!(failed.overall, QaOverallStatus::Fail);

    let uncertain =
        QaResponseNormalizer::normalize(&plan(), &response("uncertain", "pass")).unwrap();
    assert_eq!(uncertain.overall, QaOverallStatus::NeedsReview);

    let advisory = QaResponseNormalizer::normalize(&plan(), &response("pass", "fail")).unwrap();
    assert_eq!(advisory.overall, QaOverallStatus::NeedsReview);
}

#[test]
fn malformed_json_and_unknown_top_level_fields_are_rejected() {
    assert!(QaResponseNormalizer::normalize(&plan(), "not json").is_err());
    let mut value: serde_json::Value = serde_json::from_str(&response("pass", "pass")).unwrap();
    value["overall"] = json!("pass");
    assert!(QaResponseNormalizer::normalize(&plan(), &value.to_string()).is_err());
}

#[test]
fn unknown_missing_and_duplicate_check_ids_are_rejected() {
    let mut unknown: serde_json::Value = serde_json::from_str(&response("pass", "pass")).unwrap();
    unknown["checks"][0]["checkId"] = json!("invented");
    assert!(QaResponseNormalizer::normalize(&plan(), &unknown.to_string()).is_err());

    let mut missing: serde_json::Value = serde_json::from_str(&response("pass", "pass")).unwrap();
    missing["checks"].as_array_mut().unwrap().pop();
    assert!(QaResponseNormalizer::normalize(&plan(), &missing.to_string()).is_err());

    let mut duplicate: serde_json::Value = serde_json::from_str(&response("pass", "pass")).unwrap();
    duplicate["checks"][1]["checkId"] = json!("blocking");
    assert!(QaResponseNormalizer::normalize(&plan(), &duplicate.to_string()).is_err());
}

#[test]
fn invalid_status_confidence_and_unbounded_text_are_rejected() {
    let mut invalid_status: serde_json::Value =
        serde_json::from_str(&response("pass", "pass")).unwrap();
    invalid_status["checks"][0]["status"] = json!("maybe");
    assert!(QaResponseNormalizer::normalize(&plan(), &invalid_status.to_string()).is_err());

    let mut confidence: serde_json::Value =
        serde_json::from_str(&response("pass", "pass")).unwrap();
    confidence["checks"][0]["confidence"] = json!(1.1);
    assert!(QaResponseNormalizer::normalize(&plan(), &confidence.to_string()).is_err());

    let mut text: serde_json::Value = serde_json::from_str(&response("pass", "pass")).unwrap();
    text["checks"][0]["reason"] = json!("x".repeat(4_001));
    assert!(QaResponseNormalizer::normalize(&plan(), &text.to_string()).is_err());
}

#[test]
fn not_applicable_does_not_turn_uncertainty_into_pass() {
    let normalized =
        QaResponseNormalizer::normalize(&plan(), &response("not_applicable", "uncertain")).unwrap();
    assert_eq!(normalized.overall, QaOverallStatus::NeedsReview);
    assert_eq!(normalized.checks[0].status, QaCheckStatus::NotApplicable);
}
