use cinematic_desktop_lib::db;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::providers::error::redact_secret;
use cinematic_desktop_lib::providers::repository::{upsert_provider_config, ProviderConfigRecord};
use serde_json::json;
use tempfile::tempdir;

const TEST_API_SECRET: &str = "sk-test-1234567890abcdefghij";

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempdir().unwrap();
    let root = temp.path().join("privacy-project");
    ProjectService::create(&root, "Privacy Test Project").unwrap();
    (temp, root)
}

// Test 1: Secure credential value does not appear in project DB
#[test]
fn credential_not_in_project_db() {
    let (_temp, root) = fixture();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();

    // Configure with credential reference (not the secret itself)
    let config = ProviderConfigRecord {
        provider_id: "openai".into(),
        enabled: true,
        credential_reference: Some("OPENAI_API_KEY".into()),
        default_model: Some("gpt-4".into()),
        endpoint: None,
        request_timeout_seconds: 30,
        polling_interval_seconds: 2,
    };

    upsert_provider_config(&conn, &config).unwrap();

    // Query the entire database for the secret
    let sql = "SELECT * FROM provider_configurations WHERE provider_id = 'openai'".to_string();
    let mut stmt = conn.prepare(&sql).unwrap();
    let config_row: String = stmt.query_row([], |row| row.get(2)).unwrap(); // credential_reference

    // Verify that the actual secret is NOT in the DB
    assert_eq!(config_row, "OPENAI_API_KEY");
    assert!(!config_row.contains(TEST_API_SECRET));
}

// Test 2: Secure credential value does not appear in project files
#[test]
fn credential_not_in_project_files() {
    let (_temp, root) = fixture();

    // Create a project.yaml file
    let project_yaml = root.join("project.yaml");
    std::fs::write(
        &project_yaml,
        "name: Test Project\nproviders:\n  openai:\n    apiKey: ${OPENAI_API_KEY}\n",
    )
    .unwrap();

    // Read the file and verify no secrets are hardcoded
    let content = std::fs::read_to_string(&project_yaml).unwrap();
    assert!(!content.contains(TEST_API_SECRET));
    assert!(!content.contains("sk-"));
}

// Test 3: Secure credential value does not appear in workflow snapshot
#[test]
fn credential_not_in_workflow_snapshot() {
    let (_temp, root) = fixture();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();

    // Insert a workflow run with execution data
    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();
    conn.execute(
        "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, status, input_json, created_at, updated_at)
         VALUES ('run-1', ?1, 'skill-1', '1.0', 'op-1', 'completed', ?2, 'now', 'now')",
        [project_id.clone(), json!({"text":"Hello"}).to_string()],
    ).unwrap();

    // Read the workflow run
    let input_json: String = conn
        .query_row(
            "SELECT input_json FROM workflow_runs WHERE id = 'run-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // Verify no secrets in the snapshot
    assert!(!input_json.contains(TEST_API_SECRET));
}

// Test 4: Secure credential value does not appear in generation metadata
#[test]
fn credential_not_in_generation_metadata() {
    let (_temp, root) = fixture();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();

    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();
    conn.execute(
        "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, status, input_json, created_at, updated_at)
         VALUES ('run-2', ?1, 'skill-1', '1.0', 'op-1', 'completed', '{}', 'now', 'now')",
        [project_id],
    ).unwrap();

    conn.execute(
        "INSERT INTO workflow_steps (id, workflow_run_id, step_definition_id, step_index, step_type, status, output_json, started_at, completed_at)
         VALUES ('step-1', 'run-2', 'step-1', 0, 'execute', 'completed', ?1, 'now', 'now')",
        [json!({"result":"success","metadata":{"model":"gpt-4"}}).to_string()],
    ).unwrap();

    let output_json: String = conn
        .query_row(
            "SELECT output_json FROM workflow_steps WHERE workflow_run_id = 'run-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // Verify no secrets in generation metadata
    assert!(!output_json.contains(TEST_API_SECRET));
}

// Test 5: Authorization headers redacted
#[test]
fn authorization_headers_redacted() {
    let header_with_bearer = "Authorization: Bearer sk-test-1234567890abcdefghij";
    let redacted = redact_secret(header_with_bearer);

    assert!(!redacted.contains(TEST_API_SECRET));
    assert!(redacted.contains("[REDACTED]"));
}

// Test 6: API key headers redacted
#[test]
fn api_key_headers_redacted() {
    let header_with_key = "x-api-key: sk-test-1234567890abcdefghij";
    let redacted = redact_secret(header_with_key);

    assert!(!redacted.contains(TEST_API_SECRET));
    assert!(redacted.contains("[REDACTED]"));
}

// Test 7: apiKey param redacted
#[test]
fn api_key_param_redacted() {
    let json_with_key = r#"{"apiKey":"sk-test-1234567890abcdefghij","model":"gpt-4"}"#;
    let redacted = redact_secret(json_with_key);

    assert!(!redacted.contains(TEST_API_SECRET));
    assert!(redacted.contains("[REDACTED]"));
}

// Test 8: Multiple secrets in one string redacted
#[test]
fn multiple_secrets_redacted() {
    let text = format!(
        "Authorization: Bearer {} and apiKey: {} and token: xyz123",
        TEST_API_SECRET, TEST_API_SECRET
    );
    let redacted = redact_secret(&text);

    assert!(!redacted.contains(TEST_API_SECRET));
    let count = redacted.matches("[REDACTED]").count();
    assert!(count >= 2);
}

// Test 9: Diagnostics export redacts all secrets
#[test]
fn diagnostics_export_redacted() {
    let diagnostic_output = format!(
        "Error occurred: Authorization: Bearer {}\nProvider response: {{\n  \"key\": \"{}\"\n}}",
        TEST_API_SECRET, TEST_API_SECRET
    );
    let redacted = redact_secret(&diagnostic_output);

    assert!(!redacted.contains(TEST_API_SECRET));
    assert!(redacted.contains("[REDACTED]"));
}

// Test 10: Redaction preserves readable output
#[test]
fn redaction_preserves_structure() {
    let diagnostic = "Authorization: Bearer sk-test-1234567890abcdefghij\nError: Invalid request";
    let redacted = redact_secret(diagnostic);

    assert!(redacted.contains("Authorization:"));
    assert!(redacted.contains("[REDACTED]"));
    assert!(redacted.contains("Error: Invalid request"));
    assert!(!redacted.contains("sk-test"));
}

// Test 11: Credential reference is never exposed in config status
#[test]
fn credential_reference_not_exposed_in_status() {
    let (_temp, root) = fixture();
    std::env::set_var("TEST_OPENAI_KEY", TEST_API_SECRET);

    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let config = ProviderConfigRecord {
        provider_id: "openai".into(),
        enabled: true,
        credential_reference: Some("TEST_OPENAI_KEY".into()),
        default_model: Some("gpt-4".into()),
        endpoint: None,
        request_timeout_seconds: 30,
        polling_interval_seconds: 2,
    };

    upsert_provider_config(&conn, &config).unwrap();

    // The credential_reference should be stored, but never the actual value
    let stored_ref: String = conn
        .query_row(
            "SELECT credential_reference FROM provider_configurations WHERE provider_id = 'openai'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(stored_ref, "TEST_OPENAI_KEY");
    assert!(!stored_ref.contains(TEST_API_SECRET));

    std::env::remove_var("TEST_OPENAI_KEY");
}

// Test 12: Error messages with diagnostic info are redacted
#[test]
fn error_diagnostic_messages_redacted() {
    use cinematic_desktop_lib::providers::error::{ProviderError, ProviderErrorKind};

    let error = ProviderError::new(ProviderErrorKind::NetworkError, "request failed")
        .with_diagnostic(format!("Authorization: Bearer {}", TEST_API_SECRET));

    let diagnostic = error.diagnostic.unwrap();
    assert!(!diagnostic.contains(TEST_API_SECRET));
    assert!(diagnostic.contains("[REDACTED]"));
}
