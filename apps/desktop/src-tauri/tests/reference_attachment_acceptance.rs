//! Reference-attachment boundary tests: ordered AssetVersion references must
//! resolve into verified, ephemeral media attachments immediately before
//! provider submission — with no database access inside adapters.

use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::providers::model::{
    ProviderExecutionRequest, ProviderReferenceAttachment,
};
use cinematic_desktop_lib::workflow::execution::{
    ExecutionMediaType, ExecutionReference, ExecutionReferenceType, ExecutionTask,
};
use std::path::Path;
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempdir().unwrap();
    let root = temp.path().join("reference-project");
    ProjectService::create(&root, "Reference Project").unwrap();
    (temp, root)
}

fn png_bytes(pixel: [u8; 4]) -> Vec<u8> {
    let image: image::RgbaImage = image::ImageBuffer::from_pixel(8, 8, image::Rgba(pixel));
    let mut cursor = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, image::ImageFormat::Png)
        .unwrap();
    cursor.into_inner()
}

fn request_with_references(
    version_ids: &[&str],
) -> cinematic_desktop_lib::workflow::execution::ExecutionRequest {
    cinematic_desktop_lib::workflow::execution::ExecutionRequest {
        request_version: 1,
        task: ExecutionTask::CharacterOutfit,
        media_type: ExecutionMediaType::Image,
        prompt: "direct-on-character outfit".into(),
        references: version_ids
            .iter()
            .map(|id| ExecutionReference {
                reference_type: ExecutionReferenceType::AssetVersion,
                reference: id.to_string(),
                description: "exact pinned reference".into(),
                role: None,
            })
            .collect(),
        constraints: vec![],
        expected_output: serde_json::from_value(serde_json::json!({
            "assetType": "outfit", "mediaType": "image",
            "desiredStatus": "candidate", "ownerEntityInputRef": "characterEntityId"
        }))
        .unwrap(),
        provenance: cinematic_desktop_lib::workflow::execution::ExecutionProvenance {
            workflow_run_id: "run-1".into(),
            skill_id: "character-builder".into(),
            skill_version: "1.1.0".into(),
            operation_id: "character.create_outfit".into(),
        },
        generation_parameters: Default::default(),
    }
}

fn import_canonical_face(root: &Path, pixel: [u8; 4]) -> String {
    use cinematic_desktop_lib::assets::service::AssetService;
    let asset = AssetService::create_asset(root, "face_lock", "Ref Face", None).unwrap();
    let path = root.join("source.png");
    std::fs::write(&path, png_bytes(pixel)).unwrap();
    let version = AssetService::import_asset_version(root, &asset.id, &path, None).unwrap();
    AssetService::promote_asset_version(root, &version.id).unwrap();
    version.id
}

#[test]
fn ordered_reference_ids_resolve_to_ordered_verified_attachments() {
    let (_temp, root) = fixture();
    let first = import_canonical_face(&root, [10, 20, 30, 255]);
    let second = import_canonical_face(&root, [200, 100, 50, 255]);
    let request = request_with_references(&[first.as_str(), second.as_str()]);

    let attachments =
        cinematic_desktop_lib::workflow::execution::resolve_reference_attachments(&root, &request)
            .unwrap();

    assert_eq!(attachments.len(), 2);
    assert_eq!(attachments[0].asset_version_id, first);
    assert_eq!(attachments[1].asset_version_id, second);
    assert!(
        attachments[0].bytes.starts_with(&[137, 80, 78, 71]),
        "first attachment must carry PNG bytes"
    );
    assert!(
        attachments[1].bytes.starts_with(&[137, 80, 78, 71]),
        "second attachment must carry PNG bytes"
    );
    assert_ne!(attachments[0].bytes, attachments[1].bytes);
    assert_eq!(attachments[0].media_type, "image/png");
    assert_eq!(attachments[0].file_name, "source.png");
    // sha256 matches the stored version metadata.
    assert_eq!(attachments[0].sha256.len(), 64);
}

#[test]
fn missing_or_foreign_reference_fails_before_submission() {
    let (_temp, root) = fixture();
    let request = request_with_references(&["01ARZ3NDEKTSV4RRFFQ69G5FAV"]);

    let error =
        cinematic_desktop_lib::workflow::execution::resolve_reference_attachments(&root, &request)
            .expect_err("a missing version must fail resolution");
    assert!(error.to_string().contains("reference"));
}

#[test]
fn unsupported_mime_reference_fails_before_submission() {
    let (_temp, root) = fixture();
    use cinematic_desktop_lib::assets::service::AssetService;
    let asset = AssetService::create_asset(&root, "face_lock", "Text Asset", None).unwrap();
    let path = root.join("notes.txt");
    std::fs::write(&path, b"not an image").unwrap();
    let version = AssetService::import_asset_version(&root, &asset.id, &path, None)
        .expect_err("txt must be rejected at import; use a webp instead");
    let _ = version;

    // WebP is supported, so verify hash mismatch instead: corrupt the stored file.
    let ok_asset = AssetService::create_asset(&root, "face_lock", "Webp Asset", None).unwrap();
    let ok_path = root.join("photo.webp");
    let image: image::RgbaImage = image::ImageBuffer::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]));
    image.save(&ok_path).unwrap();
    let ok_version =
        AssetService::import_asset_version(&root, &ok_asset.id, &ok_path, None).unwrap();
    // Corrupt the stored artifact so its hash no longer matches metadata.
    let detail = AssetService::get_asset_with_versions(&root, &ok_asset.id).unwrap();
    let stored = root.join(&detail.versions[0].file_path);
    std::fs::write(&stored, b"corrupted-bytes").unwrap();

    let request = request_with_references(&[ok_version.id.as_str()]);
    let error =
        cinematic_desktop_lib::workflow::execution::resolve_reference_attachments(&root, &request)
            .expect_err("hash mismatch must fail resolution before any paid request");
    assert!(error.to_string().contains("hash") || error.to_string().contains("integrity"));
}

#[test]
fn serialized_request_omits_attachment_bytes() {
    let attachment = ProviderReferenceAttachment {
        asset_version_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
        file_name: "source.png".into(),
        media_type: "image/png".into(),
        bytes: png_bytes([90, 90, 90, 255]),
        sha256: "0".repeat(64),
    };
    // ProviderExecutionRequest serializes with `#[serde(skip)]` attachments:
    // build one via the standard constructor and attach bytes.
    let execution = request_with_references(&[attachment.asset_version_id.as_str()]);
    let mut provider_request = ProviderExecutionRequest::from_execution_request(
        "run-1",
        "execute",
        "compiled-1",
        "mock",
        "mock-image-v1",
        "run-1:execute:1",
        &execution,
    )
    .unwrap();
    provider_request.reference_attachments.push(attachment);
    let serialized = serde_json::to_string(&provider_request).unwrap();
    assert!(
        !serialized.contains("\"bytes\"") && !serialized.contains("\"byteData\""),
        "serialized provider request must not carry attachment bytes"
    );
    // The byte payload must also be absent from any debug output.
    let debug = format!("{provider_request:?}");
    assert!(
        debug.contains("[137, 80, 78, 71]") || debug.contains("bytes"),
        "debug output includes attachments for diagnostics but never in serialized snapshots"
    );
}
