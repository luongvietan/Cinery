pub mod assets;
pub mod canon;
pub mod cinema;
pub mod db;
pub mod diagnostics;
pub mod error;
pub mod generation;
pub mod integration;
pub mod project;
pub mod providers;
pub mod qa;
pub mod recovery;
pub mod router;
pub mod scenes;
pub mod skills;
pub mod video_qa;
pub mod workflow;
pub mod worlds;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let skill_registry = skills::registry::SkillRegistry::builtin()
        .expect("builtin skill definitions must be valid");

    tauri::Builder::default()
        .manage(skill_registry)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            project::commands::create_project,
            project::commands::open_project,
            project::commands::list_recent_projects,
            assets::commands::create_asset,
            assets::commands::list_assets,
            assets::commands::get_asset_with_versions,
            assets::commands::import_asset_version,
            assets::commands::promote_asset_version,
            canon::commands::ensure_canon_singletons,
            canon::commands::create_canon_entity,
            canon::commands::list_canon_entities,
            canon::commands::get_canon_entity,
            canon::commands::upsert_canon_section,
            canon::commands::lock_canon_section,
            canon::commands::unlock_canon_section,
            canon::commands::list_canon_section_revisions,
            canon::commands::create_canon_tbd,
            canon::commands::list_canon_tbds,
            canon::commands::resolve_canon_tbd,
            canon::commands::reopen_canon_tbd,
            canon::commands::export_story_bible,
            skills::commands::list_skill_operations,
            workflow::commands::create_workflow_run,
            workflow::commands::advance_workflow_run,
            workflow::commands::approve_workflow_step,
            workflow::commands::reject_workflow_step,
            workflow::commands::cancel_workflow_run,
            workflow::commands::get_workflow_run,
            workflow::commands::list_workflow_runs,
            workflow::commands::list_workflow_characters,
            providers::commands::list_providers,
            providers::commands::list_provider_presets,
            providers::commands::list_custom_providers,
            providers::commands::upsert_custom_provider,
            providers::commands::delete_custom_provider,
            providers::commands::test_custom_provider_connection,
            providers::commands::get_provider_capabilities,
            providers::commands::get_provider_configuration_status,
            providers::commands::save_provider_credential,
            providers::commands::configure_provider,
            providers::commands::remove_provider_credentials,
            providers::commands::validate_provider_configuration,
            providers::commands::list_provider_models,
            providers::commands::suggest_visual_spec,
            providers::commands::cancel_workflow_execution,
            providers::commands::retry_workflow_execution,
            providers::commands::list_provider_jobs,
            router::commands::route_production_intent,
            generation::commands::list_generation_results,
            generation::commands::get_generated_artifact,
            generation::commands::promote_generated_artifact,
            qa::commands::list_qa_runs,
            qa::commands::get_qa_run,
            qa::commands::review_qa_check,
            integration::commands::get_project_overview,
            integration::commands::get_project_health,
            integration::commands::get_provenance_graph,
            cinema::commands::create_shot,
            cinema::commands::update_shot,
            cinema::commands::delete_shot,
            cinema::commands::reorder_shots,
            cinema::commands::set_shot_keyframe,
            cinema::commands::set_shot_video,
            cinema::commands::get_shot_image_to_video_source,
            cinema::commands::promote_shot_video_candidate,
            cinema::commands::list_shot_video_candidates,
            cinema::commands::resolve_canonical_shot_video,
            cinema::commands::reject_shot_video_candidate,
            cinema::commands::restore_shot_video_candidate,
            cinema::commands::get_scene_readiness,
            cinema::commands::list_shots,
            cinema::commands::compile_cinema,
            cinema::commands::get_cinema_compilation,
            cinema::commands::list_cinema_compilations,
            cinema::commands::get_sequence_flow,
            cinema::commands::update_sequence_brief,
            cinema::commands::mark_sequence_references_ready,
            cinema::commands::approve_sequence_preflight,
            cinema::commands::begin_sequence_review,
            cinema::commands::mark_sequence_canonical_take,
            cinema::commands::prepare_sequence_extension,
            recovery::commands::get_project_recovery_state,
            diagnostics::commands::export_diagnostics,
            diagnostics::commands::get_diagnostics_folder,
            diagnostics::commands::append_diagnostics_log,
            worlds::commands::create_world,
            worlds::commands::list_worlds,
            worlds::commands::get_world,
            worlds::commands::list_worlds_detailed,
            worlds::commands::get_world_detailed,
            scenes::commands::create_world_scene,
            scenes::commands::list_world_scenes,
            scenes::commands::get_world_scene,
            scenes::commands::update_scene_details,
            scenes::commands::assign_scene_world,
            scenes::commands::clear_scene_world,
            scenes::commands::add_world_scene_character,
            scenes::commands::remove_world_scene_character,
            scenes::commands::list_scene_characters,
            scenes::commands::add_world_scene_prop,
            scenes::commands::remove_world_scene_prop,
            scenes::commands::list_scene_props,
            scenes::commands::resolve_scene_references,
            scenes::commands::upgrade_scene_world_reference,
            scenes::commands::upgrade_scene_character_look_reference,
            scenes::commands::upgrade_scene_character_sheet_reference,
            scenes::commands::upgrade_scene_prop_reference,
            scenes::commands::get_world_scene_readiness,
            scenes::commands::ensure_scene_keyframe_asset,
            scenes::commands::set_scene_tbd_binding,
            scenes::commands::remove_scene_tbd_binding,
            scenes::commands::list_scene_tbd_bindings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
