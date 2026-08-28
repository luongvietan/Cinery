use crate::error::AppError;
use chrono::Utc;
use rusqlite::Connection;

/// A single, immutable database migration.
pub struct Migration {
    pub version: i64,
    pub sql: &'static str,
}

/// The full, ordered list of migrations that bring a fresh database up to
/// the current schema. Append new migrations here; never edit an existing
/// entry once it has shipped.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../../migrations/0001_project_kernel.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("../../migrations/0002_assets.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("../../migrations/0003_asset_version_dimensions.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("../../migrations/0004_canon_engine.sql"),
    },
    Migration {
        version: 5,
        sql: include_str!("../../migrations/0005_workflow_runtime.sql"),
    },
    Migration {
        version: 6,
        sql: include_str!("../../migrations/0006_provider_integrations.sql"),
    },
    Migration {
        version: 7,
        sql: include_str!("../../migrations/0007_provider_audit_events.sql"),
    },
];

/// Applies every migration that has not yet been recorded in
/// `schema_migrations`, each inside its own transaction. If a migration's
/// SQL or its bookkeeping insert fails, that migration's transaction is
/// rolled back and the error is returned; migrations already committed in
/// earlier calls are left in place.
pub fn run_migrations(conn: &mut Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    let applied_versions = read_applied_versions(conn)?;

    for migration in MIGRATIONS {
        if applied_versions.contains(&migration.version) {
            continue;
        }

        let tx = conn
            .transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;

        tx.execute_batch(migration.sql)
            .map_err(|e| AppError::Database(e.to_string()))?;

        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![migration.version, Utc::now().to_rfc3339()],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
    }

    Ok(())
}

fn read_applied_versions(conn: &Connection) -> Result<Vec<i64>, AppError> {
    let mut stmt = conn
        .prepare("SELECT version FROM schema_migrations")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| row.get::<_, i64>(0))
        .map_err(|e| AppError::Database(e.to_string()))?;

    let mut versions = Vec::new();
    for row in rows {
        versions.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(versions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_pending_migrations_and_records_versions() {
        let mut conn = Connection::open_in_memory().unwrap();

        run_migrations(&mut conn).unwrap();

        let recorded: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(recorded, MIGRATIONS.len() as i64);

        // The projects table from migration 0001 should now exist and be usable.
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) \
             VALUES ('01', 'Red Door', 'a', 'b', 1)",
            [],
        )
        .unwrap();

        // The assets table from migration 0002 should now exist and be usable.
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, created_at, updated_at) \
             VALUES ('a1', '01', 'face_lock', 'MARA-FACE', 'a', 'b')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn canon_migration_creates_required_tables() {
        let mut conn = Connection::open_in_memory().unwrap();

        run_migrations(&mut conn).unwrap();

        for table in [
            "canon_entities",
            "canon_sections",
            "canon_section_revisions",
            "canon_tbds",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "table {table} should exist");
        }
    }

    #[test]
    fn running_migrations_twice_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();

        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();

        let recorded: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(recorded, MIGRATIONS.len() as i64);
    }

    #[test]
    fn workflow_migration_creates_required_tables() {
        let mut conn = Connection::open_in_memory().unwrap();

        run_migrations(&mut conn).unwrap();

        for table in [
            "workflow_runs",
            "workflow_steps",
            "workflow_events",
            "workflow_approvals",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "table {table} should exist");
        }
    }

    #[test]
    fn provider_migration_creates_required_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();

        for table in [
            "provider_configurations",
            "workflow_step_executions",
            "provider_jobs",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "table {table} should exist");
        }
        let audit_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'provider_audit_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_exists, 1);
    }
}
