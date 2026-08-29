#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use rusqlite::Connection;

    #[test]
    fn custom_provider_storage_round_trips_without_secret_values() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let definition = CustomProviderDefinition {
            provider_id: "video_provider".into(),
            display_name: "Video".into(),
            base_url: "https://video.example.test".into(),
            purpose: CustomProviderPurpose::Video,
            api_key: Some("secret".into()),
            api_key_hint: None,
            models: vec![CustomProviderModel {
                id: "v1".into(),
                name: "Video 1".into(),
            }],
            headers: vec![CustomProviderHeader {
                name: "X-Org".into(),
                value: Some("header-secret".into()),
            }],
        };
        upsert_custom_provider(&conn, &definition).unwrap();
        let saved = get_custom_provider(&conn, "video_provider")
            .unwrap()
            .unwrap();
        assert_eq!(saved.provider_id, "video_provider");
        assert_eq!(saved.models[0].id, "v1");
        assert!(saved.api_key.is_none());
        assert!(saved.headers[0].value.is_none());

        let mut changed = definition.clone();
        changed.purpose = CustomProviderPurpose::Llm;
        upsert_custom_provider(&conn, &changed).unwrap();
        assert_eq!(
            get_custom_provider(&conn, "video_provider")
                .unwrap()
                .unwrap()
                .purpose,
            CustomProviderPurpose::Llm
        );
    }

    #[test]
    fn provider_storage_survives_reopen_and_keeps_attempts_immutable() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('p', 'Project', 'now', 'now', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, status, input_json, created_at, updated_at) VALUES ('run-1', 'p', 's', '1', 'o', 'ready_for_execution', '{}', 'now', 'now')",
            [],
        ).unwrap();

        let config = ProviderConfigRecord {
            provider_id: "openai".into(),
            enabled: true,
            credential_reference: Some("OPENAI_API_KEY".into()),
            default_model: Some("gpt-image-1".into()),
            endpoint: None,
            request_timeout_seconds: 30,
            polling_interval_seconds: 2,
        };
        upsert_provider_config(&conn, &config).unwrap();
        assert_eq!(get_provider_config(&conn, "openai").unwrap(), Some(config));

        let attempt = create_attempt(
            &conn,
            "run-1",
            "execute",
            1,
            "compiled-1",
            "openai",
            "gpt-image-1",
            "run-1:execute:1",
        )
        .unwrap();
        assert_eq!(attempt.attempt_number, 1);
        persist_job(&conn, &attempt.id, "openai", "job-1", "submitted").unwrap();
        let active = find_active_attempt(&conn, "run-1", "execute")
            .unwrap()
            .unwrap();
        assert_eq!(active.provider_job_id.as_deref(), Some("job-1"));
    }

    #[test]
    fn duplicate_idempotency_key_is_rejected() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('p', 'Project', 'now', 'now', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, status, input_json, created_at, updated_at) VALUES ('run', 'p', 's', '1', 'o', 'ready_for_execution', '{}', 'now', 'now')",
            [],
        ).unwrap();

        create_attempt(
            &conn, "run", "execute", 1, "compiled", "mock", "model", "same-key",
        )
        .unwrap();
        let result = create_attempt(
            &conn, "run", "execute", 2, "compiled", "mock", "model", "same-key",
        );
        assert!(result.is_err());
    }

    #[test]
    fn provider_attempt_records_materialized_artifact_ids() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('p', 'Project', 'now', 'now', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, status, input_json, created_at, updated_at) VALUES ('run', 'p', 's', '1', 'o', 'running', '{}', 'now', 'now')",
            [],
        ).unwrap();
        let attempt = create_attempt(
            &conn,
            "run",
            "execute",
            1,
            "compiled",
            "mock",
            "model",
            "artifact-key",
        )
        .unwrap();

        update_artifact_ids(
            &conn,
            &attempt.id,
            &["artifact-1".into(), "artifact-2".into()],
        )
        .unwrap();

        let saved: String = conn
            .query_row(
                "SELECT artifact_ids_json FROM workflow_step_executions WHERE id = ?1",
                [&attempt.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(saved, "[\"artifact-1\",\"artifact-2\"]");
    }
}
use super::model::{
    CustomProviderDefinition, CustomProviderHeader, CustomProviderModel, CustomProviderPurpose,
};
use crate::error::AppError;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigRecord {
    pub provider_id: String,
    pub enabled: bool,
    pub credential_reference: Option<String>,
    pub default_model: Option<String>,
    pub endpoint: Option<String>,
    pub request_timeout_seconds: i64,
    pub polling_interval_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionAttemptRecord {
    pub id: String,
    pub workflow_run_id: String,
    pub step_definition_id: String,
    pub attempt_number: i64,
    pub compiled_request_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub adapter_version: i64,
    pub idempotency_key: String,
    pub status: String,
    pub provider_job_id: Option<String>,
    pub normalized_error_json: Option<String>,
    pub artifact_ids_json: String,
    pub started_at: String,
    pub completed_at: Option<String>,
}

pub fn upsert_custom_provider(
    conn: &Connection,
    definition: &CustomProviderDefinition,
) -> Result<(), AppError> {
    definition
        .validate()
        .map_err(AppError::ProviderConfiguration)?;
    let metadata = definition.without_secrets();
    let models_json = serde_json::to_string(&metadata.models)
        .map_err(|error| AppError::Database(error.to_string()))?;
    let headers_json = serde_json::to_string(&metadata.headers)
        .map_err(|error| AppError::Database(error.to_string()))?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO custom_provider_definitions
         (provider_id, display_name, base_url, purpose, models_json, headers_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(provider_id) DO UPDATE SET
           display_name = excluded.display_name, base_url = excluded.base_url,
           purpose = excluded.purpose,
           models_json = excluded.models_json, headers_json = excluded.headers_json,
           updated_at = excluded.updated_at",
        params![
            metadata.provider_id,
            metadata.display_name,
            metadata.base_url,
            serde_json::to_string(&metadata.purpose).unwrap().trim_matches('"'),
            models_json,
            headers_json,
            now,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn get_custom_provider(
    conn: &Connection,
    provider_id: &str,
) -> Result<Option<CustomProviderDefinition>, AppError> {
    conn.query_row(
        "SELECT provider_id, display_name, base_url, purpose, models_json, headers_json
         FROM custom_provider_definitions WHERE provider_id = ?1",
        [provider_id],
        |row| {
            let purpose_text: String = row.get(3)?;
            let models_json: String = row.get(4)?;
            let headers_json: String = row.get(5)?;
            let models = serde_json::from_str::<Vec<CustomProviderModel>>(&models_json).map_err(
                |error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                },
            )?;
            let headers = serde_json::from_str::<Vec<CustomProviderHeader>>(&headers_json)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            let purpose =
                serde_json::from_str::<CustomProviderPurpose>(&format!("\"{purpose_text}\""))
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
            Ok(CustomProviderDefinition {
                provider_id: row.get(0)?,
                display_name: row.get(1)?,
                base_url: row.get(2)?,
                purpose,
                api_key: None,
                api_key_hint: None,
                models,
                headers,
            })
        },
    )
    .optional()
    .map_err(db_error)
}

pub fn list_custom_providers(conn: &Connection) -> Result<Vec<CustomProviderDefinition>, AppError> {
    let mut statement = conn
        .prepare(
            "SELECT provider_id, display_name, base_url, purpose, models_json, headers_json
         FROM custom_provider_definitions ORDER BY provider_id",
        )
        .map_err(db_error)?;
    let rows = statement
        .query_map([], |row| {
            let purpose_text: String = row.get(3)?;
            let purpose =
                serde_json::from_str::<CustomProviderPurpose>(&format!("\"{purpose_text}\""))
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
            let models: Vec<CustomProviderModel> = serde_json::from_str(&row.get::<_, String>(4)?)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            let headers: Vec<CustomProviderHeader> =
                serde_json::from_str(&row.get::<_, String>(5)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            Ok(CustomProviderDefinition {
                provider_id: row.get(0)?,
                display_name: row.get(1)?,
                base_url: row.get(2)?,
                purpose,
                api_key: None,
                api_key_hint: None,
                models,
                headers,
            })
        })
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub fn delete_custom_provider(conn: &Connection, provider_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM custom_provider_definitions WHERE provider_id = ?1",
        [provider_id],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn delete_provider_config(conn: &Connection, provider_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM provider_configurations WHERE provider_id = ?1",
        [provider_id],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn upsert_provider_config(
    conn: &Connection,
    config: &ProviderConfigRecord,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO provider_configurations
         (provider_id, enabled, credential_reference, default_model, endpoint,
          request_timeout_seconds, polling_interval_seconds, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(provider_id) DO UPDATE SET
           enabled = excluded.enabled,
           credential_reference = excluded.credential_reference,
           default_model = excluded.default_model,
           endpoint = excluded.endpoint,
           request_timeout_seconds = excluded.request_timeout_seconds,
           polling_interval_seconds = excluded.polling_interval_seconds,
           updated_at = excluded.updated_at",
        params![
            config.provider_id,
            config.enabled,
            config.credential_reference,
            config.default_model,
            config.endpoint,
            config.request_timeout_seconds,
            config.polling_interval_seconds,
            now,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn get_provider_config(
    conn: &Connection,
    provider_id: &str,
) -> Result<Option<ProviderConfigRecord>, AppError> {
    conn.query_row(
        "SELECT provider_id, enabled, credential_reference, default_model, endpoint,
                request_timeout_seconds, polling_interval_seconds
         FROM provider_configurations WHERE provider_id = ?1",
        [provider_id],
        row_to_provider_config,
    )
    .optional()
    .map_err(db_error)
}

pub fn create_attempt(
    conn: &Connection,
    workflow_run_id: &str,
    step_definition_id: &str,
    attempt_number: i64,
    compiled_request_id: &str,
    provider_id: &str,
    model_id: &str,
    idempotency_key: &str,
) -> Result<ExecutionAttemptRecord, AppError> {
    let record = ExecutionAttemptRecord {
        id: ulid::Ulid::new().to_string(),
        workflow_run_id: workflow_run_id.into(),
        step_definition_id: step_definition_id.into(),
        attempt_number,
        compiled_request_id: compiled_request_id.into(),
        provider_id: provider_id.into(),
        model_id: model_id.into(),
        adapter_version: 1,
        idempotency_key: idempotency_key.into(),
        status: "queued".into(),
        provider_job_id: None,
        normalized_error_json: None,
        artifact_ids_json: "[]".into(),
        started_at: Utc::now().to_rfc3339(),
        completed_at: None,
    };
    conn.execute(
        "INSERT INTO workflow_step_executions
         (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id,
          provider_id, model_id, adapter_version, idempotency_key, status,
          provider_job_id, normalized_error_json, artifact_ids_json, started_at, completed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            record.id,
            record.workflow_run_id,
            record.step_definition_id,
            record.attempt_number,
            record.compiled_request_id,
            record.provider_id,
            record.model_id,
            record.adapter_version,
            record.idempotency_key,
            record.status,
            record.provider_job_id,
            record.normalized_error_json,
            record.artifact_ids_json,
            record.started_at,
            record.completed_at,
        ],
    )
    .map_err(db_error)?;
    Ok(record)
}

pub fn persist_job(
    conn: &Connection,
    execution_id: &str,
    provider_id: &str,
    provider_job_id: &str,
    status: &str,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO provider_jobs
         (id, execution_id, provider_id, provider_job_id, status, submitted_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![
            ulid::Ulid::new().to_string(),
            execution_id,
            provider_id,
            provider_job_id,
            status,
            now
        ],
    )
    .map_err(db_error)?;
    conn.execute(
        "UPDATE workflow_step_executions SET provider_job_id = ?1, status = ?2 WHERE id = ?3",
        params![provider_job_id, status, execution_id],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn find_active_attempt(
    conn: &Connection,
    workflow_run_id: &str,
    step_definition_id: &str,
) -> Result<Option<ExecutionAttemptRecord>, AppError> {
    conn.query_row(
        "SELECT id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id,
                provider_id, model_id, adapter_version, idempotency_key, status,
                provider_job_id, normalized_error_json, artifact_ids_json, started_at, completed_at
         FROM workflow_step_executions
         WHERE workflow_run_id = ?1 AND step_definition_id = ?2
           AND status IN ('queued', 'submitted', 'running', 'cancellation_requested', 'unknown')
         ORDER BY attempt_number DESC LIMIT 1",
        params![workflow_run_id, step_definition_id],
        row_to_execution_attempt,
    )
    .optional()
    .map_err(db_error)
}

pub fn list_provider_configs(conn: &Connection) -> Result<Vec<ProviderConfigRecord>, AppError> {
    let mut statement = conn
        .prepare(
            "SELECT provider_id, enabled, credential_reference, default_model, endpoint,
                    request_timeout_seconds, polling_interval_seconds
             FROM provider_configurations ORDER BY provider_id",
        )
        .map_err(db_error)?;
    let result = statement
        .query_map([], row_to_provider_config)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error);
    result
}

pub fn update_attempt_status(
    conn: &Connection,
    execution_id: &str,
    status: &str,
    normalized_error_json: Option<&str>,
) -> Result<(), AppError> {
    let completed_at =
        matches!(status, "succeeded" | "failed" | "cancelled").then(|| Utc::now().to_rfc3339());
    conn.execute(
        "UPDATE workflow_step_executions
         SET status = ?1, normalized_error_json = ?2, completed_at = COALESCE(?3, completed_at)
         WHERE id = ?4",
        params![status, normalized_error_json, completed_at, execution_id],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn update_artifact_ids(
    conn: &Connection,
    execution_id: &str,
    artifact_ids: &[String],
) -> Result<(), AppError> {
    let artifact_ids_json = serde_json::to_string(artifact_ids)
        .map_err(|error| AppError::Database(error.to_string()))?;
    conn.execute(
        "UPDATE workflow_step_executions SET artifact_ids_json = ?1 WHERE id = ?2",
        params![artifact_ids_json, execution_id],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn next_attempt_number(
    conn: &Connection,
    workflow_run_id: &str,
    step_definition_id: &str,
) -> Result<i64, AppError> {
    conn.query_row(
        "SELECT COALESCE(MAX(attempt_number), 0) + 1
         FROM workflow_step_executions WHERE workflow_run_id = ?1 AND step_definition_id = ?2",
        params![workflow_run_id, step_definition_id],
        |row| row.get(0),
    )
    .map_err(db_error)
}

pub fn latest_attempt(
    conn: &Connection,
    workflow_run_id: &str,
    step_definition_id: &str,
) -> Result<Option<ExecutionAttemptRecord>, AppError> {
    conn.query_row(
        "SELECT id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id,
                provider_id, model_id, adapter_version, idempotency_key, status,
                provider_job_id, normalized_error_json, artifact_ids_json, started_at, completed_at
         FROM workflow_step_executions
         WHERE workflow_run_id = ?1 AND step_definition_id = ?2
         ORDER BY attempt_number DESC LIMIT 1",
        params![workflow_run_id, step_definition_id],
        row_to_execution_attempt,
    )
    .optional()
    .map_err(db_error)
}

pub fn append_audit_event(
    conn: &Connection,
    execution_id: Option<&str>,
    workflow_run_id: &str,
    event_type: &str,
    payload: Option<&serde_json::Value>,
) -> Result<(), AppError> {
    let payload_json = payload
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| AppError::Database(error.to_string()))?;
    conn.execute(
        "INSERT INTO provider_audit_events
         (id, execution_id, workflow_run_id, event_type, payload_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            ulid::Ulid::new().to_string(),
            execution_id,
            workflow_run_id,
            event_type,
            payload_json,
            Utc::now().to_rfc3339(),
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub fn list_attempt_summaries(
    conn: &Connection,
    workflow_run_id: &str,
) -> Result<Vec<crate::workflow::model::ProviderExecutionSummary>, AppError> {
    let mut statement = conn
        .prepare(
            "SELECT id, step_definition_id, attempt_number, provider_id, model_id,
                    adapter_version, status, provider_job_id, normalized_error_json,
                    started_at, completed_at
             FROM workflow_step_executions WHERE workflow_run_id = ?1 ORDER BY attempt_number",
        )
        .map_err(db_error)?;
    let result = statement
        .query_map([workflow_run_id], |row| {
            Ok(crate::workflow::model::ProviderExecutionSummary {
                id: row.get(0)?,
                step_definition_id: row.get(1)?,
                attempt_number: row.get(2)?,
                provider_id: row.get(3)?,
                model_id: row.get(4)?,
                adapter_version: row.get(5)?,
                status: row.get(6)?,
                provider_job_id: row.get(7)?,
                normalized_error_json: row.get(8)?,
                started_at: row.get(9)?,
                completed_at: row.get(10)?,
            })
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error);
    result
}

fn row_to_provider_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderConfigRecord> {
    Ok(ProviderConfigRecord {
        provider_id: row.get(0)?,
        enabled: row.get::<_, i64>(1)? != 0,
        credential_reference: row.get(2)?,
        default_model: row.get(3)?,
        endpoint: row.get(4)?,
        request_timeout_seconds: row.get(5)?,
        polling_interval_seconds: row.get(6)?,
    })
}

fn row_to_execution_attempt(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExecutionAttemptRecord> {
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
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}
