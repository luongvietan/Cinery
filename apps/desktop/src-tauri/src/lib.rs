pub mod assets;
pub mod canon;
pub mod db;
pub mod error;
pub mod project;
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
      skills::commands::list_skill_operations,
      workflow::commands::create_workflow_run,
      workflow::commands::advance_workflow_run,
      workflow::commands::approve_workflow_step,
      workflow::commands::reject_workflow_step,
      workflow::commands::cancel_workflow_run,
      workflow::commands::get_workflow_run,
      workflow::commands::list_workflow_runs,
      workflow::commands::list_workflow_characters,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
