use super::lineage::{build_lineage, LineageInput};
use super::model::{
    ArtifactLineage, GeneratedArtifact, GeneratedArtifactSource, GenerationResultSet,
};
use super::model::{GeneratedArtifactDetail, GenerationResultSetDetail};
use super::recovery;
use super::repository;
use super::storage::{read_and_verify, MaterializedArtifact};
use crate::assets::service::AssetService;
use crate::db;
use crate::error::AppError;
use crate::project::repository::read_project;
use crate::providers::http::download_bytes;
use crate::providers::model::{ProviderOutput, ProviderResult};
use chrono::Utc;
use image::{ImageBuffer, ImageFormat, Rgba, RgbaImage};
use rusqlite::{OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::Path;

/// Download cap for image outputs. Images are small; 50 MiB is generous.
const MAX_PROVIDER_OUTPUT_BYTES: usize = 50 * 1024 * 1024;

/// Download cap for video outputs (P10.0). Videos are one to two orders of
/// magnitude larger than images at the durations this pipeline generates
/// (<= 120 s), so the image cap is doubled into a distinct, still-bounded
/// limit rather than raising the image cap or allowing unbounded downloads.
const MAX_PROVIDER_VIDEO_OUTPUT_BYTES: usize = 100 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationCaptureInput {
    pub project_id: String,
    pub workflow_run_id: String,
    pub workflow_step_key: String,
    pub workflow_definition_id: String,
    pub workflow_version: String,
    pub skill_id: String,
    pub skill_version: String,
    pub compiled_execution_artifact_id: String,
    pub compiled_request_sha256: String,
    pub canon_snapshot_id: Option<String>,
    pub canon_snapshot_sha256: Option<String>,
    pub provider_attempt_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub source_asset_version_ids: Vec<String>,
    pub requested_output_count: i64,
    /// "image" or "video" — selects capture-time materialization behavior.
    pub media_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationCaptureResult {
    pub result_set: GenerationResultSet,
    pub artifacts: Vec<GeneratedArtifact>,
}

pub struct GenerationService;

impl GenerationService {
    pub fn list_results(
        project_root: &Path,
        workflow_run_id: Option<&str>,
    ) -> Result<Vec<GenerationResultSetDetail>, AppError> {
        recovery::quarantine_orphan_generated_files(project_root)?;
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let project = read_project(&conn)?;
        repository::list_result_sets_for_project(&conn, &project.id, workflow_run_id)?
            .into_iter()
            .map(|result_set| {
                let artifacts = repository::list_artifacts_for_result_set(&conn, &result_set.id)?
                    .into_iter()
                    .map(|artifact| {
                        Ok(GeneratedArtifactDetail {
                            lineage: repository::get_lineage(&conn, &artifact.id)?,
                            artifact,
                        })
                    })
                    .collect::<Result<Vec<_>, AppError>>()?;
                Ok(GenerationResultSetDetail {
                    result_set,
                    artifacts,
                })
            })
            .collect()
    }

    pub fn get_artifact_detail(
        project_root: &Path,
        artifact_id: &str,
    ) -> Result<GeneratedArtifactDetail, AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let project = read_project(&conn)?;
        let artifact = repository::get_artifact_for_project(&conn, &project.id, artifact_id)?
            .ok_or(AppError::GenerationArtifactNotPromotable)?;
        Ok(GeneratedArtifactDetail {
            lineage: repository::get_lineage(&conn, artifact_id)?,
            artifact,
        })
    }

    pub fn capture_provider_result(
        project_root: &Path,
        input: &GenerationCaptureInput,
        provider_result: &ProviderResult,
    ) -> Result<GenerationCaptureResult, AppError> {
        recovery::quarantine_orphan_generated_files(project_root)?;
        if provider_result.outputs.is_empty() {
            return Err(AppError::GenerationArtifactCaptureFailed(
                if input.media_kind == "video" {
                    "provider returned no video outputs".into()
                } else {
                    "provider returned no image outputs".into()
                },
            ));
        }
        let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let project = read_project(&conn)?;
        if project.id != input.project_id {
            return Err(AppError::GenerationProjectMismatch);
        }
        let result_set = GenerationResultSet {
            id: ulid::Ulid::new().to_string(),
            project_id: input.project_id.clone(),
            workflow_run_id: input.workflow_run_id.clone(),
            workflow_step_key: input.workflow_step_key.clone(),
            provider_attempt_id: input.provider_attempt_id.clone(),
            media_kind: input.media_kind.clone(),
            requested_output_count: input.requested_output_count,
            created_at: Utc::now().to_rfc3339(),
        };

        let mut stored = Vec::with_capacity(provider_result.outputs.len());
        // Videos use a distinct, larger, still-bounded download cap.
        let output_cap = if input.media_kind == "video" {
            MAX_PROVIDER_VIDEO_OUTPUT_BYTES
        } else {
            MAX_PROVIDER_OUTPUT_BYTES
        };
        for (index, output) in provider_result.outputs.iter().enumerate() {
            let ordinal = (index + 1) as i64;
            let bytes = provider_output_bytes(output, output_cap)?;
            let materialized = if input.media_kind == "video" {
                if output.mime_type != "video/mp4" || !super::storage::looks_like_mp4(&bytes) {
                    return Err(AppError::GenerationArtifactCaptureFailed(
                        "provider returned a payload that is not a valid MP4 video".into(),
                    ));
                }
                super::storage::materialize_media(
                    project_root,
                    &input.workflow_run_id,
                    &input.provider_attempt_id,
                    ordinal,
                    &bytes,
                    &output.mime_type,
                    "mp4",
                    None,
                    None,
                )
            } else {
                super::storage::materialize_image(
                    project_root,
                    &input.workflow_run_id,
                    &input.provider_attempt_id,
                    ordinal,
                    &bytes,
                )
            };
            match materialized {
                Ok(metadata) => stored.push((ordinal, metadata)),
                Err(error) => {
                    cleanup_stored(project_root, &stored);
                    return Err(error);
                }
            }
        }

        let artifacts = stored
            .iter()
            .map(|(ordinal, metadata)| GeneratedArtifact {
                id: ulid::Ulid::new().to_string(),
                result_set_id: result_set.id.clone(),
                ordinal: *ordinal,
                media_kind: input.media_kind.clone(),
                mime_type: metadata.mime_type.clone(),
                width: metadata.width,
                height: metadata.height,
                byte_size: metadata.byte_size,
                sha256: metadata.sha256.clone(),
                storage_path: metadata.storage_path.clone(),
                capture_status: "available".into(),
                capture_error_code: None,
                created_at: result_set.created_at.clone(),
            })
            .collect::<Vec<_>>();

        let lineages = match artifacts
            .iter()
            .map(|artifact| {
                build_lineage(LineageInput {
                    artifact_id: artifact.id.clone(),
                    workflow_run_id: input.workflow_run_id.clone(),
                    workflow_step_key: input.workflow_step_key.clone(),
                    workflow_definition_id: input.workflow_definition_id.clone(),
                    workflow_version: input.workflow_version.clone(),
                    skill_id: input.skill_id.clone(),
                    skill_version: input.skill_version.clone(),
                    compiled_execution_artifact_id: input.compiled_execution_artifact_id.clone(),
                    compiled_request_sha256: input.compiled_request_sha256.clone(),
                    canon_snapshot_id: input.canon_snapshot_id.clone(),
                    canon_snapshot_sha256: input.canon_snapshot_sha256.clone(),
                    provider_attempt_id: input.provider_attempt_id.clone(),
                    provider_id: input.provider_id.clone(),
                    model_id: input.model_id.clone(),
                    source_asset_version_ids: input.source_asset_version_ids.clone(),
                    created_at: artifact.created_at.clone(),
                })
            })
            .collect::<Result<Vec<ArtifactLineage>, _>>()
        {
            Ok(lineages) => lineages,
            Err(error) => {
                cleanup_stored(project_root, &stored);
                return Err(error);
            }
        };
        let sources = artifacts
            .iter()
            .flat_map(|artifact| {
                input
                    .source_asset_version_ids
                    .iter()
                    .enumerate()
                    .map(|(index, version_id)| GeneratedArtifactSource {
                        artifact_id: artifact.id.clone(),
                        asset_version_id: version_id.clone(),
                        role: if index == 0 {
                            "identity_reference".into()
                        } else {
                            "source_reference".into()
                        },
                        ordinal: (index + 1) as i64,
                    })
            })
            .collect::<Vec<_>>();

        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::Database(error.to_string()))?;
        let persist = (|| {
            repository::insert_result_set(&tx, &result_set)?;
            for artifact in &artifacts {
                repository::insert_artifact(&tx, artifact)?;
            }
            repository::insert_sources(&tx, &sources)?;
            for lineage in &lineages {
                repository::insert_lineage(&tx, lineage)?;
            }
            tx.commit()
                .map_err(|error| AppError::Database(error.to_string()))
        })();
        if let Err(error) = persist {
            cleanup_stored(project_root, &stored);
            return Err(error);
        }

        let _ = crate::providers::repository::append_audit_event(
            &conn,
            Some(&input.provider_attempt_id),
            &input.workflow_run_id,
            "generation.result_set.created",
            Some(&serde_json::json!({ "resultSetId": result_set.id })),
        );
        for artifact in &artifacts {
            let _ = crate::providers::repository::append_audit_event(
                &conn,
                Some(&input.provider_attempt_id),
                &input.workflow_run_id,
                "generation.artifact.materialized",
                Some(&serde_json::json!({
                    "artifactId": artifact.id,
                    "sha256": artifact.sha256,
                })),
            );
        }

        Ok(GenerationCaptureResult {
            result_set,
            artifacts,
        })
    }

    pub fn promote_generated_artifact(
        project_root: &Path,
        artifact_id: &str,
        target_asset_id: &str,
        set_canonical: bool,
    ) -> Result<crate::assets::model::AssetVersionRecord, AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let project = read_project(&conn)?;
        if let Some(existing) = repository::find_promotion(&conn, artifact_id)? {
            if existing.asset_id != target_asset_id {
                return Err(AppError::GenerationArtifactNotPromotable);
            }
            return AssetService::get_asset_with_versions(project_root, target_asset_id).map(
                |asset| {
                    asset
                        .versions
                        .into_iter()
                        .find(|version| version.id == existing.asset_version_id)
                        .ok_or(AppError::GenerationArtifactNotPromotable)
                },
            )?;
        }
        let artifact = repository::get_artifact_for_project(&conn, &project.id, artifact_id)?
            .ok_or(AppError::GenerationArtifactNotPromotable)?;
        if artifact.capture_status != "available" {
            return Err(AppError::GenerationArtifactNotPromotable);
        }
        let lineage = repository::get_lineage(&conn, artifact_id)?
            .ok_or(AppError::GenerationLineageIncomplete)?;
        read_and_verify(project_root, &artifact.storage_path, &artifact.sha256)?;

        let target = AssetService::get_asset_with_versions(project_root, target_asset_id)?;
        if target.asset.project_id != project.id {
            return Err(AppError::GenerationProjectMismatch);
        }
        // Eligibility: the target must match the artifact's expected asset
        // type and owning entity. The owner is resolved from the workflow
        // run input that produced the artifact (ownerEntityInputRef).
        let expected = expected_output_for_run(&conn, &lineage.workflow_run_id)?;
        validate_promotion_target(
            &conn,
            &lineage.workflow_run_id,
            expected.as_ref(),
            target.asset.asset_type.as_str(),
            target.asset.owner_entity_id.as_deref(),
        )?;
        let source_path = project_root.join(&artifact.storage_path);
        // Video artifacts import through the MP4 path; images through the
        // image path (decode + dimensions + thumbnail).
        let imported = match artifact.media_kind.as_str() {
            "video" => {
                match AssetService::import_media_version(
                    project_root,
                    target_asset_id,
                    &source_path,
                    target.asset.canonical_version_id.clone(),
                ) {
                    Ok(version) => version,
                    Err(AppError::DuplicateAssetVersion) => {
                        find_version_by_sha(project_root, target_asset_id, &artifact.sha256)?
                    }
                    Err(error) => return Err(error),
                }
            }
            _ => {
                match AssetService::import_asset_version(
                    project_root,
                    target_asset_id,
                    &source_path,
                    target.asset.canonical_version_id.clone(),
                ) {
                    Ok(version) => version,
                    Err(AppError::DuplicateAssetVersion) => {
                        find_version_by_sha(project_root, target_asset_id, &artifact.sha256)?
                    }
                    Err(error) => return Err(error),
                }
            }
        };
        let promoted = if set_canonical {
            AssetService::promote_asset_version(project_root, &imported.id)?.promoted_version
        } else {
            imported
        };
        let promotion = crate::generation::model::ArtifactPromotion {
            id: ulid::Ulid::new().to_string(),
            artifact_id: artifact.id,
            asset_id: target_asset_id.into(),
            asset_version_id: promoted.id.clone(),
            set_canonical,
            created_at: Utc::now().to_rfc3339(),
        };
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        match repository::insert_promotion(&conn, &promotion) {
            Ok(()) => {
                let _ = crate::providers::repository::append_audit_event(
                    &conn,
                    Some(&lineage.provider_attempt_id),
                    &lineage.workflow_run_id,
                    "generation.artifact.promoted",
                    Some(&serde_json::json!({
                        "artifactId": artifact_id,
                        "assetVersionId": promoted.id,
                    })),
                );
                if set_canonical {
                    let _ = crate::providers::repository::append_audit_event(
                        &conn,
                        Some(&lineage.provider_attempt_id),
                        &lineage.workflow_run_id,
                        "generation.artifact.canonicalized",
                        Some(&serde_json::json!({
                            "artifactId": artifact_id,
                            "assetVersionId": promoted.id,
                        })),
                    );
                }
                Ok(promoted)
            }
            Err(error) => repository::find_promotion(&conn, artifact_id)?
                .and_then(|existing| {
                    AssetService::get_asset_with_versions(project_root, target_asset_id)
                        .ok()
                        .and_then(|asset| {
                            asset
                                .versions
                                .into_iter()
                                .find(|version| version.id == existing.asset_version_id)
                        })
                })
                .ok_or(error),
        }
    }
}

/// Resolves the existing version of `target_asset_id` whose content hash
/// matches `sha256` -- the idempotent-promotion reconciliation for
/// content-deduped imports (both image and video).
fn find_version_by_sha(
    project_root: &Path,
    target_asset_id: &str,
    sha256: &str,
) -> Result<crate::assets::model::AssetVersionRecord, AppError> {
    AssetService::get_asset_with_versions(project_root, target_asset_id)?
        .versions
        .into_iter()
        .find(|version| version.sha256 == sha256)
        .ok_or(AppError::GenerationArtifactNotPromotable)
}

fn provider_output_bytes(output: &ProviderOutput, max_bytes: usize) -> Result<Vec<u8>, AppError> {
    if output.uri.starts_with("mock://") || output.uri.starts_with("dry-run://") {
        return Ok(deterministic_mock_png(&output.uri));
    }
    if output.uri.starts_with("data:") {
        // Inline base64 payload (image or video). Decode and persist; the
        // data URI itself is never stored in metadata or logs.
        let payload = output
            .uri
            .split_once(",")
            .map(|(_, payload)| payload)
            .ok_or_else(|| {
                AppError::GenerationArtifactCaptureFailed(
                    "provider returned a malformed data URI".into(),
                )
            })?;
        return base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload)
            .map_err(|_| {
                AppError::GenerationArtifactCaptureFailed(
                    "provider returned an undecodable data URI payload".into(),
                )
            });
    }
    if output.uri.starts_with("https://") || output.uri.starts_with("http://") {
        return download_bytes(&output.uri, max_bytes).map_err(|_| {
            AppError::GenerationArtifactCaptureFailed("provider output download failed".into())
        });
    }
    Err(AppError::GenerationArtifactCaptureFailed(
        "provider returned a non-durable output reference".into(),
    ))
}

fn deterministic_mock_png(seed: &str) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(seed.as_bytes());
    let digest = hash.finalize();
    let pixel = Rgba([digest[0], digest[1], digest[2], 255]);
    let image: RgbaImage = ImageBuffer::from_pixel(64, 64, pixel);
    let mut cursor = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .expect("PNG encoder must be available");
    cursor.into_inner()
}

fn cleanup_stored(project_root: &Path, stored: &[(i64, MaterializedArtifact)]) {
    for (_, metadata) in stored {
        let _ = std::fs::remove_file(project_root.join(&metadata.storage_path));
    }
    if let Some(attempt_dir) = stored.first().and_then(|(_, metadata)| {
        project_root
            .join(&metadata.storage_path)
            .parent()
            .map(Path::to_path_buf)
    }) {
        let _ = std::fs::remove_dir(&attempt_dir);
        if let Some(run_dir) = attempt_dir.parent() {
            let _ = std::fs::remove_dir(run_dir);
        }
    }
}

/// Resolves the expected output definition of the operation that produced
/// an artifact, from the producing run's skill/operation identity recorded
/// in `workflow_runs`. Returns `None` for runs without a resolvable
/// operation (then only project checks apply).
fn expected_output_for_run(
    conn: &rusqlite::Connection,
    workflow_run_id: &str,
) -> Result<Option<crate::skills::model::ExpectedOutputDefinition>, AppError> {
    let row = conn
        .query_row(
            "SELECT skill_id, skill_version, operation_id FROM workflow_runs WHERE id = ?1",
            rusqlite::params![workflow_run_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::Database(error.to_string()))?;
    let Some((skill_id, skill_version, operation_id)) = row else {
        return Ok(None);
    };
    let registry = crate::skills::registry::SkillRegistry::builtin()?;
    let operation = match registry.find_operation(&skill_id, &skill_version, &operation_id) {
        Ok((_, operation)) => Some(operation.clone()),
        Err(_) => registry
            .find_skill_any_version(&skill_id)
            .and_then(|skill| {
                skill
                    .operations
                    .iter()
                    .find(|operation| operation.id == operation_id)
            })
            .cloned(),
    };
    Ok(operation.and_then(|operation| operation.expected_output))
}

/// Central promotion-eligibility validation shared by every operation:
/// - the target asset type must equal the operation's expected asset type;
/// - when the operation declares an owner entity input reference, the
///   target's owner must equal the run input's value for that reference.
fn validate_promotion_target(
    conn: &rusqlite::Connection,
    workflow_run_id: &str,
    expected: Option<&crate::skills::model::ExpectedOutputDefinition>,
    target_asset_type: &str,
    target_owner: Option<&str>,
) -> Result<(), AppError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if target_asset_type != expected.asset_type.as_str() {
        return Err(AppError::GenerationArtifactNotPromotable);
    }
    let Some(owner_ref) = &expected.owner_entity_input_ref else {
        return Ok(());
    };
    let input_json: Option<String> = conn
        .query_row(
            "SELECT input_json FROM workflow_runs WHERE id = ?1",
            rusqlite::params![workflow_run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| AppError::Database(error.to_string()))?;
    let run_owner = input_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .and_then(|value| {
            value
                .get(owner_ref)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
    if target_owner != run_owner.as_deref() {
        return Err(AppError::GenerationArtifactNotPromotable);
    }
    Ok(())
}
