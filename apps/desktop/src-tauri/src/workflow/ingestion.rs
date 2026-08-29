#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::service::ProjectService;
    use crate::providers::model::{ProviderOutput, ProviderResult};
    use tempfile::tempdir;

    #[test]
    fn mock_output_is_materialized_as_a_candidate_asset_version() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("project");
        ProjectService::create(&root, "Red Door").unwrap();
        let result = ProviderResult {
            outputs: vec![ProviderOutput {
                uri: "mock://face-lock.png".into(),
                mime_type: "image/png".into(),
                filename: Some("face-lock.png".into()),
            }],
            provider_reported_model: Some("mock-image-v1".into()),
            metadata: serde_json::json!({}),
        };

        let persisted =
            persist_provider_result(&root, "run-1", &result, "face_lock", Some("mara".into()))
                .unwrap();
        assert_eq!(persisted.status, "candidate");
        assert_eq!(persisted.mime_type, "image/png");
        assert!(root.join(&persisted.file_path).exists());
    }
}
use crate::assets::model::AssetVersionRecord;
use crate::assets::service::AssetService;
use crate::error::AppError;
use crate::providers::http::download_bytes;
use crate::providers::model::ProviderResult;
use crate::workflow::artifacts::workflow_artifact_dir;
use image::{ImageBuffer, Rgba, RgbaImage};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::Duration;

const MAX_PROVIDER_OUTPUT_BYTES: usize = 50 * 1024 * 1024;

pub fn persist_provider_result(
    project_root: &Path,
    run_id: &str,
    result: &ProviderResult,
    asset_type: &str,
    owner_entity_id: Option<String>,
) -> Result<AssetVersionRecord, AppError> {
    let output = result.outputs.first().ok_or_else(|| {
        AppError::WorkflowArtifactWriteFailed("provider returned no outputs".into())
    })?;
    let bytes = load_output_bytes(&output.uri, &output.mime_type)?;
    if bytes.is_empty() {
        return Err(AppError::WorkflowArtifactWriteFailed(
            "provider output is empty".into(),
        ));
    }
    let extension = extension_for_mime(&output.mime_type).ok_or_else(|| {
        AppError::WorkflowArtifactWriteFailed(format!(
            "unsupported provider output MIME type {}",
            output.mime_type
        ))
    })?;
    let output_dir = workflow_artifact_dir(project_root, run_id);
    fs::create_dir_all(&output_dir)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;
    let source_path = output_dir.join(format!("provider-output.{extension}"));
    fs::write(&source_path, &bytes)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;

    let summaries = AssetService::list_assets(project_root)?;
    let existing = summaries
        .into_iter()
        .find(|asset| asset.asset_type == asset_type && asset.owner_entity_id == owner_entity_id);
    let asset_id = match existing {
        Some(asset) => asset.id,
        None => {
            AssetService::create_asset(
                project_root,
                asset_type,
                &format!("{asset_type} candidate"),
                owner_entity_id,
            )?
            .id
        }
    };
    match AssetService::import_asset_version(project_root, &asset_id, &source_path, None) {
        Ok(version) => Ok(version),
        Err(AppError::DuplicateAssetVersion) => {
            let hash = hash_bytes(&bytes);
            AssetService::get_asset_with_versions(project_root, &asset_id)?
                .versions
                .into_iter()
                .find(|version| version.sha256 == hash)
                .ok_or_else(|| {
                    AppError::WorkflowArtifactWriteFailed(
                        "duplicate provider output could not be reconciled".into(),
                    )
                })
        }
        Err(error) => Err(error),
    }
}

pub fn persist_repair_provider_result(
    project_root: &Path,
    run_id: &str,
    result: &ProviderResult,
    asset_id: &str,
    parent_version_id: &str,
) -> Result<AssetVersionRecord, AppError> {
    let output = result.outputs.first().ok_or_else(|| {
        AppError::WorkflowArtifactWriteFailed("provider returned no repair output".into())
    })?;
    let bytes = load_output_bytes(&output.uri, &output.mime_type)?;
    if bytes.is_empty() {
        return Err(AppError::WorkflowArtifactWriteFailed(
            "provider repair output is empty".into(),
        ));
    }
    let extension = extension_for_mime(&output.mime_type).ok_or_else(|| {
        AppError::WorkflowArtifactWriteFailed(format!(
            "unsupported provider output MIME type {}",
            output.mime_type
        ))
    })?;
    let output_dir = workflow_artifact_dir(project_root, run_id);
    fs::create_dir_all(&output_dir)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;
    let source_path = output_dir.join(format!("repair-provider-output.{extension}"));
    fs::write(&source_path, &bytes)
        .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;

    AssetService::import_asset_version(
        project_root,
        asset_id,
        &source_path,
        Some(parent_version_id.into()),
    )
}

fn load_output_bytes(uri: &str, mime_type: &str) -> Result<Vec<u8>, AppError> {
    if uri.starts_with("mock://") || uri.starts_with("dry-run://") {
        let image: RgbaImage = ImageBuffer::from_pixel(64, 64, Rgba([128, 128, 128, 255]));
        let mut cursor = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|error| AppError::WorkflowArtifactWriteFailed(error.to_string()))?;
        return Ok(cursor.into_inner());
    }
    // Base64 outputs normalize to data: URIs; decode them exactly like the
    // capture path so providers returning inline payloads work everywhere.
    if let Some((_, payload)) = uri
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(','))
    {
        return base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload)
            .map_err(|_| {
                AppError::WorkflowArtifactWriteFailed(
                    "provider returned an undecodable data URI payload".into(),
                )
            });
    }
    if uri.starts_with("https://") || uri.starts_with("http://") {
        return download_bytes(uri, MAX_PROVIDER_OUTPUT_BYTES).map_err(|error| {
            AppError::WorkflowArtifactWriteFailed(format!(
                "provider output download failed: {error}"
            ))
        });
    }
    Err(AppError::WorkflowArtifactWriteFailed(format!(
        "provider output URI is not an allowed remote artifact: {uri} ({mime_type})"
    )))
}

fn extension_for_mime(mime_type: &str) -> Option<&'static str> {
    match mime_type {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
