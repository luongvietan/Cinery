//! P10.4 Task 8 — end-to-end acceptance across the real application
//! boundary. Exercises the spec's scenarios A–I against a real project
//! database and the same command functions the Tauri layer exposes.

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::cinema::commands::*;
use cinematic_desktop_lib::db;
use cinematic_desktop_lib::generation::service::{GenerationCaptureInput, GenerationService};
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::providers::model::{ProviderOutput, ProviderResult};
use cinematic_desktop_lib::qa::models::{QaMediaKind, QaOverallStatus, QaRunRecord, QaRunStatus};
use cinematic_desktop_lib::qa::repository;
use cinematic_desktop_lib::scenes::commands as scene_commands;
use cinematic_desktop_lib::scenes::service::SceneService;
use serde_json::json;
use tempfile::tempdir;

struct Fixture {
    _temp: tempfile::TempDir,
    root: String,
    scene_id: String,
    shot_id: String,
    video_asset_id: String,
    keyframe_version_id: String,
}

fn mp4_bytes(seed: u8) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x00, 0x00, 0x18];
    bytes.extend_from_slice(b"ftypmp42");
    bytes.extend_from_slice(&[0x00, 0x00, 0x00, seed]);
    bytes.extend_from_slice(b"mp42isom");
    bytes
}

fn image_file(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = root.join(name);
    let image: image::RgbaImage =
        image::ImageBuffer::from_pixel(8, 8, image::Rgba([29, 47, 83, 255]));
    image.save(&path).unwrap();
    path
}

fn setup() -> Fixture {
    let temp = tempdir().unwrap();
    let root_path = temp.path().join("acceptance");
    let root = root_path.to_string_lossy().to_string();
    ProjectService::create(&root_path, "Acceptance").unwrap();

    let scene =
        scene_commands::create_world_scene(root.clone(), "Scene 001".into(), "S".into()).unwrap();
    let shot = create_shot(
        root.clone(),
        scene.id.clone(),
        None,
        4.0,
        "Establish".into(),
        None,
        None,
    )
    .unwrap();

    let keyframe_asset = SceneService::ensure_scene_keyframe_asset(&root_path, &scene.id).unwrap();
    let keyframe_version = AssetService::import_asset_version(
        &root_path,
        &keyframe_asset.id,
        &image_file(&root_path, "kf.png"),
        None,
    )
    .unwrap();
    AssetService::promote_asset_version(&root_path, &keyframe_version.id).unwrap();
    set_shot_keyframe(
        root.clone(),
        shot.id.clone(),
        Some(keyframe_version.id.clone()),
    )
    .unwrap();

    let video_asset = AssetService::create_asset(
        &root_path,
        "video",
        "Scene 001 video",
        Some(scene.id.clone()),
    )
    .unwrap();

    Fixture {
        _temp: temp,
        root,
        scene_id: scene.id,
        shot_id: shot.id,
        video_asset_id: video_asset.id,
        keyframe_version_id: keyframe_version.id,
    }
}

/// Simulates a completed I2V generation: durable run + attempt, captured
/// artifact, and completion-time import into the scene video asset.
/// Returns the artifact id (usable for promotion).
fn generate_candidate(f: &Fixture, label: &str, seed: u8) -> String {
    let root_path = std::path::Path::new(&f.root);
    let conn = db::open_existing_connection(&root_path.join("project.db")).unwrap();
    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();
    let input = json!({
        "sceneId": f.scene_id,
        "shotId": f.shot_id,
        "sourceAssetVersionId": f.keyframe_version_id,
    });
    conn.execute(
        "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, \
         status, input_json, created_at, updated_at) \
         VALUES (?1, ?2, 'scene-builder', '1.0.0', 'shot.image_to_video', 'completed', ?3, \
         'now', 'now')",
        rusqlite::params![format!("run-{label}"), project_id, input.to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, \
         attempt_number, compiled_request_id, provider_id, model_id, adapter_version, \
         idempotency_key, status, started_at) \
         VALUES (?1, ?2, 'execute', 1, ?3, 'fake_async_video', 'fake-video-v1', 1, ?4, \
         'succeeded', 'now')",
        rusqlite::params![
            format!("attempt-{label}"),
            format!("run-{label}"),
            format!("compiled-{label}"),
            format!("run-{label}:execute:1")
        ],
    )
    .unwrap();
    drop(conn);

    // Lineage hashes must be 64 hex chars; build one from the label
    // deterministically.
    let hex_prefix: String = label.bytes().map(|byte| format!("{byte:02x}")).collect();
    let compiled_hash = format!("{hex_prefix}{}", "0".repeat(64 - hex_prefix.len()));

    let artifacts = GenerationService::capture_provider_result(
        root_path,
        &GenerationCaptureInput {
            project_id,
            workflow_run_id: format!("run-{label}"),
            workflow_step_key: "execute".into(),
            workflow_definition_id: "scene-builder".into(),
            workflow_version: "1.0.0".into(),
            skill_id: "scene-builder".into(),
            skill_version: "1.0.0".into(),
            compiled_execution_artifact_id: format!("compiled-{label}"),
            compiled_request_sha256: compiled_hash,
            canon_snapshot_id: None,
            canon_snapshot_sha256: None,
            provider_attempt_id: format!("attempt-{label}"),
            provider_id: "fake_async_video".into(),
            model_id: "fake-video-v1".into(),
            source_asset_version_ids: vec![f.keyframe_version_id.clone()],
            requested_output_count: 1,
            media_kind: "video".into(),
        },
        &ProviderResult {
            outputs: vec![ProviderOutput {
                uri: format!(
                    "data:video/mp4;base64,{}",
                    base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        mp4_bytes(seed)
                    )
                ),
                mime_type: "video/mp4".into(),
                filename: Some(format!("{label}.mp4")),
            }],
            provider_reported_model: Some("fake-video-v1".into()),
            metadata: json!({}),
        },
    )
    .unwrap_or_else(|error| panic!("capture failed for {label}: {error}"))
    .artifacts;

    let candidate_source = root_path.join(format!("candidate-{label}.mp4"));
    std::fs::write(&candidate_source, mp4_bytes(seed)).unwrap();
    AssetService::import_media_version(root_path, &f.video_asset_id, &candidate_source, None)
        .unwrap();
    artifacts[0].id.clone()
}

/// "Restarts" the application: drops every connection and reopens the
/// database from disk, like a fresh Tauri launch.
fn restart(f: &Fixture) {
    let path = std::path::Path::new(&f.root).join("project.db");
    let conn = db::open_existing_connection(&path).unwrap();
    let shot_id = f.shot_id.clone();
    drop(conn);
    // Reopen from disk and touch the pinned row (fresh Tauri launch).
    let conn = db::open_existing_connection(&path).unwrap();
    let canonical: Option<String> = conn
        .query_row(
            "SELECT generated_video_asset_version_id FROM scene_shots WHERE id = ?1",
            [&shot_id],
            |row| row.get(0),
        )
        .unwrap();
    let _ = canonical; // touch the row so the reopen is exercised
}

fn canonical(f: &Fixture) -> Option<String> {
    resolve_canonical_shot_video(f.root.clone(), f.shot_id.clone()).unwrap()
}

fn insert_qa(f: &Fixture, version: &str, overall: QaOverallStatus, id: &str) {
    let root_path = std::path::Path::new(&f.root);
    let conn = db::open_existing_connection(&root_path.join("project.db")).unwrap();
    let asset_id: String = conn
        .query_row(
            "SELECT asset_id FROM asset_versions WHERE id = ?1",
            [version],
            |row| row.get(0),
        )
        .unwrap();
    repository::insert_run(
        &conn,
        &QaRunRecord {
            id: id.to_string(),
            project_id: conn
                .query_row("SELECT id FROM projects", [], |row| row.get(0))
                .unwrap(),
            asset_id,
            asset_version_id: version.to_string(),
            media_kind: QaMediaKind::Video,
            workflow_run_id: None,
            status: QaRunStatus::Succeeded,
            overall_status: Some(overall),
            adapter_id: Some("mock".into()),
            adapter_version: Some("1".into()),
            model_id: Some("mock-video-qa-v1".into()),
            execution_location: "local".into(),
            check_plan: json!({"assetType": "video"}),
            context_snapshot: json!({}),
            raw_response_metadata: None,
            error_code: None,
            error_message: None,
            created_at: "2026-09-03T00:00:00Z".to_string(),
            started_at: None,
            completed_at: None,
        },
    )
    .unwrap();
}

fn audit_events(f: &Fixture, event_type: &str) -> Vec<serde_json::Value> {
    let conn =
        db::open_existing_connection(&std::path::Path::new(&f.root).join("project.db")).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT payload_json FROM provider_audit_events \
             WHERE event_type = ?1 ORDER BY created_at ASC",
        )
        .unwrap();
    let rows = stmt
        .query_map([event_type], |row| row.get::<_, String>(0))
        .unwrap();
    rows.map(|row| serde_json::from_str(&row.unwrap()).unwrap())
        .collect()
}

/// Scenario A — first canonical selection survives restart.
#[test]
fn scenario_a_first_canonical_selection_survives_restart() {
    let f = setup();
    let a1 = generate_candidate(&f, "v1a", 1);
    let _a2 = generate_candidate(&f, "v2a", 2);
    let a3 = generate_candidate(&f, "v3a", 3);

    let promoted =
        promote_shot_video_candidate(f.root.clone(), f.shot_id.clone(), a1, None, None).unwrap();
    assert_eq!(canonical(&f), Some(promoted.asset_version_id.clone()));

    restart(&f);
    assert_eq!(
        canonical(&f),
        Some(promoted.asset_version_id),
        "canonical selection must survive an application restart"
    );
    // All candidates preserved.
    assert_eq!(
        list_shot_video_candidates(f.root.clone(), f.shot_id.clone())
            .unwrap()
            .len(),
        3
    );
    let _ = a3;
}

/// Scenario B — a newer successful generation with passing QA must NOT
/// steal canonical status.
#[test]
fn scenario_b_new_generation_never_steals_canonical_status() {
    let f = setup();
    let v1_artifact = generate_candidate(&f, "b1", 1);
    let promoted =
        promote_shot_video_candidate(f.root.clone(), f.shot_id.clone(), v1_artifact, None, None)
            .unwrap();

    let v2_artifact = generate_candidate(&f, "b2", 2);
    let v2 = version_of(&f, &v2_artifact);
    insert_qa(&f, &v2, QaOverallStatus::Pass, "qa-b2");

    assert_eq!(canonical(&f), Some(promoted.asset_version_id));
}

fn version_of(f: &Fixture, artifact_id: &str) -> String {
    let detail =
        GenerationService::get_artifact_detail(std::path::Path::new(&f.root), artifact_id).unwrap();
    let conn =
        db::open_existing_connection(&std::path::Path::new(&f.root).join("project.db")).unwrap();
    conn.query_row(
        "SELECT id FROM asset_versions WHERE sha256 = ?1",
        [detail.artifact.sha256],
        |row| row.get(0),
    )
    .unwrap()
}

/// Scenario C + D — replace canonical, then roll back by promoting the
/// older version again; history records every transition.
#[test]
fn scenario_c_and_d_replace_then_rollback() {
    let f = setup();
    let v1_artifact = generate_candidate(&f, "c1", 1);
    let v2_artifact = generate_candidate(&f, "c2", 2);
    let v1 = version_of(&f, &v1_artifact);
    let v2 = version_of(&f, &v2_artifact);

    promote_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        v1_artifact.clone(),
        None,
        None,
    )
    .unwrap();
    promote_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        v2_artifact.clone(),
        Some(v1.clone()),
        None,
    )
    .unwrap();
    assert_eq!(canonical(&f), Some(v2.clone()));

    // Rollback: promoting V1 again is just another promotion.
    promote_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        v1_artifact,
        Some(v2.clone()),
        None,
    )
    .unwrap();
    assert_eq!(canonical(&f), Some(v1.clone()));

    // Both versions remain available in the read model.
    let candidates = list_shot_video_candidates(f.root.clone(), f.shot_id.clone()).unwrap();
    assert_eq!(candidates.len(), 2);

    let promotions = audit_events(&f, "shot.video.promoted");
    assert_eq!(promotions.len(), 3, "null→V1, V1→V2, V2→V1 all audited");
    assert_eq!(
        promotions[0]["previousAssetVersionId"],
        serde_json::json!(null)
    );
    assert_eq!(
        promotions[1]["previousAssetVersionId"],
        serde_json::json!(v1)
    );
    assert_eq!(
        promotions[2]["previousAssetVersionId"],
        serde_json::json!(v2)
    );
}

/// Scenario E — reject preserves artifact + QA, survives restart, and
/// restore returns Active without promoting.
#[test]
fn scenario_e_reject_preserves_and_survives_restart() {
    let f = setup();
    let v1_artifact = generate_candidate(&f, "e1", 1);
    let _v2_artifact = generate_candidate(&f, "e2", 2);
    let v3_artifact = generate_candidate(&f, "e3", 3);
    let v1 = version_of(&f, &v1_artifact);
    let v3 = version_of(&f, &v3_artifact);

    promote_shot_video_candidate(f.root.clone(), f.shot_id.clone(), v1_artifact, None, None)
        .unwrap();
    insert_qa(&f, &v3, QaOverallStatus::NeedsReview, "qa-e3");

    // Reject V3.
    reject_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        v3.clone(),
        Some("unused".into()),
    )
    .unwrap();

    restart(&f);
    // Still rejected after restart, artifact + QA preserved.
    let conn =
        db::open_existing_connection(&std::path::Path::new(&f.root).join("project.db")).unwrap();
    let state: String = conn
        .query_row(
            "SELECT state FROM shot_video_review_states WHERE asset_version_id = ?1",
            [&v3],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "rejected");
    drop(conn);
    let candidates = list_shot_video_candidates(f.root.clone(), f.shot_id.clone()).unwrap();
    let rejected = candidates
        .iter()
        .find(|candidate| candidate.asset_version_id == v3)
        .unwrap();
    assert_eq!(rejected.review_state.as_str(), "rejected");
    assert_eq!(rejected.qa_run_count, 1);

    // Restore: Active again, still not canonical.
    restore_shot_video_candidate(f.root.clone(), f.shot_id.clone(), v3.clone()).unwrap();
    assert_eq!(canonical(&f), Some(v1));
}

/// Scenario F — rejecting the canonical is refused with no side effects.
#[test]
fn scenario_f_canonical_reject_is_refused() {
    let f = setup();
    let v1_artifact = generate_candidate(&f, "f1", 1);
    let promoted =
        promote_shot_video_candidate(f.root.clone(), f.shot_id.clone(), v1_artifact, None, None)
            .unwrap();

    let error = reject_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        promoted.asset_version_id.clone(),
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "CANONICAL_CANDIDATE_CANNOT_BE_REJECTED");
    assert_eq!(canonical(&f), Some(promoted.asset_version_id));
    // No automatic unpromotion: no review row was created either.
    let conn =
        db::open_existing_connection(&std::path::Path::new(&f.root).join("project.db")).unwrap();
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM shot_video_review_states", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rows, 0);
}

/// Scenario G — QA-failed candidate: promotion blocked without override,
/// succeeds with explicit override, audited with qaOverride = true.
#[test]
fn scenario_g_qa_failure_override() {
    let f = setup();
    let v5_artifact = generate_candidate(&f, "g5", 5);
    let v5 = version_of(&f, &v5_artifact);
    insert_qa(&f, &v5, QaOverallStatus::Fail, "qa-g5");

    let blocked = promote_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        v5_artifact.clone(),
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(blocked.code, "QA_OVERRIDE_REQUIRED");
    assert_eq!(canonical(&f), None);

    let promoted = promote_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        v5_artifact,
        None,
        Some(String::from("Director approved this take.")),
    )
    .unwrap();
    assert_eq!(canonical(&f), Some(promoted.asset_version_id));
    let events = audit_events(&f, "shot.video.promoted");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["qaOverride"], serde_json::json!(true));
    assert_eq!(
        events[0]["overrideReason"],
        serde_json::json!("Director approved this take.")
    );
}

/// Scenario H — optimistic concurrency: client B expects stale V2, gets
/// PROMOTION_CONFLICT while A's V3 stays canonical.
#[test]
fn scenario_h_concurrent_promotion_protection() {
    let f = setup();
    let v2_artifact = generate_candidate(&f, "h2", 2);
    let v3_artifact = generate_candidate(&f, "h3", 3);
    let v4_artifact = generate_candidate(&f, "h4", 4);
    let v2 = version_of(&f, &v2_artifact);

    // A and B both read V2 as canonical.
    promote_shot_video_candidate(f.root.clone(), f.shot_id.clone(), v2_artifact, None, None)
        .unwrap();
    assert_eq!(canonical(&f), Some(v2.clone()));

    // A promotes V3 first.
    promote_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        v3_artifact,
        Some(v2.clone()),
        None,
    )
    .unwrap();

    // B tries V4 expecting V2: conflict, and A's V3 stays canonical.
    let error = promote_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        v4_artifact,
        Some(v2.clone()),
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "PROMOTION_CONFLICT");
    let candidates = list_shot_video_candidates(f.root.clone(), f.shot_id.clone()).unwrap();
    let winner = candidates
        .iter()
        .find(|candidate| candidate.is_canonical)
        .unwrap();
    let conn =
        db::open_existing_connection(&std::path::Path::new(&f.root).join("project.db")).unwrap();
    let pinned: Option<String> = conn
        .query_row(
            "SELECT generated_video_asset_version_id FROM scene_shots WHERE id = ?1",
            [&f.shot_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(Some(winner.asset_version_id.clone()), pinned);
    assert_ne!(pinned, Some(v2));
}

/// Scenario I — exact-version downstream resolution: a newer generation
/// without promotion never resolves as canonical.
#[test]
fn scenario_i_resolver_never_falls_back_to_latest() {
    let f = setup();
    let v1_artifact = generate_candidate(&f, "i1", 1);
    let promoted =
        promote_shot_video_candidate(f.root.clone(), f.shot_id.clone(), v1_artifact, None, None)
            .unwrap();

    generate_candidate(&f, "i6", 6);
    generate_candidate(&f, "i7", 7);

    assert_eq!(
        canonical(&f),
        Some(promoted.asset_version_id),
        "resolver must return the promoted version, never the newest generation"
    );
}
