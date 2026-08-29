//! MVP command-boundary acceptance: the complete canonical production
//! journey driven exclusively through public Tauri command functions and
//! DTOs. No service/repository shortcut may advance product state.
//!
//! GUARD: acceptance-flow mutations MUST call `*_commands::*` functions
//! only. Fixture setup (source images on disk, no product state) may use
//! support helpers. If a needed command is missing, add the command â€” do
//! not call services directly here.

mod support;

use cinematic_desktop_lib::assets::commands::{
    create_asset, import_asset_version, list_assets, promote_asset_version,
};
use cinematic_desktop_lib::canon::commands::{
    create_canon_entity, ensure_canon_singletons, lock_canon_section, upsert_canon_section,
};
use cinematic_desktop_lib::cinema::commands::{
    add_scene_character, add_scene_prop, compile_cinema, create_scene, create_shot,
    get_cinema_compilation, get_scene, get_scene_readiness, list_scenes, rename_scene,
    set_scene_world, set_shot_keyframe, update_shot,
};
use cinematic_desktop_lib::generation::commands::{
    list_generation_results, promote_generated_artifact,
};
use cinematic_desktop_lib::project::commands::{create_project_standalone, open_project_standalone};
use cinematic_desktop_lib::workflow::commands::{
    advance_workflow_run, approve_workflow_step, create_workflow_run, get_workflow_run,
};
use serde_json::json;
use support::command_harness::CommandHarness;

const SKILL_VERSION: &str = "1.1.0";

fn run_to_waiting(harness: &CommandHarness, operation_id: &str, input: serde_json::Value) -> String {
    let created = create_workflow_run(
        harness.root.to_string_lossy().to_string(),
        "character-builder".into(),
        SKILL_VERSION.into(),
        operation_id.into(),
        input,
    )
    .unwrap();
    let waiting = advance_workflow_run(harness.root.to_string_lossy().to_string(), created.run.id.clone()).unwrap();
    assert_eq!(waiting.run.status, "waiting_for_approval");
    approve_workflow_step(
        harness.root.to_string_lossy().to_string(),
        created.run.id.clone(),
        "approve-request".into(),
        None,
    )
    .unwrap();
    let completed = advance_workflow_run(harness.root.to_string_lossy().to_string(), created.run.id.clone()).unwrap();
    assert_eq!(completed.run.status, "completed");
    created.run.id
}

#[test]
fn mvp_full_journey_through_command_boundaries() {
    let harness = CommandHarness::new("mvp-command-acceptance");

    // 1. Create a project and reopen it through project commands.
    let project = create_project_standalone(
        harness.root.to_string_lossy().to_string(),
        "MVP Command Acceptance".into(),
    )
    .unwrap();
    let opened = open_project_standalone(harness.root.to_string_lossy().to_string()).unwrap();
    assert_eq!(opened.id, project.id);
    let root = harness.root.to_string_lossy().to_string();

    // 2. Story + Character Canon: the story is a project singleton (created
    //    via `ensure_canon_singletons`); create the character entity, save
    //    required sections, add permanent visual locks, lock authoritative
    //    sections.
    let singletons = ensure_canon_singletons(root.clone()).unwrap();
    let story_section_target = singletons.story.id.clone();
    let character = create_canon_entity(root.clone(), "character".into(), "Mara".into()).unwrap();

    let summary = upsert_canon_section(root.clone(), character.id.clone(), "visual_summary".into(), json!({"text": "Angular face and dark hair."}), None).unwrap();
    let locks = upsert_canon_section(
        root.clone(),
        character.id.clone(),
        "visual_locks".into(),
        json!({"locks": [
            {"id": "scar", "key": "right_eyebrow_scar", "description": "Small healed scar.", "severity": "required", "validatorHint": null}
        ]}),
        None,
    )
    .unwrap();
    let _ = upsert_canon_section(root.clone(), story_section_target.clone(), "premise".into(), json!({"text": "A courier outruns a siege."}), None).unwrap();
    lock_canon_section(root.clone(), summary.id.clone(), None).unwrap();
    lock_canon_section(root.clone(), locks.id.clone(), None).unwrap();

    // Behavioral canon: locked speech/movement/stillness are prerequisites
    // for cinema compilation.
    for key in ["speech", "movement", "stillness"] {
        let section = upsert_canon_section(
            root.clone(),
            character.id.clone(),
            key.into(),
            json!({"text": format!("locked {key}")}),
            None,
        )
        .unwrap();
        lock_canon_section(root.clone(), section.id.clone(), None).unwrap();
    }

    // 3. Face Lock with mock provider: launch, approve, execute, list
    //    result sets, create a Face asset, promote the new version.
    let face_run = run_to_waiting(
        &harness,
        "character.create_face_lock",
        json!({
            "projectRootPath": root,
            "characterEntityId": character.id,
            "visualSpec": {"head":"oval","eyes":"brown","brows":"straight","nose":"narrow","lips":"neutral","skin":"olive","hair":"black","build":"athletic","expression":"neutral"},
            "baselineWardrobe": "charcoal crew neck",
            "providerId": "mock",
            "modelId": "mock-image-v1"
        }),
    );
    let face_results = list_generation_results(root.clone(), Some(face_run.clone())).unwrap();
    assert!(!face_results.is_empty(), "face run must expose its result set");
    let face_artifact = &face_results[0].artifacts[0];

    let face_asset = create_asset(
        root.clone(),
        "face_lock".into(),
        "Mara Face".into(),
        Some(character.id.clone()),
    )
    .unwrap();
    let face_version = promote_generated_artifact(root.clone(), face_artifact.artifact.id.clone(), face_asset.id.clone(), true).unwrap();

    // 4. Outfit from the canonical Face: launch, promote its generated
    //    version, keep the canonical Outfit.
    let outfit_run = run_to_waiting(
        &harness,
        "character.create_outfit",
        json!({
            "projectRootPath": root,
            "characterEntityId": character.id,
            "wardrobeProposal": {"description": "charcoal long-sleeve top, utility trousers, boots"},
            "providerId": "mock",
            "modelId": "mock-image-v1"
        }),
    );
    let outfit_results = list_generation_results(root.clone(), Some(outfit_run.clone())).unwrap();
    let outfit_artifact = &outfit_results[0].artifacts[0];
    let outfit_asset = create_asset(root.clone(), "outfit".into(), "Mara Outfit".into(), Some(character.id.clone())).unwrap();
    let outfit_version = promote_generated_artifact(root.clone(), outfit_artifact.artifact.id.clone(), outfit_asset.id.clone(), true).unwrap();

    // 5. Character Sheet from the canonical Outfit.
    let sheet_run = run_to_waiting(
        &harness,
        "character.create_character_sheet",
        json!({
            "projectRootPath": root,
            "characterEntityId": character.id,
            "providerId": "mock",
            "modelId": "mock-image-v1"
        }),
    );
    let sheet_results = list_generation_results(root.clone(), Some(sheet_run.clone())).unwrap();
    let sheet_artifact = &sheet_results[0].artifacts[0];
    let sheet_asset = create_asset(root.clone(), "character_sheet".into(), "Mara Sheet".into(), Some(character.id.clone())).unwrap();
    let sheet_version = promote_generated_artifact(root.clone(), sheet_artifact.artifact.id.clone(), sheet_asset.id.clone(), true).unwrap();

    // 6. Import and promote World Plate, Prop Plate, Shot Keyframe through
    //    asset commands (import fixture + promote through commands).
    let world_asset = create_asset(root.clone(), "world_plate".into(), "Station".into(), None).unwrap();
    let world_source = harness.image("world.png", [10, 20, 30, 255]);
    let world_version = import_asset_version(root.clone(), world_asset.id.clone(), world_source.to_string_lossy().to_string(), None).unwrap();
    promote_asset_version(root.clone(), world_version.id.clone()).unwrap();

    let prop_asset = create_asset(root.clone(), "prop_plate".into(), "Console".into(), None).unwrap();
    let prop_source = harness.image("prop.png", [200, 100, 50, 255]);
    let prop_version = import_asset_version(root.clone(), prop_asset.id.clone(), prop_source.to_string_lossy().to_string(), None).unwrap();
    promote_asset_version(root.clone(), prop_version.id.clone()).unwrap();

    let keyframe_asset = create_asset(root.clone(), "shot_keyframe".into(), "KF 1".into(), None).unwrap();
    let keyframe_source = harness.image("keyframe.png", [90, 90, 90, 255]);
    let keyframe_version = import_asset_version(root.clone(), keyframe_asset.id.clone(), keyframe_source.to_string_lossy().to_string(), None).unwrap();
    promote_asset_version(root.clone(), keyframe_version.id.clone()).unwrap();

    // 7. Scene assembly: create, rename, choose world, cast with exact
    //    look/sheet, attach prop, create/edit shot, attach keyframe.
    let scene = create_scene(root.clone(), "Scene 001".into(), None, None).unwrap();
    rename_scene(root.clone(), scene.id.clone(), "Scene 001 - Ops".into()).unwrap();
    set_scene_world(root.clone(), scene.id.clone(), Some(world_version.id.clone())).unwrap();
    add_scene_character(
        root.clone(),
        scene.id.clone(),
        character.id.clone(),
        outfit_version.id.clone(),
        Some(sheet_version.id.clone()),
    )
    .unwrap();
    add_scene_prop(root.clone(), scene.id.clone(), prop_version.id.clone()).unwrap();

    let shot = create_shot(root.clone(), scene.id.clone(), None, 4.0, "Establish the ops room".into(), None, None).unwrap();
    update_shot(root.clone(), shot.id.clone(), Some(5.0), Some("Close on console".into()), Some("Mara leans in".into()), Some("medium".into())).unwrap();
    set_shot_keyframe(root.clone(), shot.id.clone(), Some(keyframe_version.id.clone())).unwrap();

    // Readiness must be structured, not an exception.
    let readiness = get_scene_readiness(root.clone(), scene.id.clone()).unwrap();
    assert!(readiness.ready, "scene must be ready before compile: {:?}", readiness.blockers);

    // 8. Compile the cinema prompt and verify runtime, export, hash.
    let compilation = compile_cinema(root.clone(), scene.id.clone(), 5.0, None).unwrap();
    assert_eq!(compilation.export_sha256.len(), 64);
    let persisted = get_cinema_compilation(root.clone(), compilation.id.clone()).unwrap();
    assert_eq!(persisted.id, compilation.id);
    assert_eq!(persisted.export_path, compilation.export_path);
    let compiled_prompt: serde_json::Value = serde_json::from_str(&compilation.compilation_json).unwrap();
    let prompt_text = compiled_prompt["providerPrompt"].as_str().unwrap_or_default();
    assert!(prompt_text.contains("right_eyebrow_scar") || prompt_text.contains("scar"), "behavioral/visual locks must appear in the compiled prompt");

    // 9. Provenance traversal: compilation -> scene, results -> lineage.
    let all_results = list_generation_results(root.clone(), None).unwrap();
    assert!(all_results.len() >= 3, "face/outfit/sheet result sets must persist");
    let face_detail = get_workflow_run(root.clone(), face_run.clone()).unwrap();
    assert_eq!(face_detail.run.status, "completed");
    assert!(face_detail.provider_executions.iter().any(|e| e.provider_id == "mock" && e.model_id == "mock-image-v1"));

    // 10. Close and reopen the project, then verify exact references.
    drop(opened);
    open_project_standalone(root.clone()).unwrap();

    let reopened_scene = get_scene(root.clone(), scene.id.clone()).unwrap();
    assert_eq!(reopened_scene.scene.world_asset_version_id.as_deref(), Some(world_version.id.as_str()));
    assert_eq!(reopened_scene.characters[0].look_asset_version_id, outfit_version.id);
    assert_eq!(reopened_scene.characters[0].sheet_asset_version_id.as_deref(), Some(sheet_version.id.as_str()));
    assert_eq!(reopened_scene.props[0].prop_asset_version_id, prop_version.id);
    assert_eq!(reopened_scene.shots[0].keyframe_asset_version_id.as_deref(), Some(keyframe_version.id.as_str()));

    let reopened_results = list_generation_results(root.clone(), Some(face_run.clone())).unwrap();
    assert_eq!(reopened_results.len(), face_results.len());
    assert_eq!(reopened_results[0].artifacts[0].artifact.id, face_artifact.artifact.id);

    let reopened_compilation = get_cinema_compilation(root.clone(), compilation.id.clone()).unwrap();
    assert_eq!(reopened_compilation.export_sha256, compilation.export_sha256);

    let reopened_readiness = get_scene_readiness(root.clone(), scene.id.clone()).unwrap();
    assert!(reopened_readiness.ready);

    let reopened_assets = list_assets(root.clone()).unwrap();
    let reopened_face = reopened_assets.iter().find(|a| a.id == face_asset.id).unwrap();
    assert_eq!(reopened_face.canonical_version_id.as_deref(), Some(face_version.id.as_str()));
}

#[test]
fn mvp_character_operations_reject_prerequisites_through_command_boundary() {
    let harness = CommandHarness::new("mvp-command-prereq");
    let project = create_project_standalone(
        harness.root.to_string_lossy().to_string(),
        "MVP Prereq".into(),
    )
    .unwrap();
    let root = harness.root.to_string_lossy().to_string();
    let _ = project;
    let character = create_canon_entity(root.clone(), "character".into(), "Solo".into()).unwrap();

    // Outfit cannot launch without a canonical Face.
    let error = create_workflow_run(
        root.clone(),
        "character-builder".into(),
        SKILL_VERSION.into(),
        "character.create_outfit".into(),
        json!({
            "projectRootPath": root,
            "characterEntityId": character.id,
            "wardrobeProposal": {"description": "jacket"},
            "providerId": "mock",
            "modelId": "mock-image-v1"
        }),
    )
    .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_PREREQUISITE_FAILED");

    // Sheet cannot launch without a canonical Outfit.
    let error = create_workflow_run(
        root,
        "character-builder".into(),
        SKILL_VERSION.into(),
        "character.create_character_sheet".into(),
        json!({
            "projectRootPath": harness.root.to_string_lossy().to_string(),
            "characterEntityId": character.id,
            "providerId": "mock",
            "modelId": "mock-image-v1"
        }),
    )
    .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_PREREQUISITE_FAILED");

    let scenes = list_scenes(harness.root.to_string_lossy().to_string()).unwrap();
    assert!(scenes.is_empty());
}
