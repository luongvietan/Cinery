use cinematic_desktop_lib::db::migrations::run_migrations;
use cinematic_desktop_lib::qa::models::{
    QaCheckRecord, QaCheckSource, QaCheckStatus, QaCheckType, QaMediaKind, QaOverallStatus,
    QaReviewStatus, QaRunRecord, QaRunStatus,
};
use cinematic_desktop_lib::qa::repository;
use cinematic_desktop_lib::qa::service::QaService;
use rusqlite::Connection;
use serde_json::json;

fn fixture() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&mut conn).unwrap();
    conn.execute_batch(
        "INSERT INTO projects (id, name, created_at, updated_at, schema_version)
         VALUES ('project-1', 'Project', 'now', 'now', 1);
         INSERT INTO assets (id, project_id, type, label, created_at, updated_at)
         VALUES ('asset-1', 'project-1', 'image', 'Candidate', 'now', 'now');
         INSERT INTO asset_versions
         (id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
          original_filename, mime_type, byte_size, created_at)
         VALUES ('version-1', 'asset-1', 1, 'candidate', 'candidate.png', 'thumb.png',
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 'candidate.png', 'image/png', 1, 'now');",
    )
    .unwrap();
    conn
}

#[test]
fn qa_history_round_trips_and_review_preserves_model_result() {
    let mut conn = fixture();
    let run = QaRunRecord {
        id: "qa-1".into(),
        project_id: "project-1".into(),
        asset_id: "asset-1".into(),
        asset_version_id: "version-1".into(),
        media_kind: QaMediaKind::Image,
        workflow_run_id: None,
        status: QaRunStatus::Succeeded,
        overall_status: Some(QaOverallStatus::Fail),
        adapter_id: Some("mock".into()),
        adapter_version: Some("1".into()),
        model_id: Some("mock-vlm".into()),
        execution_location: "local".into(),
        check_plan: json!({"schemaVersion": 1, "checks": ["lock:scar"]}),
        context_snapshot: json!({"canonRevision": 3}),
        raw_response_metadata: None,
        error_code: None,
        error_message: None,
        created_at: "2026-08-28T00:00:00Z".into(),
        started_at: Some("2026-08-28T00:00:01Z".into()),
        completed_at: Some("2026-08-28T00:00:02Z".into()),
    };
    let check = QaCheckRecord {
        id: "qa-check-1".into(),
        qa_run_id: run.id.clone(),
        check_id: "lock:scar".into(),
        check_type: QaCheckType::PermanentVisualLock,
        source: QaCheckSource::VisualLock,
        requirement: json!({"requirement": "Scar on character-right eyebrow"}),
        status: QaCheckStatus::Fail,
        confidence: Some(0.92),
        observed: "Scar appears on character-left.".into(),
        reason: "Wrong side.".into(),
        repair_hint: Some("Move only the scar.".into()),
        review_status: QaReviewStatus::Unreviewed,
        review_note: None,
        reviewed_at: None,
        created_at: "2026-08-28T00:00:02Z".into(),
    };

    repository::insert_run(&conn, &run).unwrap();
    repository::insert_checks(&mut conn, &[check]).unwrap();
    repository::review_check(
        &conn,
        "project-1",
        "qa-1",
        "lock:scar",
        QaReviewStatus::OverriddenPass,
        Some("False positive"),
        "2026-08-28T00:01:00Z",
    )
    .unwrap();

    let loaded = repository::get_run(&conn, "project-1", "qa-1")
        .unwrap()
        .unwrap();
    assert_eq!(loaded.run.asset_version_id, "version-1");
    assert_eq!(loaded.run.context_snapshot["canonRevision"], 3);
    assert_eq!(loaded.checks[0].status, QaCheckStatus::Fail);
    assert_eq!(
        loaded.checks[0].review_status,
        QaReviewStatus::OverriddenPass
    );
    assert_eq!(
        loaded.checks[0].review_note.as_deref(),
        Some("False positive")
    );

    drop(conn);
}

#[test]
fn qa_history_is_scoped_to_the_exact_asset_version() {
    let conn = fixture();
    let run = QaRunRecord {
        id: "qa-1".into(),
        project_id: "project-1".into(),
        asset_id: "asset-1".into(),
        asset_version_id: "version-1".into(),
        media_kind: QaMediaKind::Image,
        workflow_run_id: None,
        status: QaRunStatus::Queued,
        overall_status: None,
        adapter_id: None,
        adapter_version: None,
        model_id: None,
        execution_location: "local".into(),
        check_plan: json!({}),
        context_snapshot: json!({}),
        raw_response_metadata: None,
        error_code: None,
        error_message: None,
        created_at: "now".into(),
        started_at: None,
        completed_at: None,
    };
    repository::insert_run(&conn, &run).unwrap();

    assert_eq!(
        repository::list_runs_for_asset_version(&conn, "project-1", "version-1")
            .unwrap()
            .len(),
        1
    );
    assert!(
        repository::list_runs_for_asset_version(&conn, "project-1", "missing")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn qa_history_survives_database_close_and_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("project.db");
    let mut conn = Connection::open(&database_path).unwrap();
    run_migrations(&mut conn).unwrap();
    conn.execute_batch(
        "INSERT INTO projects (id, name, created_at, updated_at, schema_version)
         VALUES ('project-1', 'Project', 'now', 'now', 1);
         INSERT INTO assets (id, project_id, type, label, created_at, updated_at)
         VALUES ('asset-1', 'project-1', 'image', 'Candidate', 'now', 'now');
         INSERT INTO asset_versions
         (id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
          original_filename, mime_type, byte_size, created_at)
         VALUES ('version-1', 'asset-1', 1, 'candidate', 'candidate.png', 'thumb.png',
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 'candidate.png', 'image/png', 1, 'now');",
    )
    .unwrap();
    repository::insert_run(
        &conn,
        &QaRunRecord {
            id: "qa-reopen".into(),
            project_id: "project-1".into(),
            asset_id: "asset-1".into(),
            asset_version_id: "version-1".into(),
            media_kind: QaMediaKind::Image,
            workflow_run_id: None,
            status: QaRunStatus::Queued,
            overall_status: None,
            adapter_id: None,
            adapter_version: None,
            model_id: None,
            execution_location: "local".into(),
            check_plan: json!({}),
            context_snapshot: json!({"immutable": true}),
            raw_response_metadata: None,
            error_code: None,
            error_message: None,
            created_at: "now".into(),
            started_at: None,
            completed_at: None,
        },
    )
    .unwrap();
    drop(conn);

    let reopened = Connection::open(&database_path).unwrap();
    let loaded = repository::get_run(&reopened, "project-1", "qa-reopen")
        .unwrap()
        .unwrap();
    assert_eq!(loaded.run.context_snapshot["immutable"], true);
}

#[test]
fn human_override_recomputes_effective_overall_without_rewriting_model_status() {
    let mut conn = fixture();
    let run = QaRunRecord {
        id: "qa-review".into(),
        project_id: "project-1".into(),
        asset_id: "asset-1".into(),
        asset_version_id: "version-1".into(),
        media_kind: QaMediaKind::Image,
        workflow_run_id: None,
        status: QaRunStatus::Succeeded,
        overall_status: Some(QaOverallStatus::Fail),
        adapter_id: Some("mock".into()),
        adapter_version: Some("1".into()),
        model_id: Some("mock-vlm".into()),
        execution_location: "local".into(),
        check_plan: json!({
            "schemaVersion": 1,
            "assetId": "asset-1",
            "assetVersionId": "version-1",
            "ownerEntityId": null,
            "assetType": "image",
            "referenceAssetVersionIds": [],
            "checks": [{
                "id": "lock:scar",
                "checkType": "permanent_visual_lock",
                "source": "visual_lock",
                "key": "scar",
                "label": "Scar",
                "requirement": "Correct side",
                "validatorHint": null,
                "blocking": true,
                "referenceAssetVersionIds": []
            }],
            "createdAt": "now"
        }),
        context_snapshot: json!({}),
        raw_response_metadata: None,
        error_code: None,
        error_message: None,
        created_at: "now".into(),
        started_at: Some("now".into()),
        completed_at: Some("now".into()),
    };
    repository::insert_run(&conn, &run).unwrap();
    repository::insert_checks(
        &mut conn,
        &[QaCheckRecord {
            id: "row-review".into(),
            qa_run_id: run.id.clone(),
            check_id: "lock:scar".into(),
            check_type: QaCheckType::PermanentVisualLock,
            source: QaCheckSource::VisualLock,
            requirement: json!({}),
            status: QaCheckStatus::Fail,
            confidence: Some(0.9),
            observed: "Wrong side".into(),
            reason: "Mismatch".into(),
            repair_hint: None,
            review_status: QaReviewStatus::Unreviewed,
            review_note: None,
            reviewed_at: None,
            created_at: "now".into(),
        }],
    )
    .unwrap();

    let reviewed = QaService::review_check(
        &conn,
        "project-1",
        "qa-review",
        "lock:scar",
        QaReviewStatus::OverriddenPass,
        Some("Model confused viewer-left"),
    )
    .unwrap();

    assert_eq!(reviewed.run.overall_status, Some(QaOverallStatus::Pass));
    assert_eq!(reviewed.checks[0].status, QaCheckStatus::Fail);
    assert_eq!(reviewed.checks[0].effective_status(), QaCheckStatus::Pass);
}
