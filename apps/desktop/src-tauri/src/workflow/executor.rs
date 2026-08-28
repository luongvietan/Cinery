use crate::error::AppError;
use crate::workflow::artifacts::write_dry_run_result;
use crate::workflow::execution::{ExecutionRequest, ExecutionResult};
use std::path::Path;

pub trait ExecutionExecutor {
    fn kind(&self) -> &'static str;
    fn execute(
        &self,
        request: &ExecutionRequest,
        output_dir: &Path,
    ) -> Result<ExecutionResult, AppError>;
}

pub struct DryRunExecutor;

impl ExecutionExecutor for DryRunExecutor {
    fn kind(&self) -> &'static str {
        "dry_run"
    }
    fn execute(
        &self,
        request: &ExecutionRequest,
        output_dir: &Path,
    ) -> Result<ExecutionResult, AppError> {
        let project_root = output_dir
            .parent()
            .and_then(Path::parent)
            .unwrap_or(output_dir);
        write_dry_run_result(project_root, &request.provenance.workflow_run_id, request)
    }
}
