pub mod migrations;

use crate::error::AppError;
use rusqlite::Connection;
use std::path::Path;

/// Opens (creating if necessary) a SQLite database file at `path`.
///
/// `PRAGMA foreign_keys = ON` is session-scoped in SQLite -- it does not
/// persist in the database file, only for the lifetime of a single
/// connection. It must therefore be set here, on every connection this
/// function returns, rather than relying on a migration's SQL to have set
/// it once on the connection that happened to run that migration.
pub fn open_connection(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path).map_err(|e| AppError::Database(e.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn foreign_key_enforcement_is_on_for_every_connection() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("fk-check.db");

        // Open and drop a connection first (as `ProjectService::create` would
        // for the migration pass), then open a second, independent
        // connection -- this is the scenario a session-scoped pragma set
        // only inside a one-time migration would miss.
        {
            let first = open_connection(&db_path).unwrap();
            drop(first);
        }
        let conn = open_connection(&db_path).unwrap();

        let enabled: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(enabled, 1);

        conn.execute_batch(
            "CREATE TABLE parents (id INTEGER PRIMARY KEY);
             CREATE TABLE children (
                 id INTEGER PRIMARY KEY,
                 parent_id INTEGER NOT NULL,
                 FOREIGN KEY (parent_id) REFERENCES parents (id)
             );",
        )
        .unwrap();

        let result = conn.execute(
            "INSERT INTO children (id, parent_id) VALUES (1, 999)",
            [],
        );

        assert!(
            result.is_err(),
            "insert referencing a non-existent parent should be rejected by FK enforcement"
        );
    }
}
