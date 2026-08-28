use super::errors::QaError;
use super::models::{QaCheckRecord, QaReviewStatus, QaRunDetail, QaRunRecord};
use crate::error::AppError;
use rusqlite::{params, types::Type, Connection, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{fmt::Display, str::FromStr};

pub fn insert_run(conn: &Connection, run: &QaRunRecord) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO qa_runs
         (id, project_id, asset_id, asset_version_id, workflow_run_id, status, overall_status,
          adapter_id, adapter_version, model_id, execution_location, check_plan_json,
          context_snapshot_json, raw_response_metadata_json, error_code, error_message,
          created_at, started_at, completed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                 ?15, ?16, ?17, ?18, ?19)",
        params![
            run.id,
            run.project_id,
            run.asset_id,
            run.asset_version_id,
            run.workflow_run_id,
            run.status.as_str(),
            run.overall_status.map(|value| value.as_str()),
            run.adapter_id,
            run.adapter_version,
            run.model_id,
            run.execution_location,
            run.check_plan.to_string(),
            run.context_snapshot.to_string(),
            run.raw_response_metadata.as_ref().map(Value::to_string),
            run.error_code,
            run.error_message,
            run.created_at,
            run.started_at,
            run.completed_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn insert_checks(conn: &mut Connection, checks: &[QaCheckRecord]) -> Result<(), AppError> {
    let transaction = conn.transaction().map_err(db_error)?;
    for check in checks {
        transaction
            .execute(
                "INSERT INTO qa_checks
                 (id, qa_run_id, check_id, check_type, source, requirement_json, status,
                  confidence, observed, reason, repair_hint, review_status, review_note,
                  reviewed_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    check.id,
                    check.qa_run_id,
                    check.check_id,
                    check.check_type.as_str(),
                    check.source.as_str(),
                    check.requirement.to_string(),
                    check.status.as_str(),
                    check.confidence,
                    check.observed,
                    check.reason,
                    check.repair_hint,
                    check.review_status.as_str(),
                    check.review_note,
                    check.reviewed_at,
                    check.created_at,
                ],
            )
            .map_err(db_error)?;
    }
    transaction.commit().map_err(db_error)
}

pub fn get_run(
    conn: &Connection,
    project_id: &str,
    qa_run_id: &str,
) -> Result<Option<QaRunDetail>, AppError> {
    let run = conn
        .query_row(
            "SELECT id, project_id, asset_id, asset_version_id, workflow_run_id, status,
                    overall_status, adapter_id, adapter_version, model_id, execution_location,
                    check_plan_json, context_snapshot_json, raw_response_metadata_json,
                    error_code, error_message, created_at, started_at, completed_at
             FROM qa_runs WHERE id = ?1 AND project_id = ?2",
            params![qa_run_id, project_id],
            row_to_run,
        )
        .optional()
        .map_err(db_error)?;

    let Some(run) = run else { return Ok(None) };
    let checks = list_checks(conn, qa_run_id)?;
    Ok(Some(QaRunDetail { run, checks }))
}

pub fn list_runs_for_asset_version(
    conn: &Connection,
    project_id: &str,
    asset_version_id: &str,
) -> Result<Vec<QaRunRecord>, AppError> {
    let mut statement = conn
        .prepare(
            "SELECT id, project_id, asset_id, asset_version_id, workflow_run_id, status,
                    overall_status, adapter_id, adapter_version, model_id, execution_location,
                    check_plan_json, context_snapshot_json, raw_response_metadata_json,
                    error_code, error_message, created_at, started_at, completed_at
             FROM qa_runs
             WHERE project_id = ?1 AND asset_version_id = ?2
             ORDER BY created_at DESC, id DESC",
        )
        .map_err(db_error)?;
    let records = statement
        .query_map(params![project_id, asset_version_id], row_to_run)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(records)
}

pub fn review_check(
    conn: &Connection,
    project_id: &str,
    qa_run_id: &str,
    check_id: &str,
    review_status: QaReviewStatus,
    note: Option<&str>,
    reviewed_at: &str,
) -> Result<(), AppError> {
    let changed = conn
        .execute(
            "UPDATE qa_checks
             SET review_status = ?1, review_note = ?2, reviewed_at = ?3
             WHERE qa_run_id = ?4 AND check_id = ?5
               AND EXISTS (
                 SELECT 1 FROM qa_runs r
                 WHERE r.id = qa_checks.qa_run_id AND r.project_id = ?6
               )",
            params![
                review_status.as_str(),
                note,
                reviewed_at,
                qa_run_id,
                check_id,
                project_id
            ],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(QaError::CheckNotFound.into());
    }
    Ok(())
}

pub fn mark_run_running(
    conn: &Connection,
    qa_run_id: &str,
    started_at: &str,
) -> Result<(), AppError> {
    let changed = conn
        .execute(
            "UPDATE qa_runs SET status = 'running', started_at = ?1
             WHERE id = ?2 AND status = 'queued'",
            params![started_at, qa_run_id],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(QaError::RunNotFound.into());
    }
    Ok(())
}

pub fn complete_run(
    conn: &mut Connection,
    qa_run_id: &str,
    overall: super::models::QaOverallStatus,
    raw_response_metadata: &serde_json::Value,
    checks: &[QaCheckRecord],
    completed_at: &str,
) -> Result<(), AppError> {
    let transaction = conn.transaction().map_err(db_error)?;
    for check in checks {
        transaction
            .execute(
                "INSERT INTO qa_checks
                 (id, qa_run_id, check_id, check_type, source, requirement_json, status,
                  confidence, observed, reason, repair_hint, review_status, review_note,
                  reviewed_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    check.id,
                    check.qa_run_id,
                    check.check_id,
                    check.check_type.as_str(),
                    check.source.as_str(),
                    check.requirement.to_string(),
                    check.status.as_str(),
                    check.confidence,
                    check.observed,
                    check.reason,
                    check.repair_hint,
                    check.review_status.as_str(),
                    check.review_note,
                    check.reviewed_at,
                    check.created_at,
                ],
            )
            .map_err(db_error)?;
    }
    let changed = transaction
        .execute(
            "UPDATE qa_runs
             SET status = 'succeeded', overall_status = ?1, raw_response_metadata_json = ?2,
                 error_code = NULL, error_message = NULL, completed_at = ?3
             WHERE id = ?4 AND status = 'running'",
            params![
                overall.as_str(),
                raw_response_metadata.to_string(),
                completed_at,
                qa_run_id
            ],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(QaError::RunNotFound.into());
    }
    transaction.commit().map_err(db_error)
}

pub fn mark_run_failed(
    conn: &Connection,
    qa_run_id: &str,
    error_code: &str,
    error_message: &str,
    raw_response_metadata: Option<&serde_json::Value>,
    completed_at: &str,
) -> Result<(), AppError> {
    let changed = conn
        .execute(
            "UPDATE qa_runs
             SET status = 'failed', overall_status = NULL, error_code = ?1, error_message = ?2,
                 raw_response_metadata_json = ?3, completed_at = ?4
             WHERE id = ?5 AND status IN ('queued', 'running')",
            params![
                error_code,
                error_message,
                raw_response_metadata.map(serde_json::Value::to_string),
                completed_at,
                qa_run_id
            ],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(QaError::RunNotFound.into());
    }
    Ok(())
}

pub fn cancel_for_workflow(
    conn: &Connection,
    workflow_run_id: &str,
    completed_at: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE qa_runs SET status = 'cancelled', completed_at = ?1
         WHERE workflow_run_id = ?2 AND status IN ('queued', 'running')",
        params![completed_at, workflow_run_id],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn fail_for_workflow(
    conn: &Connection,
    workflow_run_id: &str,
    error_code: &str,
    error_message: &str,
    completed_at: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE qa_runs
         SET status = 'failed', overall_status = NULL, error_code = ?1,
             error_message = ?2, completed_at = ?3
         WHERE workflow_run_id = ?4 AND status IN ('queued', 'running')",
        params![error_code, error_message, completed_at, workflow_run_id],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn update_overall_status(
    conn: &Connection,
    qa_run_id: &str,
    overall: super::models::QaOverallStatus,
) -> Result<(), AppError> {
    let changed = conn
        .execute(
            "UPDATE qa_runs SET overall_status = ?1 WHERE id = ?2 AND status = 'succeeded'",
            params![overall.as_str(), qa_run_id],
        )
        .map_err(db_error)?;
    if changed == 0 {
        return Err(QaError::RunNotFound.into());
    }
    Ok(())
}

fn list_checks(conn: &Connection, qa_run_id: &str) -> Result<Vec<QaCheckRecord>, AppError> {
    let mut statement = conn
        .prepare(
            "SELECT id, qa_run_id, check_id, check_type, source, requirement_json, status,
                    confidence, observed, reason, repair_hint, review_status, review_note,
                    reviewed_at, created_at
             FROM qa_checks WHERE qa_run_id = ?1 ORDER BY id",
        )
        .map_err(db_error)?;
    let checks = statement
        .query_map([qa_run_id], row_to_check)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(checks)
}

fn row_to_run(row: &Row<'_>) -> rusqlite::Result<QaRunRecord> {
    Ok(QaRunRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        asset_id: row.get(2)?,
        asset_version_id: row.get(3)?,
        workflow_run_id: row.get(4)?,
        status: parse_enum(5, row.get::<_, String>(5)?)?,
        overall_status: row
            .get::<_, Option<String>>(6)?
            .map(|value| parse_enum(6, value))
            .transpose()?,
        adapter_id: row.get(7)?,
        adapter_version: row.get(8)?,
        model_id: row.get(9)?,
        execution_location: row.get(10)?,
        check_plan: parse_json(11, row.get(11)?)?,
        context_snapshot: parse_json(12, row.get(12)?)?,
        raw_response_metadata: row
            .get::<_, Option<String>>(13)?
            .map(|value| parse_json(13, value))
            .transpose()?,
        error_code: row.get(14)?,
        error_message: row.get(15)?,
        created_at: row.get(16)?,
        started_at: row.get(17)?,
        completed_at: row.get(18)?,
    })
}

fn row_to_check(row: &Row<'_>) -> rusqlite::Result<QaCheckRecord> {
    Ok(QaCheckRecord {
        id: row.get(0)?,
        qa_run_id: row.get(1)?,
        check_id: row.get(2)?,
        check_type: parse_enum(3, row.get::<_, String>(3)?)?,
        source: parse_enum(4, row.get::<_, String>(4)?)?,
        requirement: parse_json(5, row.get(5)?)?,
        status: parse_enum(6, row.get::<_, String>(6)?)?,
        confidence: row.get(7)?,
        observed: row.get(8)?,
        reason: row.get(9)?,
        repair_hint: row.get(10)?,
        review_status: parse_enum(11, row.get::<_, String>(11)?)?,
        review_note: row.get(12)?,
        reviewed_at: row.get(13)?,
        created_at: row.get(14)?,
    })
}

fn parse_enum<T>(index: usize, value: String) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: Display + Send + Sync + 'static,
{
    value.parse::<T>().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            Box::new(ParseError(error.to_string())),
        )
    })
}

fn parse_json<T: DeserializeOwned>(index: usize, value: String) -> rusqlite::Result<T> {
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

#[derive(Debug)]
struct ParseError(String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}
