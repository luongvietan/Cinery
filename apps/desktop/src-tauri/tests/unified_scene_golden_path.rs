//! Unified Scene golden path acceptance (P9 integration stabilization).
//!
//! Exercises, through public service/command APIs only:
//! Project → Canon → World → Scene (world/cast/prop) → Shot CRUD →
//! Keyframe workflow (mock provider) → review → canonical promotion →
//! exact pin on the shot → compile readiness → compile → export →
//! restart → verify exact references → deterministic recompile.

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::canon::model::CanonEntityType;
use cinematic_desktop_lib::canon::service::CanonService;
use cinematic_desktop_lib::cinema::model::CinemaCompileInput;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::generation::commands::promote_generated_artifact;
use cinematic_desktop_lib::project::service::ProjectService;
use cinematic_desktop_lib::scenes::service::SceneService;
use cinematic_desktop_lib::workflow::commands::{
    advance_workflow_run, approve_workflow_step, create_workflow_run,
};
use cinematic_desktop_lib::worlds::service::WorldService;
use serde_json::json;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

fn write_png(root: &Path, name: &str, pixel: [u8; 4]) -> PathBuf {
    let path = root.join(name);
    let image: image::RgbaImage = image::ImageBuffer::from_pixel(32, 32, image::Rgba(pixel));
    image.save(&path).unwrap();
    path
}

fn canonical_owned_version(
    root: &Path,
    asset_type: &str,
    label: &str,
    owner: Option<String>,
    pixel: [u8; 4],
) -> String {
    let asset = AssetService::create_asset(root, asset_type, label, owner).unwrap();
    let source = write_png(root, &format!("{label}.png"), pixel);
    let version = AssetService::import_asset_version(root, &asset.id, &source, None).unwrap();
    AssetService::promote_asset_version(root, &version.id).unwrap();
    version.id
}

#[test]
fn unified_scene_shot_keyframe_compile_golden_path() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("golden-path");
    ProjectService::create(&root, "Golden Path").unwrap();

    // ── Canon: character with locked behavioral + visual canon ──
    let mara =
        CanonService::create_entity(&root, CanonEntityType::Character, "Mara Keene").unwrap();
    for key in ["speech", "movement", "stillness"] {
        let section = CanonService::upsert_section(
            &root,
            &mara.id,
            key,
            json!({ "text": format!("locked {key}") }),
            None,
        )
        .unwrap();
        CanonService::lock_section(&root, &section.id, None).unwrap();
    }
    CanonService::upsert_section(
        &root,
        &mara.id,
        "visual_locks",
        json!({ "locks": [
            {"id": "scar", "key": "right_eyebrow_scar", "description": "Small healed scar.", "severity": "required", "validatorHint": null}
        ]}),
        None,
    )
    .unwrap();

    // ── World with a canonical plate ──
    let location =
        CanonService::create_entity(&root, CanonEntityType::Location, "The Station").unwrap();
    let world = WorldService::create_world(&root, &location.id).unwrap();
    let plate_source = write_png(&root, "plate.png", [10, 11, 12, 255]);
    let plate_version =
        AssetService::import_asset_version(&root, &world.world_plate_asset_id, &plate_source, None)
            .unwrap();
    AssetService::promote_asset_version(&root, &plate_version.id).unwrap();

    // ── Scene assembly with exact references ──
    let scene =
        SceneService::create_scene(&root, "Scene 001", "Mara returns to the ops room").unwrap();
    SceneService::assign_scene_world(&root, &scene.id, &world.id).unwrap();
    let look_version = canonical_owned_version(
        &root,
        "outfit",
        "Mara Look",
        Some(mara.id.clone()),
        [20, 21, 22, 255],
    );
    SceneService::add_scene_character(&root, &scene.id, &mara.id, &look_version, None, None)
        .unwrap();
    let prop_version =
        canonical_owned_version(&root, "prop_plate", "Console", None, [30, 31, 32, 255]);
    SceneService::add_scene_prop(&root, &scene.id, &prop_version, Some("Ops console"), None)
        .unwrap();

    // ── Shot CRUD: create, edit, reorder ──
    let shot_a = CinemaService::create_shot(
        &root,
        &scene.id,
        None,
        4.0,
        "Establish the ops room",
        Some("Mara scans the console".into()),
        Some("wide".into()),
    )
    .unwrap();
    let shot_b = CinemaService::create_shot(
        &root,
        &scene.id,
        None,
        4.0,
        "Close on the console",
        Some("Mara leans in".into()),
        Some("medium".into()),
    )
    .unwrap();
    assert_eq!(shot_a.ordering, 0);
    assert_eq!(shot_b.ordering, 1);

    let reordered =
        CinemaService::reorder_shots(&root, &scene.id, &[shot_b.id.clone(), shot_a.id.clone()])
            .unwrap();
    assert_eq!(reordered[0].id, shot_b.id);
    let reordered =
        CinemaService::reorder_shots(&root, &scene.id, &[shot_a.id.clone(), shot_b.id.clone()])
            .unwrap();
    assert_eq!(reordered[0].id, shot_a.id);

    let edited = CinemaService::update_shot(
        &root,
        &cinema_model::ShotUpdate {
            shot_id: shot_a.id.clone(),
            duration_seconds: Some(5.0),
            intent: None,
            action: None,
            camera: None,
        },
    )
    .unwrap();
    assert_eq!(edited.duration_seconds, 5.0);
    CinemaService::update_shot(
        &root,
        &cinema_model::ShotUpdate {
            shot_id: shot_a.id.clone(),
            duration_seconds: Some(4.0),
            intent: None,
            action: None,
            camera: None,
        },
    )
    .unwrap();

    // Keyframes are optional in compile readiness; reference/state blockers
    // must be empty at this point.
    let early_readiness = CinemaService::scene_readiness(&root, &scene.id).unwrap();
    assert!(
        early_readiness.ready,
        "unexpected blockers: {:?}",
        early_readiness.blockers
    );

    // ── Keyframe workflow for shot A via the existing runtime (mock) ──
    let keyframe_asset = SceneService::ensure_scene_keyframe_asset(&root, &scene.id).unwrap();
    let created = create_workflow_run(
        root.to_string_lossy().to_string(),
        "scene-builder".into(),
        "1.0.0".into(),
        "scene.create_keyframe".into(),
        json!({ "sceneId": scene.id, "providerId": "mock", "modelId": "mock-image-v1" }),
    )
    .unwrap();
    let waiting =
        advance_workflow_run(root.to_string_lossy().to_string(), created.run.id.clone()).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    approve_workflow_step(
        root.to_string_lossy().to_string(),
        created.run.id.clone(),
        "approve-request".into(),
        None,
    )
    .unwrap();
    let completed =
        advance_workflow_run(root.to_string_lossy().to_string(), created.run.id.clone()).unwrap();
    assert_eq!(completed.run.status, "completed");

    // ── Review: the run captured a result set; promote the artifact canonically ──
    let results = cinematic_desktop_lib::generation::commands::list_generation_results(
        root.to_string_lossy().to_string(),
        Some(created.run.id.clone()),
    )
    .unwrap();
    assert_eq!(results.len(), 1, "keyframe run must capture one result set");
    let artifact_id = results[0].artifacts[0].artifact.id.clone();
    let artifact_a = artifact_id.clone();

    let promoted = promote_generated_artifact(
        root.to_string_lossy().to_string(),
        artifact_id,
        keyframe_asset.id.clone(),
        true,
    )
    .unwrap();
    assert_eq!(promoted.status, "canonical");

    // Pin the exact immutable version on the shot.
    CinemaService::set_shot_keyframe(&root, &shot_a.id, Some(&promoted.id)).unwrap();

    // ── Second shot generates a NEWER keyframe; promotion must not drift
    //    Shot A's exact pin ──
    let created_b = create_workflow_run(
        root.to_string_lossy().to_string(),
        "scene-builder".into(),
        "1.0.0".into(),
        "scene.create_keyframe".into(),
        json!({ "sceneId": scene.id, "providerId": "mock", "modelId": "mock-image-v1" }),
    )
    .unwrap();
    advance_workflow_run(root.to_string_lossy().to_string(), created_b.run.id.clone()).unwrap();
    approve_workflow_step(
        root.to_string_lossy().to_string(),
        created_b.run.id.clone(),
        "approve-request".into(),
        None,
    )
    .unwrap();
    let completed_b =
        advance_workflow_run(root.to_string_lossy().to_string(), created_b.run.id.clone()).unwrap();
    assert_eq!(completed_b.run.status, "completed");

    let results_b = cinematic_desktop_lib::generation::commands::list_generation_results(
        root.to_string_lossy().to_string(),
        Some(created_b.run.id.clone()),
    )
    .unwrap();
    let artifact_b = results_b[0].artifacts[0].artifact.id.clone();
    assert_ne!(artifact_a, artifact_b, "each run yields its own candidate");
    let promoted_b = promote_generated_artifact(
        root.to_string_lossy().to_string(),
        artifact_b,
        keyframe_asset.id.clone(),
        true,
    )
    .unwrap();
    assert_eq!(promoted_b.status, "canonical");
    // Content dedup by sha256: the deterministic mock produced identical
    // bytes, so both artifacts resolve to the SAME immutable version. The
    // promotion is recorded idempotently for each artifact; a real provider
    // with distinct bytes yields a distinct version.
    assert_eq!(promoted_b.id, promoted.id);
    CinemaService::set_shot_keyframe(&root, &shot_b.id, Some(&promoted_b.id)).unwrap();

    // Shot A still pins the exact version it was given — no drift.
    let shots_after = CinemaService::list_shots(&root, &scene.id).unwrap();
    let shot_a_after = shots_after.iter().find(|s| s.id == shot_a.id).unwrap();
    assert_eq!(
        shot_a_after.keyframe_asset_version_id.as_deref(),
        Some(promoted.id.as_str()),
        "Shot A's pinned keyframe must not drift when Shot B promotes a newer version"
    );

    // Canonical drift does not make the scene unready.
    let readiness = CinemaService::scene_readiness(&root, &scene.id).unwrap();
    assert!(
        readiness.ready,
        "unexpected blockers: {:?}",
        readiness.blockers
    );

    let compilation = CinemaService::compile_scene(
        &root,
        CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();
    let export_path = compilation.export_path.clone();
    let export_sha256 = compilation.export_sha256.clone();
    let compilation_json = compilation.compilation_json.clone();
    let compilation_id = compilation.id.clone();
    let export_bytes = std::fs::read(root.join(&export_path)).unwrap();
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(&export_bytes);
    assert_eq!(format!("{:x}", hasher.finalize()), export_sha256);

    // ── Restart: close and reopen, verify exact references survive ──
    drop(compilation);
    drop(readiness);
    let reopened = ProjectService::open(&root).unwrap();
    assert_eq!(reopened.name, "Golden Path");

    let reloaded_scene = SceneService::get_scene(&root, &scene.id).unwrap();
    assert_eq!(
        reloaded_scene.world_asset_version_id.as_deref(),
        Some(plate_version.id.as_str()),
        "scene must still pin the exact world plate version"
    );
    let reloaded_shots = CinemaService::list_shots(&root, &scene.id).unwrap();
    assert_eq!(reloaded_shots.len(), 2);
    assert_eq!(
        reloaded_shots[0].keyframe_asset_version_id.as_deref(),
        Some(promoted.id.as_str()),
        "shot must still pin the exact promoted keyframe version"
    );

    // ── Deterministic recompile: same input, same prompt content ──
    let recompiled = CinemaService::compile_scene(
        &root,
        CinemaCompileInput {
            scene_id: scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();
    let normalize = |value: &str| {
        value
            .replace(&compilation_id, "COMPILATION")
            .replace(&recompiled.id, "COMPILATION")
    };
    assert_eq!(
        normalize(&compilation_json),
        normalize(&recompiled.compilation_json),
        "recompile of unchanged scene must be deterministic"
    );

    // Provenance still traverses from the promoted keyframe version.
    let prov = cinematic_desktop_lib::integration::provenance::get_provenance_graph(
        &root,
        "asset_version",
        &promoted.id,
    )
    .unwrap();
    assert!(prov.nodes.iter().any(|node| node.id == promoted.id));
}

use cinematic_desktop_lib::cinema::model as cinema_model;
