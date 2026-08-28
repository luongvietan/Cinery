use super::models::{QaCheckPlan, QaCheckResult, QaReviewStatus, QaRunDetail, QaRunRecord};
use super::normalizer::compute_overall;
use super::repository;
use crate::db;
use crate::error::AppError;
use crate::project::repository::read_project;
use chrono::Utc;
use rusqlite::Connection;
use std::path::Path;

pub struct QaService;

impl QaService {
    pub fn list_runs(
        project_root: &Path,
        asset_version_id: &str,
    ) -> Result<Vec<QaRunRecord>, AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let project = read_project(&conn)?;
        repository::list_runs_for_asset_version(&conn, &project.id, asset_version_id)
    }

    pub fn get_run(project_root: &Path, qa_run_id: &str) -> Result<QaRunDetail, AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let project = read_project(&conn)?;
        repository::get_run(&conn, &project.id, qa_run_id)?.ok_or(AppError::QaRunNotFound)
    }

    pub fn review_run_check(
        project_root: &Path,
        qa_run_id: &str,
        check_id: &str,
        review_status: QaReviewStatus,
        note: Option<&str>,
    ) -> Result<QaRunDetail, AppError> {
        let conn = db::open_existing_connection(&project_root.join("project.db"))?;
        let project = read_project(&conn)?;
        Self::review_check(&conn, &project.id, qa_run_id, check_id, review_status, note)
    }

    pub fn review_check(
        conn: &Connection,
        project_id: &str,
        qa_run_id: &str,
        check_id: &str,
        review_status: QaReviewStatus,
        note: Option<&str>,
    ) -> Result<QaRunDetail, AppError> {
        let existing =
            repository::get_run(conn, project_id, qa_run_id)?.ok_or(AppError::QaRunNotFound)?;
        if existing.run.status != super::models::QaRunStatus::Succeeded {
            return Err(AppError::InvalidQaData(
                "only succeeded QA runs can be reviewed".into(),
            ));
        }
        repository::review_check(
            conn,
            project_id,
            qa_run_id,
            check_id,
            review_status,
            note,
            &Utc::now().to_rfc3339(),
        )?;
        let reviewed =
            repository::get_run(conn, project_id, qa_run_id)?.ok_or(AppError::QaRunNotFound)?;
        let plan: QaCheckPlan = serde_json::from_value(reviewed.run.check_plan.clone())
            .map_err(|error| AppError::InvalidQaData(error.to_string()))?;
        let results = reviewed
            .checks
            .iter()
            .map(|check| QaCheckResult {
                check_id: check.check_id.clone(),
                status: check.effective_status(),
                confidence: check.confidence,
                observed: check.observed.clone(),
                reason: check.reason.clone(),
                repair_hint: check.repair_hint.clone(),
            })
            .collect::<Vec<_>>();
        let overall = compute_overall(&plan, &results)?;
        repository::update_overall_status(conn, qa_run_id, overall)?;
        repository::get_run(conn, project_id, qa_run_id)?.ok_or(AppError::QaRunNotFound)
    }
}
