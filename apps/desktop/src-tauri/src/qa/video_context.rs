//! Immutable generated-video context resolution for Shot Video QA.

use super::models::{
    ResolvedVideoQaContext, VideoGenerationIntent, VideoGenerationOrigin, VideoQaContextRequest,
    VideoQaReferenceContext, VideoQaTargetContext,
};
use crate::error::AppError;
use crate::generation::repository as generation_repository;
use crate::workflow::execution::{
    ExecutionGenerationParameters, ExecutionMediaType, ExecutionReferenceType, ExecutionRequest,
    ExecutionTask, ReferenceRole,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const VIDEO_QA_CONTEXT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
struct AttemptRecord {
    workflow_run_id: String,
    step_definition_id: String,
    compiled_request_id: String,
    provider_id: String,
    model_id: String,
    status: String,
    artifact_ids_json: String,
}

#[derive(Debug)]
struct WorkflowRecord {
    skill_id: String,
    skill_version: String,
    operation_id: String,
    input_json: String,
    status: String,
}

pub fn resolve_video_qa_context(
    conn: &Connection,
    project_root: &Path,
    request: &VideoQaContextRequest,
) -> Result<ResolvedVideoQaContext, AppError> {
    let target = load_target(conn, &request.project_id, &request.asset_version_id)?;
    if target.asset_type != "video" || target.mime_type != "video/mp4" {
        return Err(AppError::InvalidQaData(
            "Video QA target must be an exact video/mp4 AssetVersion".into(),
        ));
    }
    verify_target_file(project_root, &target)?;

    let artifact = generation_repository::get_artifact_for_promoted_asset_version(
        conn,
        &request.project_id,
        &request.asset_version_id,
    )?
    .ok_or(AppError::VideoQaProvenanceUnsupported)?;
    let result_set = generation_repository::get_result_set_for_project(
        conn,
        &request.project_id,
        &artifact.result_set_id,
    )?
    .ok_or(AppError::VideoQaProvenanceUnsupported)?;
    let lineage = generation_repository::get_lineage(conn, &artifact.id)?
        .ok_or(AppError::VideoQaProvenanceUnsupported)?;
    let promotion = generation_repository::find_promotion(conn, &artifact.id)?
        .ok_or(AppError::VideoQaProvenanceUnsupported)?;
    let sources = generation_repository::list_sources(conn, &artifact.id)?;

    if artifact.media_kind != "video"
        || artifact.mime_type != "video/mp4"
        || artifact.capture_status != "available"
        || artifact.sha256 != target.content_sha256
        || artifact.byte_size < 0
        || artifact.byte_size as u64 != target.size_bytes
        || promotion.asset_id != target.asset_id
        || promotion.asset_version_id != target.asset_version_id
        || result_set.media_kind != "video"
        || result_set.workflow_run_id != lineage.workflow_run_id
        || result_set.provider_attempt_id != lineage.provider_attempt_id
        || result_set.workflow_step_key != lineage.workflow_step_key
        || sources
            .iter()
            .map(|source| source.asset_version_id.as_str())
            .ne(lineage.source_asset_version_ids.iter().map(String::as_str))
    {
        return Err(AppError::VideoQaProvenanceUnsupported);
    }

    let attempt = load_attempt(conn, &lineage.provider_attempt_id)?
        .ok_or(AppError::VideoQaProvenanceUnsupported)?;
    let attempt_artifact_ids: Vec<String> = serde_json::from_str(&attempt.artifact_ids_json)
        .map_err(|_| AppError::VideoQaProvenanceUnsupported)?;
    if attempt.workflow_run_id != lineage.workflow_run_id
        || attempt.step_definition_id != lineage.workflow_step_key
        || attempt.compiled_request_id != lineage.compiled_request_sha256
        || attempt.provider_id != lineage.provider_id
        || attempt.model_id != lineage.model_id
        || attempt.status != "succeeded"
        || !attempt_artifact_ids.iter().any(|id| id == &artifact.id)
        || lineage.compiled_execution_artifact_id != lineage.compiled_request_sha256
    {
        return Err(AppError::VideoQaProvenanceUnsupported);
    }

    let workflow = load_workflow(conn, &request.project_id, &lineage.workflow_run_id)?
        .ok_or(AppError::VideoQaProvenanceUnsupported)?;
    if workflow.status != "completed"
        || workflow.operation_id != "shot.image_to_video"
        || workflow.operation_id != lineage.workflow_definition_id
        || workflow.skill_id != lineage.skill_id
        || workflow.skill_version != lineage.skill_version
    {
        return Err(AppError::VideoQaProvenanceUnsupported);
    }

    let compiled_json = load_compiled_request(conn, &lineage.workflow_run_id)?
        .ok_or(AppError::VideoQaProvenanceUnsupported)?;
    if sha256(compiled_json.as_bytes()) != lineage.compiled_request_sha256 {
        return Err(AppError::VideoQaProvenanceUnsupported);
    }
    let compiled: ExecutionRequest =
        serde_json::from_str(&compiled_json).map_err(|_| AppError::VideoQaProvenanceUnsupported)?;
    if compiled.task != ExecutionTask::ShotImageToVideo
        || compiled.media_type != ExecutionMediaType::Video
        || compiled.provenance.workflow_run_id != lineage.workflow_run_id
        || compiled.provenance.operation_id != workflow.operation_id
        || compiled.provenance.skill_id != workflow.skill_id
        || compiled.provenance.skill_version != workflow.skill_version
    {
        return Err(AppError::VideoQaProvenanceUnsupported);
    }

    let frozen_input: Value = serde_json::from_str(&workflow.input_json)
        .map_err(|_| AppError::VideoQaProvenanceUnsupported)?;
    let frozen_prompt = frozen_input
        .get("prompt")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(AppError::VideoQaProvenanceUnsupported)?;
    let frozen_parameters: ExecutionGenerationParameters = frozen_input
        .get("generationParameters")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| AppError::VideoQaProvenanceUnsupported)?
        .unwrap_or_default();
    if frozen_prompt != compiled.prompt || frozen_parameters != compiled.generation_parameters {
        return Err(AppError::VideoQaProvenanceUnsupported);
    }
    let source_id = frozen_input
        .get("sourceAssetVersionId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(AppError::VideoQaProvenanceUnsupported)?;
    if !lineage
        .source_asset_version_ids
        .iter()
        .any(|id| id == source_id)
    {
        return Err(AppError::VideoQaProvenanceUnsupported);
    }

    let mut references = Vec::with_capacity(compiled.references.len());
    for reference in &compiled.references {
        if reference.reference_type != ExecutionReferenceType::AssetVersion
            || !lineage
                .source_asset_version_ids
                .iter()
                .any(|id| id == &reference.reference)
        {
            return Err(AppError::VideoQaProvenanceUnsupported);
        }
        references.push(load_reference(
            conn,
            &request.project_id,
            &reference.reference,
            purpose(reference.role.as_ref()),
        )?);
    }
    let source_keyframe = references
        .iter()
        .find(|reference| reference.asset_version_id == source_id)
        .cloned()
        .ok_or(AppError::VideoQaProvenanceUnsupported)?;

    let expected_duration_seconds = compiled.generation_parameters.duration_seconds;
    Ok(ResolvedVideoQaContext {
        schema_version: VIDEO_QA_CONTEXT_SCHEMA_VERSION,
        target,
        origin: VideoGenerationOrigin {
            workflow_run_id: lineage.workflow_run_id,
            operation_id: workflow.operation_id,
            provider_attempt_id: lineage.provider_attempt_id,
            provider_id: lineage.provider_id,
            model_id: lineage.model_id,
            compiled_request_sha256: lineage.compiled_request_sha256,
            source_asset_version_ids: lineage.source_asset_version_ids,
        },
        source_keyframe: Some(source_keyframe),
        references,
        generation_intent: VideoGenerationIntent {
            prompt: compiled.prompt,
            generation_parameters: compiled.generation_parameters,
            expected_duration_seconds,
            motion_requirement: optional_string(&frozen_input, "motionRequirement"),
            camera_requirement: optional_string(&frozen_input, "cameraRequirement"),
        },
        created_at: request.created_at.clone(),
    })
}

fn load_target(
    conn: &Connection,
    project_id: &str,
    asset_version_id: &str,
) -> Result<VideoQaTargetContext, AppError> {
    conn.query_row(
        "SELECT a.id, av.id, a.type, av.file_path, av.mime_type, av.sha256, av.byte_size
         FROM asset_versions av
         JOIN assets a ON a.id = av.asset_id
         WHERE av.id = ?1 AND a.project_id = ?2",
        params![asset_version_id, project_id],
        |row| {
            let size_bytes = row.get::<_, i64>(6)?;
            Ok(VideoQaTargetContext {
                asset_id: row.get(0)?,
                asset_version_id: row.get(1)?,
                asset_type: row.get(2)?,
                file_path: row.get(3)?,
                mime_type: row.get(4)?,
                content_sha256: row.get(5)?,
                size_bytes: u64::try_from(size_bytes)
                    .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(6, size_bytes))?,
            })
        },
    )
    .optional()
    .map_err(db_error)?
    .ok_or(AppError::AssetVersionNotFound)
}

fn load_reference(
    conn: &Connection,
    project_id: &str,
    asset_version_id: &str,
    purpose: &str,
) -> Result<VideoQaReferenceContext, AppError> {
    conn.query_row(
        "SELECT a.id, av.id, a.type, av.file_path, av.mime_type, av.sha256, av.byte_size
         FROM asset_versions av
         JOIN assets a ON a.id = av.asset_id
         WHERE av.id = ?1 AND a.project_id = ?2",
        params![asset_version_id, project_id],
        |row| {
            let size_bytes = row.get::<_, i64>(6)?;
            Ok(VideoQaReferenceContext {
                asset_id: row.get(0)?,
                asset_version_id: row.get(1)?,
                asset_type: row.get(2)?,
                file_path: row.get(3)?,
                mime_type: row.get(4)?,
                content_sha256: row.get(5)?,
                size_bytes: u64::try_from(size_bytes)
                    .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(6, size_bytes))?,
                purpose: purpose.to_string(),
            })
        },
    )
    .optional()
    .map_err(db_error)?
    .ok_or(AppError::VideoQaProvenanceUnsupported)
}

fn load_attempt(conn: &Connection, id: &str) -> Result<Option<AttemptRecord>, AppError> {
    conn.query_row(
        "SELECT workflow_run_id, step_definition_id, compiled_request_id,
                provider_id, model_id, status, artifact_ids_json
         FROM workflow_step_executions WHERE id = ?1",
        [id],
        |row| {
            Ok(AttemptRecord {
                workflow_run_id: row.get(0)?,
                step_definition_id: row.get(1)?,
                compiled_request_id: row.get(2)?,
                provider_id: row.get(3)?,
                model_id: row.get(4)?,
                status: row.get(5)?,
                artifact_ids_json: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(db_error)
}

fn load_workflow(
    conn: &Connection,
    project_id: &str,
    id: &str,
) -> Result<Option<WorkflowRecord>, AppError> {
    conn.query_row(
        "SELECT skill_id, skill_version, operation_id, input_json, status
         FROM workflow_runs WHERE id = ?1 AND project_id = ?2",
        params![id, project_id],
        |row| {
            Ok(WorkflowRecord {
                skill_id: row.get(0)?,
                skill_version: row.get(1)?,
                operation_id: row.get(2)?,
                input_json: row.get(3)?,
                status: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(db_error)
}

fn load_compiled_request(conn: &Connection, run_id: &str) -> Result<Option<String>, AppError> {
    conn.query_row(
        "SELECT output_json FROM workflow_steps
         WHERE workflow_run_id = ?1 AND step_type = 'compile_request' AND status = 'completed'",
        [run_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(db_error)
}

fn verify_target_file(project_root: &Path, target: &VideoQaTargetContext) -> Result<(), AppError> {
    let path = project_root.join(&target.file_path);
    let mut file = File::open(&path).map_err(|_| {
        AppError::GenerationArtifactUnavailable(format!(
            "target asset version {} has no readable file",
            target.asset_version_id
        ))
    })?;
    let metadata = file.metadata().map_err(|_| {
        AppError::GenerationArtifactUnavailable(format!(
            "target asset version {} has no readable file",
            target.asset_version_id
        ))
    })?;
    if !metadata.is_file() || metadata.len() != target.size_bytes {
        return Err(AppError::GenerationArtifactIntegrityMismatch(
            target.asset_version_id.clone(),
        ));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            AppError::GenerationArtifactUnavailable(format!(
                "target asset version {} could not be read",
                target.asset_version_id
            ))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if format!("{:x}", hasher.finalize()) != target.content_sha256 {
        return Err(AppError::GenerationArtifactIntegrityMismatch(
            target.asset_version_id.clone(),
        ));
    }
    Ok(())
}

fn purpose(role: Option<&ReferenceRole>) -> &'static str {
    match role {
        Some(ReferenceRole::SourceImage) => "source_keyframe",
        Some(ReferenceRole::World) => "world_reference",
        Some(ReferenceRole::CharacterLook) => "character_look_reference",
        Some(ReferenceRole::CharacterSheet) => "character_sheet_reference",
        Some(ReferenceRole::Prop) => "prop_reference",
        None => "generation_reference",
    }
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}
