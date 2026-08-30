use crate::error::AppError;
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};

/// A single, immutable database migration.
pub struct Migration {
    pub version: i64,
    pub sql: &'static str,
    /// True for the rare migration that rebuilds a table which other tables
    /// reference via FOREIGN KEY (required to change a CHECK constraint,
    /// which SQLite cannot ALTER in place). Such migrations follow SQLite's
    /// documented rebuild procedure: the runner disables `foreign_keys`
    /// *before* beginning the migration transaction (the pragma is a
    /// silent no-op inside a transaction), executes the rebuild
    /// transactionally, verifies `PRAGMA foreign_key_check` inside the
    /// transaction, and re-enables enforcement afterwards.
    pub rebuilds_foreign_key_tables: bool,
}

impl Migration {
    /// An ordinary migration: runs inside a single transaction with
    /// foreign-key enforcement left on.
    pub const fn new(version: i64, sql: &'static str) -> Self {
        Migration {
            version,
            sql,
            rebuilds_foreign_key_tables: false,
        }
    }

    /// A table-rebuild migration that requires foreign keys to be disabled
    /// while it runs. See [`Migration::rebuilds_foreign_key_tables`].
    pub const fn foreign_key_rebuild(version: i64, sql: &'static str) -> Self {
        Migration {
            version,
            sql,
            rebuilds_foreign_key_tables: true,
        }
    }
}

/// The full, ordered list of migrations that bring a fresh database up to
/// the current schema. Append new migrations here; never edit an existing
/// entry once it has shipped.
pub const MIGRATIONS: &[Migration] = &[
    Migration::new(1, include_str!("../../migrations/0001_project_kernel.sql")),
    Migration::new(2, include_str!("../../migrations/0002_assets.sql")),
    Migration::new(3, include_str!("../../migrations/0003_asset_version_dimensions.sql")),
    Migration::new(4, include_str!("../../migrations/0004_canon_engine.sql")),
    Migration::new(5, include_str!("../../migrations/0005_workflow_runtime.sql")),
    Migration::new(6, include_str!("../../migrations/0006_provider_integrations.sql")),
    Migration::new(7, include_str!("../../migrations/0007_provider_audit_events.sql")),
    Migration::new(8, include_str!("../../migrations/0008_generated_artifacts.sql")),
    Migration::new(9, include_str!("../../migrations/0009_artifact_lineage.sql")),
    Migration::new(10, include_str!("../../migrations/0010_visual_qa.sql")),
    Migration::new(11, include_str!("../../migrations/0011_visual_qa_repairs.sql")),
    Migration::new(12, include_str!("../../migrations/0012_cinema_compiler.sql")),
    Migration::new(13, include_str!("../../migrations/0013_performance_indexes.sql")),
    Migration::new(
        14,
        include_str!("../../migrations/0014_custom_provider_definitions.sql"),
    ),
    Migration::new(15, include_str!("../../migrations/0015_custom_provider_purpose.sql")),
    Migration::new(16, include_str!("../../migrations/0016_world_scene_pipeline.sql")),
    Migration::new(17, include_str!("../../migrations/0017_unified_scene_shots.sql")),
    Migration::new(
        18,
        include_str!("../../migrations/0018_artifact_promotion_idempotency.sql"),
    ),
    Migration::new(
        19,
        include_str!("../../migrations/0019_custom_provider_operations.sql"),
    ),
    Migration::foreign_key_rebuild(
        20,
        include_str!("../../migrations/0020_video_media_kinds.sql"),
    ),
    Migration::foreign_key_rebuild(
        21,
        include_str!("../../migrations/0021_shot_video_pin.sql"),
    ),
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

        if migration.rebuilds_foreign_key_tables {
            // SQLite's documented procedure for rebuilding a table that
            // other tables reference: disable foreign-key enforcement
            // *outside* any transaction (the pragma is a no-op inside
            // one), run the rebuild inside the transaction, then verify
            // integrity with `foreign_key_check` before committing.
            conn.execute_batch("PRAGMA foreign_keys = OFF;")
                .map_err(|e| AppError::Database(e.to_string()))?;
        }

        let result = (|| -> Result<(), AppError> {
            let tx = conn
                .transaction()
                .map_err(|e| AppError::Database(e.to_string()))?;

            tx.execute_batch(migration.sql)
                .map_err(|e| AppError::Database(e.to_string()))?;

            if migration.rebuilds_foreign_key_tables {
                let violations = tx
                    .query_row("PRAGMA foreign_key_check", [], |row| row.get::<_, i64>(0))
                    .optional()
                    .map_err(|e| AppError::Database(e.to_string()))?;
                if violations.is_some() {
                    return Err(AppError::Database(
                        "migration 0020 failed its foreign_key_check: the rebuilt \
                         tables would violate a foreign key; the database was left \
                         unchanged"
                            .into(),
                    ));
                }
            }

            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![migration.version, Utc::now().to_rfc3339()],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

            tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
            Ok(())
        })();

        if migration.rebuilds_foreign_key_tables {
            // Enforcement is restored on every connection by
            // `db::open_connection`; this only guards this session in case
            // later migrations (or the caller) use the same connection.
            let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");
        }

        result?;
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

    #[test]
    fn p6_project_still_migrates_successfully_after_p7() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        // Verify P6 tables still exist and are usable.
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('p6-proj', 'P6', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, created_at, updated_at) VALUES ('a-p6', 'p6-proj', 'world_plate', 'World', 'now', 'now')",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);
    }

    #[test]
    fn p7_tables_exist() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        for table in [
            "worlds",
            "scenes",
            "world_scene_characters",
            "world_scene_props",
            "scene_tbd_bindings",
            "scene_reference_events",
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
    fn p7_foreign_keys_are_enforced() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run_migrations(&mut conn).unwrap();
        let enabled: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(enabled, 1, "foreign_keys pragma should be ON");
        // Attempt to insert worlds with invalid project_id should fail via FK.
        let fk_result = conn.execute(
            "INSERT INTO worlds (id, project_id, canon_location_entity_id, world_plate_asset_id, created_at, updated_at) VALUES ('w-1', 'missing-project', 'loc-1', 'asset-1', 'now', 'now')",
            [],
        );
        assert!(
            fk_result.is_err(),
            "FK violation should be rejected, got {fk_result:?}"
        );
    }

    #[test]
    fn p7_rejects_duplicate_world_per_location() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('proj-1', 'Project', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('loc-1', 'proj-1', 'location', 'Station', 'station', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, created_at, updated_at) VALUES ('asset-1', 'proj-1', 'world_plate', 'STATION-WORLD', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO worlds (id, project_id, canon_location_entity_id, world_plate_asset_id, created_at, updated_at) VALUES ('world-1', 'proj-1', 'loc-1', 'asset-1', 'now', 'now')",
            [],
        )
        .unwrap();
        let dup = conn.execute(
            "INSERT INTO worlds (id, project_id, canon_location_entity_id, world_plate_asset_id, created_at, updated_at) VALUES ('world-2', 'proj-1', 'loc-1', 'asset-1', 'now', 'now')",
            [],
        );
        assert!(
            dup.is_err(),
            "duplicate World per Location should be rejected"
        );
    }

    #[test]
    fn p7_rejects_duplicate_scene_ordinal() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('proj-1', 'Project', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO world_scenes (id, project_id, ordinal, title, summary, created_at, updated_at) VALUES ('scene-1', 'proj-1', 1, 'Title', 'Summary', 'now', 'now')",
            [],
        )
        .unwrap();
        let dup = conn.execute(
            "INSERT INTO world_scenes (id, project_id, ordinal, title, summary, created_at, updated_at) VALUES ('scene-2', 'proj-1', 1, 'Other', 'Summary2', 'now', 'now')",
            [],
        );
        assert!(dup.is_err(), "duplicate Scene ordinal should be rejected");
    }

    #[test]
    fn p7_rejects_duplicate_character_assignment() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('proj-1', 'Project', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('char-1', 'proj-1', 'character', 'Mara', 'mara', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, created_at, updated_at) VALUES ('asset-look', 'proj-1', 'face_lock', 'MARA-LOOK', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) VALUES ('ver-1', 'asset-look', 1, 'candidate', 'a.png', 't.png', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'a.png', 'image/png', 1, 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) VALUES ('ver-2', 'asset-look', 2, 'candidate', 'b.png', 't2.png', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'b.png', 'image/png', 1, 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO world_scenes (id, project_id, ordinal, title, summary, created_at, updated_at) VALUES ('scene-1', 'proj-1', 1, 'Title', 'Summary', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO world_scene_characters (id, scene_id, character_entity_id, look_asset_version_id, created_at, updated_at) VALUES ('sc-1', 'scene-1', 'char-1', 'ver-1', 'now', 'now')",
            [],
        )
        .unwrap();
        let dup = conn.execute(
            "INSERT INTO world_scene_characters (id, scene_id, character_entity_id, look_asset_version_id, created_at, updated_at) VALUES ('sc-2', 'scene-1', 'char-1', 'ver-2', 'now', 'now')",
            [],
        );
        assert!(
            dup.is_err(),
            "duplicate Character assignment should be rejected"
        );
    }

    #[test]
    fn p7_rejects_invalid_reference_ids() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) VALUES ('proj-1', 'Project', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO world_scenes (id, project_id, ordinal, title, summary, created_at, updated_at) VALUES ('scene-1', 'proj-1', 1, 'Title', 'Summary', 'now', 'now')",
            [],
        )
        .unwrap();
        // Invalid scene_id FK
        let fk1 = conn.execute(
            "INSERT INTO world_scene_characters (id, scene_id, character_entity_id, look_asset_version_id, created_at, updated_at) VALUES ('sc-bad', 'missing-scene', 'char-1', 'ver-1', 'now', 'now')",
            [],
        );
        assert!(
            fk1.is_err(),
            "invalid scene_id should be rejected, got {fk1:?}"
        );
        // Invalid asset_version FK
        let fk2 = conn.execute(
            "INSERT INTO world_scene_characters (id, scene_id, character_entity_id, look_asset_version_id, created_at, updated_at) VALUES ('sc-bad2', 'scene-1', 'char-1', 'missing-version', 'now', 'now')",
            [],
        );
        assert!(
            fk2.is_err(),
            "invalid look_asset_version_id should be rejected, got {fk2:?}"
        );
        // Invalid world FK
        let fk3 = conn.execute(
            "INSERT INTO world_scenes (id, project_id, ordinal, title, summary, world_id, created_at, updated_at) VALUES ('scene-2', 'proj-1', 2, 'Title2', 'Summary2', 'missing-world', 'now', 'now')",
            [],
        );
        assert!(
            fk3.is_err(),
            "invalid world_id should be rejected, got {fk3:?}"
        );
    }
}

    // -------------------------------------------------------------------
    // Migration 0017 regression: deterministic, unambiguous legacy mapping
    // -------------------------------------------------------------------

    /// Applies migrations 1..=16 (recorded), seeds legacy P8 rows via
    /// `seed`, then runs the pending migration 17.
    fn migrate_legacy_project(seed: &dyn Fn(&mut Connection)) -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations \
             (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
        )
        .unwrap();
        for migration in MIGRATIONS.iter().filter(|m| m.version <= 16) {
            conn.execute_batch(migration.sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, 'now')",
                rusqlite::params![migration.version],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) \
             VALUES ('p1', 'P', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        seed(&mut conn);
        run_migrations(&mut conn).unwrap();
        conn
    }

    fn insert_legacy_scene(conn: &Connection, id: &str, title: &str) {
        conn.execute(
            "INSERT INTO scenes (id, project_id, title, created_at, updated_at) \
             VALUES (?1, 'p1', ?2, 'now', 'now')",
            rusqlite::params![id, title],
        )
        .unwrap();
    }

    fn insert_legacy_shot(conn: &Connection, id: &str, scene_id: &str, ordering: i64) {
        conn.execute(
            "INSERT INTO shots (id, scene_id, ordering, duration_seconds, intent, \
             created_at, updated_at) \
             VALUES (?1, ?2, ?3, 4, 'Establish', 'now', 'now')",
            rusqlite::params![id, scene_id, ordering],
        )
        .unwrap();
    }

    fn insert_authoritative_scene(conn: &Connection, id: &str, ordinal: i64, title: &str) {
        conn.execute(
            "INSERT INTO world_scenes (id, project_id, ordinal, title, summary, created_at, updated_at) \
             VALUES (?1, 'p1', ?2, ?3, '', 'now', 'now')",
            rusqlite::params![id, ordinal, title],
        )
        .unwrap();
    }

    fn shot_scene(conn: &Connection, shot_id: &str) -> String {
        conn.query_row(
            "SELECT scene_id FROM scene_shots WHERE id = ?1",
            [shot_id],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn migration_0017_duplicate_legacy_titles_never_merge() {
        let conn = migrate_legacy_project(&|conn| {
            insert_legacy_scene(conn, "L1", "Same Title");
            insert_legacy_scene(conn, "L2", "Same Title");
            insert_legacy_shot(conn, "s1", "L1", 0);
            insert_legacy_shot(conn, "s2", "L2", 0);
        });

        // Each legacy scene must get its own derived authoritative scene.
        let derived: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM world_scenes WHERE id IN ('wsc-L1','wsc-L2')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(derived, 2, "duplicate titles must map to separate scenes");

        // Shots must stay with their own legacy scene, never merged.
        assert_eq!(shot_scene(&conn, "s1"), "wsc-L1");
        assert_eq!(shot_scene(&conn, "s2"), "wsc-L2");

        // Ordinals remain unique per project despite the shared statement.
        let distinct_ordinals: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT ordinal) FROM world_scenes WHERE project_id = 'p1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let total_scenes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM world_scenes WHERE project_id = 'p1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(distinct_ordinals, total_scenes);
    }

    #[test]
    fn migration_0017_unique_title_match_attaches() {
        let conn = migrate_legacy_project(&|conn| {
            insert_authoritative_scene(&conn, "ws-1", 0, "Alpha");
            insert_legacy_scene(&conn, "L1", "Alpha");
            insert_legacy_shot(&conn, "s1", "L1", 0);
        });

        // Unambiguous: exactly one authoritative scene with that title.
        assert_eq!(shot_scene(&conn, "s1"), "ws-1");
        let derived: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM world_scenes WHERE id LIKE 'wsc-%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(derived, 0, "no derived row is needed for a unique match");
    }

    #[test]
    fn migration_0017_ambiguous_titles_fail_closed_to_derived_rows() {
        let conn = migrate_legacy_project(&|conn| {
            // Two authoritative scenes already share the legacy title.
            insert_authoritative_scene(&conn, "ws-1", 0, "Beta");
            insert_authoritative_scene(&conn, "ws-2", 1, "Beta");
            insert_legacy_scene(&conn, "L1", "Beta");
            insert_legacy_shot(&conn, "s1", "L1", 0);
        });

        // The migration must not guess between the two same-titled scenes:
        // the legacy scene gets its own derived row instead.
        assert_eq!(shot_scene(&conn, "s1"), "wsc-L1");

        // Pre-existing authoritative scenes remain untouched.
        let beta_shots: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scene_shots WHERE scene_id IN ('ws-1','ws-2')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(beta_shots, 0);
    }

    #[test]
    fn migration_0017_moves_compilations_with_their_scene() {
        let conn = migrate_legacy_project(&|conn| {
            insert_legacy_scene(&conn, "L1", "Solo");
            conn.execute(
                "INSERT INTO cinema_compilations (id, project_id, scene_id, input_json, \
                 compilation_json, export_path, export_sha256, created_at) \
                 VALUES ('c-1', 'p1', 'L1', '{}', '{}', 'prompts/cinema/c-1.json', ?, 'now')",
                rusqlite::params!["a".repeat(64)],
            )
            .unwrap();
        });

        let mapped: String = conn
            .query_row(
                "SELECT scene_id FROM scene_compilations WHERE id = 'c-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mapped, "wsc-L1");
    }

    /// Builds a fully migrated (head = latest) database seeded with one
    /// project, one image asset, and one image asset version, plus a second
    /// asset reserved for video versions.
    fn video_ready_conn() -> (Connection, String, String) {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) \
             VALUES ('p1', 'Red Door', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, owner_entity_id, \
             canonical_version_id, created_at, updated_at) \
             VALUES ('asset-img', 'p1', 'image', 'IMG', NULL, NULL, 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, owner_entity_id, \
             canonical_version_id, created_at, updated_at) \
             VALUES ('asset-vid', 'p1', 'video', 'VID', NULL, NULL, 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, \
             thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) \
             VALUES ('v-img-1', 'asset-img', 1, 'candidate', 'a.png', 't.webp', ?, 'a.png', \
             'image/png', 100, 'now')",
            rusqlite::params!["b".repeat(64)],
        )
        .unwrap();
        (conn, "asset-img".into(), "asset-vid".into())
    }

    #[test]
    fn migration_0020_accepts_video_media_kinds_and_mime_types() {
        let (mut conn, _, video_asset) = video_ready_conn();

        // A video asset version persists with video/mp4.
        conn.execute(
            "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, \
             thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) \
             VALUES ('v-vid-1', ?1, 1, 'candidate', 'v.mp4', '', ?, 'v.mp4', 'video/mp4', 24, 'now')",
            rusqlite::params![video_asset, "c".repeat(64)],
        )
        .unwrap();
        conn.execute(
            "UPDATE assets SET canonical_version_id = 'v-vid-1' WHERE id = ?1",
            [&video_asset],
        )
        .unwrap();

        // A workflow run + provider attempt exist so a video result set can
        // reference them (minimal seed rows satisfying the FKs).
        conn.execute(
            "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, \
             status, input_json, created_at, updated_at) \
             VALUES ('run-1', 'p1', 's', '1.0.0', 'o', 'completed', '{}', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workflow_steps (id, workflow_run_id, step_index, step_definition_id, \
             step_type, status, input_json) \
             VALUES ('step-1', 'run-1', 0, 'execute', 'execute', 'completed', '{}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, \
             attempt_number, idempotency_key, provider_id, model_id, adapter_version, status, \
             compiled_request_id, started_at) \
             VALUES ('attempt-1', 'run-1', 'execute', 1, 'run-1:execute:1', 'fake_async_video', \
             'fake-video-v1', 1, 'succeeded', ?, 'now')",
            rusqlite::params!["d".repeat(64)],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO generation_result_sets (id, project_id, workflow_run_id, \
             workflow_step_key, provider_attempt_id, media_kind, requested_output_count, \
             created_at) \
             VALUES ('rs-1', 'p1', 'run-1', 'execute', 'attempt-1', 'video', 1, 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO generated_artifacts (id, result_set_id, ordinal, media_kind, \
             mime_type, byte_size, sha256, storage_path, capture_status, created_at) \
             VALUES ('ga-1', 'rs-1', 1, 'video', 'video/mp4', 24, ?, \
             'generated/run-1/attempt-1/0001.mp4', 'available', 'now')",
            rusqlite::params!["e".repeat(64)],
        )
        .unwrap();

        // Image semantics are unchanged: the pre-existing image rows survive.
        let image_mime: String = conn
            .query_row(
                "SELECT mime_type FROM asset_versions WHERE id = 'v-img-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(image_mime, "image/png");
    }

    #[test]
    fn migration_0020_still_rejects_unsupported_media() {
        let (conn, _, video_asset) = video_ready_conn();

        // Unknown media kinds are still rejected (constraints stay explicit).
        assert!(conn
            .execute(
                "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, \
                 thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) \
                 VALUES ('bad-1', ?1, 1, 'candidate', 'x.avi', '', ?, 'x.avi', 'video/avi', 1, 'now')",
                rusqlite::params![video_asset, "f".repeat(64)],
            )
            .is_err());
        assert!(conn
            .execute(
                "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, \
                 thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) \
                 VALUES ('bad-2', ?1, 1, 'candidate', 'x.txt', '', ?, 'x.txt', 'text/plain', 1, 'now')",
                rusqlite::params![video_asset, "0".repeat(64)],
            )
            .is_err());
    }

    #[test]
    fn migration_0020_upgrades_a_project_at_0019_without_losing_data() {
        // Build a database stopped at 0019 with image data, then upgrade.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
        )
        .unwrap();
        for migration in MIGRATIONS
            .iter()
            .filter(|migration| migration.version <= 19)
        {
            conn.execute_batch(migration.sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, 'now')",
                [migration.version],
            )
            .unwrap();
        }
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, created_at, updated_at, schema_version) \
             VALUES ('p1', 'Red Door', 'now', 'now', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, owner_entity_id, \
             canonical_version_id, created_at, updated_at) \
             VALUES ('a-img', 'p1', 'face_lock', 'Face', NULL, 'v-1', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, \
             thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) \
             VALUES ('v-1', 'a-img', 1, 'canonical', 'a.png', 't.webp', ?, 'a.png', 'image/png', 100, 'now')",
            rusqlite::params!["1".repeat(64)],
        )
        .unwrap();

        // The upgrade must preserve the existing image row exactly.
        run_migrations(&mut conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        let (status, mime, canonical): (String, String, Option<String>) = conn
            .query_row(
                "SELECT av.status, av.mime_type, a.canonical_version_id \
                 FROM asset_versions av JOIN assets a ON a.id = av.asset_id WHERE av.id = 'v-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(status, "canonical");
        assert_eq!(mime, "image/png");
        assert_eq!(canonical.as_deref(), Some("v-1"));

        // And the upgraded schema now accepts video versions.
        conn.execute(
            "INSERT INTO assets (id, project_id, type, label, owner_entity_id, \
             canonical_version_id, created_at, updated_at) \
             VALUES ('a-vid', 'p1', 'video', 'Scene 001 - Video', NULL, NULL, 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, \
             thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) \
             VALUES ('v-vid', 'a-vid', 1, 'candidate', 'v.mp4', '', ?, 'v.mp4', 'video/mp4', 24, 'now')",
            rusqlite::params!["2".repeat(64)],
        )
        .unwrap();

        // FK integrity held through the rebuild.
        let violations: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_foreign_key_check",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(violations, 0);
    }

    #[test]
    fn migration_0021_adds_the_shot_video_pin() {
        let (conn, _, _) = video_ready_conn();
        conn.execute(
            "INSERT INTO world_scenes (id, project_id, ordinal, title, summary, created_at, updated_at) \
             VALUES ('scene-1', 'p1', 0, 'Scene 001', 'A scene', 'now', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, \
             thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) \
             VALUES ('v-vid-2', 'asset-vid', 2, 'canonical', 'v2.mp4', '', ?, 'v2.mp4', 'video/mp4', 24, 'now')",
            rusqlite::params!["3".repeat(64)],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO scene_shots (id, scene_id, ordering, duration_seconds, \
             generated_video_asset_version_id, intent, created_at, updated_at) \
             VALUES ('shot-1', 'scene-1', 0, 4.0, 'v-vid-2', 'Establish', 'now', 'now')",
            [],
        )
        .unwrap();

        // The pin is nullable, exact, and FK-enforced.
        conn.execute(
            "INSERT INTO scene_shots (id, scene_id, ordering, duration_seconds, intent, \
             created_at, updated_at) \
             VALUES ('shot-2', 'scene-1', 1, 4.0, 'Second', 'now', 'now')",
            [],
        )
        .unwrap();
        let pinned: Option<String> = conn
            .query_row(
                "SELECT generated_video_asset_version_id FROM scene_shots WHERE id = 'shot-2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pinned, None);

        assert!(conn
            .execute(
                "INSERT INTO scene_shots (id, scene_id, ordering, duration_seconds, \
                 generated_video_asset_version_id, intent, created_at, updated_at) \
                 VALUES ('shot-3', 'scene-1', 2, 4.0, 'missing-version', 'Third', 'now', 'now')",
                [],
            )
            .is_err());
    }

