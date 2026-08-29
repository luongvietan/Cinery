mod support;

use support::compilable_scene;

use cinematic_desktop_lib::cinema::commands;
use cinematic_desktop_lib::error::AppError;

fn path_string(setup: &support::CompiledScene) -> String {
    setup.root.to_string_lossy().to_string()
}

#[test]
fn tauri_create_scene_and_compile_via_commands() {
    let setup = compilable_scene();
    let root = path_string(&setup);

    // create_scene + list + get detail
    let scene = commands::create_scene(root.clone(), "Scene 002".to_string(), None, None).unwrap();
    assert!(!scene.id.is_empty());
    assert_eq!(commands::list_scenes(root.clone()).unwrap().len(), 2);

    let existing = commands::get_scene(root.clone(), setup.scene.id.clone()).unwrap();
    assert_eq!(existing.scene.id, setup.scene.id);
    assert_eq!(existing.characters.len(), 1);
    assert_eq!(existing.shots.len(), 2);

    // add character/prop + create shot via commands
    let detail = commands::add_scene_character(
        root.clone(),
        scene.id.clone(),
        setup.character_id.clone(),
        setup.characters[0].look_asset_version_id.clone(),
        None,
    )
    .unwrap();
    assert_eq!(detail.characters.len(), 1);
    commands::create_shot(
        root.clone(),
        scene.id.clone(),
        None,
        4.0,
        "Close".to_string(),
        None,
        None,
    )
    .unwrap();
    let shots = commands::list_shots(root.clone(), scene.id.clone()).unwrap();
    assert_eq!(shots.len(), 1);

    // compile via command
    let compilation = commands::compile_cinema(root.clone(), scene.id.clone(), 8.0, None).unwrap();
    assert!(setup.root.join(&compilation.export_path).exists());
    assert!(!compilation.compilation_json.is_empty());

    let fetched = commands::get_cinema_compilation(root.clone(), compilation.id.clone()).unwrap();
    assert_eq!(fetched.id, compilation.id);
    let listed = commands::list_cinema_compilations(root.clone(), scene.id.clone()).unwrap();
    assert_eq!(listed.len(), 1);
}

#[test]
fn command_errors_map_to_app_command_error_codes() {
    let setup = compilable_scene();
    let root = path_string(&setup);

    let error = commands::get_scene(root.clone(), "missing-scene".to_string()).unwrap_err();
    assert_eq!(error.code, "SCENE_NOT_FOUND");

    let error = commands::get_cinema_compilation(root.clone(), "missing-compilation".to_string())
        .unwrap_err();
    assert_eq!(error.code, "CINEMA_COMPILATION_NOT_FOUND");

    let error = commands::create_scene(root.clone(), "   ".to_string(), None, None).unwrap_err();
    assert_eq!(error.code, "INVALID_SCENE_TITLE");

    // A blocked compilation surfaces the TBD firewall code.
    cinematic_desktop_lib::canon::tbd::create(
        &setup.root,
        None,
        None,
        "What is behind the red door?",
        None,
        true,
    )
    .unwrap();
    let error =
        commands::compile_cinema(root.clone(), setup.scene.id.clone(), 8.0, None).unwrap_err();
    assert_eq!(error.code, "WORKFLOW_BLOCKED_BY_PROTECTED_TBD");
}

#[test]
fn commands_reject_invalid_project_paths() {
    let error = commands::list_scenes("".to_string()).unwrap_err();
    assert_eq!(error.code, "INVALID_PROJECT_PATH");
    assert!(matches!(
        AppError::InvalidProjectPath.code().as_str(),
        "INVALID_PROJECT_PATH"
    ));
}
