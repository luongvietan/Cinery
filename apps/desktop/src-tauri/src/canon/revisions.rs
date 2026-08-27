use crate::canon::model::CanonSectionRevisionRecord;
use crate::canon::repository;
use crate::error::AppError;
use rusqlite::Connection;

pub fn list(
    conn: &Connection,
    section_id: &str,
) -> Result<Vec<CanonSectionRevisionRecord>, AppError> {
    repository::list_revisions(conn, section_id)
}
