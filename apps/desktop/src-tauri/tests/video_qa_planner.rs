use cinematic_desktop_lib::qa::models::{
    QaCheckResult, QaCheckStatus, QaOverallStatus, ResolvedVideoQaContext, VideoGenerationIntent,
    VideoGenerationOrigin, VideoQaReferenceContext, VideoQaTargetContext,
};
use cinematic_desktop_lib::qa::normalizer::compute_overall;
use cinematic_desktop_lib::qa::video_check_planner::VideoQaCheckPlanner;

fn reference(asset_id: &str, version_id: &str, purpose: &str) -> VideoQaReferenceContext {
    VideoQaReferenceContext {
        asset_id: asset_id.into(),
        asset_version_id: version_id.into(),
        asset_type: "image".into(),
        file_path: format!("assets/{version_id}.png"),
        mime_type: "image/png".into(),
        content_sha256: format!("hash-{version_id}"),
        size_bytes: 1,
        purpose: purpose.into(),
    }
}

fn context() -> ResolvedVideoQaContext {
    let source_keyframe = reference("keyframe-asset", "keyframe-v1", "source_keyframe");
    ResolvedVideoQaContext {
        schema_version: 1,
        target: VideoQaTargetContext {
            asset_id: "video-asset".into(),
            asset_version_id: "video-v1".into(),
            asset_type: "video".into(),
            file_path: "assets/video-v1.mp4".into(),
            mime_type: "video/mp4".into(),
            content_sha256: "video-hash".into(),
            size_bytes: 42,
        },
        origin: VideoGenerationOrigin {
            workflow_run_id: "run-1".into(),
            operation_id: "shot.image_to_video".into(),
            provider_attempt_id: "attempt-1".into(),
            provider_id: "provider".into(),
            model_id: "model".into(),
            compiled_request_sha256: "request-hash".into(),
            source_asset_version_ids: vec!["keyframe-v1".into()],
        },
        source_keyframe: Some(source_keyframe.clone()),
        references: vec![
            source_keyframe,
            reference("character-7", "look-v1", "character_look_reference"),
            reference("world-3", "world-v1", "world_reference"),
        ],
        generation_intent: VideoGenerationIntent {
            prompt: "Mara turns toward the window".into(),
            generation_parameters: Default::default(),
            expected_duration_seconds: Some(4.0),
            motion_requirement: Some("Mara makes one continuous turn".into()),
            camera_requirement: Some("One measured push-in with no cut".into()),
        },
        created_at: "2026-09-01T09:30:00Z".into(),
    }
}

fn result(check_id: &str, status: QaCheckStatus) -> QaCheckResult {
    QaCheckResult {
        check_id: check_id.into(),
        status,
        confidence: Some(0.9),
        observed: "observed".into(),
        reason: "reason".into(),
        repair_hint: None,
    }
}

#[test]
fn frozen_context_compiles_to_a_byte_stable_video_plan() {
    let frozen = context();
    let first = VideoQaCheckPlanner::compile(&frozen).unwrap();
    let second = VideoQaCheckPlanner::compile(&frozen).unwrap();

    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert_eq!(
        first
            .checks
            .iter()
            .map(|check| check.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "character:character-7:look-v1:temporal-identity",
            "video:camera-motion-adherence",
            "video:deformation",
            "video:flicker",
            "video:integrity",
            "video:motion-adherence",
            "video:reference:look-v1",
            "video:reference:world-v1",
            "video:start-frame:keyframe-v1",
            "video:temporal-coherence",
            "video:unexpected-artifact",
            "video:unexpected-cut",
            "video:watermark",
        ]
    );
}

#[test]
fn identity_and_camera_checks_require_their_exact_frozen_evidence() {
    let mut frozen = context();
    frozen
        .references
        .retain(|item| item.purpose != "character_look_reference");
    frozen.generation_intent.camera_requirement = None;

    let plan = VideoQaCheckPlanner::compile(&frozen).unwrap();

    assert!(!plan
        .checks
        .iter()
        .any(|check| check.id.contains("temporal-identity")));
    assert!(!plan
        .checks
        .iter()
        .any(|check| check.id == "video:camera-motion-adherence"));
}

#[test]
fn identity_checks_distinguish_exact_versions_of_the_same_reference_asset() {
    let mut frozen = context();
    frozen.references.push(reference(
        "character-7",
        "look-v2",
        "character_look_reference",
    ));

    let plan = VideoQaCheckPlanner::compile(&frozen).unwrap();

    assert_eq!(
        plan.checks
            .iter()
            .filter(|check| check.id.contains("temporal-identity"))
            .map(|check| check.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "character:character-7:look-v1:temporal-identity",
            "character:character-7:look-v2:temporal-identity",
        ]
    );
}

#[test]
fn missing_planned_check_cannot_pass_and_raw_aggregation_uses_planner_blocking() {
    let plan = VideoQaCheckPlanner::compile(&context()).unwrap();
    let mut results = plan
        .checks
        .iter()
        .map(|check| result(&check.id, QaCheckStatus::Pass))
        .collect::<Vec<_>>();

    assert_eq!(
        compute_overall(&plan, &results).unwrap(),
        QaOverallStatus::Pass
    );

    results.retain(|item| item.check_id != "video:integrity");
    assert!(compute_overall(&plan, &results).is_err());

    results.push(result("video:watermark", QaCheckStatus::Pass));
    assert!(compute_overall(&plan, &results).is_err());

    let mut results = plan
        .checks
        .iter()
        .map(|check| result(&check.id, QaCheckStatus::Pass))
        .collect::<Vec<_>>();
    results
        .iter_mut()
        .find(|item| item.check_id == "video:integrity")
        .unwrap()
        .status = QaCheckStatus::Fail;
    assert_eq!(
        compute_overall(&plan, &results).unwrap(),
        QaOverallStatus::Fail
    );

    results
        .iter_mut()
        .find(|item| item.check_id == "video:integrity")
        .unwrap()
        .status = QaCheckStatus::Pass;
    results
        .iter_mut()
        .find(|item| item.check_id == "video:flicker")
        .unwrap()
        .status = QaCheckStatus::Uncertain;
    assert_eq!(
        compute_overall(&plan, &results).unwrap(),
        QaOverallStatus::NeedsReview
    );
}
