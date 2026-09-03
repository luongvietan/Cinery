//! P10.4 Task 6 — the canonical resolver is the downstream contract for
//! P11 Scene Assembly / Timeline. It must return the exact promoted video
//! version, never "latest". Newer generations, QA outcomes, restarts, or
//! candidate volume must never silently change the selection.

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::cinema::commands::*;
use cinematic_desktop_lib::cinema::service::CinemaService;
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
    artifact_ids: Vec<String>,
}

fn mp4_bytes(seed: u8) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x00, 0x00, 0x18];
    bytes.extend_from_slice(b"ftypmp42");
    bytes.extend_from_slice(&[0x00, 0x00, 0x00, seed]);
    bytes.extend_from_slice(b"mp42isom");
    bytes
}

fn image_file(root: &std::path::Path, name: &str, pixel: [u8; 4]) -> std::path::PathBuf {
    let path = root.join(name);
    let image: image::RgbaImage = image::ImageBuffer::from_pixel(8, 8, image::Rgba(pixel));
    image.save(&path).unwrap();
    path
}

/// Builds a project with one shot whose pinned keyframe was animated by
/// `count` distinct completed I2V captures (V1..Vn, each its own artifact
/// and imported candidate version).
fn fixture_with_candidates(count: usize) -> Fixture {
    let temp = tempdir().unwrap();
    let root_path = temp.path().join("resolver");
    let root = root_path.to_string_lossy().to_string();
    ProjectService::create(&root_path, "Resolver").unwrap();

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
        &image_file(&root_path, "kf.png", [29, 47, 83, 255]),
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

    let conn = db::open_existing_connection(&root_path.join("project.db")).unwrap();
    let project_id: String = conn
        .query_row("SELECT id FROM projects", [], |row| row.get(0))
        .unwrap();

    let video_asset = AssetService::create_asset(
        &root_path,
        "video",
        "Scene 001 video",
        Some(scene.id.clone()),
    )
    .unwrap();

    let mut artifact_ids = Vec::new();
    for index in 0..count {
        let run_id = format!("run-{index}");
        let attempt_id = format!("attempt-{index}");
        let input = json!({
            "sceneId": scene.id,
            "shotId": shot.id,
            "sourceAssetVersionId": keyframe_version.id,
        });
        conn.execute(
            "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, \
             status, input_json, created_at, updated_at) \
             VALUES (?1, ?2, 'scene-builder', '1.0.0', 'shot.image_to_video', \
             'completed', ?3, 'now', 'now')",
            rusqlite::params![run_id, project_id, input.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, \
             attempt_number, compiled_request_id, provider_id, model_id, adapter_version, \
             idempotency_key, status, started_at) \
             VALUES (?1, ?2, 'execute', 1, ?3, 'fake_async_video', 'fake-video-v1', 1, ?4, \
             'succeeded', 'now')",
            rusqlite::params![
                attempt_id,
                run_id,
                format!("compiled-{index}"),
                format!("{run_id}:execute:1")
            ],
        )
        .unwrap();

        let artifacts = GenerationService::capture_provider_result(
            &root_path,
            &GenerationCaptureInput {
                project_id: project_id.clone(),
                workflow_run_id: run_id,
                workflow_step_key: "execute".into(),
                workflow_definition_id: "scene-builder".into(),
                workflow_version: "1.0.0".into(),
                skill_id: "scene-builder".into(),
                skill_version: "1.0.0".into(),
                compiled_execution_artifact_id: format!("compiled-{index}"),
                compiled_request_sha256: format!("{:0>64}", index),
                canon_snapshot_id: None,
                canon_snapshot_sha256: None,
                provider_attempt_id: attempt_id,
                provider_id: "fake_async_video".into(),
                model_id: "fake-video-v1".into(),
                source_asset_version_ids: vec![keyframe_version.id.clone()],
                requested_output_count: 1,
                media_kind: "video".into(),
            },
            &ProviderResult {
                outputs: vec![ProviderOutput {
                    uri: format!(
                        "data:video/mp4;base64,{}",
                        base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            mp4_bytes(10 + index as u8)
                        )
                    ),
                    mime_type: "video/mp4".into(),
                    filename: Some(format!("v{}.mp4", index + 1)),
                }],
                provider_reported_model: Some("fake-video-v1".into()),
                metadata: json!({}),
            },
        )
        .unwrap()
        .artifacts;
        assert_eq!(artifacts.len(), 1);
        let artifact_id = artifacts[0].id.clone();

        let candidate_source = root_path.join(format!("candidate-{index}.mp4"));
        std::fs::write(&candidate_source, mp4_bytes(10 + index as u8)).unwrap();
        AssetService::import_media_version(&root_path, &video_asset.id, &candidate_source, None)
            .unwrap();
        artifact_ids.push(artifact_id);
    }
    drop(conn);

    Fixture {
        _temp: temp,
        root,
        scene_id: scene.id,
        shot_id: shot.id,
        artifact_ids,
    }
}

fn version_of_artifact(f: &Fixture, artifact_id: &str) -> String {
    let root_path = std::path::Path::new(&f.root);
    let detail =
        cinematic_desktop_lib::generation::service::GenerationService::get_artifact_detail(
            root_path,
            artifact_id,
        )
        .unwrap();
    let conn = db::open_existing_connection(&root_path.join("project.db")).unwrap();
    conn.query_row(
        "SELECT id FROM asset_versions WHERE sha256 = ?1",
        [detail.artifact.sha256],
        |row| row.get(0),
    )
    .unwrap()
}

fn insert_video_qa(f: &Fixture, version_id: &str, overall: QaOverallStatus, id: &str) {
    let root_path = std::path::Path::new(&f.root);
    let conn = db::open_existing_connection(&root_path.join("project.db")).unwrap();
    let asset_id: String = conn
        .query_row(
            "SELECT asset_id FROM asset_versions WHERE id = ?1",
            [version_id],
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
            asset_version_id: version_id.to_string(),
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
            created_at: format!("2026-09-03T00:0{}:00Z", id.len() % 10),
            started_at: None,
            completed_at: None,
        },
    )
    .unwrap();
}

#[test]
fn resolver_returns_none_then_exact_promotions_and_never_latest() {
    let f = fixture_with_candidates(3);
    let v1 = version_of_artifact(&f, &f.artifact_ids[0]);
    let v2 = version_of_artifact(&f, &f.artifact_ids[1]);
    let v3 = version_of_artifact(&f, &f.artifact_ids[2]);

    // No canonical -> None (never V3, the latest).
    assert_eq!(
        resolve_canonical_shot_video(f.root.clone(), f.shot_id.clone()).unwrap(),
        None
    );

    // Promote V2 -> exactly V2.
    promote_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        f.artifact_ids[1].clone(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        resolve_canonical_shot_video(f.root.clone(), f.shot_id.clone()).unwrap(),
        Some(v2.clone())
    );

    // Newer generation exists (V3 captured in the fixture) -> still V2.
    assert_eq!(
        resolve_canonical_shot_video(f.root.clone(), f.shot_id.clone()).unwrap(),
        Some(v2.clone())
    );

    // V3's QA passes -> still V2. QA is advisory.
    insert_video_qa(&f, &v3, QaOverallStatus::Pass, "qa-v3");
    assert_eq!(
        resolve_canonical_shot_video(f.root.clone(), f.shot_id.clone()).unwrap(),
        Some(v2.clone())
    );

    // Promote V3 explicitly -> now exactly V3.
    promote_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        f.artifact_ids[2].clone(),
        Some(v2),
        None,
    )
    .unwrap();
    assert_eq!(
        resolve_canonical_shot_video(f.root.clone(), f.shot_id.clone()).unwrap(),
        Some(v3.clone())
    );

    // Rollback: promoting V1 again is another normal promotion event.
    promote_shot_video_candidate(
        f.root.clone(),
        f.shot_id.clone(),
        f.artifact_ids[0].clone(),
        Some(v3),
        None,
    )
    .unwrap();
    assert_eq!(
        resolve_canonical_shot_video(f.root.clone(), f.shot_id.clone()).unwrap(),
        Some(v1)
    );

    // All three candidates remain available in the read model.
    let candidates = list_shot_video_candidates(f.root.clone(), f.shot_id.clone()).unwrap();
    assert_eq!(candidates.len(), 3);

    // The shot row keeps the exact pin; the keyframe pin is untouched.
    let shot = CinemaService::list_shots(std::path::Path::new(&f.root), &f.scene_id)
        .unwrap()
        .into_iter()
        .find(|shot| shot.id == f.shot_id)
        .unwrap();
    assert!(shot.keyframe_asset_version_id.is_some());
}
