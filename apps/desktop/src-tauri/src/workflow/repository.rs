use crate::error::AppError;
use crate::workflow::model::{
    PrerequisiteReport, WorkflowEventRecord, WorkflowRunDetail, WorkflowRunRecord,
    WorkflowStepDefinition, WorkflowStepRecord,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::Value;
use ulid::Ulid;

pub struct WorkflowRepository;

impl WorkflowRepository {
    pub fn create_run(
        conn: &mut Connection,
        project_id: &str,
        skill_id: &str,
        skill_version: &str,
        operation_id: &str,
        input: Value,
        prerequisite_report: &PrerequisiteReport,
        steps: &[WorkflowStepDefinition],
    ) -> Result<String, AppError> {
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::Database(error.to_string()))?;
        let run_id = Ulid::new().to_string();
        let now = Utc::now().to_rfc3339();
        let input_json =
            serde_json::to_string(&input).map_err(|error| AppError::Database(error.to_string()))?;
        let prerequisite_report_json = serde_json::to_string(prerequisite_report)
            .map_err(|error| AppError::Database(error.to_string()))?;

        transaction
            .execute(
                "INSERT INTO workflow_runs
                    (id, project_id, skill_id, skill_version, operation_id, status,
                     input_json, prerequisite_report_json, current_step_index, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'created', ?6, ?7, 0, ?8, ?8)",
                params![
                    run_id,
                    project_id,
                    skill_id,
                    skill_version,
                    operation_id,
                    input_json,
                    prerequisite_report_json,
                    now,
                ],
            )
            .map_err(|error| AppError::Database(error.to_string()))?;

        for (step_index, step) in steps.iter().enumerate() {
            let step_json = serde_json::to_string(step)
                .map_err(|error| AppError::Database(error.to_string()))?;
            transaction
                .execute(
                    "INSERT INTO workflow_steps
                        (id, workflow_run_id, step_definition_id, step_index, step_type,
                         status, input_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
                    params![
                        Ulid::new().to_string(),
                        run_id,
                        step.id(),
                        step_index as i64,
                        step.step_type(),
                        step_json,
                    ],
                )
                .map_err(|error| AppError::Database(error.to_string()))?;
        }

        append_event_in_transaction(&transaction, &run_id, "run_created", None, None, &now)?;
        transaction
            .commit()
            .map_err(|error| AppError::Database(error.to_string()))?;
        Ok(run_id)
    }

    pub fn append_event(
        conn: &mut Connection,
        run_id: &str,
        event_type: &str,
        step_definition_id: Option<&str>,
        payload: Option<Value>,
    ) -> Result<WorkflowEventRecord, AppError> {
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AppError::Database(error.to_string()))?;
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE id = ?1)",
                [run_id],
                |row| row.get(0),
            )
            .map_err(|error| AppError::Database(error.to_string()))?;
        if !exists {
            return Err(AppError::WorkflowRunNotFound(run_id.to_string()));
        }

        let now = Utc::now().to_rfc3339();
        let payload_json = payload
            .map(|value| serde_json::to_string(&value))
            .transpose()
            .map_err(|error| AppError::Database(error.to_string()))?;
        let event = append_event_in_transaction(
            &transaction,
            run_id,
            event_type,
            step_definition_id,
            payload_json.as_deref(),
            &now,
        )?;
        transaction
            .commit()
            .map_err(|error| AppError::Database(error.to_string()))?;
        Ok(event)
    }

    pub fn get_run(
        conn: &Connection,
        project_id: &str,
        run_id: &str,
    ) -> Result<WorkflowRunDetail, AppError> {
        let run = conn
            .query_row(
                "SELECT id, project_id, skill_id, skill_version, operation_id, status,
                        input_json, prerequisite_report_json, context_snapshot_json,
                        current_step_index, failure_code, failure_message, created_at,
                        updated_at, completed_at
                 FROM workflow_runs WHERE id = ?1 AND project_id = ?2",
                params![run_id, project_id],
                map_run,
            )
            .optional()
            .map_err(|error| AppError::Database(error.to_string()))?
            .ok_or_else(|| AppError::WorkflowRunNotFound(run_id.to_string()))?;

        let mut step_statement = conn
            .prepare(
                "SELECT id, workflow_run_id, step_definition_id, step_index, step_type,
                        status, input_json, output_json, started_at, completed_at
                 FROM workflow_steps WHERE workflow_run_id = ?1 ORDER BY step_index ASC",
            )
            .map_err(|error| AppError::Database(error.to_string()))?;
        let steps = step_statement
            .query_map([run_id], map_step)
            .map_err(|error| AppError::Database(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::Database(error.to_string()))?;

        let mut event_statement = conn
            .prepare(
                "SELECT id, workflow_run_id, sequence, type, step_definition_id,
                        payload_json, created_at
                 FROM workflow_events WHERE workflow_run_id = ?1 ORDER BY sequence ASC",
            )
            .map_err(|error| AppError::Database(error.to_string()))?;
        let events = event_statement
            .query_map([run_id], map_event)
            .map_err(|error| AppError::Database(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::Database(error.to_string()))?;

        Ok(WorkflowRunDetail { run, steps, events })
    }

    pub fn list_runs(
        conn: &Connection,
        project_id: &str,
    ) -> Result<Vec<WorkflowRunRecord>, AppError> {
        let mut statement = conn
            .prepare(
                "SELECT id, project_id, skill_id, skill_version, operation_id, status,
                        input_json, prerequisite_report_json, context_snapshot_json,
                        current_step_index, failure_code, failure_message, created_at,
                        updated_at, completed_at
                 FROM workflow_runs WHERE project_id = ?1 ORDER BY created_at DESC, id DESC",
            )
            .map_err(|error| AppError::Database(error.to_string()))?;
        let result = statement
            .query_map([project_id], map_run)
            .map_err(|error| AppError::Database(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::Database(error.to_string()));
        result
    }
}

pub(crate) fn append_event_in_transaction(
    transaction: &Transaction<'_>,
    run_id: &str,
    event_type: &str,
    step_definition_id: Option<&str>,
    payload_json: Option<&str>,
    created_at: &str,
) -> Result<WorkflowEventRecord, AppError> {
    let next_sequence: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_run_id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .map_err(|error| AppError::Database(error.to_string()))?;
    let event = WorkflowEventRecord {
        id: Ulid::new().to_string(),
        workflow_run_id: run_id.to_string(),
        sequence: next_sequence,
        event_type: event_type.to_string(),
        step_definition_id: step_definition_id.map(str::to_string),
        payload_json: payload_json.map(str::to_string),
        created_at: created_at.to_string(),
    };
    transaction
        .execute(
            "INSERT INTO workflow_events
                (id, workflow_run_id, sequence, type, step_definition_id, payload_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.id,
                event.workflow_run_id,
                event.sequence,
                event.event_type,
                event.step_definition_id,
                event.payload_json,
                event.created_at,
            ],
        )
        .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(event)
}

fn map_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowRunRecord> {
    Ok(WorkflowRunRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        skill_id: row.get(2)?,
        skill_version: row.get(3)?,
        operation_id: row.get(4)?,
        status: row.get(5)?,
        input_json: row.get(6)?,
        prerequisite_report_json: row.get(7)?,
        context_snapshot_json: row.get(8)?,
        current_step_index: row.get(9)?,
        failure_code: row.get(10)?,
        failure_message: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        completed_at: row.get(14)?,
    })
}

fn map_step(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowStepRecord> {
    Ok(WorkflowStepRecord {
        id: row.get(0)?,
        workflow_run_id: row.get(1)?,
        step_definition_id: row.get(2)?,
        step_index: row.get(3)?,
        step_type: row.get(4)?,
        status: row.get(5)?,
        input_json: row.get(6)?,
        output_json: row.get(7)?,
        started_at: row.get(8)?,
        completed_at: row.get(9)?,
    })
}

fn map_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowEventRecord> {
    Ok(WorkflowEventRecord {
        id: row.get(0)?,
        workflow_run_id: row.get(1)?,
        sequence: row.get(2)?,
        event_type: row.get(3)?,
        step_definition_id: row.get(4)?,
        payload_json: row.get(5)?,
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;

    fn connection_with_projects() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES (?1, ?2, ?3, ?3, 1)",
            ["project-1", "Red Door", "2026-08-28T00:00:00Z"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES (?1, ?2, ?3, ?3, 1)",
            ["project-2", "Blue Door", "2026-08-28T00:00:00Z"],
        )
        .unwrap();
        conn
    }

    fn steps() -> Vec<WorkflowStepDefinition> {
        vec![
            WorkflowStepDefinition::ValidateInput {
                id: "validate-input".to_string(),
            },
            WorkflowStepDefinition::Complete {
                id: "complete".to_string(),
            },
        ]
    }

    fn report() -> PrerequisiteReport {
        PrerequisiteReport {
            passed: true,
            checks: vec![],
        }
    }

    #[test]
    fn creates_run_steps_and_first_event_transactionally() {
        let mut conn = connection_with_projects();
        let run_id = WorkflowRepository::create_run(
            &mut conn,
            "project-1",
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            serde_json::json!({"characterEntityId": "character-1"}),
            &report(),
            &steps(),
        )
        .unwrap();

        let detail = WorkflowRepository::get_run(&conn, "project-1", &run_id).unwrap();
        assert_eq!(detail.run.project_id, "project-1");
        assert_eq!(detail.steps.len(), 2);
        assert!(detail.steps.iter().all(|step| step.status == "pending"));
        assert_eq!(detail.events.len(), 1);
        assert_eq!(detail.events[0].sequence, 1);
        assert_eq!(detail.events[0].event_type, "run_created");
    }

    #[test]
    fn appends_contiguous_events() {
        let mut conn = connection_with_projects();
        let run_id = WorkflowRepository::create_run(
            &mut conn,
            "project-1",
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            serde_json::json!({}),
            &report(),
            &steps(),
        )
        .unwrap();

        let second = WorkflowRepository::append_event(
            &mut conn,
            &run_id,
            "run_started",
            None,
            Some(serde_json::json!({"source": "test"})),
        )
        .unwrap();
        let third = WorkflowRepository::append_event(
            &mut conn,
            &run_id,
            "step_started",
            Some("validate-input"),
            None,
        )
        .unwrap();

        assert_eq!(second.sequence, 2);
        assert_eq!(third.sequence, 3);
        assert_eq!(third.step_definition_id.as_deref(), Some("validate-input"));
    }

    #[test]
    fn detail_lookup_cannot_cross_project_boundary() {
        let mut conn = connection_with_projects();
        let run_id = WorkflowRepository::create_run(
            &mut conn,
            "project-1",
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            serde_json::json!({}),
            &report(),
            &steps(),
        )
        .unwrap();

        let result = WorkflowRepository::get_run(&conn, "project-2", &run_id);
        assert!(matches!(result, Err(AppError::WorkflowRunNotFound(id)) if id == run_id));
    }

    #[test]
    fn appending_to_missing_run_returns_stable_not_found_error() {
        let mut conn = connection_with_projects();
        let result =
            WorkflowRepository::append_event(&mut conn, "missing-run", "run_started", None, None);

        assert!(matches!(result, Err(AppError::WorkflowRunNotFound(id)) if id == "missing-run"));
        let event_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM workflow_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(event_count, 0);
    }

    #[test]
    fn lists_runs_only_for_the_requested_project() {
        let mut conn = connection_with_projects();
        WorkflowRepository::create_run(
            &mut conn,
            "project-1",
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            serde_json::json!({}),
            &report(),
            &steps(),
        )
        .unwrap();
        WorkflowRepository::create_run(
            &mut conn,
            "project-2",
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            serde_json::json!({}),
            &report(),
            &steps(),
        )
        .unwrap();

        let project_one = WorkflowRepository::list_runs(&conn, "project-1").unwrap();
        let project_two = WorkflowRepository::list_runs(&conn, "project-2").unwrap();
        assert_eq!(project_one.len(), 1);
        assert_eq!(project_two.len(), 1);
        assert_eq!(project_one[0].project_id, "project-1");
        assert_eq!(project_two[0].project_id, "project-2");
    }
}
