pub mod assets;
pub mod canon;
pub mod cinema;
pub mod db;
pub mod error;
pub mod generation;
pub mod project;
pub mod providers;
pub mod qa;
pub mod skills;
pub mod workflow;

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
      providers::commands::get_provider_capabilities,
      providers::commands::get_provider_configuration_status,
      providers::commands::configure_provider,
      providers::commands::remove_provider_credentials,
      providers::commands::validate_provider_configuration,
      providers::commands::list_provider_models,
      providers::commands::cancel_workflow_execution,
      providers::commands::retry_workflow_execution,
      generation::commands::list_generation_results,
      generation::commands::get_generated_artifact,
      generation::commands::promote_generated_artifact,
      qa::commands::list_qa_runs,
      qa::commands::get_qa_run,
      qa::commands::review_qa_check,
      cinema::commands::create_scene,
      cinema::commands::list_scenes,
      cinema::commands::get_scene,
      cinema::commands::add_scene_character,
      cinema::commands::add_scene_prop,
      cinema::commands::create_shot,
      cinema::commands::list_shots,
      cinema::commands::compile_cinema,
      cinema::commands::get_cinema_compilation,
      cinema::commands::list_cinema_compilations,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
