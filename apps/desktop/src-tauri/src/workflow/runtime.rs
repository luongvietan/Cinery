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
use crate::workflow::repository::{append_event_in_transaction, WorkflowRepository};
use crate::providers::repository::{append_audit_event, create_attempt, next_attempt_number, persist_job, update_artifact_ids, update_attempt_status};
use crate::providers::service::ProviderService;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::Value;
use sha2::{Digest, Sha256};
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
        let registry = SkillRegistry::builtin()?;
        let (_skill, operation) = registry.find_operation(skill_id, skill_version, operation_id)?;
        Self::create_run_for_operation(
            project_root,
            skill_id,
            skill_version,
            operation_id,
            input,
            operation,
        )
    }

    fn create_run_for_operation(
        project_root: &Path,
        skill_id: &str,
        skill_version: &str,
        operation_id: &str,
        input: Value,
        operation: &crate::skills::model::SkillOperation,
    ) -> Result<WorkflowRunDetail, AppError> {
        match operation.input_schema_id.as_str() {
            "create_face_lock" => validate_face_lock_input(&input)?,
            "run_visual_qa" => validate_visual_qa_input(&input)?,
            schema_id => {
                return Err(AppError::WorkflowInputInvalid(format!(
                    "unsupported input schema: {schema_id}"
                )))
            }
        }
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
            &report,
            &operation.workflow,
        )?;
        WorkflowRepository::get_run(&conn, &project.id, &run_id)
    }

    pub fn advance_run(
        project_root: &Path,
        workflow_run_id: &str,
    ) -> Result<WorkflowRunDetail, AppError> {
        let result = Self::advance_run_inner(project_root, workflow_run_id);
        if let Err(error) = &result {
            let _ = finalize_run_failure_if_running(project_root, workflow_run_id, error);
        }
        result
    }

    fn advance_run_inner(
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
            return execute_ready(&mut conn, project_root, &project.id, detail, operation);
        }
        start_run(&mut conn, workflow_run_id, detail.run.status == "created")?;
        let mut index = detail.run.current_step_index as usize;
        while index < operation.workflow.len() {
            let step = &operation.workflow[index];
            let step_id = step.id();
            start_step(&mut conn, workflow_run_id, index as i64, step_id)?;
            match step {
                crate::workflow::model::WorkflowStepDefinition::ValidateInput { .. } => {
                    complete_step(&mut conn, workflow_run_id, index as i64, step_id, None)?;
                }
                crate::workflow::model::WorkflowStepDefinition::ResolveContext {
                    resolver_id,
                    ..
                } => {
                    let input: Value = serde_json::from_str(&detail.run.input_json)
                        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
                    let context_json = match resolver_id.as_str() {
                        "character_face_lock_context" => {
                            let report: crate::workflow::model::PrerequisiteReport =
                                serde_json::from_str(
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
                                .map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            let context = resolve_character_face_lock_context(
                                &conn,
                                &project.id,
                                &detail.run.skill_id,
                                &detail.run.skill_version,
                                &detail.run.operation_id,
                                &input,
                                report,
                            )?;
                            serde_json::to_string(&context)
                                .map_err(|error| AppError::Database(error.to_string()))?
                        }
                        "visual_qa_context" => {
                            let context = crate::qa::workflow::resolve_and_persist(
                                &conn,
                                &project.id,
                                workflow_run_id,
                                &input,
                            )?;
                            serde_json::to_string(&context)
                                .map_err(|error| AppError::Database(error.to_string()))?
                        }
                        _ => return Err(AppError::WorkflowResolverNotFound(resolver_id.clone())),
                    };
                    complete_context_step(
                        &mut conn,
                        workflow_run_id,
                        index as i64,
                        step_id,
                        &context_json,
                    )?;
                }
                crate::workflow::model::WorkflowStepDefinition::CompileRequest {
                    compiler_id,
                    ..
                } => {
                    let context_json = load_context(&conn, workflow_run_id)?;
                    let request_json = match compiler_id.as_str() {
                        "character_face_lock_v1" => {
                            let context: WorkflowContextSnapshot =
                                serde_json::from_str(&context_json).map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            let request = CharacterFaceLockCompiler.compile(
                                workflow_run_id,
                                skill,
                                operation,
                                &context,
                            )?;
                            write_run_artifacts(
                                project_root,
                                workflow_run_id,
                                &context,
                                &request,
                            )?;
                            serde_json::to_string(&request)
                                .map_err(|error| AppError::Database(error.to_string()))?
                        }
                        "visual_qa_v1" => {
                            let context: crate::qa::workflow::QaWorkflowContext =
                                serde_json::from_str(&context_json).map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            serde_json::to_string(&crate::qa::workflow::compile_request(
                                project_root,
                                &context,
                            ))
                            .map_err(|error| AppError::Database(error.to_string()))?
                        }
                        _ => return Err(AppError::WorkflowCompilerNotFound(compiler_id.clone())),
                    };
                    complete_step(
                        &mut conn,
                        workflow_run_id,
                        index as i64,
                        step_id,
                        Some(&request_json),
                    )?;
                }
                crate::workflow::model::WorkflowStepDefinition::Approval { .. } => {
                    enter_approval(&mut conn, workflow_run_id, index as i64, step_id)?;
                    return WorkflowRepository::get_run(&conn, &project.id, workflow_run_id);
                }
                crate::workflow::model::WorkflowStepDefinition::Execute { .. } => {
                    return Err(AppError::WorkflowApprovalRequired)
                }
                crate::workflow::model::WorkflowStepDefinition::Complete { .. } => {
                    complete_run(&mut conn, workflow_run_id, index as i64, step_id)?;
                    return WorkflowRepository::get_run(&conn, &project.id, workflow_run_id);
                }
            }
            index += 1;
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
        let step = active_approval_step(&detail, step_definition_id)?;
        let artifact_json = detail
            .steps
            .iter()
            .find(|candidate| candidate.step_type == "compile_request")
            .and_then(|candidate| candidate.output_json.clone())
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent("compiled request is missing".into())
            })?;
        let step_index = step.step_index;
        let now = Utc::now().to_rfc3339();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        tx.execute("INSERT INTO workflow_approvals (id, workflow_run_id, step_definition_id, decision, artifact_json, note, created_at) VALUES (?1, ?2, ?3, 'approved', ?4, ?5, ?6)", params![ulid::Ulid::new().to_string(), workflow_run_id, step_definition_id, artifact_json, note, now]).map_err(|error| if error.to_string().contains("UNIQUE") { AppError::WorkflowApprovalAlreadyDecided(step_definition_id.into()) } else { db_error(error) })?;
        mark_step(&tx, workflow_run_id, step_index, "completed", None)?;
        tx.execute("UPDATE workflow_runs SET status = 'ready_for_execution', current_step_index = ?1, updated_at = ?2 WHERE id = ?3", params![step_index + 1, now, workflow_run_id]).map_err(db_error)?;
        append_event_in_transaction(
            &tx,
            workflow_run_id,
            "approval_granted",
            Some(step_definition_id),
            None,
            &now,
        )?;
        append_event_in_transaction(
            &tx,
            workflow_run_id,
            "step_completed",
            Some(step_definition_id),
            None,
            &now,
        )?;
        tx.commit().map_err(db_error)?;
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
        let step_index = active_approval_step(&detail, step_definition_id)?.step_index;
        let artifact_json = detail
            .steps
            .iter()
            .find(|candidate| candidate.step_type == "compile_request")
            .and_then(|candidate| candidate.output_json.clone())
            .ok_or_else(|| {
                AppError::WorkflowRunInconsistent("compiled request is missing".into())
            })?;
        let now = Utc::now().to_rfc3339();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        tx.execute("INSERT INTO workflow_approvals (id, workflow_run_id, step_definition_id, decision, artifact_json, note, created_at) VALUES (?1, ?2, ?3, 'rejected', ?4, ?5, ?6)", params![ulid::Ulid::new().to_string(), workflow_run_id, step_definition_id, artifact_json, note, now]).map_err(db_error)?;
        mark_step(&tx, workflow_run_id, step_index, "completed", None)?;
        tx.execute("UPDATE workflow_steps SET status = 'skipped', completed_at = ?1 WHERE workflow_run_id = ?2 AND step_index > ?3 AND status = 'pending'", params![now, workflow_run_id, step_index]).map_err(db_error)?;
        tx.execute("UPDATE workflow_runs SET status = 'rejected', current_step_index = ?1, completed_at = ?2, updated_at = ?2 WHERE id = ?3", params![detail.steps.len(), now, workflow_run_id]).map_err(db_error)?;
        append_event_in_transaction(
            &tx,
            workflow_run_id,
            "approval_rejected",
            Some(step_definition_id),
            None,
            &now,
        )?;
        tx.commit().map_err(db_error)?;
        crate::qa::repository::cancel_for_workflow(&conn, workflow_run_id, &now)?;
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
        let now = Utc::now().to_rfc3339();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        tx.execute("UPDATE workflow_steps SET status = 'skipped', completed_at = ?1 WHERE workflow_run_id = ?2 AND status IN ('pending', 'waiting')", params![now, workflow_run_id]).map_err(db_error)?;
        tx.execute("UPDATE workflow_runs SET status = 'cancelled', completed_at = ?1, updated_at = ?1 WHERE id = ?2", params![now, workflow_run_id]).map_err(db_error)?;
        append_event_in_transaction(&tx, workflow_run_id, "run_cancelled", None, None, &now)?;
        tx.commit().map_err(db_error)?;
        crate::qa::repository::cancel_for_workflow(&conn, workflow_run_id, &now)?;
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

fn execute_visual_qa_ready(
    conn: &mut Connection,
    project_id: &str,
    detail: WorkflowRunDetail,
    operation: &crate::skills::model::SkillOperation,
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
    let compiled: crate::qa::workflow::CompiledVisualQaRequest =
        serde_json::from_str(&request_json)
            .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
    let (execute_step_id, complete_step_id) = match (
        operation.workflow.get(execute_index as usize),
        operation.workflow.get(execute_index as usize + 1),
    ) {
        (
            Some(crate::workflow::model::WorkflowStepDefinition::Execute { id, .. }),
            Some(crate::workflow::model::WorkflowStepDefinition::Complete { id: complete_id }),
        ) => (id.as_str(), complete_id.as_str()),
        _ => {
            return Err(AppError::WorkflowRunInconsistent(
                "visual QA execute/complete definitions are missing".into(),
            ))
        }
    };
    let started_at = Utc::now().to_rfc3339();
    let execution_payload = serde_json::json!({
        "qaRunId": compiled.qa_run_id,
        "executionLocation": compiled.execution_location,
        "adapterId": compiled.adapter_id,
        "modelId": compiled.model_id,
    })
    .to_string();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    tx.execute(
        "UPDATE workflow_runs SET status = 'running', updated_at = ?1 WHERE id = ?2",
        params![started_at, detail.run.id],
    )
    .map_err(db_error)?;
    mark_step(&tx, &detail.run.id, execute_index, "running", None)?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "step_started",
        Some(execute_step_id),
        None,
        &started_at,
    )?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "execution_started",
        Some(execute_step_id),
        Some(&execution_payload),
        &started_at,
    )?;
    tx.commit().map_err(db_error)?;

    let input: Value = serde_json::from_str(&detail.run.input_json)
        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
    let result = crate::qa::workflow::execute(conn, &input, &compiled)?;
    let result_json =
        serde_json::to_string(&result).map_err(|error| AppError::Database(error.to_string()))?;
    let complete_index = execute_index + 1;
    let completed_at = Utc::now().to_rfc3339();
    let completed_payload = serde_json::json!({"qaRunId": result.qa_run_id}).to_string();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    mark_step(
        &tx,
        &detail.run.id,
        execute_index,
        "completed",
        Some(&result_json),
    )?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "execution_completed",
        Some(execute_step_id),
        Some(&completed_payload),
        &completed_at,
    )?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "step_completed",
        Some(execute_step_id),
        None,
        &completed_at,
    )?;
    mark_step(&tx, &detail.run.id, complete_index, "running", None)?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "step_started",
        Some(complete_step_id),
        None,
        &completed_at,
    )?;
    mark_step(&tx, &detail.run.id, complete_index, "completed", None)?;
    tx.execute(
        "UPDATE workflow_runs
         SET status = 'completed', current_step_index = ?1, completed_at = ?2, updated_at = ?2
         WHERE id = ?3",
        params![complete_index + 1, completed_at, detail.run.id],
    )
    .map_err(db_error)?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "step_completed",
        Some(complete_step_id),
        None,
        &completed_at,
    )?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "run_completed",
        Some(complete_step_id),
        None,
        &completed_at,
    )?;
    tx.commit().map_err(db_error)?;
    WorkflowRepository::get_run(conn, project_id, &detail.run.id)
}

fn execute_ready(
    conn: &mut Connection,
    project_root: &Path,
    project_id: &str,
    detail: WorkflowRunDetail,
    operation: &crate::skills::model::SkillOperation,
) -> Result<WorkflowRunDetail, AppError> {
    if operation.id == "asset.run_visual_qa" {
        return execute_visual_qa_ready(conn, project_id, detail, operation);
    }
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
    let (execute_step_id, complete_step_id) = match (
        operation.workflow.get(execute_index as usize),
        operation.workflow.get(execute_index as usize + 1),
    ) {
        (
            Some(crate::workflow::model::WorkflowStepDefinition::Execute {
                id,
                ..
            }),
            Some(crate::workflow::model::WorkflowStepDefinition::Complete { id: complete_id }),
        ) => (id.as_str(), complete_id.as_str()),
        (
            Some(crate::workflow::model::WorkflowStepDefinition::Execute { executor_kind, .. }),
            _,
        ) => {
            return Err(AppError::WorkflowExecutorNotFound(format!(
                "{executor_kind:?}"
            )))
        }
        _ => {
            return Err(AppError::WorkflowRunInconsistent(
                "execute/complete definitions are missing".into(),
            ))
        }
    };
    let started_at = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    tx.execute(
        "UPDATE workflow_runs SET status = 'running', updated_at = ?1 WHERE id = ?2",
        params![started_at, detail.run.id],
    )
    .map_err(db_error)?;
    mark_step(&tx, &detail.run.id, execute_index, "running", None)?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "step_started",
        Some(execute_step_id),
        None,
        &started_at,
    )?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "execution_started",
        Some(execute_step_id),
        None,
        &started_at,
    )?;
    tx.commit().map_err(db_error)?;
    let input: Value = serde_json::from_str(&detail.run.input_json)
        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
    let configured_default = ProviderService::configured_default(project_root)?;
    let provider_id = input
        .get("providerId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| configured_default.as_ref().map(|(provider, _)| provider.clone()))
        .unwrap_or_else(|| "dry_run".into());
    let model_id = input
        .get("modelId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| configured_default.as_ref().and_then(|(_, model)| (provider_id != "dry_run").then_some(model.clone())))
        .unwrap_or_else(|| if provider_id == "dry_run" { "dry-run-v1".into() } else { "mock-image-v1".into() });
    let result = if provider_id == "dry_run" {
        DryRunExecutor.execute(
            &request,
            &workflow_artifact_dir(project_root, &detail.run.id),
        )?
    } else {
        let compiled_hash = compiled_request_id(&request_json);
        let attempt_number = next_attempt_number(conn, &detail.run.id, execute_step_id)?;
        let idempotency_key = ProviderService::idempotency_key(&detail.run.id, execute_step_id, attempt_number);
        let attempt = create_attempt(
            conn,
            &detail.run.id,
            execute_step_id,
            attempt_number,
            &compiled_hash,
            &provider_id,
            &model_id,
            &idempotency_key,
        )?;
        append_audit_event(conn, Some(&attempt.id), &detail.run.id, "provider.execution.queued", Some(&serde_json::json!({"providerId": provider_id, "modelId": model_id, "attemptNumber": attempt_number})))?;
        let submission = match ProviderService::submit_compiled_request(
            &request,
            execute_step_id,
            &compiled_hash,
            &provider_id,
            &model_id,
            attempt_number,
        ) {
            Ok(submission) => submission,
            Err(error) => {
                let error_json = serde_json::json!({"message": error.to_string()}).to_string();
                let _ = update_attempt_status(conn, &attempt.id, "failed", Some(&error_json));
                let _ = append_audit_event(conn, Some(&attempt.id), &detail.run.id, "provider.execution.failed", Some(&serde_json::json!({"error": error.to_string()})));
                return Err(error);
            }
        };
        persist_job(
            conn,
            &attempt.id,
            &provider_id,
            &submission.submission.job.provider_job_id,
            "submitted",
        )?;
        append_audit_event(conn, Some(&attempt.id), &detail.run.id, "provider.execution.submitted", Some(&serde_json::json!({"providerJobId": submission.submission.job.provider_job_id})))?;
        let (status, provider_result) = match ProviderService::finish_submission(&submission) {
            Ok(result) => result,
            Err(error) => {
                let error_json = serde_json::json!({"message": error.to_string()}).to_string();
                let _ = update_attempt_status(conn, &attempt.id, "failed", Some(&error_json));
                let _ = append_audit_event(conn, Some(&attempt.id), &detail.run.id, "provider.execution.failed", Some(&serde_json::json!({"error": error.to_string()})));
                return Err(error);
            }
        };
        let outcome = crate::providers::service::ProviderExecutionOutcome {
            provider_id: provider_id.clone(),
            adapter_version: submission.adapter_version,
            submission: submission.submission,
            status,
            result: provider_result,
        };
        update_attempt_status(conn, &attempt.id, "succeeded", None)?;
        let snapshot: crate::workflow::model::WorkflowContextSnapshot = detail
            .run
            .context_snapshot_json
            .as_deref()
            .ok_or_else(|| AppError::WorkflowRunInconsistent("context snapshot is missing".into()))
            .and_then(|json| serde_json::from_str(json).map_err(|error| AppError::WorkflowRunInconsistent(error.to_string())))?;
        if !snapshot.assets.is_empty() {
            let compiled_request_hash = compiled_request_id(&request_json);
            let captured = crate::generation::service::GenerationService::capture_provider_result(
                project_root,
                &crate::generation::service::GenerationCaptureInput {
                    project_id: project_id.into(),
                    workflow_run_id: detail.run.id.clone(),
                    workflow_step_key: execute_step_id.into(),
                    workflow_definition_id: operation.id.clone(),
                    workflow_version: detail.run.skill_version.clone(),
                    skill_id: detail.run.skill_id.clone(),
                    skill_version: detail.run.skill_version.clone(),
                    compiled_execution_artifact_id: compiled_request_hash.clone(),
                    compiled_request_sha256: compiled_request_hash,
                    canon_snapshot_id: (!snapshot.canon.is_empty()).then(|| format!("canon:{}", detail.run.id)),
                    canon_snapshot_sha256: (!snapshot.canon.is_empty()).then(|| sha256_json(&snapshot.canon)),
                    provider_attempt_id: attempt.id.clone(),
                    provider_id: provider_id.clone(),
                    model_id: model_id.clone(),
                    source_asset_version_ids: snapshot.assets.iter().map(|asset| asset.asset_version_id.clone()).collect(),
                    requested_output_count: 4,
                },
                &outcome.result,
            )?;
            let artifact_ids = captured
                .artifacts
                .iter()
                .map(|artifact| artifact.id.clone())
                .collect::<Vec<_>>();
            update_artifact_ids(conn, &attempt.id, &artifact_ids)?;
            append_audit_event(conn, Some(&attempt.id), &detail.run.id, "provider.execution.completed", Some(&serde_json::json!({"resultSetId": captured.result_set.id, "artifactCount": captured.artifacts.len()})))?;
            crate::workflow::execution::ExecutionResult {
                kind: provider_id,
                artifact_path: project_root.join(&captured.artifacts[0].storage_path),
                result_set_id: Some(captured.result_set.id),
                artifact_ids,
                request,
            }
        } else {
            let asset_type = request.expected_output.asset_type.as_str();
            let owner_entity_id = request
                .expected_output
                .owner_entity_input_ref
                .as_deref()
                .and_then(|reference| input.get(reference))
                .and_then(Value::as_str)
                .map(str::to_string);
            let version = crate::workflow::ingestion::persist_provider_result(
                project_root,
                &detail.run.id,
                &outcome.result,
                asset_type,
                owner_entity_id,
            )?;
            append_audit_event(conn, Some(&attempt.id), &detail.run.id, "provider.execution.completed", Some(&serde_json::json!({"artifactPath": version.file_path})))?;
            crate::workflow::execution::ExecutionResult {
                kind: provider_id,
                artifact_path: project_root.join(version.file_path),
                result_set_id: None,
                artifact_ids: Vec::new(),
                request,
            }
        }
    };
    let result_json =
        serde_json::to_string(&result).map_err(|error| AppError::Database(error.to_string()))?;
    let complete_index = execute_index + 1;
    let completed_at = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    mark_step(
        &tx,
        &detail.run.id,
        execute_index,
        "completed",
        Some(&result_json),
    )?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "execution_completed",
        Some(execute_step_id),
        None,
        &completed_at,
    )?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "step_completed",
        Some(execute_step_id),
        None,
        &completed_at,
    )?;
    mark_step(&tx, &detail.run.id, complete_index, "running", None)?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "step_started",
        Some(complete_step_id),
        None,
        &completed_at,
    )?;
    mark_step(&tx, &detail.run.id, complete_index, "completed", None)?;
    tx.execute("UPDATE workflow_runs SET status = 'completed', current_step_index = ?1, completed_at = ?2, updated_at = ?2 WHERE id = ?3", params![complete_index + 1, completed_at, detail.run.id]).map_err(db_error)?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "step_completed",
        Some(complete_step_id),
        None,
        &completed_at,
    )?;
    append_event_in_transaction(
        &tx,
        &detail.run.id,
        "run_completed",
        Some(complete_step_id),
        None,
        &completed_at,
    )?;
    tx.commit().map_err(db_error)?;
    WorkflowRepository::get_run(conn, project_id, &detail.run.id)
}

fn compiled_request_id(request_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(request_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn sha256_json<T: serde::Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn start_run(conn: &mut Connection, run_id: &str, emit_started: bool) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    tx.execute(
        "UPDATE workflow_runs SET status = 'running', updated_at = ?1 WHERE id = ?2",
        params![now, run_id],
    )
    .map_err(db_error)?;
    if emit_started {
        append_event_in_transaction(&tx, run_id, "run_started", None, None, &now)?;
    }
    tx.commit().map_err(db_error)
}

fn start_step(
    conn: &mut Connection,
    run_id: &str,
    index: i64,
    step_id: &str,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    mark_step(&tx, run_id, index, "running", None)?;
    append_event_in_transaction(&tx, run_id, "step_started", Some(step_id), None, &now)?;
    tx.commit().map_err(db_error)
}

fn complete_step(
    conn: &mut Connection,
    run_id: &str,
    index: i64,
    step_id: &str,
    output_json: Option<&str>,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    mark_step(&tx, run_id, index, "completed", output_json)?;
    tx.execute(
        "UPDATE workflow_runs SET current_step_index = ?1, updated_at = ?2 WHERE id = ?3",
        params![index + 1, now, run_id],
    )
    .map_err(db_error)?;
    append_event_in_transaction(&tx, run_id, "step_completed", Some(step_id), None, &now)?;
    tx.commit().map_err(db_error)
}

fn complete_context_step(
    conn: &mut Connection,
    run_id: &str,
    index: i64,
    step_id: &str,
    context_json: &str,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    tx.execute("UPDATE workflow_runs SET context_snapshot_json = ?1, current_step_index = ?2, updated_at = ?3 WHERE id = ?4", params![context_json, index + 1, now, run_id]).map_err(db_error)?;
    mark_step(&tx, run_id, index, "completed", None)?;
    append_event_in_transaction(&tx, run_id, "step_completed", Some(step_id), None, &now)?;
    tx.commit().map_err(db_error)
}

fn enter_approval(
    conn: &mut Connection,
    run_id: &str,
    index: i64,
    step_id: &str,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    mark_step(&tx, run_id, index, "waiting", None)?;
    tx.execute("UPDATE workflow_runs SET status = 'waiting_for_approval', current_step_index = ?1, updated_at = ?2 WHERE id = ?3", params![index, now, run_id]).map_err(db_error)?;
    append_event_in_transaction(&tx, run_id, "approval_requested", Some(step_id), None, &now)?;
    tx.commit().map_err(db_error)
}

fn complete_run(
    conn: &mut Connection,
    run_id: &str,
    index: i64,
    step_id: &str,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    mark_step(&tx, run_id, index, "completed", None)?;
    tx.execute("UPDATE workflow_runs SET status = 'completed', current_step_index = ?1, completed_at = ?2, updated_at = ?2 WHERE id = ?3", params![index + 1, now, run_id]).map_err(db_error)?;
    append_event_in_transaction(&tx, run_id, "step_completed", Some(step_id), None, &now)?;
    append_event_in_transaction(&tx, run_id, "run_completed", Some(step_id), None, &now)?;
    tx.commit().map_err(db_error)
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
fn active_approval_step<'a>(
    detail: &'a WorkflowRunDetail,
    step_definition_id: &str,
) -> Result<&'a crate::workflow::model::WorkflowStepRecord, AppError> {
    detail
        .steps
        .iter()
        .find(|step| {
            step.step_definition_id == step_definition_id
                && step.step_index == detail.run.current_step_index
                && step.step_type == "approval"
                && step.status == "waiting"
        })
        .ok_or_else(|| AppError::WorkflowStepNotFound(step_definition_id.into()))
}
fn finalize_run_failure_if_running(
    project_root: &Path,
    run_id: &str,
    error: &AppError,
) -> Result<(), AppError> {
    let mut conn = open_project(project_root)?;
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM workflow_runs WHERE id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    if !matches!(status.as_deref(), Some("running" | "ready_for_execution")) {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339();
    let payload = serde_json::json!({"code":"WORKFLOW_STEP_FAILED"}).to_string();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    tx.execute("UPDATE workflow_steps SET status = 'failed', completed_at = ?1 WHERE workflow_run_id = ?2 AND status = 'running'", params![now, run_id]).map_err(db_error)?;
    tx.execute("UPDATE workflow_runs SET status = 'failed', failure_code = 'WORKFLOW_STEP_FAILED', failure_message = ?1, completed_at = ?2, updated_at = ?2 WHERE id = ?3", params![error.to_string(), now, run_id]).map_err(db_error)?;
    append_event_in_transaction(&tx, run_id, "run_failed", None, Some(&payload), &now)?;
    tx.commit().map_err(db_error)?;
    crate::qa::repository::fail_for_workflow(
        &conn,
        run_id,
        "WORKFLOW_STEP_FAILED",
        &error.to_string(),
        &now,
    )?;
    Ok(())
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

fn validate_visual_qa_input(input: &Value) -> Result<(), AppError> {
    for key in ["projectRootPath", "assetVersionId", "adapterId"] {
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
    for forbidden in ["apiKey", "bearerToken", "credential", "secret"] {
        if input.get(forbidden).is_some() {
            return Err(AppError::WorkflowInputInvalid(format!(
                "{forbidden} must not be stored in workflow input"
            )));
        }
    }
    if let Some(expectations) = input.get("expectations") {
        serde_json::from_value::<Vec<crate::qa::models::VisualExpectation>>(
            expectations.clone(),
        )
        .map_err(|error| {
            AppError::WorkflowInputInvalid(format!("invalid expectations: {error}"))
        })?;
    }
    if input.get("adapterId").and_then(Value::as_str) == Some("mock")
        && input.get("mockResponse").is_none()
    {
        return Err(AppError::WorkflowInputInvalid(
            "mockResponse is required for mock QA".into(),
        ));
    }
    Ok(())
}
fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::service::ProjectService;
    use crate::skills::model::TbdGuard;
    use tempfile::tempdir;

    #[test]
    fn guarded_operation_blocks_launch_without_creating_a_run() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("guarded");
        ProjectService::create(&root, "Guarded").unwrap();
        let conn = open_project(&root).unwrap();
        let project_id: String = conn
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();
        conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('mara', ?1, 'character', 'Mara', 'mara', 'now', 'now')", [&project_id]).unwrap();
        conn.execute("INSERT INTO canon_tbds (id, project_id, topic, protected, status, created_at, updated_at) VALUES ('tbd-1', ?1, 'Resolve scar placement', 1, 'open', 'now', 'now')", [&project_id]).unwrap();
        drop(conn);

        let registry = SkillRegistry::builtin().unwrap();
        let (_, builtin) = registry
            .find_operation("character-builder", "1.0.0", "character.create_face_lock")
            .unwrap();
        let mut guarded = builtin.clone();
        guarded.tbd_guards = vec![TbdGuard::ProjectScope];
        let input = serde_json::json!({
            "projectRootPath": root.to_string_lossy(),
            "characterEntityId": "mara",
            "visualSpec": {"head":"oval","eyes":"brown","brows":"straight","nose":"narrow","lips":"neutral","skin":"olive","hair":"black","build":"athletic","expression":"neutral"},
            "baselineWardrobe": "charcoal"
        });

        let error = WorkflowRuntime::create_run_for_operation(
            &root,
            "character-builder",
            "1.0.0",
            "character.create_face_lock",
            input,
            &guarded,
        )
        .unwrap_err();

        assert!(matches!(error, AppError::WorkflowBlockedByProtectedTbd(_)));
        assert!(WorkflowRuntime::list_runs(&root).unwrap().is_empty());
    }
}
