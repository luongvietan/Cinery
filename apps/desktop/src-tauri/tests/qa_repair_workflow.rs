use cinematic_desktop_lib::{
    assets::service::AssetService,
    db,
    project::service::ProjectService,
    workflow::runtime::WorkflowRuntime,
};
use image::{ImageBuffer, Rgba};
use rusqlite::params;
use serde_json::{json, Value};

struct Fixture {
    _temp: tempfile::TempDir,
    root: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let project = ProjectService::create(&root, "Repair Project").unwrap();
        for (name, value) in [("face.png", 40_u8), ("look.png", 80_u8), ("target.png", 120_u8)] {
            let image = ImageBuffer::from_pixel(32, 32, Rgba([value, value, value, 255]));
            image.save(root.join(name)).unwrap();
        }
        let mut conn = db::open_existing_connection(&root.join("project.db")).unwrap();
        conn.execute_batch(&format!(
            "INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at)
             VALUES ('character-1', '{}', 'character', 'Mara', 'mara', 'now', 'now');
             INSERT INTO assets
             (id, project_id, type, label, owner_entity_id, canonical_version_id, created_at, updated_at)
             VALUES
             ('face', '{}', 'face_lock', 'Face', 'character-1', 'face-v1', 'now', 'now'),
             ('look', '{}', 'character_sheet', 'Look', 'character-1', 'look-v1', 'now', 'now'),
             ('target', '{}', 'image', 'Candidate', 'character-1', NULL, 'now', 'now');
             INSERT INTO asset_versions
             (id, asset_id, version_number, status, file_path, thumbnail_path, sha256,
              original_filename, mime_type, byte_size, created_at)
             VALUES
             ('face-v1', 'face', 1, 'canonical', 'face.png', 'face.png',
              'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'face.png', 'image/png', 1, 'now'),
             ('look-v1', 'look', 1, 'canonical', 'look.png', 'look.png',
              'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'look.png', 'image/png', 1, 'now'),
             ('target-v1', 'target', 1, 'candidate', 'target.png', 'target.png',
              'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'target.png', 'image/png', 1, 'now');",
            project.id, project.id, project.id, project.id
        ))
        .unwrap();
        let plan = json!({
            "schemaVersion": 1,
            "assetId": "target",
            "assetVersionId": "target-v1",
            "ownerEntityId": "character-1",
            "assetType": "image",
            "referenceAssetVersionIds": ["face-v1", "look-v1"],
            "checks": [
                {"id":"reference:identity","checkType":"identity_similarity","source":"canonical_reference","key":"character_identity","label":"Identity","requirement":"Preserve canonical character identity.","validatorHint":null,"blocking":true,"referenceAssetVersionIds":["face-v1"]},
                {"id":"lock:right_eyebrow_scar","checkType":"permanent_visual_lock","source":"visual_lock","key":"right_eyebrow_scar","label":"Scar","requirement":"Scar is on the character-right eyebrow.","validatorHint":"character-right appears viewer-left in a frontal view.","blocking":true,"referenceAssetVersionIds":["face-v1"]},
                {"id":"lock:watch_left_wrist","checkType":"accessory_placement","source":"visual_lock","key":"watch_left_wrist","label":"Watch","requirement":"Watch remains on the character-left wrist.","validatorHint":null,"blocking":true,"referenceAssetVersionIds":["look-v1"]},
                {"id":"artifact:unexpected","checkType":"unexpected_artifact","source":"artifact_detection","key":"unexpected_artifact","label":"Artifact","requirement":"No unexpected artifact.","validatorHint":null,"blocking":true,"referenceAssetVersionIds":[]}
            ],
            "createdAt": "2026-08-28T00:00:00Z"
        });
        conn.execute(
            "INSERT INTO qa_runs
             (id, project_id, asset_id, asset_version_id, status, overall_status, adapter_id,
              adapter_version, model_id, execution_location, check_plan_json,
              context_snapshot_json, created_at, started_at, completed_at)
             VALUES ('qa-1', ?1, 'target', 'target-v1', 'succeeded', 'fail', 'mock', '1',
                     'mock-vlm', 'local', ?2, '{}', 'now', 'now', 'now')",
            params![project.id, plan.to_string()],
        )
        .unwrap();
        let checks = [
            ("reference:identity", "identity_similarity", "canonical_reference", "pass", None),
            ("lock:right_eyebrow_scar", "permanent_visual_lock", "visual_lock", "fail", Some("Move the scar to the character-right eyebrow.")),
            ("lock:watch_left_wrist", "accessory_placement", "visual_lock", "pass", None),
            ("artifact:unexpected", "unexpected_artifact", "artifact_detection", "fail", Some("Remove the unexpected lower-right artifact.")),
        ];
        for (index, (id, check_type, source, status, hint)) in checks.into_iter().enumerate() {
            conn.execute(
                "INSERT INTO qa_checks
                 (id, qa_run_id, check_id, check_type, source, requirement_json, status,
                  confidence, observed, reason, repair_hint, review_status, created_at)
                 VALUES (?1, 'qa-1', ?2, ?3, ?4, '{}', ?5, 0.95, 'fixture', 'fixture', ?6,
                         'unreviewed', 'now')",
                params![format!("check-{index}"), id, check_type, source, status, hint],
            )
            .unwrap();
        }
        drop(conn);
        Self { _temp: temp, root }
    }

    fn input(&self, provider_id: &str) -> Value {
        json!({
            "projectRootPath": self.root,
            "qaRunId": "qa-1",
            "providerId": provider_id,
            "modelId": "mock-image-v1"
        })
    }
}

#[test]
fn repair_waits_for_approval_and_creates_one_child_with_full_provenance() {
    let fixture = Fixture::new();
    let source_bytes = std::fs::read(fixture.root.join("target.png")).unwrap();
    let created = WorkflowRuntime::create_run(
        &fixture.root,
        "visual-qa",
        "1.0.0",
        "asset.repair_failed_qa",
        fixture.input("mock"),
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(&fixture.root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    let compiled: Value = serde_json::from_str(
        waiting
            .steps
            .iter()
            .find(|step| step.step_type == "compile_request")
            .and_then(|step| step.output_json.as_deref())
            .unwrap(),
    )
    .unwrap();
    assert!(compiled["prompt"].as_str().unwrap().contains("character-right"));
    let references = compiled["references"].as_array().unwrap();
    assert_eq!(references.len(), 3, "source plus two exact references");

    WorkflowRuntime::approve_run_step(
        &fixture.root,
        &created.run.id,
        "approve-repair",
        Some("Approved exact targeted edit".into()),
    )
    .unwrap();
    let completed = WorkflowRuntime::advance_run(&fixture.root, &created.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");

    let target = AssetService::get_asset_with_versions(&fixture.root, "target").unwrap();
    assert_eq!(target.versions.len(), 2);
    let source = target.versions.iter().find(|version| version.id == "target-v1").unwrap();
    let child = target.versions.iter().find(|version| version.id != "target-v1").unwrap();
    assert_eq!(source.status, "candidate");
    assert_eq!(child.status, "candidate");
    assert_eq!(child.parent_version_id.as_deref(), Some("target-v1"));
    assert_eq!(std::fs::read(fixture.root.join("target.png")).unwrap(), source_bytes);

    let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
    let provenance: (String, String, String, String, String) = conn
        .query_row(
            "SELECT child_asset_version_id, source_qa_run_id, workflow_run_id,
                    provider_id, provider_job_id FROM qa_repairs",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap();
    assert_eq!(provenance.0, child.id);
    assert_eq!(provenance.1, "qa-1");
    assert_eq!(provenance.2, created.run.id);
    assert_eq!(provenance.3, "mock");
    assert!(provenance.4.starts_with("mock:"));
}

#[test]
fn failed_provider_execution_creates_no_phantom_child() {
    let fixture = Fixture::new();
    let created = WorkflowRuntime::create_run(
        &fixture.root,
        "visual-qa",
        "1.0.0",
        "asset.repair_failed_qa",
        fixture.input("missing-provider"),
    )
    .unwrap();
    WorkflowRuntime::advance_run(&fixture.root, &created.run.id).unwrap();
    WorkflowRuntime::approve_run_step(
        &fixture.root,
        &created.run.id,
        "approve-repair",
        None,
    )
    .unwrap();
    assert!(WorkflowRuntime::advance_run(&fixture.root, &created.run.id).is_err());

    let target = AssetService::get_asset_with_versions(&fixture.root, "target").unwrap();
    assert_eq!(target.versions.len(), 1);
    assert_eq!(target.versions[0].id, "target-v1");
}
