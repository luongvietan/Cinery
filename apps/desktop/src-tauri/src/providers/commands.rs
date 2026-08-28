use super::model::ProviderCapabilities;
use super::repository::ProviderConfigRecord;
use super::registry::ProviderRegistry;
use super::service::{ProviderConfigurationStatus, ProviderService};
use crate::error::AppCommandError;
use crate::project::service::validate_root_path;
use std::path::PathBuf;

#[tauri::command]
pub fn list_providers() -> Result<Vec<String>, AppCommandError> {
    Ok(ProviderService::list_provider_ids())
}

#[tauri::command]
pub fn get_provider_capabilities(provider_id: String) -> Result<ProviderCapabilities, AppCommandError> {
    let provider = ProviderRegistry::builtin().get(&provider_id).map_err(|error| crate::error::AppError::ProviderExecution(error.message))?;
    Ok(provider.capabilities())
}

#[tauri::command]
pub fn get_provider_configuration_status(project_root_path: String, provider_id: String) -> Result<ProviderConfigurationStatus, AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::configuration_status(&PathBuf::from(project_root_path), &provider_id).map_err(Into::into)
}

#[tauri::command]
pub fn configure_provider(project_root_path: String, config: ProviderConfigRecord) -> Result<ProviderConfigurationStatus, AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::configure(&PathBuf::from(project_root_path), &config).map_err(Into::into)
}

#[tauri::command]
pub fn remove_provider_credentials(project_root_path: String, provider_id: String) -> Result<(), AppCommandError> {
    validate_root_path(&project_root_path)?;
    ProviderService::remove_credential_reference(&PathBuf::from(project_root_path), &provider_id).map_err(Into::into)
}

#[tauri::command]
pub fn validate_provider_configuration(provider_id: String) -> Result<(), AppCommandError> {
    ProviderService::validate_configuration(&provider_id).map_err(Into::into)
}

#[tauri::command]
pub fn list_provider_models(provider_id: String) -> Result<Vec<String>, AppCommandError> {
    ProviderService::models(&provider_id).map_err(Into::into)
}
