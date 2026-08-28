use cinematic_desktop_lib::{db, project::service::ProjectService};

#[test]
fn repair_schema_is_present_and_keeps_provenance_columns_durable() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    ProjectService::create(&root, "P6 acceptance").unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let table_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'qa_repairs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    for column in [
        "source_asset_version_id",
        "child_asset_version_id",
        "source_qa_run_id",
        "repair_plan_json",
        "compiled_request_json",
        "provider_job_id",
        "child_qa_run_id",
    ] {
        assert!(table_sql.contains(column), "missing provenance column {column}");
    }
}
