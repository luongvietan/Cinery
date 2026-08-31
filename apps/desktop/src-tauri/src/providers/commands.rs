use super::credential_store::KeyringCredentialStore;
use super::model::{CustomProviderDefinition, ProviderCapabilities};
use super::registry::ProviderRegistry;
use super::repository::ProviderConfigRecord;
use super::http::UreqExecutor;
use super::service::{ProviderConfigurationStatus, ProviderConnectionTestResult, ProviderService};
use crate::db;
use crate::error::{AppCommandError, AppError};
use crate::project::repository::read_project;
use crate::project::service::validate_root_path;
use crate::workflow::model::WorkflowRunDetail;
use crate::workflow::repository::WorkflowRepository;
use crate::workflow::runtime::WorkflowRuntime;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// The process-wide credential store. Tests inject their own store through
/// the service API directly; the Tauri command surface always uses the real
/// OS-backed vault.
fn command_credential_store() -> Arc<KeyringCredentialStore> {
    ProviderService::default_credential_store()
}

/// The SIMPLE-mode preset catalog (internal presets excluded).
#[tauri::command]
pub fn list_provider_presets() -> Result<Vec<super::presets::ProviderPreset>, AppCommandError> {
    Ok(super::presets::all_presets()
        .into_iter()
        .filter(|preset| !preset.internal)
        .collect())
}

#[tauri::command]
pub fn list_providers(project_root_path: Option<String>) -> Result<Vec<String>, AppCommandError> {
    // Built-in registry providers are always available; project-scoped custom
    // providers are merged in. Returning only customs (the previous behavior)
    // left every generation form's provider picker empty on fresh projects.
    let mut providers = ProviderService::list_provider_ids();
    if let Some(path) = project_root_path {
        validate_root_path(&path)?;
        for provider in ProviderService::list_custom_providers(
            &PathBuf::from(path),
            command_credential_store().as_ref(),
        )? {
            let id = provider.provider_id;
            if !providers.contains(&id) {
                providers.push(id);
            }
        }
    }
    Ok(providers)
}

#[tauri::command]
pub fn list_custom_providers(
    project_root_path: String,
) -> Result<Vec<CustomProviderDefinition>, AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::list_custom_providers(
        &PathBuf::from(project_root_path),
        command_credential_store().as_ref(),
    )
    .map_err(Into::into)
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
pub async fn test_custom_provider_connection(
    project_root_path: String,
    provider_id: String,
) -> Result<ProviderConnectionTestResult, AppCommandError> {
    validate_root_path(&project_root_path)?;
    tauri::async_runtime::spawn_blocking(move || {
        // Validation must not follow redirects: credentials never reach a
        // redirect target.
        let transport = UreqExecutor::without_redirects(Duration::from_secs(10));
        ProviderService::test_connection(
            &PathBuf::from(project_root_path),
            command_credential_store().as_ref(),
            transport,
            &provider_id,
        )
        .map_err(Into::into)
    })
    .await
    .map_err(|_| {
        AppCommandError::from(AppError::ProviderExecution(
            "connection test task failed".into(),
        ))
    })?
}

#[tauri::command]
pub fn get_provider_capabilities(
    provider_id: String,
    project_root_path: Option<String>,
) -> Result<ProviderCapabilities, AppCommandError> {
    if let Some(path) = project_root_path {
        validate_root_path(&path)?;
        return ProviderService::capabilities_for(&PathBuf::from(path), &provider_id)
            .map_err(Into::into);
    }
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
pub fn suggest_visual_spec(
    project_root_path: String,
    character_name: String,
    notes: Option<String>,
) -> Result<serde_json::Value, AppCommandError> {
    validate_root_path(&project_root_path)?;
    super::llm::suggest_visual_spec(
        &PathBuf::from(project_root_path),
        Some(command_credential_store().as_ref()),
        &character_name,
        notes.as_deref().unwrap_or(""),
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn list_provider_models(
    provider_id: String,
    project_root_path: Option<String>,
) -> Result<Vec<String>, AppCommandError> {
    if let Some(path) = project_root_path {
        validate_root_path(&path)?;
        if let Some(custom) = ProviderService::list_custom_providers(
            &PathBuf::from(path),
            command_credential_store().as_ref(),
        )?
        .into_iter()
        .find(|provider| provider.provider_id == provider_id)
        {
            return Ok(custom.models.into_iter().map(|model| model.id).collect());
        }
    }
    ProviderService::models(&provider_id).map_err(Into::into)
}

#[tauri::command]
pub fn list_provider_jobs(
    project_root_path: String,
) -> Result<Vec<crate::workflow::background::ProviderJobView>, AppCommandError> {
    validate_root_path(&project_root_path)?;
    crate::workflow::background::list_provider_jobs(&PathBuf::from(project_root_path))
        .map_err(Into::into)
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
        // Is there a durable provider_jobs row the runner owns? When there
        // is, cancellation is requested durably and the runner resolves it
        // (asking the provider to cancel, persisting truthful state). When
        // there is not, no runner will ever observe a request, so the
        // cancellation resolves here exactly as before.
        let durable_job_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM provider_jobs WHERE execution_id = ?1)",
                rusqlite::params![attempt.id],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if let Some(provider_job_id) = attempt.provider_job_id.clone() {
            let job = super::model::ProviderJobRef {
                provider_id: attempt.provider_id.clone(),
                provider_job_id,
                run_id: workflow_run_id.clone(),
                step_id: step_id.clone(),
                submission_id: attempt.idempotency_key.clone(),
                submitted_at: attempt.started_at.clone(),
                operation: None,
            };
            // Wake any in-flight legacy polling loop so the wait ends
            // immediately.
            super::cancellation::signal(&attempt.provider_id, &job.provider_job_id);
            if !durable_job_exists {
                let _ = ProviderService::cancel_job(&attempt.provider_id, &job)?;
            }
        }
        if durable_job_exists {
            // P10.1 durable cancellation: mark the request durably so the
            // background runner observes it on its next tick, asks the
            // provider to cancel when it can, and persists truthful
            // terminal state. Terminal-state safe: a completion that
            // landed first keeps its terminal status and this request
            // becomes a no-op.
            super::repository::request_attempt_cancellation(&conn, &attempt.id)?;
        } else {
            super::repository::update_attempt_status(&conn, &attempt.id, "cancelled", None)?;
        }
        super::repository::append_audit_event(
            &conn,
            Some(&attempt.id),
            &workflow_run_id,
            "provider.execution.cancel_requested",
            None,
        )?;
    }
    drop(conn);
    crate::workflow::background::wake_runner(&root);
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
    let mut conn = db::open_existing_connection(&root.join("project.db"))?;
    let project = read_project(&conn)?;
    // P10.1: the whole retry (guard + attempt creation + run/step reset)
    // happens in one Immediate transaction so two rapid clicks cannot
    // create conflicting attempt numbers or surface raw SQLite errors:
    // the second click's guard sees the run already reset and fails
    // cleanly.
    let now = chrono::Utc::now().to_rfc3339();
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| crate::error::AppError::Database(error.to_string()))?;
    let previous =
        super::repository::latest_attempt(&tx, &workflow_run_id, &step_id)?.ok_or_else(|| {
            crate::error::AppError::ProviderExecution("no execution attempt exists".into())
        })?;
    if previous.status != "failed" {
        return Err(crate::error::AppError::ProviderExecution(
            "only failed executions can be retried".into(),
        )
        .into());
    }
    let retry_number = super::repository::next_attempt_number(&tx, &workflow_run_id, &step_id)?;
    super::repository::create_attempt(
        &tx,
        &workflow_run_id,
        &step_id,
        retry_number,
        &previous.compiled_request_id,
        &previous.provider_id,
        &previous.model_id,
        &ProviderService::idempotency_key(&workflow_run_id, &step_id, retry_number),
    )?;
    tx.execute(
        "UPDATE workflow_runs SET status = 'ready_for_execution', failure_code = NULL,
           failure_message = NULL, completed_at = NULL, updated_at = ?1
         WHERE id = ?2 AND status = 'failed'",
        rusqlite::params![now, workflow_run_id],
    )
    .map_err(|error| crate::error::AppError::Database(error.to_string()))?;
    tx.execute(
        "UPDATE workflow_steps SET status = 'pending', completed_at = NULL, output_json = NULL
         WHERE workflow_run_id = ?1 AND step_definition_id = ?2 AND status = 'failed'",
        rusqlite::params![workflow_run_id, step_id],
    )
    .map_err(|error| crate::error::AppError::Database(error.to_string()))?;
    tx.commit()
        .map_err(|error| crate::error::AppError::Database(error.to_string()))?;
    WorkflowRepository::get_run(&conn, &project.id, &workflow_run_id).map_err(Into::into)
}
