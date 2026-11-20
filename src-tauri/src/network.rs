use workreport_core::network as core;

#[cfg(test)]
pub use core::Mode;
pub use core::{client, current, initialize, Policy, Settings};

#[tauri::command]
pub fn load_proxy_settings(app: tauri::AppHandle) -> Result<Settings, String> {
    core::load(&crate::legacy_config_dir(&app)?)
}

#[tauri::command]
pub fn save_proxy_settings(app: tauri::AppHandle, settings: Settings) -> Result<Settings, String> {
    core::save(&crate::legacy_config_dir(&app)?, &settings)
}

#[tauri::command]
pub async fn test_proxy_settings(settings: Settings) -> Result<String, String> {
    core::test_proxy_settings(settings).await
}
