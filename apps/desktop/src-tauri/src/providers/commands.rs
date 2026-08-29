use super::credential_store::KeyringCredentialStore;
use super::model::{CustomProviderDefinition, ProviderCapabilities};
use super::registry::ProviderRegistry;
use super::repository::ProviderConfigRecord;
use super::service::{ProviderConfigurationStatus, ProviderService};
use crate::db;
use crate::error::{AppCommandError, AppError};
use crate::project::repository::read_project;
use crate::project::service::validate_root_path;
use crate::workflow::model::WorkflowRunDetail;
use crate::workflow::repository::WorkflowRepository;
use crate::workflow::runtime::WorkflowRuntime;
use std::path::PathBuf;
use std::sync::Arc;

/// The process-wide credential store. Tests inject their own store through
/// the service API directly; the Tauri command surface always uses the real
/// OS-backed vault.
fn command_credential_store() -> Arc<KeyringCredentialStore> {
    ProviderService::default_credential_store()
}

#[tauri::command]
pub fn list_providers(project_root_path: Option<String>) -> Result<Vec<String>, AppCommandError> {
    let mut providers = ProviderService::list_provider_ids();
    if let Some(path) = project_root_path {
        validate_root_path(&path)?;
        providers.extend(
            ProviderService::list_custom_providers(&PathBuf::from(path))?
                .into_iter()
                .map(|provider| provider.provider_id),
        );
    }
    Ok(providers)
}

#[tauri::command]
pub fn list_custom_providers(
    project_root_path: String,
) -> Result<Vec<CustomProviderDefinition>, AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::list_custom_providers(&PathBuf::from(project_root_path)).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_custom_provider(
    project_root_path: String,
    definition: CustomProviderDefinition,
) -> Result<CustomProviderDefinition, AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::upsert_custom_provider(
        &PathBuf::from(project_root_path),
        command_credential_store().as_ref(),
        &definition,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn delete_custom_provider(
    project_root_path: String,
    provider_id: String,
) -> Result<(), AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::delete_custom_provider(
        &PathBuf::from(project_root_path),
        command_credential_store().as_ref(),
        &provider_id,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn get_provider_capabilities(
    provider_id: String,
) -> Result<ProviderCapabilities, AppCommandError> {
    let provider = ProviderRegistry::builtin()
        .get(&provider_id)
        .map_err(|error| crate::error::AppError::ProviderExecution(error.message))?;
    Ok(provider.capabilities())
}

#[tauri::command]
pub fn get_provider_configuration_status(
    project_root_path: String,
    provider_id: String,
) -> Result<ProviderConfigurationStatus, AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::configuration_status(
        &PathBuf::from(project_root_path),
        command_credential_store().as_ref(),
        &provider_id,
    )
    .map_err(Into::into)
}

/// Accepts a secret at the command boundary. The secret is written to the OS
/// credential vault and never persisted or echoed back.
#[tauri::command]
pub fn save_provider_credential(
    project_root_path: String,
    provider_id: String,
    secret: String,
    default_model: Option<String>,
) -> Result<ProviderConfigurationStatus, AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::save_credential(
        &PathBuf::from(project_root_path),
        command_credential_store().as_ref(),
        &provider_id,
        &secret,
        default_model.as_deref(),
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn configure_provider(
    project_root_path: String,
    config: ProviderConfigRecord,
) -> Result<ProviderConfigurationStatus, AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::configure(&PathBuf::from(project_root_path), &config).map_err(Into::into)
}

#[tauri::command]
pub fn remove_provider_credentials(
    project_root_path: String,
    provider_id: String,
) -> Result<(), AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::remove_credential(
        &PathBuf::from(project_root_path),
        command_credential_store().as_ref(),
        &provider_id,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn validate_provider_configuration(
    project_root_path: String,
    provider_id: String,
) -> Result<(), AppCommandError> {
    validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    // Local providers are always valid; vault-backed providers must resolve.
    if provider_id == "mock" || provider_id == "dry_run" {
        ProviderRegistry::builtin()
            .get(&provider_id)
            .map_err(|error| AppError::ProviderExecution(error.message))?;
        return Ok(());
    }
    ProviderService::resolve_credential(&root, command_credential_store().as_ref(), &provider_id)
        .map(|_| ())
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_provider_models(
    provider_id: String,
    project_root_path: Option<String>,
) -> Result<Vec<String>, AppCommandError> {
    if let Some(path) = project_root_path {
        validate_root_path(&path)?;
        if let Some(custom) = ProviderService::list_custom_providers(&PathBuf::from(path))?
            .into_iter()
            .find(|provider| provider.provider_id == provider_id)
        {
            return Ok(custom.models.into_iter().map(|model| model.id).collect());
        }
    }
    ProviderService::models(&provider_id).map_err(Into::into)
}

#[tauri::command]
pub fn cancel_workflow_execution(
    project_root_path: String,
    workflow_run_id: String,
    step_id: String,
) -> Result<WorkflowRunDetail, AppCommandError> {
    validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    let conn = db::open_existing_connection(&root.join("project.db"))?;
    read_project(&conn)?;
    if let Some(attempt) =
        super::repository::find_active_attempt(&conn, &workflow_run_id, &step_id)?
    {
        if let Some(provider_job_id) = attempt.provider_job_id {
            let job = super::model::ProviderJobRef {
                provider_id: attempt.provider_id.clone(),
                provider_job_id,
                run_id: workflow_run_id.clone(),
                step_id: step_id.clone(),
                submission_id: attempt.idempotency_key.clone(),
                submitted_at: attempt.started_at.clone(),
            };
            let _ = ProviderService::cancel_job(&attempt.provider_id, &job)?;
        }
        super::repository::update_attempt_status(&conn, &attempt.id, "cancelled", None)?;
    }
    drop(conn);
    WorkflowRuntime::cancel_run(&root, &workflow_run_id).map_err(Into::into)
}

#[tauri::command]
pub fn retry_workflow_execution(
    project_root_path: String,
    workflow_run_id: String,
    step_id: String,
) -> Result<WorkflowRunDetail, AppCommandError> {
    validate_root_path(&project_root_path)?;
    let root = PathBuf::from(project_root_path);
    let conn = db::open_existing_connection(&root.join("project.db"))?;
    let project = read_project(&conn)?;
    let previous = super::repository::latest_attempt(&conn, &workflow_run_id, &step_id)?
        .ok_or_else(|| {
            crate::error::AppError::ProviderExecution("no execution attempt exists".into())
        })?;
    if previous.status != "failed" {
        return Err(crate::error::AppError::ProviderExecution(
            "only failed executions can be retried".into(),
        )
        .into());
    }
    let retry_number = super::repository::next_attempt_number(&conn, &workflow_run_id, &step_id)?;
    super::repository::create_attempt(
        &conn,
        &workflow_run_id,
        &step_id,
        retry_number,
        &previous.compiled_request_id,
        &previous.provider_id,
        &previous.model_id,
        &ProviderService::idempotency_key(&workflow_run_id, &step_id, retry_number),
    )?;
    conn.execute("UPDATE workflow_runs SET status = 'ready_for_execution', failure_code = NULL, failure_message = NULL, completed_at = NULL, updated_at = ?1 WHERE id = ?2", rusqlite::params![chrono::Utc::now().to_rfc3339(), workflow_run_id]).map_err(|error| crate::error::AppError::Database(error.to_string()))?;
    conn.execute("UPDATE workflow_steps SET status = 'pending', completed_at = NULL, output_json = NULL WHERE workflow_run_id = ?1 AND step_definition_id = ?2", rusqlite::params![workflow_run_id, step_id]).map_err(|error| crate::error::AppError::Database(error.to_string()))?;
    WorkflowRepository::get_run(&conn, &project.id, &workflow_run_id).map_err(Into::into)
}
