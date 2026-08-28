use cinematic_desktop_lib::db::migrations::run_migrations;
use cinematic_desktop_lib::qa::context::{resolve_qa_context, QaPlanningRequest};
use cinematic_desktop_lib::qa::models::{QaCheckType, VisualExpectation};
use cinematic_desktop_lib::qa::check_planner::QaCheckPlanner;
use rusqlite::Connection;

fn fixture(visual_lock_status: &str) -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&mut conn).unwrap();
    let sql = format!(
        "INSERT INTO projects (id, name, created_at, updated_at, schema_version)
         VALUES ('project-1', 'Project', 'now', 'now', 1);
         INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at)
         VALUES ('character-1', 'project-1', 'character', 'Mara', 'mara', 'now', 'now');
         INSERT INTO canon_sections
         (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at)
         VALUES ('visual-locks', 'character-1', 'visual_locks',
                 '{{\"locks\":[{{\"id\":\"lock-1\",\"key\":\"right_eyebrow_scar\",\"description\":\"Scar on character-right eyebrow\",\"severity\":\"required\",\"validatorHint\":\"Character-right appears viewer-left when frontal\"}}]}}',
                 '{visual_lock_status}', 4, 'now', 'now', 'now');
         INSERT INTO assets
         (id, project_id, type, label, owner_entity_id, canonical_version_id, created_at, updated_at)
         VALUES
         ('face-asset', 'project-1', 'face_lock', 'Face', 'character-1', 'face-v1', 'now', 'now'),
         ('look-asset', 'project-1', 'character_sheet', 'Look', 'character-1', 'look-v1', 'now', 'now'),
         ('target-asset', 'project-1', 'image', 'Candidate', 'character-1', NULL, 'now', 'now');
         INSERT INTO asset_versions
         (id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
          original_filename, mime_type, byte_size, created_at)
         VALUES
         ('face-v1', 'face-asset', 1, 'canonical', 'face-v1.png', 'face-v1-thumb.png',
          'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'face.png', 'image/png', 1, 'now'),
         ('face-v2', 'face-asset', 2, 'candidate', 'face-v2.png', 'face-v2-thumb.png',
          'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'face2.png', 'image/png', 1, 'later'),
         ('look-v1', 'look-asset', 1, 'canonical', 'look-v1.png', 'look-v1-thumb.png',
          'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'look.png', 'image/png', 1, 'now'),
         ('target-v1', 'target-asset', 1, 'candidate', 'target.png', 'target-thumb.png',
          'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', 'target.png', 'image/png', 1, 'now');"
    );
    conn.execute_batch(&sql).unwrap();
    conn
}

fn request() -> QaPlanningRequest {
    QaPlanningRequest {
        project_id: "project-1".into(),
        asset_version_id: "target-v1".into(),
        created_at: "2026-08-28T00:00:00Z".into(),
        expectations: vec![VisualExpectation {
            id: "neutral-background".into(),
            expectation_type: QaCheckType::BackgroundRequirement,
            requirement: "Flat neutral background".into(),
            blocking: true,
            validator_hint: None,
        }],
    }
}

#[test]
fn planner_is_deterministic_and_uses_exact_canonical_versions() {
    let conn = fixture("locked");
    let context = resolve_qa_context(&conn, &request()).unwrap();
    let first = QaCheckPlanner::compile(&context).unwrap();
    let second = QaCheckPlanner::compile(&context).unwrap();

    assert_eq!(first, second);
    assert_eq!(
        first.reference_asset_version_ids,
        vec!["face-v1".to_string(), "look-v1".to_string()]
    );
    let ids = first
        .checks
        .iter()
        .map(|check| check.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "artifact:unexpected",
            "artifact:watermark",
            "expectation:neutral-background",
            "lock:right_eyebrow_scar",
            "reference:identity",
            "reference:look",
        ]
    );
    assert!(first
        .checks
        .iter()
        .find(|check| check.id == "lock:right_eyebrow_scar")
        .unwrap()
        .validator_hint
        .as_deref()
        .unwrap()
        .contains("Character-right"));
}

#[test]
fn draft_visual_locks_do_not_leak_and_newest_candidate_is_not_authority() {
    let conn = fixture("draft");
    let context = resolve_qa_context(&conn, &request()).unwrap();
    let plan = QaCheckPlanner::compile(&context).unwrap();

    assert!(!plan.checks.iter().any(|check| check.id.starts_with("lock:")));
    assert!(plan.reference_asset_version_ids.contains(&"face-v1".to_string()));
    assert!(!plan.reference_asset_version_ids.contains(&"face-v2".to_string()));
}

#[test]
fn missing_canonical_face_blocks_with_a_useful_reason() {
    let conn = fixture("locked");
    conn.execute(
        "UPDATE assets SET canonical_version_id = NULL WHERE id = 'face-asset'",
        [],
    )
    .unwrap();

    let error = resolve_qa_context(&conn, &request()).unwrap_err();
    assert!(error
        .to_string()
        .contains("character has no exact canonical Face version"));
}
