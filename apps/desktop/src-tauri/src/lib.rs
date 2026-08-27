mod assets;
mod db;
mod error;
mod project;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .invoke_handler(tauri::generate_handler![
      project::commands::create_project,
      project::commands::open_project,
      project::commands::list_recent_projects,
      assets::commands::create_asset,
      assets::commands::list_assets,
      assets::commands::get_asset_with_versions,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
