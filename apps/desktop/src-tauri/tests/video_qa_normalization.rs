use cinematic_desktop_lib::providers::http::{
    HttpBody, HttpExecutor, HttpRequest, HttpResponse, TransportFailure,
};
use cinematic_desktop_lib::qa::adapters::{
    MockVideoQaAdapter, OpenAiCompatibleVideoQaAdapter, VideoQaAdapter,
};
use cinematic_desktop_lib::qa::models::{
    QaCheckDefinition, QaCheckPlan, QaCheckSource, QaCheckType, QaOverallStatus, VideoQaMedia,
    VideoQaReference, VideoQaRequest,
};
use cinematic_desktop_lib::qa::normalizer::QaResponseNormalizer;
use cinematic_desktop_lib::video_qa::evidence::{
    EvidenceMode, TemporalDecoderAvailability, VIDEO_QA_EVIDENCE_UNSUPPORTED,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

fn plan() -> QaCheckPlan {
    QaCheckPlan {
        schema_version: 1,
        asset_id: "video-asset-1".into(),
        asset_version_id: "video-v1".into(),
        owner_entity_id: None,
        asset_type: "video".into(),
        reference_asset_version_ids: vec![],
        checks: vec![
            QaCheckDefinition {
                id: "video:integrity".into(),
                check_type: QaCheckType::VideoIntegrity,
                source: QaCheckSource::ArtifactDetection,
                key: "integrity".into(),
                label: "Video integrity".into(),
                requirement: "The exact video is readable.".into(),
                validator_hint: None,
                blocking: true,
                reference_asset_version_ids: vec![],
            },
            QaCheckDefinition {
                id: "video:flicker".into(),
                check_type: QaCheckType::Flicker,
                source: QaCheckSource::ArtifactDetection,
                key: "flicker".into(),
                label: "Flicker".into(),
                requirement: "No abnormal flicker is present.".into(),
                validator_hint: None,
                blocking: false,
                reference_asset_version_ids: vec![],
            },
        ],
        created_at: "2026-09-01T00:00:00Z".into(),
    }
}

fn response() -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "checks": [
            {"checkId":"video:integrity","status":"pass","confidence":0.9,"observed":"Readable","reason":"The declared video is intact.","repairHint":null},
            {"checkId":"video:flicker","status":"pass","confidence":0.8,"observed":"Stable","reason":"No flicker detected.","repairHint":null}
        ],
        "modelSummary": "All planned checks were evaluated."
    })
}

#[test]
fn video_evaluator_output_is_reconciled_atomically_with_the_plan() {
    let valid = response();
    let normalized = QaResponseNormalizer::normalize(&plan(), &valid.to_string()).unwrap();
    assert_eq!(normalized.overall, QaOverallStatus::Pass);
    assert_eq!(normalized.checks.len(), 2);

    let cases = [
        ("unknown", {
            let mut value = valid.clone();
            value["checks"][0]["checkId"] = json!("video:invented");
            value
        }),
        ("missing", {
            let mut value = valid.clone();
            value["checks"].as_array_mut().unwrap().pop();
            value
        }),
        ("duplicate", {
            let mut value = valid.clone();
            value["checks"][1]["checkId"] = json!("video:integrity");
            value
        }),
        ("invalid status", {
            let mut value = valid.clone();
            value["checks"][0]["status"] = json!("maybe");
            value
        }),
        ("out of range confidence", {
            let mut value = valid.clone();
            value["checks"][0]["confidence"] = json!(1.01);
            value
        }),
        ("wrong schema", {
            let mut value = valid.clone();
            value["schemaVersion"] = json!(2);
            value
        }),
        ("denied extra field", {
            let mut value = valid.clone();
            value["checks"][0]["invented"] = json!(true);
            value
        }),
    ];

    for (name, malformed) in cases {
        assert!(
            QaResponseNormalizer::normalize(&plan(), &malformed.to_string()).is_err(),
            "{name} output must reject the entire response"
        );
    }
    assert!(QaResponseNormalizer::normalize(&plan(), "not json").is_err());
}

#[test]
fn mock_video_adapter_emits_the_exact_typed_response() {
    let adapter = MockVideoQaAdapter::new(response());
    let raw = adapter.analyze(&request("unused.mp4", "0", 0)).unwrap();

    assert_eq!(adapter.execution_location(), "local");
    assert_eq!(adapter.evidence_mode(), EvidenceMode::DirectVideo);
    assert_eq!(raw.response_text, response().to_string());
}

#[derive(Clone)]
struct FixtureTransport {
    requests: Arc<Mutex<Vec<HttpRequest>>>,
}

impl HttpExecutor for FixtureTransport {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportFailure> {
        self.requests.lock().unwrap().push(request);
        Ok(HttpResponse {
            status: 200,
            body: json!({
                "choices": [{"message": {"content": response().to_string()}}],
                "model": "video-qa-model"
            })
            .to_string()
            .into_bytes(),
            content_type: Some("application/json".into()),
            headers: vec![],
        })
    }
}

struct RejectedTransport {
    body: Vec<u8>,
}

impl HttpExecutor for RejectedTransport {
    fn execute(&self, _request: HttpRequest) -> Result<HttpResponse, TransportFailure> {
        Ok(HttpResponse {
            status: 503,
            body: self.body.clone(),
            content_type: Some("text/plain".into()),
            headers: vec![],
        })
    }
}

fn request(path: &str, sha256: &str, size_bytes: u64) -> VideoQaRequest {
    VideoQaRequest {
        request_id: "video-qa-request-1".into(),
        target: VideoQaMedia {
            asset_version_id: "video-v1".into(),
            local_path: path.into(),
            mime_type: "video/mp4".into(),
            content_sha256: sha256.into(),
            size_bytes,
        },
        references: vec![],
        checks: plan().checks,
        response_schema_version: 1,
    }
}

#[test]
fn production_adapter_transfers_only_task_zero_bound_direct_video_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("candidate.mp4");
    let reference_path = directory.path().join("source-keyframe.png");
    let bytes = [
        0, 0, 0, 16, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0, 0, 0, 0,
    ];
    let reference_bytes = [137, 80, 78, 71];
    std::fs::write(&source, bytes).unwrap();
    std::fs::write(&reference_path, reference_bytes).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let reference_sha256 = format!("{:x}", Sha256::digest(reference_bytes));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let adapter = OpenAiCompatibleVideoQaAdapter::with_transport(
        "https://video.example/v1",
        "secret",
        "video-qa-model",
        EvidenceMode::DirectVideo,
        TemporalDecoderAvailability::Unavailable,
        FixtureTransport {
            requests: requests.clone(),
        },
    )
    .unwrap();

    let mut evaluator_request = request(&source.to_string_lossy(), &sha256, bytes.len() as u64);
    evaluator_request.references = vec![VideoQaReference {
        asset_version_id: "keyframe-v1".into(),
        local_path: reference_path.to_string_lossy().into(),
        mime_type: "image/png".into(),
        content_sha256: reference_sha256,
        size_bytes: reference_bytes.len() as u64,
        purpose: "source_keyframe".into(),
    }];
    let raw = adapter.analyze(&evaluator_request).unwrap();

    assert_eq!(adapter.evidence_mode(), EvidenceMode::DirectVideo);
    assert_eq!(adapter.execution_location(), "cloud:video.example");
    assert_eq!(raw.response_text, response().to_string());
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let HttpBody::Multipart(parts) = &requests[0].body else {
        panic!("video transfer must use multipart evidence");
    };
    let video = parts
        .iter()
        .find(|part| part.field_name == "video")
        .unwrap();
    assert_eq!(video.content_type.as_deref(), Some("video/mp4"));
    assert_eq!(video.bytes, bytes);
    let reference = parts
        .iter()
        .find(|part| part.field_name == "reference_0")
        .expect("each declared reference must be transferred");
    assert_eq!(reference.content_type.as_deref(), Some("image/png"));
    assert_eq!(reference.bytes, reference_bytes);
    let request = parts
        .iter()
        .find(|part| part.field_name == "request")
        .unwrap();
    let descriptor: serde_json::Value = serde_json::from_slice(&request.bytes).unwrap();
    assert_eq!(descriptor["model"], "video-qa-model");
    assert_eq!(descriptor["references"][0]["assetVersionId"], "keyframe-v1");
    assert!(descriptor["references"][0].get("localPath").is_none());
    assert!(!format!("{parts:?}").contains(&*reference_path.to_string_lossy()));
    assert!(!format!("{parts:?}").contains("secret"));
}

#[test]
fn production_adapter_does_not_copy_a_rejected_response_body_into_diagnostics() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("candidate.mp4");
    let bytes = [
        0, 0, 0, 16, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0, 0, 0, 0,
    ];
    std::fs::write(&source, bytes).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let hostile_body = format!(
        "provider-body-marker Authorization: Bearer sk-provider-secret {}",
        "x".repeat(8_192)
    );
    let adapter = OpenAiCompatibleVideoQaAdapter::with_transport(
        "https://video.example/v1",
        "configured-secret",
        "video-qa-model",
        EvidenceMode::DirectVideo,
        TemporalDecoderAvailability::Unavailable,
        RejectedTransport {
            body: hostile_body.into_bytes(),
        },
    )
    .unwrap();

    let error = adapter
        .analyze(&request(
            &source.to_string_lossy(),
            &sha256,
            bytes.len() as u64,
        ))
        .unwrap_err();

    assert_eq!(error.diagnostic.as_deref(), Some("HTTP 503"));
    assert!(!error.to_string().contains("provider-body-marker"));
}

#[test]
fn production_adapter_rejects_a_reference_identity_mismatch_before_network() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("candidate.mp4");
    let reference_path = directory.path().join("source-keyframe.png");
    let bytes = [
        0, 0, 0, 16, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0, 0, 0, 0,
    ];
    let reference_bytes = [137, 80, 78, 71];
    std::fs::write(&source, bytes).unwrap();
    std::fs::write(&reference_path, reference_bytes).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let adapter = OpenAiCompatibleVideoQaAdapter::with_transport(
        "https://video.example/v1",
        "secret",
        "video-qa-model",
        EvidenceMode::DirectVideo,
        TemporalDecoderAvailability::Unavailable,
        FixtureTransport {
            requests: requests.clone(),
        },
    )
    .unwrap();
    let mut evaluator_request = request(&source.to_string_lossy(), &sha256, bytes.len() as u64);
    evaluator_request.references = vec![VideoQaReference {
        asset_version_id: "keyframe-v1".into(),
        local_path: reference_path.to_string_lossy().into(),
        mime_type: "image/png".into(),
        content_sha256: "not-the-file-hash".into(),
        size_bytes: reference_bytes.len() as u64,
        purpose: "source_keyframe".into(),
    }];

    let error = adapter.analyze(&evaluator_request).unwrap_err();

    assert_eq!(
        error.kind,
        cinematic_desktop_lib::qa::adapters::VideoQaAdapterErrorKind::InvalidRequest
    );
    assert!(requests.lock().unwrap().is_empty());
}

#[test]
fn sampled_frame_mode_stops_before_network_when_task_zero_reports_unsupported() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("candidate.mp4");
    let bytes = [
        0, 0, 0, 16, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0, 0, 0, 0,
    ];
    std::fs::write(&source, bytes).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let adapter = OpenAiCompatibleVideoQaAdapter::with_transport(
        "https://video.example/v1",
        "secret",
        "video-qa-model",
        EvidenceMode::SampledFrames,
        TemporalDecoderAvailability::Unavailable,
        FixtureTransport {
            requests: requests.clone(),
        },
    )
    .unwrap();

    let error = adapter
        .analyze(&request(
            &source.to_string_lossy(),
            &sha256,
            bytes.len() as u64,
        ))
        .unwrap_err();

    assert_eq!(adapter.evidence_mode(), EvidenceMode::SampledFrames);
    assert_eq!(error.message, VIDEO_QA_EVIDENCE_UNSUPPORTED);
    assert!(requests.lock().unwrap().is_empty());
}
