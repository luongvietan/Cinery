mod support;

use support::compilable_scene;

use cinematic_desktop_lib::cinema::model::CinemaCompileInput;
use cinematic_desktop_lib::cinema::service::CinemaService;
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn compile_and_export_writes_deterministic_json_and_records_sha() {
    let setup = compilable_scene();

    let compilation = CinemaService::compile_scene(
        &setup.root,
        CinemaCompileInput {
            scene_id: setup.scene.id.clone(),
            total_duration_seconds: 8.0,
            shot_count: None,
        },
    )
    .unwrap();

    // Export file exists at the recorded project-relative path.
    let file_path = setup.root.join(&compilation.export_path);
    assert!(file_path.exists(), "export missing at {}", compilation.export_path);
    assert!(compilation.export_path.starts_with("prompts/cinema/"));

    // Recorded sha256 matches the file contents.
    let bytes = fs::read(&file_path).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha = format!("{:x}", hasher.finalize());
    assert_eq!(compilation.export_sha256, sha);
    assert_eq!(compilation.export_sha256.len(), 64);

    // The compiled JSON round-trips and contains the provider prompt.
    let stored: cinematic_desktop_lib::cinema::model::ProviderNeutralCinemaPrompt =
        serde_json::from_str(&compilation.compilation_json).unwrap();
    assert_eq!(stored.total_duration_seconds, 8.0);
    assert_eq!(stored.shots.len(), 2);
    assert!(stored.provider_prompt.contains("Provider Neutral"));

    // The exported JSON file holds exactly the persisted compilation JSON.
    let exported: cinematic_desktop_lib::cinema::model::ProviderNeutralCinemaPrompt =
        serde_json::from_slice(&bytes).unwrap();
    assert_eq!(exported.compilation_id, stored.compilation_id);
    assert_eq!(exported.provider_prompt, stored.provider_prompt);

    // A human-readable markdown twin is written next to the JSON.
    let md_path = file_path.with_extension("md");
    assert!(md_path.exists());
    let markdown = fs::read_to_string(md_path).unwrap();
    assert!(markdown.contains(&stored.provider_prompt));
}

#[test]
fn second_compile_of_same_inputs_is_prompt_deterministic() {
    let setup = compilable_scene();
    let input = CinemaCompileInput {
        scene_id: setup.scene.id.clone(),
        total_duration_seconds: 8.0,
        shot_count: None,
    };

    let first = CinemaService::compile_scene(&setup.root, input.clone()).unwrap();
    let second = CinemaService::compile_scene(&setup.root, input).unwrap();

    // New compilation id per run...
    assert_ne!(first.id, second.id);

    // ...but identical prompt content apart from the compilation id line.
    let normalize =
        |json: &str| json.replace(&first.id, "COMPILATION").replace(&second.id, "COMPILATION");
    assert_eq!(normalize(&first.compilation_json), normalize(&second.compilation_json));

    // Both compilations are listed for the scene, newest first.
    let listed = CinemaService::list_compilations(&setup.root, &setup.scene.id).unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, second.id);
    assert_eq!(listed[1].id, first.id);
}

#[test]
fn compile_scene_blocks_on_protected_tbd() {
    let setup = compilable_scene();
    cinematic_desktop_lib::canon::tbd::create(
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
    assert!(matches!(
        error,
        cinematic_desktop_lib::error::AppError::WorkflowBlockedByProtectedTbd(_)
    ));

    // After resolving the TBD, compilation succeeds again.
    let tbds = cinematic_desktop_lib::canon::tbd::list(&setup.root).unwrap();
    let protected = tbds.iter().find(|tbd| tbd.protected).unwrap();
    cinematic_desktop_lib::canon::tbd::resolve(
        &setup.root,
        &protected.id,
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
    assert!(!compilation.export_path.is_empty());
}
