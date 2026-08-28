use crate::db;
use crate::error::AppError;
use crate::project::repository::read_project;
use crate::skills::registry::SkillRegistry;
use crate::workflow::artifacts::{workflow_artifact_dir, write_run_artifacts};
use crate::workflow::compiler::{CharacterFaceLockCompiler, RequestCompiler};
use crate::workflow::context::resolve_character_face_lock_context;
use crate::workflow::executor::{DryRunExecutor, ExecutionExecutor};
use crate::workflow::model::{
    WorkflowCharacterOption, WorkflowContextSnapshot, WorkflowRunDetail, WorkflowRunRecord,
};
use crate::workflow::prerequisites::{evaluate_prerequisites, evaluate_tbd_guards};
use crate::workflow::repository::WorkflowRepository;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::Path;

pub struct WorkflowRuntime;

impl WorkflowRuntime {
    pub fn create_run(
        project_root: &Path,
        skill_id: &str,
        skill_version: &str,
        operation_id: &str,
        input: Value,
    ) -> Result<WorkflowRunDetail, AppError> {
        validate_face_lock_input(&input)?;
        let registry = SkillRegistry::builtin()?;
        let (_skill, operation) = registry.find_operation(skill_id, skill_version, operation_id)?;
        let mut conn = open_project(project_root)?;
        let project = read_project(&conn)?;
        let report = evaluate_prerequisites(&conn, &project.id, &input, &operation.prerequisites)?;
        if !report.passed {
            return Err(AppError::WorkflowPrerequisiteFailed(
                serde_json::to_string(&report).unwrap_or_else(|_| "prerequisite failed".into()),
            ));
        }
        let blocked = evaluate_tbd_guards(&conn, &project.id, &input, &operation.tbd_guards)?;
        if !blocked.is_empty() {
            return Err(AppError::WorkflowBlockedByProtectedTbd(blocked.join(", ")));
        }
        let run_id = WorkflowRepository::create_run(
            &mut conn,
            &project.id,
            skill_id,
            skill_version,
            operation_id,
            input.clone(),
            &operation.workflow,
        )?;
        conn.execute(
            "UPDATE workflow_runs SET prerequisite_report_json = ?1 WHERE id = ?2",
            params![
                serde_json::to_string(&report)
                    .map_err(|error| AppError::Database(error.to_string()))?,
                run_id
            ],
        )
        .map_err(db_error)?;
        WorkflowRepository::get_run(&conn, &project.id, &run_id)
    }

    pub fn advance_run(
        project_root: &Path,
        workflow_run_id: &str,
    ) -> Result<WorkflowRunDetail, AppError> {
        let mut conn = open_project(project_root)?;
        let project = read_project(&conn)?;
        let registry = SkillRegistry::builtin()?;
        let detail = WorkflowRepository::get_run(&conn, &project.id, workflow_run_id)?;
        if matches!(
            detail.run.status.as_str(),
            "completed" | "rejected" | "cancelled" | "failed"
        ) {
            return Err(AppError::WorkflowRunTerminal);
        }
        if detail.run.status == "waiting_for_approval" {
            return Err(AppError::WorkflowApprovalRequired);
        }
        let (skill, operation) = registry.find_operation(
            &detail.run.skill_id,
            &detail.run.skill_version,
            &detail.run.operation_id,
        )?;
        if detail.run.status == "ready_for_execution" {
            return execute_ready(&mut conn, project_root, &project.id, detail);
        }
        set_run_status(&conn, workflow_run_id, "running", None)?;
        if detail.run.status == "created" {
            WorkflowRepository::append_event(
                &mut conn,
                workflow_run_id,
                "run_started",
                None,
                None,
            )?;
        }
        let mut index = detail.run.current_step_index as usize;
        while index < operation.workflow.len() {
            let step = &operation.workflow[index];
            let step_id = step.id();
            mark_step(&conn, workflow_run_id, index as i64, "running", None)?;
            WorkflowRepository::append_event(
                &mut conn,
                workflow_run_id,
                "step_started",
                Some(step_id),
                None,
            )?;
            match step {
                crate::workflow::model::WorkflowStepDefinition::ValidateInput { .. } => {
                    mark_step(&conn, workflow_run_id, index as i64, "completed", None)?;
                    WorkflowRepository::append_event(
                        &mut conn,
                        workflow_run_id,
                        "step_completed",
                        Some(step_id),
                        None,
                    )?;
                }
                crate::workflow::model::WorkflowStepDefinition::ResolveContext { .. } => {
                    let input: Value = serde_json::from_str(&detail.run.input_json)
                        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
                    let report: crate::workflow::model::PrerequisiteReport = serde_json::from_str(
                        detail
                            .run
                            .prerequisite_report_json
                            .as_deref()
                            .ok_or_else(|| {
                                AppError::WorkflowRunInconsistent(
                                    "missing prerequisite report".into(),
                                )
                            })?,
                    )
                    .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
                    let context = resolve_character_face_lock_context(
                        &conn,
                        &project.id,
                        &detail.run.skill_id,
                        &detail.run.skill_version,
                        &detail.run.operation_id,
                        &input,
                        report,
                    )?;
                    conn.execute("UPDATE workflow_runs SET context_snapshot_json = ?1, updated_at = ?2 WHERE id = ?3", params![serde_json::to_string(&context).map_err(|error| AppError::Database(error.to_string()))?, Utc::now().to_rfc3339(), workflow_run_id]).map_err(db_error)?;
                    mark_step(&conn, workflow_run_id, index as i64, "completed", None)?;
                    WorkflowRepository::append_event(
                        &mut conn,
                        workflow_run_id,
                        "step_completed",
                        Some(step_id),
                        None,
                    )?;
                }
                crate::workflow::model::WorkflowStepDefinition::CompileRequest {
                    compiler_id,
                    ..
                } => {
                    if compiler_id != "character_face_lock_v1" {
                        return Err(AppError::WorkflowCompilerNotFound(compiler_id.clone()));
                    }
                    let context: WorkflowContextSnapshot =
                        serde_json::from_str(&load_context(&conn, workflow_run_id)?).map_err(
                            |error| AppError::WorkflowRunInconsistent(error.to_string()),
                        )?;
                    let request = CharacterFaceLockCompiler.compile(
                        workflow_run_id,
                        skill,
                        operation,
                        &context,
                    )?;
                    write_run_artifacts(project_root, workflow_run_id, &context, &request)?;
                    mark_step(
                        &conn,
                        workflow_run_id,
                        index as i64,
                        "completed",
                        Some(
                            &serde_json::to_string(&request)
                                .map_err(|error| AppError::Database(error.to_string()))?,
                        ),
                    )?;
                    WorkflowRepository::append_event(
                        &mut conn,
                        workflow_run_id,
                        "step_completed",
                        Some(step_id),
                        None,
                    )?;
                }
                crate::workflow::model::WorkflowStepDefinition::Approval { .. } => {
                    mark_step(&conn, workflow_run_id, index as i64, "waiting", None)?;
                    conn.execute("UPDATE workflow_runs SET status = 'waiting_for_approval', current_step_index = ?1, updated_at = ?2 WHERE id = ?3", params![index as i64, Utc::now().to_rfc3339(), workflow_run_id]).map_err(db_error)?;
                    WorkflowRepository::append_event(
                        &mut conn,
                        workflow_run_id,
                        "approval_requested",
                        Some(step_id),
                        None,
                    )?;
                    return WorkflowRepository::get_run(&conn, &project.id, workflow_run_id);
                }
                crate::workflow::model::WorkflowStepDefinition::Execute { .. } => {
                    return Err(AppError::WorkflowApprovalRequired)
                }
                crate::workflow::model::WorkflowStepDefinition::Complete { .. } => {
                    mark_step(&conn, workflow_run_id, index as i64, "completed", None)?;
                    conn.execute("UPDATE workflow_runs SET status = 'completed', current_step_index = ?1, completed_at = ?2, updated_at = ?2 WHERE id = ?3", params![(index + 1) as i64, Utc::now().to_rfc3339(), workflow_run_id]).map_err(db_error)?;
                    WorkflowRepository::append_event(
                        &mut conn,
                        workflow_run_id,
                        "run_completed",
                        Some(step_id),
                        None,
                    )?;
                    return WorkflowRepository::get_run(&conn, &project.id, workflow_run_id);
                }
            }
            index += 1;
            conn.execute(
                "UPDATE workflow_runs SET current_step_index = ?1, updated_at = ?2 WHERE id = ?3",
                params![index as i64, Utc::now().to_rfc3339(), workflow_run_id],
            )
            .map_err(db_error)?;
        }
        WorkflowRepository::get_run(&conn, &project.id, workflow_run_id)
    }

    pub fn approve_run_step(
        project_root: &Path,
        workflow_run_id: &str,
        step_definition_id: &str,
        note: Option<String>,
    ) -> Result<WorkflowRunDetail, AppError> {
        let mut conn = open_project(project_root)?;
        let project = read_project(&conn)?;
        let detail = WorkflowRepository::get_run(&conn, &project.id, workflow_run_id)?;
        if approval_exists(&conn, workflow_run_id, step_definition_id)? {
            return Err(AppError::WorkflowApprovalAlreadyDecided(
                step_definition_id.into(),
            ));
        }
        if detail.run.status != "waiting_for_approval" {
            return Err(AppError::WorkflowApprovalRequired);
        }
        let step = detail
            .steps
            .iter()
            .find(|step| step.step_definition_id == step_definition_id)
            .ok_or_else(|| AppError::WorkflowStepNotFound(step_definition_id.into()))?;
        let artifact_json = detail
            .steps
            .iter()
            .find(|candidate| candidate.step_type == "compile_request")
            .and_then(|candidate| candidate.output_json.clone())
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent("compiled request is missing".into())
            })?;
        conn.execute("INSERT INTO workflow_approvals (id, workflow_run_id, step_definition_id, decision, artifact_json, note, created_at) VALUES (?1, ?2, ?3, 'approved', ?4, ?5, ?6)", params![ulid::Ulid::new().to_string(), workflow_run_id, step_definition_id, artifact_json, note, Utc::now().to_rfc3339()]).map_err(|error| if error.to_string().contains("UNIQUE") { AppError::WorkflowApprovalAlreadyDecided(step_definition_id.into()) } else { db_error(error) })?;
        mark_step(&conn, workflow_run_id, step.step_index, "completed", None)?;
        conn.execute("UPDATE workflow_runs SET status = 'ready_for_execution', current_step_index = ?1, updated_at = ?2 WHERE id = ?3", params![step.step_index + 1, Utc::now().to_rfc3339(), workflow_run_id]).map_err(db_error)?;
        WorkflowRepository::append_event(
            &mut conn,
            workflow_run_id,
            "approval_granted",
            Some(step_definition_id),
            None,
        )?;
        WorkflowRepository::append_event(
            &mut conn,
            workflow_run_id,
            "step_completed",
            Some(step_definition_id),
            None,
        )?;
        WorkflowRepository::get_run(&conn, &project.id, workflow_run_id)
    }

    pub fn reject_run_step(
        project_root: &Path,
        workflow_run_id: &str,
        step_definition_id: &str,
        note: Option<String>,
    ) -> Result<WorkflowRunDetail, AppError> {
        let mut conn = open_project(project_root)?;
        let project = read_project(&conn)?;
        let detail = WorkflowRepository::get_run(&conn, &project.id, workflow_run_id)?;
        if approval_exists(&conn, workflow_run_id, step_definition_id)? {
            return Err(AppError::WorkflowApprovalAlreadyDecided(
                step_definition_id.into(),
            ));
        }
        if detail.run.status != "waiting_for_approval" {
            return Err(AppError::WorkflowApprovalRequired);
        }
        conn.execute("INSERT INTO workflow_approvals (id, workflow_run_id, step_definition_id, decision, artifact_json, note, created_at) VALUES (?1, ?2, ?3, 'rejected', '{}', ?4, ?5)", params![ulid::Ulid::new().to_string(), workflow_run_id, step_definition_id, note, Utc::now().to_rfc3339()]).map_err(db_error)?;
        mark_step(
            &conn,
            workflow_run_id,
            detail.run.current_step_index,
            "completed",
            None,
        )?;
        conn.execute("UPDATE workflow_steps SET status = 'skipped' WHERE workflow_run_id = ?1 AND step_index > ?2 AND status = 'pending'", params![workflow_run_id, detail.run.current_step_index]).map_err(db_error)?;
        conn.execute("UPDATE workflow_runs SET status = 'rejected', current_step_index = ?1, completed_at = ?2, updated_at = ?2 WHERE id = ?3", params![detail.steps.len(), Utc::now().to_rfc3339(), workflow_run_id]).map_err(db_error)?;
        WorkflowRepository::append_event(
            &mut conn,
            workflow_run_id,
            "approval_rejected",
            Some(step_definition_id),
            None,
        )?;
        WorkflowRepository::get_run(&conn, &project.id, workflow_run_id)
    }

    pub fn cancel_run(
        project_root: &Path,
        workflow_run_id: &str,
    ) -> Result<WorkflowRunDetail, AppError> {
        let mut conn = open_project(project_root)?;
        let project = read_project(&conn)?;
        let detail = WorkflowRepository::get_run(&conn, &project.id, workflow_run_id)?;
        if matches!(
            detail.run.status.as_str(),
            "completed" | "rejected" | "cancelled" | "failed"
        ) {
            return Err(AppError::WorkflowRunTerminal);
        }
        conn.execute("UPDATE workflow_steps SET status = 'skipped' WHERE workflow_run_id = ?1 AND status IN ('pending', 'waiting')", [workflow_run_id]).map_err(db_error)?;
        conn.execute("UPDATE workflow_runs SET status = 'cancelled', completed_at = ?1, updated_at = ?1 WHERE id = ?2", params![Utc::now().to_rfc3339(), workflow_run_id]).map_err(db_error)?;
        WorkflowRepository::append_event(&mut conn, workflow_run_id, "run_cancelled", None, None)?;
        WorkflowRepository::get_run(&conn, &project.id, workflow_run_id)
    }

    pub fn get_run(
        project_root: &Path,
        workflow_run_id: &str,
    ) -> Result<WorkflowRunDetail, AppError> {
        let conn = open_project(project_root)?;
        let project = read_project(&conn)?;
        WorkflowRepository::get_run(&conn, &project.id, workflow_run_id)
    }
    pub fn list_runs(project_root: &Path) -> Result<Vec<WorkflowRunRecord>, AppError> {
        let conn = open_project(project_root)?;
        let project = read_project(&conn)?;
        WorkflowRepository::list_runs(&conn, &project.id)
    }
    pub fn list_characters(project_root: &Path) -> Result<Vec<WorkflowCharacterOption>, AppError> {
        let conn = open_project(project_root)?;
        let project = read_project(&conn)?;
        let mut statement = conn.prepare("SELECT id, name FROM canon_entities WHERE project_id = ?1 AND type = 'character' ORDER BY name COLLATE NOCASE, id").map_err(db_error)?;
        let characters = statement
            .query_map([project.id], |row| {
                Ok(WorkflowCharacterOption {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        Ok(characters)
    }
}

fn execute_ready(
    conn: &mut Connection,
    project_root: &Path,
    project_id: &str,
    detail: WorkflowRunDetail,
) -> Result<WorkflowRunDetail, AppError> {
    let execute_index = detail
        .steps
        .iter()
        .find(|step| step.step_type == "execute" && step.status == "pending")
        .map(|step| step.step_index)
        .ok_or_else(|| AppError::WorkflowRunInconsistent("execute step is missing".into()))?;
    let request_json = detail
        .steps
        .iter()
        .find(|step| step.step_type == "compile_request")
        .and_then(|step| step.output_json.clone())
        .ok_or_else(|| AppError::WorkflowRunInconsistent("compiled request is missing".into()))?;
    let request = serde_json::from_str(&request_json)
        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
    mark_step(conn, &detail.run.id, execute_index, "running", None)?;
    WorkflowRepository::append_event(conn, &detail.run.id, "step_started", Some("execute"), None)?;
    WorkflowRepository::append_event(
        conn,
        &detail.run.id,
        "execution_started",
        Some("execute"),
        None,
    )?;
    let result = DryRunExecutor.execute(
        &request,
        &workflow_artifact_dir(project_root, &detail.run.id),
    )?;
    mark_step(
        conn,
        &detail.run.id,
        execute_index,
        "completed",
        Some(
            &serde_json::to_string(&result)
                .map_err(|error| AppError::Database(error.to_string()))?,
        ),
    )?;
    WorkflowRepository::append_event(
        conn,
        &detail.run.id,
        "execution_completed",
        Some("execute"),
        None,
    )?;
    WorkflowRepository::append_event(
        conn,
        &detail.run.id,
        "step_completed",
        Some("execute"),
        None,
    )?;
    let complete_index = execute_index + 1;
    mark_step(conn, &detail.run.id, complete_index, "running", None)?;
    WorkflowRepository::append_event(conn, &detail.run.id, "step_started", Some("complete"), None)?;
    mark_step(conn, &detail.run.id, complete_index, "completed", None)?;
    conn.execute("UPDATE workflow_runs SET status = 'completed', current_step_index = ?1, completed_at = ?2, updated_at = ?2 WHERE id = ?3", params![complete_index + 1, Utc::now().to_rfc3339(), detail.run.id]).map_err(db_error)?;
    WorkflowRepository::append_event(
        conn,
        &detail.run.id,
        "step_completed",
        Some("complete"),
        None,
    )?;
    WorkflowRepository::append_event(
        conn,
        &detail.run.id,
        "run_completed",
        Some("complete"),
        None,
    )?;
    WorkflowRepository::get_run(conn, project_id, &detail.run.id)
}

fn open_project(root: &Path) -> Result<Connection, AppError> {
    db::open_existing_connection(&root.join("project.db"))
}
fn load_context(conn: &Connection, run_id: &str) -> Result<String, AppError> {
    conn.query_row(
        "SELECT context_snapshot_json FROM workflow_runs WHERE id = ?1",
        [run_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .map_err(db_error)?
    .ok_or_else(|| AppError::WorkflowRunInconsistent("context snapshot is missing".into()))
}
fn set_run_status(
    conn: &Connection,
    run_id: &str,
    status: &str,
    failure: Option<&str>,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE workflow_runs SET status = ?1, failure_message = ?2, updated_at = ?3 WHERE id = ?4",
        params![status, failure, Utc::now().to_rfc3339(), run_id],
    )
    .map_err(db_error)?;
    Ok(())
}
fn mark_step(
    conn: &Connection,
    run_id: &str,
    index: i64,
    status: &str,
    output_json: Option<&str>,
) -> Result<(), AppError> {
    conn.execute("UPDATE workflow_steps SET status = ?1, output_json = COALESCE(?2, output_json), started_at = CASE WHEN ?1 = 'running' THEN COALESCE(started_at, ?3) ELSE started_at END, completed_at = CASE WHEN ?1 IN ('completed', 'skipped', 'failed') THEN ?3 ELSE completed_at END WHERE workflow_run_id = ?4 AND step_index = ?5", params![status, output_json, Utc::now().to_rfc3339(), run_id, index]).map_err(db_error)?;
    Ok(())
}
fn approval_exists(
    conn: &Connection,
    run_id: &str,
    step_definition_id: &str,
) -> Result<bool, AppError> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM workflow_approvals WHERE workflow_run_id = ?1 AND step_definition_id = ?2)", params![run_id, step_definition_id], |row| row.get(0)).map_err(db_error)
}
fn validate_face_lock_input(input: &Value) -> Result<(), AppError> {
    for key in ["projectRootPath", "characterEntityId", "baselineWardrobe"] {
        if input
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .is_none()
        {
            return Err(AppError::WorkflowInputInvalid(format!(
                "{key} must be a non-empty string"
            )));
        }
    }
    let spec = input
        .get("visualSpec")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::WorkflowInputInvalid("visualSpec is required".into()))?;
    for key in [
        "head",
        "eyes",
        "brows",
        "nose",
        "lips",
        "skin",
        "hair",
        "build",
        "expression",
    ] {
        if spec
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .is_none()
        {
            return Err(AppError::WorkflowInputInvalid(format!(
                "visualSpec.{key} must be a non-empty string"
            )));
        }
    }
    Ok(())
}
fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}
