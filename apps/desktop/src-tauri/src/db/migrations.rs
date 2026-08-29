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
    Migration {
        version: 8,
        sql: include_str!("../../migrations/0008_generated_artifacts.sql"),
    },
    Migration {
        version: 9,
        sql: include_str!("../../migrations/0009_artifact_lineage.sql"),
    },
    Migration {
        version: 10,
        sql: include_str!("../../migrations/0010_visual_qa.sql"),
    },
    Migration {
        version: 11,
        sql: include_str!("../../migrations/0011_visual_qa_repairs.sql"),
    },
    Migration {
        version: 12,
        sql: include_str!("../../migrations/0012_cinema_compiler.sql"),
    },
    Migration {
        version: 13,
        sql: include_str!("../../migrations/0013_performance_indexes.sql"),
    },
    Migration {
        version: 14,
        sql: include_str!("../../migrations/0014_custom_provider_definitions.sql"),
    },
    Migration {
        version: 15,
        sql: include_str!("../../migrations/0015_custom_provider_purpose.sql"),
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
    fn schema_14_custom_providers_upgrade_as_legacy_instead_of_guessing_a_purpose() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);").unwrap();
        for migration in MIGRATIONS
            .iter()
            .filter(|migration| migration.version <= 14)
        {
            conn.execute_batch(migration.sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, 'now')",
                [migration.version],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO custom_provider_definitions (provider_id, display_name, base_url, models_json, headers_json, created_at, updated_at) VALUES ('old', 'Old', 'https://example.test/v1', '[{\"id\":\"m\",\"name\":\"M\"}]', '[]', 'now', 'now')",
            [],
        ).unwrap();

        run_migrations(&mut conn).unwrap();

        let purpose: String = conn
            .query_row(
                "SELECT purpose FROM custom_provider_definitions WHERE provider_id = 'old'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(purpose, "legacy");
    }

    #[test]
    fn cinema_migration_creates_required_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        for table in [
            "scenes",
            "scene_characters",
            "scene_props",
            "shots",
            "cinema_compilations",
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
    fn cinema_migration_enforces_foreign_keys_and_uniqueness() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();

        // FK: scenes must reference a real project.
        assert!(conn
            .execute(
                "INSERT INTO scenes (id, project_id, title, created_at, updated_at) \
                 VALUES ('scene-1', 'missing-project', 'Scene 001', 'now', 'now')",
                [],
            )
            .is_err());

        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) \
             VALUES ('project-1', 'Red Door', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO scenes (id, project_id, title, created_at, updated_at) \
             VALUES ('scene-1', 'project-1', 'Scene 001', 'now', 'now')",
            [],
        )
        .unwrap();

        // Check constraint: blank titles are rejected.
        assert!(conn
            .execute(
                "INSERT INTO scenes (id, project_id, title, created_at, updated_at) \
                 VALUES ('scene-2', 'project-1', '   ', 'now', 'now')",
                [],
            )
            .is_err());

        // Check constraint: durations must be positive and bounded.
        assert!(conn
            .execute(
                "INSERT INTO shots (id, scene_id, ordering, duration_seconds, intent, \
                 created_at, updated_at) \
                 VALUES ('shot-1', 'scene-1', 0, 0, 'Establish', 'now', 'now')",
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "INSERT INTO shots (id, scene_id, ordering, duration_seconds, intent, \
                 created_at, updated_at) \
                 VALUES ('shot-1', 'scene-1', 0, 45, 'Establish', 'now', 'now')",
                [],
            )
            .is_err());

        // Unique: duplicate (scene_id, ordering) shots are rejected.
        conn.execute(
            "INSERT INTO shots (id, scene_id, ordering, duration_seconds, intent, \
             created_at, updated_at) \
             VALUES ('shot-1', 'scene-1', 0, 4, 'Establish', 'now', 'now')",
            [],
        )
        .unwrap();
        assert!(conn
            .execute(
                "INSERT INTO shots (id, scene_id, ordering, duration_seconds, intent, \
                 created_at, updated_at) \
                 VALUES ('shot-2', 'scene-1', 0, 4, 'Close', 'now', 'now')",
                [],
            )
            .is_err());
    }

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

    #[test]
    fn generation_migrations_create_artifact_lineage_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();

        for table in [
            "generation_result_sets",
            "generated_artifacts",
            "generated_artifact_sources",
            "artifact_lineage",
            "artifact_promotions",
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
    fn generation_migrations_enforce_one_result_set_per_attempt_and_one_promotion_per_artifact() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version)
             VALUES ('project-1', 'Project', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workflow_runs
             (id, project_id, skill_id, skill_version, operation_id, status, input_json, created_at, updated_at)
             VALUES ('run-1', 'project-1', 'skill', '1.0.0', 'operation', 'completed', '{}', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workflow_step_executions
             (id, workflow_run_id, step_definition_id, attempt_number, compiled_request_id,
              provider_id, model_id, adapter_version, idempotency_key, status, started_at)
             VALUES ('attempt-1', 'run-1', 'execute', 1, 'compiled-1', 'mock', 'mock-image-v1', 1,
                     'run-1:execute:1', 'succeeded', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO generation_result_sets
             (id, project_id, workflow_run_id, workflow_step_key, provider_attempt_id,
              media_kind, requested_output_count, created_at)
             VALUES ('result-1', 'project-1', 'run-1', 'execute', 'attempt-1', 'image', 4, 'now')",
            [],
        )
        .unwrap();
        assert!(conn
            .execute(
                "INSERT INTO generation_result_sets
                 (id, project_id, workflow_run_id, workflow_step_key, provider_attempt_id,
                  media_kind, requested_output_count, created_at)
                 VALUES ('result-2', 'project-1', 'run-1', 'execute', 'attempt-1', 'image', 4, 'now')",
                [],
            )
            .is_err());
    }

    #[test]
    fn visual_qa_migration_creates_durable_version_scoped_history() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();

        for table in ["qa_runs", "qa_checks"] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "table {table} should exist");
        }

        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version)
             VALUES ('project-1', 'Project', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, created_at, updated_at)
             VALUES ('asset-1', 'project-1', 'image', 'Candidate', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_versions
             (id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
              original_filename, mime_type, byte_size, created_at)
             VALUES ('version-1', 'asset-1', 1, 'candidate', 'candidate.png', 'thumb.png',
                     'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                     'candidate.png', 'image/png', 1, 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO qa_runs
             (id, project_id, asset_id, asset_version_id, status, execution_location,
              check_plan_json, context_snapshot_json, created_at)
             VALUES ('qa-1', 'project-1', 'asset-1', 'version-1', 'queued', 'local',
                     '{}', '{}', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO qa_checks
             (id, qa_run_id, check_id, check_type, source, requirement_json, status,
              observed, reason, review_status, created_at)
             VALUES ('row-1', 'qa-1', 'artifact:watermark', 'watermark',
                     'artifact_detection', '{}', 'pass', '', '', 'unreviewed', 'now')",
            [],
        )
        .unwrap();

        assert!(conn
            .execute(
                "INSERT INTO qa_checks
                 (id, qa_run_id, check_id, check_type, source, requirement_json, status,
                  observed, reason, review_status, created_at)
                 VALUES ('row-2', 'qa-1', 'artifact:watermark', 'watermark',
                         'artifact_detection', '{}', 'fail', '', '', 'unreviewed', 'now')",
                [],
            )
            .is_err());
    }

    #[test]
    fn performance_migration_creates_required_indexes() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();

        for index in [
            "idx_workflow_steps_run",
            "idx_workflow_events_run",
            "idx_qa_runs_workflow",
            "idx_artifact_lineage_artifact",
            "idx_workflow_approvals_run_step",
            "idx_workflow_runs_project_status",
            "idx_asset_versions_asset_status",
            "idx_qa_runs_project_version",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "index {index} should exist");
        }
    }
}
