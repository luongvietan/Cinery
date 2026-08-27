use crate::canon::export::{self, StoryBibleExportResult};
use crate::canon::model::{
    CanonEntityRecord, CanonEntityType, CanonSectionRecord, CanonSectionRevisionRecord,
    CanonSingletonsDto,
};
use crate::canon::service::{CanonEntityDetailDto, CanonService};
use crate::canon::tbd;
use crate::error::AppCommandError;
use crate::project::service as project_service;
use std::path::PathBuf;

#[tauri::command]
pub fn ensure_canon_singletons(
    project_root_path: String,
) -> Result<CanonSingletonsDto, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    CanonService::ensure_singletons(&PathBuf::from(project_root_path)).map_err(Into::into)
}

#[tauri::command]
pub fn create_canon_entity(
    project_root_path: String,
    entity_type: String,
    name: String,
) -> Result<CanonEntityRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let entity_type = CanonEntityType::from_str(&entity_type)
        .ok_or(crate::error::AppError::UnknownCanonSection)?;
    CanonService::create_entity(&PathBuf::from(project_root_path), entity_type, &name)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_canon_entities(
    project_root_path: String,
    entity_type: Option<String>,
) -> Result<Vec<CanonEntityRecord>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    let entity_type = entity_type
        .as_deref()
        .map(|value| {
            CanonEntityType::from_str(value).ok_or(crate::error::AppError::UnknownCanonSection)
        })
        .transpose()?;
    CanonService::list_entities(&PathBuf::from(project_root_path), entity_type).map_err(Into::into)
}

#[tauri::command]
pub fn get_canon_entity(
    project_root_path: String,
    entity_id: String,
) -> Result<CanonEntityDetailDto, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    CanonService::get_entity(&PathBuf::from(project_root_path), &entity_id).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_canon_section(
    project_root_path: String,
    entity_id: String,
    section_key: String,
    value: serde_json::Value,
    reason: Option<String>,
) -> Result<CanonSectionRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    CanonService::upsert_section(
        &PathBuf::from(project_root_path),
        &entity_id,
        &section_key,
        value,
        reason,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn lock_canon_section(
    project_root_path: String,
    section_id: String,
    reason: Option<String>,
) -> Result<CanonSectionRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    CanonService::lock_section(&PathBuf::from(project_root_path), &section_id, reason)
        .map_err(Into::into)
}

#[tauri::command]
pub fn unlock_canon_section(
    project_root_path: String,
    section_id: String,
    reason: Option<String>,
) -> Result<CanonSectionRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    CanonService::unlock_section(&PathBuf::from(project_root_path), &section_id, reason)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_canon_section_revisions(
    project_root_path: String,
    section_id: String,
) -> Result<Vec<CanonSectionRevisionRecord>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    CanonService::list_section_revisions(&PathBuf::from(project_root_path), &section_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn create_canon_tbd(
    project_root_path: String,
    canon_entity_id: Option<String>,
    section_key: Option<String>,
    topic: String,
    note: Option<String>,
    protected: bool,
) -> Result<crate::canon::model::CanonTbdRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    tbd::create(
        &PathBuf::from(project_root_path),
        canon_entity_id.as_deref(),
        section_key.as_deref(),
        &topic,
        note,
        protected,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn list_canon_tbds(
    project_root_path: String,
) -> Result<Vec<crate::canon::model::CanonTbdRecord>, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    tbd::list(&PathBuf::from(project_root_path)).map_err(Into::into)
}

#[tauri::command]
pub fn resolve_canon_tbd(
    project_root_path: String,
    tbd_id: String,
    resolution_text: String,
) -> Result<crate::canon::model::CanonTbdRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    tbd::resolve(&PathBuf::from(project_root_path), &tbd_id, &resolution_text).map_err(Into::into)
}

#[tauri::command]
pub fn reopen_canon_tbd(
    project_root_path: String,
    tbd_id: String,
) -> Result<crate::canon::model::CanonTbdRecord, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    tbd::reopen(&PathBuf::from(project_root_path), &tbd_id).map_err(Into::into)
}

#[tauri::command]
pub fn export_story_bible(
    project_root_path: String,
) -> Result<StoryBibleExportResult, AppCommandError> {
    project_service::validate_root_path(&project_root_path)?;
    export::export_story_bible(&PathBuf::from(project_root_path)).map_err(Into::into)
}
