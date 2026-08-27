use crate::error::AppError;
use crate::project::model::ProjectRecord;
use rusqlite::{params, Connection};

/// Inserts the single project row a project-local database holds.
pub fn insert_project(conn: &Connection, project: &ProjectRecord) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO projects (id, name, created_at, updated_at, schema_version) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            project.id,
            project.name,
            project.created_at,
            project.updated_at,
            project.schema_version,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(())
}

/// Reads the project row from a project-local database.
pub fn read_project(conn: &Connection) -> Result<ProjectRecord, AppError> {
    conn.query_row(
        "SELECT id, name, created_at, updated_at, schema_version FROM projects LIMIT 1",
        [],
        |row| {
            Ok(ProjectRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                schema_version: row.get(4)?,
            })
        },
    )
    .map_err(|e| AppError::Database(e.to_string()))
}
