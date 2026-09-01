//! Shared completion logic for provider attempts (P10.1).
//!
//! Both the synchronous workflow runtime (mock/dry_run style instant
//! completion) and the background runner (async provider jobs) end an
//! attempt the same way: capture the provider output into durable
//! generation artifacts (or a direct asset version for legacy flows),
//! then transition attempt → succeeded, step → completed, run →
//! completed. This module owns the capture half for background jobs,
//! reading *only* persisted workflow state (compiled request JSON +
//! context snapshot) so background execution never re-resolves live
//! canon and never drifts from the frozen request.

use crate::error::AppError;
use crate::providers::model::ProviderResult;
use crate::providers::repository::ExecutionAttemptRecord;
use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::Path;

/// The durable inputs the completion module needs for one attempt —
/// everything is derivable from persisted rows.
pub struct CompletionJob<'a> {
    pub attempt: &'a ExecutionAttemptRecord,
    pub provider_job_id: &'a str,
}

/// What `complete_attempt` found/did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionOutcome {
    /// The output was captured; the caller may now transition the attempt
    /// to `succeeded` and complete the run.
    Captured {
        result_set_id: Option<String>,
        artifact_ids: Vec<String>,
    },
    /// The attempt already reached a terminal state in an earlier pass
    /// (crash replay, double tick). Nothing new was persisted.
    AlreadyTerminal,
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}

fn open_project(root: &Path) -> Result<Connection, AppError> {
    crate::db::open_existing_connection(&root.join("project.db"))
}

/// Loads the compiled request JSON + context snapshot + run input for the
/// attempt's workflow step, from durable state only.
struct AttemptContext {
    request_json: String,
    snapshot_json: Option<String>,
    input_json: String,
    operation_id: String,
    skill_id: String,
    skill_version: String,
    project_id: String,
}

fn load_attempt_context(
    conn: &Connection,
    job: &CompletionJob<'_>,
) -> Result<AttemptContext, AppError> {
    // The compiled request lives on the run's compile_request step (durable
    // state only; never re-resolved from live canon).
    conn.query_row(
        "SELECT s.output_json, r.context_snapshot_json, r.input_json, r.operation_id,
                r.skill_id, r.skill_version, r.project_id
         FROM workflow_runs r
         JOIN workflow_steps s
           ON s.workflow_run_id = r.id AND s.step_type = 'compile_request'
         WHERE r.id = ?1",
        rusqlite::params![job.attempt.workflow_run_id],
        |row| {
            Ok(AttemptContext {
                request_json: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                snapshot_json: row.get(1)?,
                input_json: row.get(2)?,
                operation_id: row.get(3)?,
                skill_id: row.get(4)?,
                skill_version: row.get(5)?,
                project_id: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(db_error)?
    .ok_or_else(|| {
        AppError::WorkflowRunInconsistent(
            "the compiled request for this attempt is missing from durable state".into(),
        )
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Captures a fetched provider result for the attempt, mirroring the exact
/// per-operation capture behavior of the synchronous runtime:
///
/// - `scene.generate_video` / `shot.image_to_video` — generation result set
///   (video) + candidate import into the scene's stable video asset;
/// - `scene.create_keyframe` — generation result set (image) + candidate
///   import into the scene's shot_keyframe asset;
/// - other operations — generation result set (image) with promotion left
///   to human review (candidate status only).
///
/// Idempotency: if a result set already exists for this provider attempt
/// (a crash between fetch and completion replayed), the existing artifacts
/// are returned and no duplicate rows are written.
pub fn complete_attempt(
    project_root: &Path,
    pending: &crate::workflow::background::PendingJob,
    result: &ProviderResult,
) -> Result<CompletionOutcome, AppError> {
    let conn = open_project(project_root)?;
    let attempt = load_attempt_record(&conn, &pending.execution_id)?;

    // Terminal guard: never re-capture a finished attempt.
    if matches!(
        attempt.status.as_str(),
        "succeeded" | "failed" | "cancelled"
    ) {
        return Ok(CompletionOutcome::AlreadyTerminal);
    }

    let context = load_attempt_context(
        &conn,
        &CompletionJob {
            attempt: &attempt,
            provider_job_id: &pending.provider_job_id,
        },
    )?;
    let request: crate::workflow::execution::ExecutionRequest =
        serde_json::from_str(&context.request_json).map_err(|error| {
            AppError::WorkflowRunInconsistent(format!(
                "the persisted compiled request is no longer parseable: {error}"
            ))
        })?;
    let snapshot: Option<crate::workflow::model::WorkflowContextSnapshot> = context
        .snapshot_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok());

    // --- Idempotency: replayed completion (crash between capture and
    // terminal transitions) must not duplicate artifacts. ---
    if let Some(existing) = existing_result_set(&conn, &attempt.id)? {
        let artifact_ids = existing_artifact_ids(&conn, &existing)?;
        if matches!(
            context.operation_id.as_str(),
            "scene.generate_video" | "shot.image_to_video"
        ) {
            if let Some(artifact_id) = artifact_ids.first() {
                let artifact = crate::generation::service::GenerationService::get_artifact_detail(
                    project_root,
                    artifact_id,
                )?
                .artifact;
                import_scene_video_candidate(project_root, &conn, &attempt, &context, &artifact)?;
            }
        }
        return Ok(CompletionOutcome::Captured {
            result_set_id: Some(existing),
            artifact_ids,
        });
    }

    let media_kind = if request.media_type == crate::workflow::execution::ExecutionMediaType::Video
    {
        "video"
    } else {
        "image"
    };

    let compiled_request_hash = sha256_hex(context.request_json.as_bytes());

    // World/scene plates (world.create_plate) historically persist a
    // candidate asset version directly instead of a generation result
    // set. Keep that behavior for compatibility.
    if context.operation_id == "world.create_plate" {
        let input: serde_json::Value = serde_json::from_str(&context.input_json)
            .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
        let owner_entity_id = request
            .expected_output
            .owner_entity_input_ref
            .as_deref()
            .and_then(|reference| input.get(reference))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let version = crate::workflow::ingestion::persist_provider_result(
            project_root,
            &attempt.workflow_run_id,
            result,
            request.expected_output.asset_type.as_str(),
            owner_entity_id,
        )?;
        let _ = crate::providers::repository::append_audit_event(
            &conn,
            Some(&attempt.id),
            &attempt.workflow_run_id,
            "provider.execution.completed",
            Some(&serde_json::json!({"artifactPath": version.file_path})),
        );
        return Ok(CompletionOutcome::Captured {
            result_set_id: None,
            artifact_ids: Vec::new(),
        });
    }

    let snapshot_assets = snapshot
        .as_ref()
        .map(|snapshot| {
            snapshot
                .assets
                .iter()
                .map(|asset| asset.asset_version_id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let canon = snapshot.as_ref().map(|snapshot| &snapshot.canon);

    let requested_output_count = if matches!(
        context.operation_id.as_str(),
        "scene.generate_video" | "shot.image_to_video"
    ) || context.operation_id.starts_with("scene.")
    {
        1
    } else {
        4
    };

    let captured = crate::generation::service::GenerationService::capture_provider_result(
        project_root,
        &crate::generation::service::GenerationCaptureInput {
            project_id: context.project_id.clone(),
            workflow_run_id: attempt.workflow_run_id.clone(),
            workflow_step_key: attempt.step_definition_id.clone(),
            workflow_definition_id: context.operation_id.clone(),
            workflow_version: context.skill_version.clone(),
            skill_id: context.skill_id.clone(),
            skill_version: context.skill_version.clone(),
            compiled_execution_artifact_id: compiled_request_hash.clone(),
            compiled_request_sha256: compiled_request_hash,
            canon_snapshot_id: canon
                .filter(|sections| !sections.is_empty())
                .map(|_| format!("canon:{}", attempt.workflow_run_id)),
            canon_snapshot_sha256: canon
                .filter(|sections| !sections.is_empty())
                .map(|sections| {
                    let bytes = serde_json::to_vec(sections).unwrap_or_default();
                    sha256_hex(&bytes)
                }),
            provider_attempt_id: attempt.id.clone(),
            provider_id: attempt.provider_id.clone(),
            model_id: attempt.model_id.clone(),
            source_asset_version_ids: snapshot_assets,
            requested_output_count,
            media_kind: media_kind.into(),
        },
        result,
    )?;
    let artifact_ids = captured
        .artifacts
        .iter()
        .map(|artifact| artifact.id.clone())
        .collect::<Vec<_>>();

    // Video and keyframe runs also import the first artifact as a
    // *candidate* into the scene's durable asset slot (never canonical —
    // promotion stays under human review).
    if matches!(
        context.operation_id.as_str(),
        "scene.generate_video" | "shot.image_to_video"
    ) {
        import_scene_video_candidate(
            project_root,
            &conn,
            &attempt,
            &context,
            &captured.artifacts[0],
        )?;
    } else if context.operation_id == "scene.create_keyframe" {
        import_scene_keyframe_candidate(
            project_root,
            &conn,
            &attempt,
            &context,
            &captured.artifacts[0],
        )?;
    }

    Ok(CompletionOutcome::Captured {
        result_set_id: Some(captured.result_set.id),
        artifact_ids,
    })
}

fn load_attempt_record(
    conn: &Connection,
    execution_id: &str,
) -> Result<ExecutionAttemptRecord, AppError> {
    conn.query_row(
        "SELECT id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id,
                provider_id, model_id, adapter_version, idempotency_key, status,
                provider_job_id, normalized_error_json, artifact_ids_json, started_at, completed_at
         FROM workflow_step_executions WHERE id = ?1",
        rusqlite::params![execution_id],
        |row| {
            Ok(ExecutionAttemptRecord {
                id: row.get(0)?,
                workflow_run_id: row.get(1)?,
                step_definition_id: row.get(2)?,
                attempt_number: row.get(3)?,
                compiled_request_id: row.get(4)?,
                provider_id: row.get(5)?,
                model_id: row.get(6)?,
                adapter_version: row.get(7)?,
                idempotency_key: row.get(8)?,
                status: row.get(9)?,
                provider_job_id: row.get(10)?,
                normalized_error_json: row.get(11)?,
                artifact_ids_json: row.get(12)?,
                started_at: row.get(13)?,
                completed_at: row.get(14)?,
            })
        },
    )
    .optional()
    .map_err(db_error)?
    .ok_or_else(|| AppError::WorkflowRunInconsistent("the attempt row is missing".into()))
}

/// An existing generation result set for this provider attempt, if a prior
/// (interrupted) completion pass already captured it.
fn existing_result_set(conn: &Connection, attempt_id: &str) -> Result<Option<String>, AppError> {
    conn.query_row(
        "SELECT id FROM generation_result_sets WHERE provider_attempt_id = ?1",
        rusqlite::params![attempt_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(db_error)
}

fn existing_artifact_ids(conn: &Connection, result_set_id: &str) -> Result<Vec<String>, AppError> {
    let mut statement = conn
        .prepare("SELECT id FROM generated_artifacts WHERE result_set_id = ?1 ORDER BY ordinal")
        .map_err(db_error)?;
    let ids = statement
        .query_map(rusqlite::params![result_set_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(ids)
}

/// Imports the captured video artifact as a candidate version of the
/// scene's stable video asset (find-or-create: one video asset per scene
/// holds every run). Dedup by sha256 keeps repeated imports idempotent.
fn import_scene_video_candidate(
    project_root: &Path,
    conn: &Connection,
    attempt: &ExecutionAttemptRecord,
    context: &AttemptContext,
    artifact: &crate::generation::model::GeneratedArtifact,
) -> Result<(), AppError> {
    let input: serde_json::Value = serde_json::from_str(&context.input_json)
        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
    let scene_id = input
        .get("sceneId")
        .or_else(|| input.get("scene_id"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AppError::WorkflowInputInvalid("sceneId is required".into()))?
        .to_string();
    let project_id = context.project_id.clone();
    let video_asset_id =
        find_or_create_scene_video_asset(conn, project_root, &project_id, &scene_id)?;
    let source_path = project_root.join(&artifact.storage_path);
    let imported = match crate::assets::service::AssetService::import_media_version(
        project_root,
        &video_asset_id,
        &source_path,
        None,
    ) {
        Ok(version) => version,
        Err(AppError::DuplicateAssetVersion) => {
            crate::assets::service::AssetService::get_asset_with_versions(
                project_root,
                &video_asset_id,
            )?
            .versions
            .into_iter()
            .find(|version| version.sha256 == artifact.sha256)
            .ok_or_else(|| {
                AppError::GenerationArtifactCaptureFailed("duplicate not found".into())
            })?
        }
        Err(error) => return Err(error),
    };
    let _ = crate::providers::repository::append_audit_event(
        conn,
        Some(&attempt.id),
        &attempt.workflow_run_id,
        "generation.scene_video.imported",
        Some(&serde_json::json!({
            "assetVersionId": imported.id,
            "assetId": video_asset_id,
        })),
    );
    Ok(())
}

fn find_or_create_scene_video_asset(
    conn: &Connection,
    project_root: &Path,
    project_id: &str,
    scene_id: &str,
) -> Result<String, AppError> {
    if let Some(existing) = conn
        .query_row(
            "SELECT id FROM assets WHERE project_id = ?1 AND type = 'video' AND owner_entity_id = ?2
             ORDER BY created_at ASC, id ASC LIMIT 1",
            rusqlite::params![project_id, scene_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(db_error)?
    {
        return Ok(existing);
    }
    let scene_title: String = conn
        .query_row(
            "SELECT title FROM world_scenes WHERE id = ?1",
            rusqlite::params![scene_id],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "Scene".into());
    let asset = crate::assets::service::AssetService::create_asset(
        project_root,
        "video",
        &format!("{scene_title} — Video"),
        Some(scene_id.to_string()),
    )?;
    Ok(asset.id)
}

/// Imports the captured keyframe artifact as a candidate version of the
/// scene's stable shot_keyframe asset (created by the resolver step).
fn import_scene_keyframe_candidate(
    project_root: &Path,
    conn: &Connection,
    attempt: &ExecutionAttemptRecord,
    _context: &AttemptContext,
    artifact: &crate::generation::model::GeneratedArtifact,
) -> Result<(), AppError> {
    let run_input: String = conn
        .query_row(
            "SELECT input_json FROM workflow_runs WHERE id = ?1",
            rusqlite::params![attempt.workflow_run_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    let input: serde_json::Value = serde_json::from_str(&run_input)
        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
    let scene_id = input
        .get("sceneId")
        .or_else(|| input.get("scene_id"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AppError::WorkflowInputInvalid("sceneId is required".into()))?;
    let scene_asset_id: Option<String> = conn
        .query_row(
            "SELECT keyframe_asset_id FROM world_scenes WHERE id = ?1",
            rusqlite::params![scene_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?
        .flatten();
    let keyframe_asset_id = scene_asset_id
        .ok_or_else(|| AppError::WorkflowRunInconsistent("keyframe asset id missing".into()))?;
    let source_path = project_root.join(&artifact.storage_path);
    let imported = match crate::assets::service::AssetService::import_asset_version(
        project_root,
        &keyframe_asset_id,
        &source_path,
        None,
    ) {
        Ok(version) => version,
        Err(AppError::DuplicateAssetVersion) => {
            crate::assets::service::AssetService::get_asset_with_versions(
                project_root,
                &keyframe_asset_id,
            )?
            .versions
            .into_iter()
            .find(|version| version.sha256 == artifact.sha256)
            .ok_or_else(|| {
                AppError::GenerationArtifactCaptureFailed("duplicate not found".into())
            })?
        }
        Err(error) => return Err(error),
    };
    let _ = crate::providers::repository::append_audit_event(
        conn,
        Some(&attempt.id),
        &attempt.workflow_run_id,
        "generation.scene_keyframe.imported",
        Some(&serde_json::json!({"assetVersionId": imported.id})),
    );
    Ok(())
}
