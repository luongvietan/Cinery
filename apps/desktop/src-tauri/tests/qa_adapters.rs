use cinematic_desktop_lib::providers::http::{
    HttpBody, HttpExecutor, HttpRequest, HttpResponse, TransportFailure,
};
use cinematic_desktop_lib::qa::adapters::{
    MockVisualQaAdapter, OpenAiCompatibleVisualQaAdapter, VisualQaAdapter,
};
use cinematic_desktop_lib::qa::models::{
    QaCheckDefinition, QaCheckSource, QaCheckType, VisualQaMedia, VisualQaReference,
    VisualQaRequest,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

fn request(target_path: String, reference_path: String) -> VisualQaRequest {
    VisualQaRequest {
        request_id: "request-1".into(),
        target: VisualQaMedia {
            asset_version_id: "target-v1".into(),
            local_path: target_path,
            media_type: "image".into(),
        },
        references: vec![VisualQaReference {
            asset_version_id: "face-v1".into(),
            local_path: reference_path,
            purpose: "identity_reference".into(),
        }],
        checks: vec![QaCheckDefinition {
            id: "reference:identity".into(),
            check_type: QaCheckType::IdentitySimilarity,
            source: QaCheckSource::CanonicalReference,
            key: "identity".into(),
            label: "Identity".into(),
            requirement: "Match the canonical face".into(),
            validator_hint: None,
            blocking: true,
            reference_asset_version_ids: vec!["face-v1".into()],
        }],
        response_schema_version: 1,
    }
}

#[test]
fn mock_adapter_is_deterministic_and_declares_local_execution() {
    let adapter = MockVisualQaAdapter::new(json!({
        "schemaVersion": 1,
        "checks": [{
            "checkId": "reference:identity",
            "status": "pass",
            "confidence": 0.9,
            "observed": "Match",
            "reason": "Same identity",
            "repairHint": null
        }],
        "modelSummary": null
    }));
    let request = request("target.png".into(), "face.png".into());

    assert_eq!(adapter.execution_location(), "local");
    assert!(adapter.capabilities().supports_image_analysis);
    assert_eq!(
        adapter.analyze(&request).unwrap(),
        adapter.analyze(&request).unwrap()
    );
}

struct FixtureTransport {
    requests: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
}

impl HttpExecutor for FixtureTransport {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportFailure> {
        let body = match &request.body {
            HttpBody::Json(value) => value.clone(),
            _ => json!({}),
        };
        self.requests
            .lock()
            .unwrap()
            .push((request.url.clone(), body));
        Ok(HttpResponse {
            status: 200,
            body: json!({
                "choices": [{"message": {"content": "{\"schemaVersion\":1,\"checks\":[]}"}}],
                "model": "gpt-4o-mini",
                "usage": {"total_tokens": 42}
            })
            .to_string()
            .into_bytes(),
            content_type: Some("application/json".into()),
            headers: vec![],
        })
    }
}

#[test]
fn openai_compatible_adapter_sends_only_declared_media_and_marks_cloud() {
    let directory = tempfile::tempdir().unwrap();
    let target_path = directory.path().join("target.png");
    let reference_path = directory.path().join("face.png");
    std::fs::write(&target_path, [137, 80, 78, 71]).unwrap();
    std::fs::write(&reference_path, [137, 80, 78, 71]).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let adapter = OpenAiCompatibleVisualQaAdapter::with_transport(
        "https://api.example/v1",
        "secret",
        "gpt-4o-mini",
        FixtureTransport {
            requests: requests.clone(),
        },
    )
    .unwrap();

    let response = adapter
        .analyze(&request(
            target_path.to_string_lossy().into(),
            reference_path.to_string_lossy().into(),
        ))
        .unwrap();

    assert_eq!(adapter.execution_location(), "cloud:api.example");
    assert_eq!(
        response.response_text,
        "{\"schemaVersion\":1,\"checks\":[]}"
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests[0].0, "https://api.example/v1/chat/completions");
    assert_eq!(requests[0].1["model"], "gpt-4o-mini");
    let encoded = requests[0].1.to_string();
    assert_eq!(encoded.matches("data:image/png;base64,").count(), 2);
    assert!(!encoded.contains("secret"));
}

#[test]
fn local_endpoint_is_not_silently_reported_as_cloud_and_nonvision_model_is_rejected() {
    let local = OpenAiCompatibleVisualQaAdapter::with_transport(
        "http://192.168.1.5:1234/v1",
        "local-token",
        "qwen2.5-vl",
        FixtureTransport {
            requests: Arc::new(Mutex::new(Vec::new())),
        },
    )
    .unwrap();
    assert_eq!(local.execution_location(), "local");

    assert!(OpenAiCompatibleVisualQaAdapter::with_transport(
        "https://api.example/v1",
        "secret",
        "gpt-image-1",
        FixtureTransport {
            requests: Arc::new(Mutex::new(Vec::new())),
        },
    )
    .is_err());
}
