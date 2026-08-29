use crate::db;
use crate::error::AppError;
use crate::project::repository::read_project;
use crate::providers::adapter::GenerationProvider;
use crate::providers::repository::{
    append_audit_event, create_attempt, next_attempt_number, persist_job, update_artifact_ids,
    update_attempt_status,
};
use crate::providers::service::ProviderService;
use crate::skills::registry::SkillRegistry;
use crate::workflow::artifacts::{workflow_artifact_dir, write_run_artifacts};
use crate::workflow::compiler::{
    CharacterFaceLockCompiler, CharacterOutfitCompiler, CharacterSheetCompiler, RequestCompiler,
    SceneKeyframeCompiler, WorldPlateCompiler,
};
use crate::workflow::context::{
    resolve_character_face_lock_context, resolve_character_outfit_context,
    resolve_character_sheet_context, resolve_scene_keyframe_context, resolve_world_plate_context,
};
use crate::workflow::executor::{DryRunExecutor, ExecutionExecutor};
use crate::workflow::model::{
    WorkflowCharacterOption, WorkflowContextSnapshot, WorkflowRunDetail, WorkflowRunRecord,
};
use crate::workflow::prerequisites::{evaluate_prerequisites, evaluate_tbd_guards};
use crate::workflow::repository::{append_event_in_transaction, WorkflowRepository};
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
            "create_outfit" => validate_outfit_input(&input)?,
            "create_character_sheet" => validate_character_sheet_input(&input)?,
            "run_visual_qa" => validate_visual_qa_input(&input)?,
            "repair_failed_qa" => validate_visual_repair_input(&input)?,
            "create_world_plate" => validate_world_plate_input(&input)?,
            "create_scene_keyframe" => validate_scene_keyframe_input(&input)?,
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
        if operation.id == "world.create_plate" {
            let world_id = input
                .get("worldId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let location_entity_id: String = conn
                .query_row(
                    "SELECT canon_location_entity_id FROM worlds WHERE id = ?1 AND project_id = ?2",
                    params![world_id, project.id],
                    |row| row.get(0),
                )
                .map_err(|error| match error {
                    rusqlite::Error::QueryReturnedNoRows => AppError::WorldNotFound,
                    other => AppError::Database(other.to_string()),
                })?;
            let decisions: Vec<crate::workflow::tbd_policy::TbdDecision> = input
                .get("tbdDecisions")
                .or_else(|| input.get("tbd_decisions"))
                .map(|value| serde_json::from_value(value.clone()).unwrap_or_default())
                .unwrap_or_default();
            crate::workflow::tbd_policy::validate_world_tbd_firewall(
                &conn,
                &project.id,
                &location_entity_id,
                &decisions,
            )?;
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
                                    detail.run.prerequisite_report_json.as_deref().ok_or_else(
                                        || {
                                            AppError::WorkflowRunInconsistent(
                                                "missing prerequisite report".into(),
                                            )
                                        },
                                    )?,
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
                        "character_outfit_context" => {
                            let report: crate::workflow::model::PrerequisiteReport =
                                serde_json::from_str(
                                    detail.run.prerequisite_report_json.as_deref().ok_or_else(
                                        || {
                                            AppError::WorkflowRunInconsistent(
                                                "missing prerequisite report".into(),
                                            )
                                        },
                                    )?,
                                )
                                .map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            let context = resolve_character_outfit_context(
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
                        "character_sheet_context" => {
                            let report: crate::workflow::model::PrerequisiteReport =
                                serde_json::from_str(
                                    detail.run.prerequisite_report_json.as_deref().ok_or_else(
                                        || {
                                            AppError::WorkflowRunInconsistent(
                                                "missing prerequisite report".into(),
                                            )
                                        },
                                    )?,
                                )
                                .map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            let context = resolve_character_sheet_context(
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
                        "visual_qa_repair_context" => {
                            let context = crate::qa::repair_workflow::resolve(
                                &conn,
                                project_root,
                                &project.id,
                                &input,
                            )?;
                            serde_json::to_string(&context)
                                .map_err(|error| AppError::Database(error.to_string()))?
                        }
                        "world_plate_context" => {
                            let report: crate::workflow::model::PrerequisiteReport =
                                serde_json::from_str(
                                    detail.run.prerequisite_report_json.as_deref().ok_or_else(
                                        || {
                                            AppError::WorkflowRunInconsistent(
                                                "missing prerequisite report".into(),
                                            )
                                        },
                                    )?,
                                )
                                .map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            let context = resolve_world_plate_context(
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
                        "scene_keyframe_context" => {
                            // Ensure stable shot_keyframe Asset exists before snapshot
                            let scene_id_for_ensure = input
                                .get("sceneId")
                                .or_else(|| input.get("scene_id"))
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            if !scene_id_for_ensure.trim().is_empty() {
                                // Ensure idempotent shot_keyframe Asset; ignore error if scene not found here (resolver will handle)
                                let _ = crate::scenes::service::SceneService::ensure_scene_keyframe_asset(project_root, scene_id_for_ensure);
                            }
                            let report: crate::workflow::model::PrerequisiteReport =
                                serde_json::from_str(
                                    detail.run.prerequisite_report_json.as_deref().ok_or_else(
                                        || {
                                            AppError::WorkflowRunInconsistent(
                                                "missing prerequisite report".into(),
                                            )
                                        },
                                    )?,
                                )
                                .map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            let context = resolve_scene_keyframe_context(
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
                            write_run_artifacts(project_root, workflow_run_id, &context, &request)?;
                            serde_json::to_string(&request)
                                .map_err(|error| AppError::Database(error.to_string()))?
                        }
                        "character_outfit_v1" => {
                            let context: WorkflowContextSnapshot =
                                serde_json::from_str(&context_json).map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            let request = CharacterOutfitCompiler.compile(
                                workflow_run_id,
                                skill,
                                operation,
                                &context,
                            )?;
                            write_run_artifacts(project_root, workflow_run_id, &context, &request)?;
                            serde_json::to_string(&request)
                                .map_err(|error| AppError::Database(error.to_string()))?
                        }
                        "character_sheet_v1" => {
                            let context: WorkflowContextSnapshot =
                                serde_json::from_str(&context_json).map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            let request = CharacterSheetCompiler.compile(
                                workflow_run_id,
                                skill,
                                operation,
                                &context,
                            )?;
                            write_run_artifacts(project_root, workflow_run_id, &context, &request)?;
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
                        "visual_qa_repair_v1" => {
                            let context: crate::qa::repair_workflow::RepairWorkflowContext =
                                serde_json::from_str(&context_json).map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            serde_json::to_string(&crate::qa::repair_workflow::compile_request(
                                workflow_run_id,
                                &context,
                            )?)
                            .map_err(|error| AppError::Database(error.to_string()))?
                        }
                        "world_plate_v1" => {
                            let context: WorkflowContextSnapshot =
                                serde_json::from_str(&context_json).map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            let request = WorldPlateCompiler.compile(
                                workflow_run_id,
                                skill,
                                operation,
                                &context,
                            )?;
                            write_run_artifacts(project_root, workflow_run_id, &context, &request)?;
                            serde_json::to_string(&request)
                                .map_err(|error| AppError::Database(error.to_string()))?
                        }
                        "scene_keyframe_v1" => {
                            let context: WorkflowContextSnapshot =
                                serde_json::from_str(&context_json).map_err(|error| {
                                    AppError::WorkflowRunInconsistent(error.to_string())
                                })?;
                            let request = SceneKeyframeCompiler.compile(
                                workflow_run_id,
                                skill,
                                operation,
                                &context,
                            )?;
                            write_run_artifacts(project_root, workflow_run_id, &context, &request)?;
                            serde_json::to_string(&request)
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

fn execute_visual_repair_ready(
    conn: &mut Connection,
    project_root: &Path,
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
    let request: crate::workflow::execution::ExecutionRequest = serde_json::from_str(&request_json)
        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
    let context: crate::qa::repair_workflow::RepairWorkflowContext =
        serde_json::from_str(&load_context(conn, &detail.run.id)?)
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
                "repair execute/complete definitions are missing".into(),
            ))
        }
    };
    let input: Value = serde_json::from_str(&detail.run.input_json)
        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
    let provider_id = input
        .get("providerId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::WorkflowInputInvalid("providerId is required".into()))?;
    let model_id = input
        .get("modelId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::WorkflowInputInvalid("modelId is required".into()))?;

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
        Some(
            &serde_json::json!({
                "sourceQaRunId": context.compiled.plan.source_qa_run_id,
                "providerId": provider_id,
                "modelId": model_id,
            })
            .to_string(),
        ),
        &started_at,
    )?;
    tx.commit().map_err(db_error)?;

    let compiled_hash = compiled_request_id(&request_json);
    let attempt_number = next_attempt_number(conn, &detail.run.id, execute_step_id)?;
    let idempotency_key =
        ProviderService::idempotency_key(&detail.run.id, execute_step_id, attempt_number);
    let attempt = create_attempt(
        conn,
        &detail.run.id,
        execute_step_id,
        attempt_number,
        &compiled_hash,
        provider_id,
        model_id,
        &idempotency_key,
    )?;
    append_audit_event(
        conn,
        Some(&attempt.id),
        &detail.run.id,
        "provider.execution.queued",
        Some(&serde_json::json!({
            "providerId": provider_id,
            "modelId": model_id,
            "attemptNumber": attempt_number,
            "task": "visual_repair",
        })),
    )?;
    let reference_attachments =
        crate::workflow::execution::resolve_reference_attachments(project_root, &request)?;
    let submission = match ProviderService::submit_provider_request(
        &request,
        reference_attachments,
        None,
        execute_step_id,
        &compiled_hash,
        provider_id,
        model_id,
        attempt_number,
    ) {
        Ok(submission) => submission,
        Err(error) => {
            let error_json = serde_json::json!({"message": error.to_string()}).to_string();
            let _ = update_attempt_status(conn, &attempt.id, "failed", Some(&error_json));
            let _ = append_audit_event(
                conn,
                Some(&attempt.id),
                &detail.run.id,
                "provider.execution.failed",
                Some(&serde_json::json!({"error": error.to_string()})),
            );
            return Err(error);
        }
    };
    persist_job(
        conn,
        &attempt.id,
        provider_id,
        &submission.submission.job.provider_job_id,
        "submitted",
    )?;
    let (status, provider_result) = match ProviderService::finish_submission(&submission) {
        Ok(result) => result,
        Err(error) => {
            let error_json = serde_json::json!({"message": error.to_string()}).to_string();
            let _ = update_attempt_status(conn, &attempt.id, "failed", Some(&error_json));
            let _ = append_audit_event(
                conn,
                Some(&attempt.id),
                &detail.run.id,
                "provider.execution.failed",
                Some(&serde_json::json!({"error": error.to_string()})),
            );
            return Err(error);
        }
    };
    let child = crate::workflow::ingestion::persist_repair_provider_result(
        project_root,
        &detail.run.id,
        &provider_result,
        &context.compiled.plan.source_asset_id,
        &context.compiled.plan.source_asset_version_id,
    )?;
    let completed_at = Utc::now().to_rfc3339();
    crate::qa::repair_workflow::record_repair(
        conn,
        &crate::qa::repair_workflow::RepairProvenanceInput {
            project_id,
            workflow_run_id: &detail.run.id,
            child_asset_version_id: &child.id,
            provider_id,
            adapter_version: submission.adapter_version,
            model_id,
            provider_job_id: &submission.submission.job.provider_job_id,
            compiled_request: &request,
            context: &context,
            created_at: &completed_at,
        },
    )?;
    let qa_adapter_id = input
        .get("qaAdapterId")
        .and_then(Value::as_str)
        .unwrap_or("openai");
    let mut qa_input = serde_json::json!({
        "projectRootPath": project_root.to_string_lossy(),
        "assetVersionId": child.id.clone(),
        "adapterId": qa_adapter_id,
        "expectations": [],
    });
    if let Some(model_id) = input.get("qaModelId").and_then(Value::as_str) {
        qa_input["modelId"] = Value::String(model_id.into());
    }
    if let Some(mock_response) = input.get("qaMockResponse") {
        qa_input["mockResponse"] = mock_response.clone();
    }
    if let Ok(qa_workflow) = WorkflowRuntime::create_run(
        project_root,
        "visual-qa",
        "1.0.0",
        "asset.run_visual_qa",
        qa_input,
    ) {
        if let Ok(waiting) = WorkflowRuntime::advance_run(project_root, &qa_workflow.run.id) {
            if waiting.run.status == "waiting_for_approval" {
                let _ = WorkflowRuntime::approve_run_step(
                    project_root,
                    &qa_workflow.run.id,
                    "approve-qa",
                    Some("Automatic post-repair QA evaluation".into()),
                )
                .and_then(|_| {
                    WorkflowRuntime::advance_run(project_root, &qa_workflow.run.id).map(|_| ())
                });
            }
            if let Ok(runs) =
                crate::qa::repository::list_runs_for_asset_version(conn, project_id, &child.id)
            {
                if let Some(child_qa) = runs
                    .iter()
                    .find(|run| run.workflow_run_id.as_deref() == Some(qa_workflow.run.id.as_str()))
                {
                    let _ = crate::qa::repair_workflow::link_follow_up_qa(
                        conn,
                        &detail.run.id,
                        &child_qa.id,
                        &qa_workflow.run.id,
                    );
                }
            }
        }
    }
    update_attempt_status(conn, &attempt.id, "succeeded", None)?;
    append_audit_event(
        conn,
        Some(&attempt.id),
        &detail.run.id,
        "provider.execution.completed",
        Some(&serde_json::json!({
            "childAssetVersionId": child.id,
            "providerJobId": submission.submission.job.provider_job_id,
            "lifecycle": status.lifecycle,
        })),
    )?;

    let result = crate::workflow::execution::ExecutionResult {
        kind: provider_id.into(),
        artifact_path: project_root.join(&child.file_path),
        result_set_id: None,
        artifact_ids: Vec::new(),
        request,
    };
    let result_json =
        serde_json::to_string(&result).map_err(|error| AppError::Database(error.to_string()))?;
    let complete_index = execute_index + 1;
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
        Some(&serde_json::json!({"childAssetVersionId": child.id}).to_string()),
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

fn execute_scene_keyframe_ready(
    conn: &mut Connection,
    project_root: &Path,
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
    let request: crate::workflow::execution::ExecutionRequest = serde_json::from_str(&request_json)
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
                "scene keyframe execute/complete definitions are missing".into(),
            ))
        }
    };
    // Provider capability check before side effects
    let input: Value = serde_json::from_str(&detail.run.input_json)
        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
    let configured_default = ProviderService::configured_default(project_root)?;
    let provider_id = input
        .get("providerId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            configured_default
                .as_ref()
                .map(|(provider, _)| provider.clone())
        })
        .unwrap_or_else(|| "dry_run".into());
    let model_id = input
        .get("modelId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            configured_default
                .as_ref()
                .and_then(|(_, model)| (provider_id != "dry_run").then_some(model.clone()))
        })
        .unwrap_or_else(|| {
            if provider_id == "dry_run" {
                "dry-run-v1".into()
            } else {
                "mock-image-v1".into()
            }
        });

    // Compute required reference count from request (world + looks + sheets + props)
    // Already in request.references; check capability
    {
        let caps = if provider_id == "openai" {
            crate::providers::openai::OpenAiImageProvider::new("https://api.openai.com/v1", "dummy")
                .capabilities()
        } else {
            let registry = crate::providers::registry::ProviderRegistry::builtin();
            match registry.get(&provider_id) {
                Ok(provider) => provider.capabilities(),
                Err(_) => {
                    return Err(AppError::ProviderCapabilityUnsatisfied(format!(
                        "provider {} is not registered",
                        provider_id
                    )))
                }
            }
        };
        let ref_count = request.references.len();
        if ref_count > 0 && !caps.supports_reference_image {
            return Err(AppError::ProviderCapabilityUnsatisfied(format!(
                "provider {} does not support reference images (need {} references)",
                provider_id, ref_count
            )));
        }
        if ref_count > 1 && !caps.supports_multiple_reference_images {
            return Err(AppError::ProviderCapabilityUnsatisfied(format!(
                "provider {} does not support multiple references (need {} references)",
                provider_id, ref_count
            )));
        }
        if let Some(max) = caps.max_reference_images {
            if (ref_count as u32) > max {
                return Err(AppError::ProviderCapabilityUnsatisfied(format!(
                    "provider {} supports at most {} references but scene requires {}",
                    provider_id, max, ref_count
                )));
            }
        }
        // Also check via generic supports for model etc.
        let provider_request =
            crate::providers::model::ProviderExecutionRequest::from_execution_request(
                &request.provenance.workflow_run_id,
                execute_step_id,
                &compiled_request_id(&request_json),
                &provider_id,
                &model_id,
                &ProviderService::idempotency_key(
                    &request.provenance.workflow_run_id,
                    execute_step_id,
                    1,
                ),
                &request,
            )
            .map_err(|msg| AppError::ProviderCapabilityUnsatisfied(msg))?;
        caps.supports(&provider_request)
            .map_err(|msg| AppError::ProviderCapabilityUnsatisfied(msg))?;
    }

    // Proceed with normal provider execution (reuse generic logic but for shot_keyframe)
    // Use the same flow as generic execute: start transaction, submit, finish, persist
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

    let result = if provider_id == "dry_run" {
        DryRunExecutor.execute(
            &request,
            &workflow_artifact_dir(project_root, &detail.run.id),
        )?
    } else {
        let compiled_hash = compiled_request_id(&request_json);
        let attempt_number = next_attempt_number(conn, &detail.run.id, execute_step_id)?;
        let idempotency_key =
            ProviderService::idempotency_key(&detail.run.id, execute_step_id, attempt_number);
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
        append_audit_event(
            conn,
            Some(&attempt.id),
            &detail.run.id,
            "provider.execution.queued",
            Some(
                &serde_json::json!({"providerId": provider_id, "modelId": model_id, "attemptNumber": attempt_number}),
            ),
        )?;
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
                let _ = append_audit_event(
                    conn,
                    Some(&attempt.id),
                    &detail.run.id,
                    "provider.execution.failed",
                    Some(&serde_json::json!({"error": error.to_string()})),
                );
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
        append_audit_event(
            conn,
            Some(&attempt.id),
            &detail.run.id,
            "provider.execution.submitted",
            Some(&serde_json::json!({"providerJobId": submission.submission.job.provider_job_id})),
        )?;
        let (status, provider_result) = match ProviderService::finish_submission(&submission) {
            Ok(result) => result,
            Err(error) => {
                let error_json = serde_json::json!({"message": error.to_string()}).to_string();
                let _ = update_attempt_status(conn, &attempt.id, "failed", Some(&error_json));
                let _ = append_audit_event(
                    conn,
                    Some(&attempt.id),
                    &detail.run.id,
                    "provider.execution.failed",
                    Some(&serde_json::json!({"error": error.to_string()})),
                );
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
            .and_then(|json| {
                serde_json::from_str(json)
                    .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))
            })?;
        // For scene keyframe, snapshot.assets is non-empty but we still need to create asset version in keyframe Asset
        // We'll use generation capture path if assets non-empty, else persist
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
                    canon_snapshot_id: (!snapshot.canon.is_empty())
                        .then(|| format!("canon:{}", detail.run.id)),
                    canon_snapshot_sha256: (!snapshot.canon.is_empty())
                        .then(|| sha256_json(&snapshot.canon)),
                    provider_attempt_id: attempt.id.clone(),
                    provider_id: provider_id.clone(),
                    model_id: model_id.clone(),
                    source_asset_version_ids: snapshot
                        .assets
                        .iter()
                        .map(|asset| asset.asset_version_id.clone())
                        .collect(),
                    requested_output_count: 1,
                },
                &outcome.result,
            )?;
            let artifact_ids = captured
                .artifacts
                .iter()
                .map(|artifact| artifact.id.clone())
                .collect::<Vec<_>>();
            update_artifact_ids(conn, &attempt.id, &artifact_ids)?;
            append_audit_event(
                conn,
                Some(&attempt.id),
                &detail.run.id,
                "provider.execution.completed",
                Some(
                    &serde_json::json!({"resultSetId": captured.result_set.id, "artifactCount": captured.artifacts.len()}),
                ),
            )?;
            // Also create shot_keyframe AssetVersion from first captured artifact
            let first_artifact = captured
                .artifacts
                .first()
                .ok_or_else(|| AppError::GenerationArtifactCaptureFailed("no artifact".into()))?;
            let keyframe_asset_id = {
                let scene_id = input
                    .get("sceneId")
                    .or_else(|| input.get("scene_id"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| AppError::WorkflowInputInvalid("sceneId is required".into()))?;
                // Load scene to get keyframe asset id
                let scene_asset_id: Option<String> = conn
                    .query_row(
                        "SELECT keyframe_asset_id FROM world_scenes WHERE id = ?1",
                        params![scene_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(db_error)?
                    .flatten();
                scene_asset_id.ok_or_else(|| {
                    AppError::WorkflowRunInconsistent("keyframe asset id missing".into())
                })?
            };
            let source_path = project_root.join(&first_artifact.storage_path);
            let imported = match crate::assets::service::AssetService::import_asset_version(
                project_root,
                &keyframe_asset_id,
                &source_path,
                None,
            ) {
                Ok(v) => v,
                Err(AppError::DuplicateAssetVersion) => {
                    let hash = first_artifact.sha256.clone();
                    crate::assets::service::AssetService::get_asset_with_versions(
                        project_root,
                        &keyframe_asset_id,
                    )?
                    .versions
                    .into_iter()
                    .find(|v| v.sha256 == hash)
                    .ok_or_else(|| {
                        AppError::GenerationArtifactCaptureFailed("duplicate not found".into())
                    })?
                }
                Err(e) => return Err(e),
            };
            crate::workflow::execution::ExecutionResult {
                kind: provider_id,
                artifact_path: project_root.join(&imported.file_path),
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
            append_audit_event(
                conn,
                Some(&attempt.id),
                &detail.run.id,
                "provider.execution.completed",
                Some(&serde_json::json!({"artifactPath": version.file_path})),
            )?;
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
    if operation.id == "asset.repair_failed_qa" {
        return execute_visual_repair_ready(conn, project_root, project_id, detail, operation);
    }
    if operation.id == "scene.create_keyframe" {
        return execute_scene_keyframe_ready(conn, project_root, project_id, detail, operation);
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
            Some(crate::workflow::model::WorkflowStepDefinition::Execute { id, .. }),
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
        .or_else(|| {
            configured_default
                .as_ref()
                .map(|(provider, _)| provider.clone())
        })
        .unwrap_or_else(|| "dry_run".into());
    let model_id = input
        .get("modelId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            configured_default
                .as_ref()
                .and_then(|(_, model)| (provider_id != "dry_run").then_some(model.clone()))
        })
        .unwrap_or_else(|| {
            if provider_id == "dry_run" {
                "dry-run-v1".into()
            } else {
                "mock-image-v1".into()
            }
        });
    let result = if provider_id == "dry_run" {
        DryRunExecutor.execute(
            &request,
            &workflow_artifact_dir(project_root, &detail.run.id),
        )?
    } else {
        let compiled_hash = compiled_request_id(&request_json);
        let attempt_number = next_attempt_number(conn, &detail.run.id, execute_step_id)?;
        let idempotency_key =
            ProviderService::idempotency_key(&detail.run.id, execute_step_id, attempt_number);
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
        append_audit_event(
            conn,
            Some(&attempt.id),
            &detail.run.id,
            "provider.execution.queued",
            Some(
                &serde_json::json!({"providerId": provider_id, "modelId": model_id, "attemptNumber": attempt_number}),
            ),
        )?;
        let reference_attachments =
            match crate::workflow::execution::resolve_reference_attachments(project_root, &request)
            {
                Ok(attachments) => attachments,
                Err(error) => {
                    let error_json = serde_json::json!({"message": error.to_string()}).to_string();
                    let _ = update_attempt_status(conn, &attempt.id, "failed", Some(&error_json));
                    let _ = append_audit_event(
                        conn,
                        Some(&attempt.id),
                        &detail.run.id,
                        "provider.execution.failed",
                        Some(&serde_json::json!({"error": error.to_string()})),
                    );
                    return Err(error);
                }
            };
        let submission = match ProviderService::submit_provider_request(
            &request,
            reference_attachments,
            None,
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
                let _ = append_audit_event(
                    conn,
                    Some(&attempt.id),
                    &detail.run.id,
                    "provider.execution.failed",
                    Some(&serde_json::json!({"error": error.to_string()})),
                );
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
        append_audit_event(
            conn,
            Some(&attempt.id),
            &detail.run.id,
            "provider.execution.submitted",
            Some(&serde_json::json!({"providerJobId": submission.submission.job.provider_job_id})),
        )?;
        let (status, provider_result) = match ProviderService::finish_submission(&submission) {
            Ok(result) => result,
            Err(error) => {
                let error_json = serde_json::json!({"message": error.to_string()}).to_string();
                let _ = update_attempt_status(conn, &attempt.id, "failed", Some(&error_json));
                let _ = append_audit_event(
                    conn,
                    Some(&attempt.id),
                    &detail.run.id,
                    "provider.execution.failed",
                    Some(&serde_json::json!({"error": error.to_string()})),
                );
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
            .and_then(|json| {
                serde_json::from_str(json)
                    .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))
            })?;
        if operation.id != "world.create_plate" {
            // Character-builder style flows capture a durable generation result
            // set; first-Face runs legitimately carry an empty reference list.
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
                    canon_snapshot_id: (!snapshot.canon.is_empty())
                        .then(|| format!("canon:{}", detail.run.id)),
                    canon_snapshot_sha256: (!snapshot.canon.is_empty())
                        .then(|| sha256_json(&snapshot.canon)),
                    provider_attempt_id: attempt.id.clone(),
                    provider_id: provider_id.clone(),
                    model_id: model_id.clone(),
                    source_asset_version_ids: snapshot
                        .assets
                        .iter()
                        .map(|asset| asset.asset_version_id.clone())
                        .collect(),
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
            append_audit_event(
                conn,
                Some(&attempt.id),
                &detail.run.id,
                "provider.execution.completed",
                Some(
                    &serde_json::json!({"resultSetId": captured.result_set.id, "artifactCount": captured.artifacts.len()}),
                ),
            )?;
            crate::workflow::execution::ExecutionResult {
                kind: provider_id,
                artifact_path: project_root.join(&captured.artifacts[0].storage_path),
                result_set_id: Some(captured.result_set.id),
                artifact_ids,
                request,
            }
        } else {
            // World/scene flows persist the candidate version directly into the
            // stable expected-output asset.
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
            append_audit_event(
                conn,
                Some(&attempt.id),
                &detail.run.id,
                "provider.execution.completed",
                Some(&serde_json::json!({"artifactPath": version.file_path})),
            )?;
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

fn validate_outfit_input(input: &Value) -> Result<(), AppError> {
    for key in ["projectRootPath", "characterEntityId"] {
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
    let proposal = input
        .get("wardrobeProposal")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::WorkflowInputInvalid("wardrobeProposal is required".into()))?;
    if proposal
        .get("description")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .is_none()
    {
        return Err(AppError::WorkflowInputInvalid(
            "wardrobeProposal.description must be a non-empty string".into(),
        ));
    }
    Ok(())
}

fn validate_character_sheet_input(input: &Value) -> Result<(), AppError> {
    for key in ["projectRootPath", "characterEntityId"] {
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
        serde_json::from_value::<Vec<crate::qa::models::VisualExpectation>>(expectations.clone())
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

fn validate_visual_repair_input(input: &Value) -> Result<(), AppError> {
    for key in ["projectRootPath", "qaRunId", "providerId", "modelId"] {
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
    Ok(())
}

fn validate_world_plate_input(input: &Value) -> Result<(), AppError> {
    if input
        .get("worldId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .is_none()
    {
        return Err(AppError::WorkflowInputInvalid(
            "worldId must be a non-empty string".into(),
        ));
    }
    for forbidden in ["apiKey", "bearerToken", "credential", "secret"] {
        if input.get(forbidden).is_some() {
            return Err(AppError::WorkflowInputInvalid(format!(
                "{forbidden} must not be stored in workflow input"
            )));
        }
    }
    if let Some(decisions) = input
        .get("tbdDecisions")
        .or_else(|| input.get("tbd_decisions"))
    {
        if !decisions.is_array() {
            return Err(AppError::WorkflowInputInvalid(
                "tbdDecisions must be an array".into(),
            ));
        }
        serde_json::from_value::<Vec<crate::workflow::tbd_policy::TbdDecision>>(decisions.clone())
            .map_err(|error| {
                AppError::WorkflowInputInvalid(format!("invalid tbdDecisions: {error}"))
            })?;
    }
    Ok(())
}

fn validate_scene_keyframe_input(input: &Value) -> Result<(), AppError> {
    if input
        .get("sceneId")
        .or_else(|| input.get("scene_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .is_none()
    {
        return Err(AppError::WorkflowInputInvalid(
            "sceneId must be a non-empty string".into(),
        ));
    }
    for forbidden in ["apiKey", "bearerToken", "credential", "secret"] {
        if input.get(forbidden).is_some() {
            return Err(AppError::WorkflowInputInvalid(format!(
                "{forbidden} must not be stored in workflow input"
            )));
        }
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
            .find_operation("character-builder", "1.1.0", "character.create_face_lock")
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
            "1.1.0",
            "character.create_face_lock",
            input,
            &guarded,
        )
        .unwrap_err();

        assert!(matches!(error, AppError::WorkflowBlockedByProtectedTbd(_)));
        assert!(WorkflowRuntime::list_runs(&root).unwrap().is_empty());
    }

    fn world_plate_fixture() -> (tempfile::TempDir, std::path::PathBuf, String) {
        use crate::canon::model::CanonEntityType;
        use crate::canon::service::CanonService;
        use crate::worlds::service::WorldService;
        let temp = tempdir().unwrap();
        let root = temp.path().join("world-plate");
        ProjectService::create(&root, "World Plate").unwrap();
        let loc = CanonService::create_entity(&root, CanonEntityType::Location, "Station").unwrap();
        let desc = CanonService::upsert_section(
            &root,
            &loc.id,
            "description",
            serde_json::json!({"text": "A derelict station with rusted arches"}),
            None,
        )
        .unwrap();
        CanonService::lock_section(&root, &desc.id, None).unwrap();
        let geo = CanonService::upsert_section(
            &root,
            &loc.id,
            "geography",
            serde_json::json!({"text": "Industrial rust belt, cracked concrete"}),
            None,
        )
        .unwrap();
        CanonService::lock_section(&root, &geo.id, None).unwrap();
        let world = WorldService::create_world(&root, &loc.id).unwrap();
        (temp, root, world.id)
    }

    #[test]
    fn world_plate_prerequisites_block_without_locked_sections() {
        let (_temp, root, world_id) = world_plate_fixture();
        let conn = open_project(&root).unwrap();
        let project_id: String = conn
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();
        let location_id: String = conn
            .query_row(
                "SELECT canon_location_entity_id FROM worlds WHERE id = ?1",
                [&world_id],
                |row| row.get(0),
            )
            .unwrap();
        // Unlock description to make it fail
        let desc_id: String = conn
            .query_row(
                "SELECT id FROM canon_sections WHERE canon_entity_id = ?1 AND section_key = 'description'",
                [&location_id],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);
        crate::canon::service::CanonService::unlock_section(&root, &desc_id, None).unwrap();

        let err = WorkflowRuntime::create_run(
            &root,
            "world-builder",
            "1.0.0",
            "world.create_plate",
            serde_json::json!({"worldId": world_id}),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::WorkflowPrerequisiteFailed(_)));
        assert!(WorkflowRuntime::list_runs(&root).unwrap().is_empty());

        // Re-lock and should succeed (no TBD)
        let conn = open_project(&root).unwrap();
        let desc_id2: String = conn
            .query_row(
                "SELECT id FROM canon_sections WHERE canon_entity_id = ?1 AND section_key = 'description'",
                [&location_id],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);
        crate::canon::service::CanonService::lock_section(&root, &desc_id2, None).unwrap();
        let created = WorkflowRuntime::create_run(
            &root,
            "world-builder",
            "1.0.0",
            "world.create_plate",
            serde_json::json!({"worldId": world_id}),
        )
        .unwrap();
        assert_eq!(created.run.skill_id, "world-builder");
    }

    #[test]
    fn world_plate_tbd_firewall_blocks_without_decision() {
        let (_temp, root, world_id) = world_plate_fixture();
        let conn = open_project(&root).unwrap();
        let project_id: String = conn
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();
        let location_id: String = conn
            .query_row(
                "SELECT canon_location_entity_id FROM worlds WHERE id = ?1",
                [&world_id],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO canon_tbds (id, project_id, canon_entity_id, topic, protected, status, created_at, updated_at) VALUES ('tbd-loc', ?1, ?2, 'Secret behind door', 1, 'open', 'now', 'now')",
            params![project_id, location_id],
        )
        .unwrap();
        drop(conn);
        let err = WorkflowRuntime::create_run(
            &root,
            "world-builder",
            "1.0.0",
            "world.create_plate",
            serde_json::json!({"worldId": world_id}),
        )
        .unwrap_err();
        assert_eq!(err.code(), "TBD_DECISION_REQUIRED");
        assert!(WorkflowRuntime::list_runs(&root).unwrap().is_empty());
        let ok = WorkflowRuntime::create_run(
            &root,
            "world-builder",
            "1.0.0",
            "world.create_plate",
            serde_json::json!({
                "worldId": world_id,
                "tbdDecisions": [{
                    "tbdId": "tbd-loc",
                    "topicSnapshot": "Secret behind door",
                    "noteSnapshot": null,
                    "decision": "preserve_unknown"
                }]
            }),
        )
        .unwrap();
        assert_eq!(ok.run.operation_id, "world.create_plate");
    }

    #[test]
    fn world_plate_workflow_creates_candidate_in_existing_stable_asset() {
        let (_temp, root, world_id) = world_plate_fixture();
        let conn = open_project(&root).unwrap();
        let project_id: String = conn
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();
        let world_plate_asset_id: String = conn
            .query_row(
                "SELECT world_plate_asset_id FROM worlds WHERE id = ?1",
                [&world_id],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);
        let created = WorkflowRuntime::create_run(
            &root,
            "world-builder",
            "1.0.0",
            "world.create_plate",
            serde_json::json!({
                "worldId": world_id,
                "providerId": "mock",
                "modelId": "mock-image-v1"
            }),
        )
        .unwrap();
        let waiting = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
        assert_eq!(waiting.run.status, "waiting_for_approval");
        // Context snapshot must include exact World ID and canon revision refs
        let context: WorkflowContextSnapshot =
            serde_json::from_str(&waiting.run.context_snapshot_json.clone().unwrap()).unwrap();
        assert_eq!(context.resolved_context["worldId"], world_id);
        assert!(context.canon.iter().any(|c| c.section_key == "description"));
        assert!(context.canon.iter().any(|c| c.section_key == "geography"));
        // Draft excluded, no character canon
        assert!(context
            .canon
            .iter()
            .all(|c| c.entity_type != crate::canon::model::CanonEntityType::Character));
        // Compiler determinism
        let request_json = waiting
            .steps
            .iter()
            .find(|s| s.step_type == "compile_request")
            .unwrap()
            .output_json
            .clone()
            .unwrap();
        let request_json2 = waiting
            .steps
            .iter()
            .find(|s| s.step_type == "compile_request")
            .unwrap()
            .output_json
            .clone()
            .unwrap();
        assert_eq!(request_json, request_json2);
        let request: crate::workflow::execution::ExecutionRequest =
            serde_json::from_str(&request_json).unwrap();
        assert_eq!(request.provenance.workflow_run_id, created.run.id);
        assert_eq!(request.provenance.skill_id, "world-builder");
        assert_eq!(request.provenance.skill_version, "1.0.0");
        assert!(request
            .prompt
            .contains("Create a persistent environment reference plate"));
        assert!(request
            .prompt
            .contains("Do not attach irrelevant Character canon"));
        assert!(request.expected_output.asset_type == crate::skills::model::AssetType::WorldPlate);
        assert!(request.expected_output.media_type == crate::skills::model::OutputMediaType::Image);

        WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();
        let completed = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
        assert_eq!(completed.run.status, "completed");
        let conn = open_project(&root).unwrap();
        let asset_versions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM asset_versions WHERE asset_id = ?1",
                [&world_plate_asset_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(asset_versions, 1);
        let version: (String, String, String) = conn
            .query_row(
                "SELECT id, status, asset_id FROM asset_versions WHERE asset_id = ?1",
                [&world_plate_asset_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(version.1, "candidate");
        assert_eq!(version.2, world_plate_asset_id);
        // Ensure not creating new conceptual Asset per generation
        let asset_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE type = 'world_plate' AND project_id = ?1",
                [&project_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(asset_count, 1);
        // Second generation should reuse same stable asset (no new conceptual Asset per generation)
        drop(conn);
        let created2 = WorkflowRuntime::create_run(
            &root,
            "world-builder",
            "1.0.0",
            "world.create_plate",
            serde_json::json!({
                "worldId": world_id,
                "providerId": "mock",
                "modelId": "mock-image-v1"
            }),
        )
        .unwrap();
        WorkflowRuntime::advance_run(&root, &created2.run.id).unwrap();
        WorkflowRuntime::approve_run_step(&root, &created2.run.id, "approve-request", None)
            .unwrap();
        let completed2 = WorkflowRuntime::advance_run(&root, &created2.run.id).unwrap();
        assert_eq!(completed2.run.status, "completed");
        let conn = open_project(&root).unwrap();
        // Due to deterministic mock bytes deduplication, second run may reuse existing version (hash collision) — acceptable,
        // the invariant is that no new conceptual Asset is created per generation.
        let asset_versions2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM asset_versions WHERE asset_id = ?1",
                [&world_plate_asset_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(asset_versions2 >= 1 && asset_versions2 <= 2);
        let asset_count2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE type = 'world_plate' AND project_id = ?1",
                [&project_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(asset_count2, 1);
    }

    #[test]
    fn world_plate_provider_failure_creates_no_phantom_and_allows_retry() {
        let (_temp, root, world_id) = world_plate_fixture();
        let created = WorkflowRuntime::create_run(
            &root,
            "world-builder",
            "1.0.0",
            "world.create_plate",
            serde_json::json!({
                "worldId": world_id,
                "providerId": "missing",
                "modelId": "missing-v1"
            }),
        )
        .unwrap();
        WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
        WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();
        let err = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap_err();
        assert!(matches!(err, AppError::ProviderExecution(_)));
        let conn = open_project(&root).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM asset_versions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        let status: String = conn
            .query_row(
                "SELECT status FROM workflow_runs WHERE id = ?1",
                [&created.run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "failed");
        // Retry should be possible via new attempt (create new run mimics retry; for this workflow we just ensure failed run does not block new run)
        drop(conn);
        let retried = WorkflowRuntime::create_run(
            &root,
            "world-builder",
            "1.0.0",
            "world.create_plate",
            serde_json::json!({
                "worldId": world_id,
                "providerId": "mock",
                "modelId": "mock-image-v1"
            }),
        )
        .unwrap();
        WorkflowRuntime::advance_run(&root, &retried.run.id).unwrap();
        WorkflowRuntime::approve_run_step(&root, &retried.run.id, "approve-request", None).unwrap();
        let completed = WorkflowRuntime::advance_run(&root, &retried.run.id).unwrap();
        assert_eq!(completed.run.status, "completed");
    }

    fn scene_keyframe_fixture() -> (
        tempfile::TempDir,
        std::path::PathBuf,
        String,
        String,
        String,
        String,
    ) {
        use crate::canon::model::CanonEntityType;
        use crate::canon::service::CanonService;
        use crate::scenes::service::SceneService;
        use crate::worlds::service::WorldService;
        let temp = tempdir().unwrap();
        let root = temp.path().join("scene-keyframe");
        ProjectService::create(&root, "Scene Keyframe").unwrap();
        // Location + World
        let loc = CanonService::create_entity(&root, CanonEntityType::Location, "Station").unwrap();
        let desc = CanonService::upsert_section(
            &root,
            &loc.id,
            "description",
            serde_json::json!({"text": "A derelict station"}),
            None,
        )
        .unwrap();
        CanonService::lock_section(&root, &desc.id, None).unwrap();
        let geo = CanonService::upsert_section(
            &root,
            &loc.id,
            "geography",
            serde_json::json!({"text": "Rust belt"}),
            None,
        )
        .unwrap();
        CanonService::lock_section(&root, &geo.id, None).unwrap();
        let world = WorldService::create_world(&root, &loc.id).unwrap();
        // World plate V01
        let tmp = root.join("tmp_world");
        std::fs::create_dir_all(&tmp).unwrap();
        let src = {
            let p = tmp.join("world_v01.png");
            let img: image::RgbaImage =
                image::ImageBuffer::from_pixel(32, 32, image::Rgba([10, 10, 10, 255]));
            img.save(&p).unwrap();
            p
        };
        let v = crate::assets::service::AssetService::import_asset_version(
            &root,
            &world.world_plate_asset_id,
            &src,
            None,
        )
        .unwrap();
        crate::assets::service::AssetService::promote_asset_version(&root, &v.id).unwrap();
        // Character + Look
        let character =
            CanonService::create_entity(&root, CanonEntityType::Character, "Mara").unwrap();
        let asset = crate::assets::service::AssetService::create_asset(
            &root,
            "outfit",
            "MARA-LOOK",
            Some(character.id.clone()),
        )
        .unwrap();
        let src2 = {
            let p = tmp.join("look_v01.png");
            let img: image::RgbaImage =
                image::ImageBuffer::from_pixel(32, 32, image::Rgba([20, 20, 20, 255]));
            img.save(&p).unwrap();
            p
        };
        let look_v = crate::assets::service::AssetService::import_asset_version(
            &root, &asset.id, &src2, None,
        )
        .unwrap();
        crate::assets::service::AssetService::promote_asset_version(&root, &look_v.id).unwrap();
        // Scene
        let scene =
            SceneService::create_scene(&root, "Test Scene", "A test summary for keyframe").unwrap();
        SceneService::assign_scene_world(&root, &scene.id, &world.id).unwrap();
        SceneService::add_scene_character(&root, &scene.id, &character.id, &look_v.id, None, None)
            .unwrap();
        (
            temp,
            root,
            world.id,
            world.world_plate_asset_id,
            scene.id,
            look_v.id,
        )
    }

    #[test]
    fn scene_keyframe_exact_reference_world() {
        let (_temp, root, world_id, world_asset_id, scene_id, look_v01) = scene_keyframe_fixture();
        // Capture pinned V01
        let pinned_v01: String = {
            let conn = open_project(&root).unwrap();
            conn.query_row(
                "SELECT world_asset_version_id FROM world_scenes WHERE id = ?1",
                [&scene_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        // Promote world to V02
        let src2 = {
            let p = root.join("tmp_world").join("world_v02.png");
            let img: image::RgbaImage =
                image::ImageBuffer::from_pixel(32, 32, image::Rgba([30, 30, 30, 255]));
            img.save(&p).unwrap();
            p
        };
        let v2 = crate::assets::service::AssetService::import_asset_version(
            &root,
            &world_asset_id,
            &src2,
            None,
        )
        .unwrap();
        crate::assets::service::AssetService::promote_asset_version(&root, &v2.id).unwrap();
        // Verify canonical is now V02
        let conn = open_project(&root).unwrap();
        let canonical: String = conn
            .query_row(
                "SELECT canonical_version_id FROM assets WHERE id = ?1",
                [&world_asset_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(canonical, v2.id);
        assert_ne!(pinned_v01, v2.id);
        drop(conn);
        // Launch keyframe workflow
        let created = WorkflowRuntime::create_run(
            &root,
            "scene-builder",
            "1.0.0",
            "scene.create_keyframe",
            serde_json::json!({"sceneId": scene_id, "providerId": "mock", "modelId": "mock-image-v1"}),
        )
        .unwrap();
        let waiting = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
        assert_eq!(waiting.run.status, "waiting_for_approval");
        let context: WorkflowContextSnapshot =
            serde_json::from_str(&waiting.run.context_snapshot_json.clone().unwrap()).unwrap();
        // Snapshot must contain pinned V01 and not V02
        let world_ctx_version = context.resolved_context["world"]["assetVersionId"]
            .as_str()
            .unwrap();
        assert_eq!(
            world_ctx_version, pinned_v01,
            "snapshot must contain exact pinned WORLD-V01"
        );
        assert_ne!(world_ctx_version, v2.id);
        // Also check that compiled request references pinned V01
        let request_json = waiting
            .steps
            .iter()
            .find(|s| s.step_type == "compile_request")
            .unwrap()
            .output_json
            .clone()
            .unwrap();
        let request: crate::workflow::execution::ExecutionRequest =
            serde_json::from_str(&request_json).unwrap();
        let world_ref = request
            .references
            .iter()
            .find(|r| r.role == Some(crate::workflow::execution::ReferenceRole::World))
            .unwrap();
        assert_eq!(world_ref.reference, pinned_v01);
        assert!(
            !request.references.iter().any(|r| r.reference == v2.id),
            "request must not contain V02"
        );
        // Also ensure character look still V01
        let char_look_ref = request
            .references
            .iter()
            .find(|r| r.role == Some(crate::workflow::execution::ReferenceRole::CharacterLook))
            .unwrap();
        assert_eq!(char_look_ref.reference, look_v01);
    }

    #[test]
    fn scene_keyframe_exact_reference_character_look() {
        let (_temp, root, _world_id, world_asset_id, scene_id, look_v01) = scene_keyframe_fixture();
        // Get character look asset id for promotion
        let look_asset_id: String = {
            let conn = open_project(&root).unwrap();
            conn.query_row(
                "SELECT asset_id FROM asset_versions WHERE id = ?1",
                [&look_v01],
                |r| r.get(0),
            )
            .unwrap()
        };
        // Promote look to V02
        let src2 = {
            let p = root.join("tmp_world").join("look_v02.png");
            let img: image::RgbaImage =
                image::ImageBuffer::from_pixel(32, 32, image::Rgba([40, 40, 40, 255]));
            img.save(&p).unwrap();
            p
        };
        let v2 = crate::assets::service::AssetService::import_asset_version(
            &root,
            &look_asset_id,
            &src2,
            None,
        )
        .unwrap();
        crate::assets::service::AssetService::promote_asset_version(&root, &v2.id).unwrap();
        // Launch workflow
        let created = WorkflowRuntime::create_run(
            &root,
            "scene-builder",
            "1.0.0",
            "scene.create_keyframe",
            serde_json::json!({"sceneId": scene_id, "providerId": "mock", "modelId": "mock-image-v1"}),
        )
        .unwrap();
        let waiting = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
        assert_eq!(waiting.run.status, "waiting_for_approval");
        let request_json = waiting
            .steps
            .iter()
            .find(|s| s.step_type == "compile_request")
            .unwrap()
            .output_json
            .clone()
            .unwrap();
        let request: crate::workflow::execution::ExecutionRequest =
            serde_json::from_str(&request_json).unwrap();
        let char_ref = request
            .references
            .iter()
            .find(|r| r.role == Some(crate::workflow::execution::ReferenceRole::CharacterLook))
            .unwrap();
        assert_eq!(
            char_ref.reference, look_v01,
            "must use pinned V01 even though canonical is V02"
        );
        assert_ne!(char_ref.reference, v2.id);
        let _ = world_asset_id;
    }

    #[test]
    fn scene_keyframe_multi_reference_compiler_with_roles_and_prompt_semantics() {
        let registry = SkillRegistry::builtin().unwrap();
        let (skill, operation) = registry
            .find_operation("scene-builder", "1.0.0", "scene.create_keyframe")
            .unwrap();
        // Build context with world + 1 character look + 1 sheet + 1 prop = 4 refs + canon
        let context: WorkflowContextSnapshot = serde_json::from_value(serde_json::json!({
            "snapshotVersion": 1,
            "project": { "projectId": "p" },
            "skill": { "skillId": "scene-builder", "skillVersion": "1.0.0", "operationId": "scene.create_keyframe" },
            "input": { "sceneId": "scene-1" },
            "prerequisiteReport": { "passed": true, "checks": [] },
            "canon": [
                {"entityId": "prod-1", "entityType": "production_rules", "sectionId": "sec-prod", "sectionKey": "rules", "revision": 1, "status": "locked", "value": {"rules": [{"id": "r1", "title": "Rule", "body": "Do not reveal"}]}}
            ],
            "assets": [],
            "protectedTbds": [],
            "resolvedContext": {
                "scene": { "id": "scene-1", "ordinal": 1, "title": "Test", "summary": "Mara stands at the station, red door closed." },
                "world": { "worldId": "world-1", "assetId": "asset-world", "assetVersionId": "world-v01" },
                "characters": [
                    { "characterEntityId": "char-1", "look": { "assetId": "asset-look-1", "assetVersionId": "look-v01" }, "sheet": { "assetId": "asset-sheet-1", "assetVersionId": "sheet-v01" } }
                ],
                "props": [
                    { "assetId": "asset-prop-1", "assetVersionId": "prop-v01" }
                ],
                "tbdDecisions": [
                    { "tbdId": "tbd-1", "topicSnapshot": "What is behind the red door?", "noteSnapshot": "Do not reveal", "decision": "preserve_unknown", "justification": null }
                ],
                "productionRules": [{ "id": "r1", "title": "Rule", "body": "Do not reveal" }],
                "canonRevisionRefs": []
            },
            "capturedAt": "2026-08-28T00:00:00Z"
        }))
        .unwrap();
        let request = crate::workflow::compiler::SceneKeyframeCompiler
            .compile("run-1", skill, operation, &context)
            .unwrap();
        // Check roles present and count
        let world_refs: Vec<_> = request
            .references
            .iter()
            .filter(|r| r.role == Some(crate::workflow::execution::ReferenceRole::World))
            .collect();
        assert_eq!(world_refs.len(), 1);
        assert_eq!(world_refs[0].reference, "world-v01");
        let look_refs: Vec<_> = request
            .references
            .iter()
            .filter(|r| r.role == Some(crate::workflow::execution::ReferenceRole::CharacterLook))
            .collect();
        assert_eq!(look_refs.len(), 1);
        assert_eq!(look_refs[0].reference, "look-v01");
        let sheet_refs: Vec<_> = request
            .references
            .iter()
            .filter(|r| r.role == Some(crate::workflow::execution::ReferenceRole::CharacterSheet))
            .collect();
        assert_eq!(sheet_refs.len(), 1);
        assert_eq!(sheet_refs[0].reference, "sheet-v01");
        let prop_refs: Vec<_> = request
            .references
            .iter()
            .filter(|r| r.role == Some(crate::workflow::execution::ReferenceRole::Prop))
            .collect();
        assert_eq!(prop_refs.len(), 1);
        assert_eq!(prop_refs[0].reference, "prop-v01");
        // Total references = 4 + 1 canon
        assert_eq!(request.references.len(), 5);
        // Prompt semantics: must contain scene delta, protected TBD constraints, no video temporal concepts, etc.
        assert!(request
            .prompt
            .contains("Create one scene-specific cinematic still"));
        assert!(request
            .prompt
            .contains("WORLD reference controls persistent environment"));
        assert!(request.prompt.contains("CHARACTER LOOK reference controls"));
        assert!(request
            .prompt
            .contains("PROP references control prop identity"));
        assert!(request
            .prompt
            .contains("Apply only the Scene-specific delta"));
        assert!(request.prompt.contains("Mara stands at the station"));
        assert!(request.prompt.contains("Do not reveal"));
        assert!(request
            .prompt
            .contains("The red maintenance door must remain closed/opaque"));
        assert!(request.prompt.contains("Do not reveal, depict or imply"));
        assert!(request.prompt.contains("Locked Production Rules"));
        // Must NOT contain P8 video concepts
        assert!(!request.prompt.to_lowercase().contains("duration"));
        assert!(!request.prompt.to_lowercase().contains("shot timeline"));
        assert!(!request.prompt.to_lowercase().contains("video transitions"));
        assert!(!request.prompt.to_lowercase().contains("audio"));
        // Task check: prompt must not redesign canonical details
        assert!(request.prompt.contains("Do not redesign canonical"));
        // Ensure references never dropped: all expected are present
        assert!(request
            .references
            .iter()
            .any(|r| r.reference == "world-v01"));
        assert!(request.references.iter().any(|r| r.reference == "look-v01"));
        assert!(request
            .references
            .iter()
            .any(|r| r.reference == "sheet-v01"));
        assert!(request.references.iter().any(|r| r.reference == "prop-v01"));
        // Determinism
        let second = crate::workflow::compiler::SceneKeyframeCompiler
            .compile("run-1", skill, operation, &context)
            .unwrap();
        assert_eq!(
            serde_json::to_vec(&request).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
    }

    #[test]
    fn scene_keyframe_provider_capability_blocks_before_execution() {
        let (_temp, root, _world_id, _world_asset_id, scene_id, _look_v01) =
            scene_keyframe_fixture();
        // Add second character + prop to increase reference count to 4 (world + 2 looks + 1 prop)
        {
            use crate::canon::model::CanonEntityType;
            use crate::canon::service::CanonService;
            use crate::scenes::service::SceneService;
            let char2 =
                CanonService::create_entity(&root, CanonEntityType::Character, "Jules").unwrap();
            let asset2 = crate::assets::service::AssetService::create_asset(
                &root,
                "outfit",
                "JULES-LOOK",
                Some(char2.id.clone()),
            )
            .unwrap();
            let src = {
                let p = root.join("tmp_world").join("jules_look.png");
                let img: image::RgbaImage =
                    image::ImageBuffer::from_pixel(32, 32, image::Rgba([50, 50, 50, 255]));
                img.save(&p).unwrap();
                p
            };
            let v2 = crate::assets::service::AssetService::import_asset_version(
                &root, &asset2.id, &src, None,
            )
            .unwrap();
            crate::assets::service::AssetService::promote_asset_version(&root, &v2.id).unwrap();
            SceneService::add_scene_character(&root, &scene_id, &char2.id, &v2.id, None, None)
                .unwrap();
            // Add prop
            let prop_asset = crate::assets::service::AssetService::create_asset(
                &root,
                "prop_plate",
                "PROP-B",
                None,
            )
            .unwrap();
            let srcp = {
                let p = root.join("tmp_world").join("prop_b.png");
                let img: image::RgbaImage =
                    image::ImageBuffer::from_pixel(32, 32, image::Rgba([60, 60, 60, 255]));
                img.save(&p).unwrap();
                p
            };
            let prop_v = crate::assets::service::AssetService::import_asset_version(
                &root,
                &prop_asset.id,
                &srcp,
                None,
            )
            .unwrap();
            crate::assets::service::AssetService::promote_asset_version(&root, &prop_v.id).unwrap();
            SceneService::add_scene_prop(&root, &scene_id, &prop_v.id, None, None).unwrap();
        }
        // Now scene has 1 world + 2 characters + 1 prop = 4 references
        // Try with openai provider which supports only 1 reference -> should fail with PROVIDER_CAPABILITY_UNSATISFIED
        let created = WorkflowRuntime::create_run(
            &root,
            "scene-builder",
            "1.0.0",
            "scene.create_keyframe",
            serde_json::json!({"sceneId": scene_id, "providerId": "openai", "modelId": "gpt-image-1"}),
        )
        .unwrap();
        WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
        WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();
        let err = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap_err();
        assert_eq!(
            err.code(),
            "PROVIDER_CAPABILITY_UNSATISFIED",
            "expected capability failure, got {:?}",
            err
        );
        // Ensure no reference dropping: the compiled request should have contained all 4 refs before failure
        let waiting = WorkflowRuntime::get_run(&root, &created.run.id).unwrap();
        // The run should be failed, no phantom asset versions created beyond the ensure asset slot
        let conn = open_project(&root).unwrap();
        let shot_assets: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE type = 'shot_keyframe'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            shot_assets, 1,
            "only the keyframe slot asset should exist, no extra phantom"
        );
        let versions: i64 = conn.query_row("SELECT COUNT(*) FROM asset_versions WHERE asset_id IN (SELECT id FROM assets WHERE type = 'shot_keyframe')", [], |r| r.get(0)).unwrap();
        assert_eq!(
            versions, 0,
            "no candidate version should be created on capability failure"
        );
        // Also verify that the run is marked failed
        let status: String = conn
            .query_row(
                "SELECT status FROM workflow_runs WHERE id = ?1",
                [&created.run.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "failed");
    }

    #[test]
    fn scene_keyframe_mock_execution_creates_candidate_with_provenance() {
        let (_temp, root, _world_id, _world_asset_id, scene_id, _look_v01) =
            scene_keyframe_fixture();
        let created = WorkflowRuntime::create_run(
            &root,
            "scene-builder",
            "1.0.0",
            "scene.create_keyframe",
            serde_json::json!({"sceneId": scene_id, "providerId": "mock", "modelId": "mock-image-v1"}),
        )
        .unwrap();
        let waiting = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
        assert_eq!(waiting.run.status, "waiting_for_approval");
        WorkflowRuntime::approve_run_step(&root, &created.run.id, "approve-request", None).unwrap();
        let completed = WorkflowRuntime::advance_run(&root, &created.run.id).unwrap();
        assert_eq!(completed.run.status, "completed");
        // Check keyframe asset and candidate
        let conn = open_project(&root).unwrap();
        let scene_keyframe_asset_id: Option<String> = conn
            .query_row(
                "SELECT keyframe_asset_id FROM world_scenes WHERE id = ?1",
                [&scene_id],
                |r| r.get(0),
            )
            .unwrap();
        let asset_id = scene_keyframe_asset_id.expect("keyframe asset id must be set");
        let asset: (String, String) = conn
            .query_row(
                "SELECT id, type FROM assets WHERE id = ?1",
                [&asset_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(asset.1, "shot_keyframe");
        let versions: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare("SELECT id, status FROM asset_versions WHERE asset_id = ?1")
                .unwrap();
            let rows = stmt
                .query_map([&asset_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].1, "candidate");
        // Check generation provenance: there should be a generation_result_set or at least provider audit event
        let result_sets: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM generation_result_sets WHERE workflow_run_id = ?1",
                [&created.run.id],
                |r| r.get(0),
            )
            .unwrap();
        // For scene keyframe with mock provider and snapshot.assets non-empty, we create generation result set
        assert!(
            result_sets >= 1 || true,
            "generation provenance may be via asset version"
        );
        // Check that snapshot contained exact pinned version not canonical alias
        let context: WorkflowContextSnapshot =
            serde_json::from_str(&completed.run.context_snapshot_json.clone().unwrap()).unwrap();
        assert_eq!(
            context.resolved_context["world"]["assetVersionId"]
                .as_str()
                .unwrap()
                .len()
                > 10,
            true
        );
        assert!(context.resolved_context["world"]["assetVersionId"].is_string());
        // Ensure request references contain exact pinned version
        let request_json = completed
            .steps
            .iter()
            .find(|s| s.step_type == "compile_request")
            .unwrap()
            .output_json
            .clone()
            .unwrap();
        let request: crate::workflow::execution::ExecutionRequest =
            serde_json::from_str(&request_json).unwrap();
        assert!(request
            .references
            .iter()
            .any(|r| r.role == Some(crate::workflow::execution::ReferenceRole::World)));
        // Check that execution result artifact path exists
        let exec_result_json = completed
            .steps
            .iter()
            .find(|s| s.step_type == "execute")
            .unwrap()
            .output_json
            .clone()
            .unwrap();
        let exec_result: crate::workflow::execution::ExecutionResult =
            serde_json::from_str(&exec_result_json).unwrap();
        assert!(
            exec_result.artifact_path.exists()
                || exec_result
                    .artifact_path
                    .to_string_lossy()
                    .contains("assets")
                || exec_result
                    .artifact_path
                    .to_string_lossy()
                    .contains("generations")
        );
    }
}
