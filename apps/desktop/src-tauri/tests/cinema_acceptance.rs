//! P8 end-to-end acceptance (master plan #41): one locked-behavior character
//! with canonical sheet/world assets compiles an 8s two-shot cinema prompt,
//! protected TBDs block until resolved, and exports are durable.

mod support;

use support::compilable_scene;

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::cinema::model::CinemaCompileInput;
use cinematic_desktop_lib::cinema::service::CinemaService;
use cinematic_desktop_lib::error::AppError;
use std::fs;

fn canonical_asset(root: &std::path::Path, asset_type: &str, label: &str) -> String {
    let asset = AssetService::create_asset(root, asset_type, label, None).unwrap();
    let source = support::test_image(root, "acceptance.png", [7, 7, 7, 255]);
    let version = AssetService::import_asset_version(root, &asset.id, &source, None).unwrap();
    AssetService::promote_asset_version(root, &version.id).unwrap();
    version.id
}

#[test]
fn p8_acceptance_one_sheet_one_world_8s_compiles_coherently() {
    let setup = compilable_scene();

    // Canonical face_lock + character_sheet assets exist alongside the outfit
    // look and world plate (the "one sheet, one world" P8 baseline).
    canonical_asset(&setup.root, "face_lock", "Mara Face Lock");
    let sheet_version = canonical_asset(&setup.root, "character_sheet", "Mara Sheet");
    assert!(!sheet_version.is_empty());

    // 5. No protected TBD -> compile succeeds.
    let compilation = CinemaService::compile_scene(
        &setup.root,
        CinemaCompileInput {
            scene_id: setup.scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    // 8. Compilation invariants.
    let prompt: cinematic_desktop_lib::cinema::model::ProviderNeutralCinemaPrompt =
        serde_json::from_str(&compilation.compilation_json).unwrap();
    assert_eq!(prompt.total_duration_seconds, 8.0);
    assert_eq!(prompt.shots.len(), 2);
    let sum: f64 = prompt.shots.iter().map(|shot| shot.duration_seconds).sum();
    assert!((sum - 8.0).abs() < 1e-9);

    assert_eq!(
        prompt.behavioral_locks.speech.as_deref(),
        Some("locked speech")
    );
    assert_eq!(
        prompt.behavioral_locks.movement.as_deref(),
        Some("locked movement")
    );
    assert_eq!(
        prompt.behavioral_locks.stillness.as_deref(),
        Some("locked stillness")
    );

    assert_eq!(
        prompt.world_continuity.plate_asset_version_id.as_deref(),
        setup.scene.world_asset_version_id.as_deref()
    );

    let text = &prompt.provider_prompt;
    assert!(text.contains("Time Budget"));
    assert!(text.contains("Establish the ops room"));
    assert!(text.contains("Close on the console"));
    assert!(text.contains("World Continuity"));
    assert!(text.contains("world plate"));
    assert!(text.contains(&prompt.compilation_id));

    // 9. Protected TBD -> next compile is blocked by the firewall.
    let tbd = cinematic_desktop_lib::canon::tbd::create(
        &setup.root,
        None,
        None,
        "What is behind the red door?",
        None,
        true,
    )
    .unwrap();
    let error = CinemaService::compile_scene(
        &setup.root,
        CinemaCompileInput {
            scene_id: setup.scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap_err();
    assert!(matches!(error, AppError::WorkflowBlockedByProtectedTbd(_)));
    assert!(error.to_string().contains("What is behind the red door?"));

    // 10. Resolve the TBD -> compilation succeeds again, and the resolved
    // topic text never appears in the prompt.
    cinematic_desktop_lib::canon::tbd::resolve(
        &setup.root,
        &tbd.id,
        "The room is intentionally withheld.",
    )
    .unwrap();
    let compilation = CinemaService::compile_scene(
        &setup.root,
        CinemaCompileInput {
            scene_id: setup.scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();
    let prompt: cinematic_desktop_lib::cinema::model::ProviderNeutralCinemaPrompt =
        serde_json::from_str(&compilation.compilation_json).unwrap();
    assert!(!prompt
        .provider_prompt
        .contains("What is behind the red door?"));

    // Export artifact is durable on disk with a matching hash.
    let bytes = fs::read(setup.root.join(&compilation.export_path)).unwrap();
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(&bytes);
    assert_eq!(
        format!("{:x}", hasher.finalize()),
        compilation.export_sha256
    );
}
