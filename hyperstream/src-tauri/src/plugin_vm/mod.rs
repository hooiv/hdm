pub mod lua_host;

pub mod updater;
pub mod manager;

pub fn get_plugins_dir(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    use tauri::Manager;

    app_handle
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")))
        .join("plugins")
}
