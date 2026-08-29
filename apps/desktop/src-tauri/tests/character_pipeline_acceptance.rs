use cinematic_desktop_lib::db;
use cinematic_desktop_lib::error::AppError;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::workflow::runtime::WorkflowRuntime;
use rusqlite::{params, OptionalExtension};
use serde_json::json;
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, String) {
    let temp = tempdir().unwrap();
    let root = temp.path().join("red-door");
    ProjectService::create(&root, "Red Door").unwrap();
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();
    conn.execute("INSERT INTO canon_entities (id, project_id, type, name, slug, created_at, updated_at) VALUES ('mara', ?1, 'character', 'Mara', 'mara', 'now', 'now')", params![project_id]).unwrap();
    conn.execute("INSERT INTO canon_sections (id, canon_entity_id, section_key, value_json, status, revision, created_at, updated_at, locked_at) VALUES ('summary', 'mara', 'visual_summary', '{\"text\":\"Angular face and dark hair.\"}', 'locked', 2, 'now', 'now', 'now')", []).unwrap();
    (temp, root, project_id)
}

fn outfit_input(root: &std::path::Path) -> serde_json::Value {
    json!({
        "projectRootPath": root.to_string_lossy(),
        "characterEntityId": "mara",
        "wardrobeProposal": {
            "description": "charcoal long-sleeve top, dark utility trousers, black boots, black watch on left wrist"
        },
        "providerId": "mock",
        "modelId": "mock-image-v1"
    })
}

fn sheet_input(root: &std::path::Path) -> serde_json::Value {
    json!({
        "projectRootPath": root.to_string_lossy(),
        "characterEntityId": "mara",
        "providerId": "mock",
        "modelId": "mock-image-v1"
    })
}

fn run_to_completion(
    root: &std::path::Path,
    skill_version: &str,
    operation_id: &str,
    input: serde_json::Value,
) -> cinematic_desktop_lib::workflow::model::WorkflowRunDetail {
    let created = WorkflowRuntime::create_run(
        root,
        "character-builder",
        skill_version,
        operation_id,
        input,
    )
    .unwrap();
    let waiting = WorkflowRuntime::advance_run(root, &created.run.id).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    let approved = WorkflowRuntime::approve_run_step(
        root,
        &created.run.id,
        "approve-request",
        None,
    )
    .unwrap();
    assert_eq!(approved.run.status, "ready_for_execution");
    let completed = WorkflowRuntime::advance_run(root, &created.run.id).unwrap();
    assert_eq!(completed.run.status, "completed");
    completed
}

fn canonical_face_version_id(root: &std::path::Path, project_id: &str) -> String {
    let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
    conn.execute(
        "INSERT INTO assets (id, project_id, type, label, owner_entity_id, canonical_version_id, created_at, updated_at) VALUES ('face-asset', ?1, 'face_lock', 'MARA-FACE', 'mara', 'face-v1', 'now', 'now')",
        params![project_id],
    )
    .unwrap();
    // Materialize the referenced file so reference-attachment resolution can
    // verify its hash against the stored metadata.
    let face_path = root.join("assets/face.png");
    std::fs::create_dir_all(face_path.parent().unwrap()).unwrap();
    let face_image: image::RgbaImage = image::ImageBuffer::from_pixel(8, 8, image::Rgba([30, 40, 50, 255]));
    face_image.save(&face_path).unwrap();
    use sha2::{Digest, Sha256};
    let face_bytes = std::fs::read(&face_path).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&face_bytes);
    let face_hash = format!("{:x}", hasher.finalize());
    conn.execute(
        "INSERT INTO asset_versions (id, asset_id, version_number, status, file_path, thumbnail_path, sha256, original_filename, mime_type, byte_size, created_at) VALUES ('face-v1', 'face-asset', 1, 'canonical', 'assets/face.png', 'thumbnails/face.webp', ?1, 'face.png', 'image/png', ?2, 'now')",
        params![face_hash, face_bytes.len() as i64],
    )
    .unwrap();
    drop(conn);
    // Verify data is visible on a fresh connection
    let verify = db::open_existing_connection(&root.join("project.db")).unwrap();
    let found: Option<String> = verify
        .query_row(
            "SELECT a.canonical_version_id FROM assets a JOIN asset_versions v ON v.id = a.canonical_version_id WHERE a.project_id = ?1 AND a.owner_entity_id = 'mara' AND a.type = 'face_lock' AND v.status = 'canonical'",
            params![project_id],
            |row| row.get(0),
        )
        .optional()
        .unwrap();
    assert!(found.is_some(), "face asset should be queryable after insertion");
    "face-v1".to_string()
}

#[test]
fn outfit_is_blocked_without_a_canonical_face() {
    let (_temp, root, _project_id) = fixture();
    let error = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.1.0",
        "character.create_outfit",
        outfit_input(&root),
    )
    .unwrap_err();
    assert!(matches!(error, AppError::WorkflowPrerequisiteFailed(_)));
    assert!(WorkflowRuntime::list_runs(&root).unwrap().is_empty());
}

#[test]
fn sheet_is_blocked_without_a_canonical_outfit() {
    let (_temp, root, project_id) = fixture();
    canonical_face_version_id(&root, &project_id);
    let error = WorkflowRuntime::create_run(
        &root,
        "character-builder",
        "1.1.0",
        "character.create_character_sheet",
        sheet_input(&root),
    )
    .unwrap_err();
    assert!(matches!(error, AppError::WorkflowPrerequisiteFailed(_)));
    assert!(WorkflowRuntime::list_runs(&root).unwrap().is_empty());
}

#[test]
fn outfit_run_resolves_the_canonical_face_and_compiles_direct_on_character_prompt() {
    let (_temp, root, project_id) = fixture();
    canonical_face_version_id(&root, &project_id);

    let completed = run_to_completion(&root, "1.1.0", "character.create_outfit", outfit_input(&root));

    let request: serde_json::Value = serde_json::from_str(
        completed
            .steps
            .iter()
            .find(|step| step.step_type == "compile_request")
            .unwrap()
            .output_json
            .as_deref()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(request["task"], "character_outfit");
    assert!(request["prompt"].as_str().unwrap().contains("WARDROBE PROPOSAL"));
    assert!(request["prompt"].as_str().unwrap().contains("CANONICAL FACE REFERENCE"));
    assert!(request["prompt"]
        .as_str()
        .unwrap()
        .contains("face-v1"));
    assert!(request["references"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reference| reference["reference"] == "face-v1"));
    let snapshot: serde_json::Value = serde_json::from_str(
        completed
            .run
            .context_snapshot_json
            .as_deref()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        snapshot["resolvedContext"]["canonicalFaceAssetVersionId"],
        "face-v1"
    );
    assert_eq!(snapshot["assets"][0]["assetVersionId"], "face-v1");
    assert_eq!(snapshot["assets"][0]["assetType"], "face_lock");
}

#[test]
fn sheet_run_compiles_three_panel_prompt_from_canonical_outfit() {
    let (_temp, root, project_id) = fixture();
    canonical_face_version_id(&root, &project_id);
    let outfit_run = run_to_completion(&root, "1.1.0", "character.create_outfit", outfit_input(&root));
    // Promote a generated outfit artifact into an outfit asset as canonical,
    // mirroring the production promote flow, so the sheet prerequisite passes.
    let result_sets =
        cinematic_desktop_lib::generation::service::GenerationService::list_results(
            &root,
            Some(&outfit_run.run.id),
        )
        .unwrap();
    let artifact = result_sets
        .iter()
        .flat_map(|set| set.artifacts.iter())
        .find(|detail| detail.artifact.capture_status == "available")
        .expect("outfit run should capture an available artifact");
    let outfit_asset = cinematic_desktop_lib::assets::service::AssetService::create_asset(
        &root,
        "outfit",
        "MARA-SHIFT-LOOK",
        Some("mara".into()),
    )
    .unwrap();
    let promoted =
        cinematic_desktop_lib::generation::service::GenerationService::promote_generated_artifact(
            &root,
            &artifact.artifact.id,
            &outfit_asset.id,
            true,
        )
        .unwrap();
    assert_eq!(promoted.status, "canonical");
    let outfit_version_id = promoted.id;

    let completed = run_to_completion(
        &root,
        "1.1.0",
        "character.create_character_sheet",
        sheet_input(&root),
    );

    let request: serde_json::Value = serde_json::from_str(
        completed
            .steps
            .iter()
            .find(|step| step.step_type == "compile_request")
            .unwrap()
            .output_json
            .as_deref()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(request["task"], "character_sheet");
    let prompt = request["prompt"].as_str().unwrap();
    assert!(prompt.contains("SHEET PANELS"));
    assert!(prompt.contains("full-body front, headless"));
    assert!(prompt.contains("full-body rear"));
    assert!(prompt.contains("tight chest-up face"));
    assert!(prompt.contains(&outfit_version_id));
    let snapshot: serde_json::Value = serde_json::from_str(
        completed
            .run
            .context_snapshot_json
            .as_deref()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        snapshot["resolvedContext"]["canonicalOutfitAssetVersionId"],
        outfit_version_id
    );
}
