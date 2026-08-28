//! Persistence helpers for generated result sets, artifacts, and lineage.

use super::model::{ArtifactLineage, ArtifactPromotion, GeneratedArtifact, GeneratedArtifactSource, GenerationResultSet};
use crate::error::AppError;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

pub fn insert_result_set(conn: &Connection, record: &GenerationResultSet) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO generation_result_sets
         (id, project_id, workflow_run_id, workflow_step_key, provider_attempt_id,
          media_kind, requested_output_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            record.id,
            record.project_id,
            record.workflow_run_id,
            record.workflow_step_key,
            record.provider_attempt_id,
            record.media_kind,
            record.requested_output_count,
            record.created_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn insert_artifact(conn: &Connection, record: &GeneratedArtifact) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO generated_artifacts
         (id, result_set_id, ordinal, media_kind, mime_type, width, height,
          byte_size, sha256, storage_path, capture_status, capture_error_code, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            record.id,
            record.result_set_id,
            record.ordinal,
            record.media_kind,
            record.mime_type,
            record.width,
            record.height,
            record.byte_size,
            record.sha256,
            record.storage_path,
            record.capture_status,
            record.capture_error_code,
            record.created_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn insert_sources(
    conn: &Connection,
    sources: &[GeneratedArtifactSource],
) -> Result<(), AppError> {
    for source in sources {
        conn.execute(
            "INSERT INTO generated_artifact_sources
             (artifact_id, asset_version_id, role, ordinal)
             VALUES (?1, ?2, ?3, ?4)",
            params![source.artifact_id, source.asset_version_id, source.role, source.ordinal],
        )
        .map_err(db_error)?;
    }
    Ok(())
}

pub fn insert_lineage(conn: &Connection, lineage: &ArtifactLineage) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO artifact_lineage
         (artifact_id, workflow_run_id, workflow_step_key, workflow_definition_id,
          workflow_version, skill_id, skill_version, compiled_execution_artifact_id,
          compiled_request_sha256, canon_snapshot_id, canon_snapshot_sha256,
          provider_attempt_id, provider_id, model_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            lineage.artifact_id,
            lineage.workflow_run_id,
            lineage.workflow_step_key,
            lineage.workflow_definition_id,
            lineage.workflow_version,
            lineage.skill_id,
            lineage.skill_version,
            lineage.compiled_execution_artifact_id,
            lineage.compiled_request_sha256,
            lineage.canon_snapshot_id,
            lineage.canon_snapshot_sha256,
            lineage.provider_attempt_id,
            lineage.provider_id,
            lineage.model_id,
            lineage.created_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn get_artifact_for_project(
    conn: &Connection,
    project_id: &str,
    artifact_id: &str,
) -> Result<Option<GeneratedArtifact>, AppError> {
    conn.query_row(
        "SELECT a.id, a.result_set_id, a.ordinal, a.media_kind, a.mime_type,
                a.width, a.height, a.byte_size, a.sha256, a.storage_path,
                a.capture_status, a.capture_error_code, a.created_at
         FROM generated_artifacts a
         JOIN generation_result_sets r ON r.id = a.result_set_id
         WHERE a.id = ?1 AND r.project_id = ?2",
        params![artifact_id, project_id],
        row_to_artifact,
    )
    .optional()
    .map_err(db_error)
}

pub fn get_result_set_for_project(
    conn: &Connection,
    project_id: &str,
    result_set_id: &str,
) -> Result<Option<GenerationResultSet>, AppError> {
    conn.query_row(
        "SELECT id, project_id, workflow_run_id, workflow_step_key, provider_attempt_id,
                media_kind, requested_output_count, created_at
         FROM generation_result_sets WHERE id = ?1 AND project_id = ?2",
        params![result_set_id, project_id],
        |row| {
            Ok(GenerationResultSet {
                id: row.get(0)?,
                project_id: row.get(1)?,
                workflow_run_id: row.get(2)?,
                workflow_step_key: row.get(3)?,
                provider_attempt_id: row.get(4)?,
                media_kind: row.get(5)?,
                requested_output_count: row.get(6)?,
                created_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(db_error)
}

pub fn list_result_sets_for_project(
    conn: &Connection,
    project_id: &str,
    workflow_run_id: Option<&str>,
) -> Result<Vec<GenerationResultSet>, AppError> {
    let mut statement = conn
        .prepare(
            "SELECT id, project_id, workflow_run_id, workflow_step_key, provider_attempt_id,
                    media_kind, requested_output_count, created_at
             FROM generation_result_sets
             WHERE project_id = ?1 AND (?2 IS NULL OR workflow_run_id = ?2)
             ORDER BY created_at DESC, id DESC",
        )
        .map_err(db_error)?;
    let result = statement
        .query_map(params![project_id, workflow_run_id], |row| {
            Ok(GenerationResultSet {
                id: row.get(0)?,
                project_id: row.get(1)?,
                workflow_run_id: row.get(2)?,
                workflow_step_key: row.get(3)?,
                provider_attempt_id: row.get(4)?,
                media_kind: row.get(5)?,
                requested_output_count: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error);
    result
}

pub fn list_artifacts_for_result_set(
    conn: &Connection,
    result_set_id: &str,
) -> Result<Vec<GeneratedArtifact>, AppError> {
    let mut statement = conn
        .prepare(
            "SELECT id, result_set_id, ordinal, media_kind, mime_type, width, height,
                    byte_size, sha256, storage_path, capture_status, capture_error_code, created_at
             FROM generated_artifacts WHERE result_set_id = ?1 ORDER BY ordinal",
        )
        .map_err(db_error)?;
    let result = statement
        .query_map([result_set_id], row_to_artifact)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error);
    result
}

pub fn get_lineage(
    conn: &Connection,
    artifact_id: &str,
) -> Result<Option<ArtifactLineage>, AppError> {
    let mut lineage = conn
        .query_row(
            "SELECT artifact_id, workflow_run_id, workflow_step_key,
                    workflow_definition_id, workflow_version, skill_id, skill_version,
                    compiled_execution_artifact_id, compiled_request_sha256,
                    canon_snapshot_id, canon_snapshot_sha256, provider_attempt_id,
                    provider_id, model_id, created_at
             FROM artifact_lineage WHERE artifact_id = ?1",
            [artifact_id],
            |row| {
                Ok(ArtifactLineage {
                    artifact_id: row.get(0)?,
                    workflow_run_id: row.get(1)?,
                    workflow_step_key: row.get(2)?,
                    workflow_definition_id: row.get(3)?,
                    workflow_version: row.get(4)?,
                    skill_id: row.get(5)?,
                    skill_version: row.get(6)?,
                    compiled_execution_artifact_id: row.get(7)?,
                    compiled_request_sha256: row.get(8)?,
                    canon_snapshot_id: row.get(9)?,
                    canon_snapshot_sha256: row.get(10)?,
                    provider_attempt_id: row.get(11)?,
                    provider_id: row.get(12)?,
                    model_id: row.get(13)?,
                    source_asset_version_ids: Vec::new(),
                    created_at: row.get(14)?,
                })
            },
        )
        .optional()
        .map_err(db_error)?;

    if let Some(record) = &mut lineage {
        let mut statement = conn
            .prepare(
                "SELECT asset_version_id FROM generated_artifact_sources
                 WHERE artifact_id = ?1 ORDER BY ordinal",
            )
            .map_err(db_error)?;
        record.source_asset_version_ids = statement
            .query_map([artifact_id], |row| row.get(0))
            .map_err(db_error)?
            .collect::<Result<Vec<String>, _>>()
            .map_err(db_error)?;
    }
    Ok(lineage)
}

pub fn find_promotion(
    conn: &Connection,
    artifact_id: &str,
) -> Result<Option<ArtifactPromotion>, AppError> {
    conn.query_row(
        "SELECT id, artifact_id, asset_id, asset_version_id, set_canonical, created_at
         FROM artifact_promotions WHERE artifact_id = ?1",
        [artifact_id],
        |row| {
            Ok(ArtifactPromotion {
                id: row.get(0)?,
                artifact_id: row.get(1)?,
                asset_id: row.get(2)?,
                asset_version_id: row.get(3)?,
                set_canonical: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(db_error)
}

pub fn insert_promotion(conn: &Connection, promotion: &ArtifactPromotion) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO artifact_promotions
         (id, artifact_id, asset_id, asset_version_id, set_canonical, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            promotion.id,
            promotion.artifact_id,
            promotion.asset_id,
            promotion.asset_version_id,
            promotion.set_canonical as i64,
            promotion.created_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn now() -> String {
    Utc::now().to_rfc3339()
}

fn row_to_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<GeneratedArtifact> {
    Ok(GeneratedArtifact {
        id: row.get(0)?,
        result_set_id: row.get(1)?,
        ordinal: row.get(2)?,
        media_kind: row.get(3)?,
        mime_type: row.get(4)?,
        width: row.get(5)?,
        height: row.get(6)?,
        byte_size: row.get(7)?,
        sha256: row.get(8)?,
        storage_path: row.get(9)?,
        capture_status: row.get(10)?,
        capture_error_code: row.get(11)?,
        created_at: row.get(12)?,
    })
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}
