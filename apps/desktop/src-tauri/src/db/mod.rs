pub mod migrations;

use crate::error::AppError;
use rusqlite::Connection;
use std::path::Path;

/// Opens (creating if necessary) a SQLite database file at `path`.
pub fn open_connection(path: &Path) -> Result<Connection, AppError> {
    Connection::open(path).map_err(|e| AppError::Database(e.to_string()))
}
