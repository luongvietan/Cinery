use crate::error::AppCommandError;
use crate::project::service::validate_root_path;
use crate::router::model::{RouteProductionIntentRequest, RouteProductionIntentResult};
use crate::router::service::ProductionRouter;
use std::path::{Path, PathBuf};

#[tauri::command]
pub fn route_production_intent(
    request: RouteProductionIntentRequest,
) -> Result<RouteProductionIntentResult, AppCommandError> {
    validate_root_path(&request.project_root_path)?;
    ProductionRouter::route(
        Path::new(&PathBuf::from(&request.project_root_path)),
        &request.text,
    )
    .map_err(AppCommandError::from)
}
