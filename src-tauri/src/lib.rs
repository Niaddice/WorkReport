mod ai;
mod workspace;

use std::{fs, sync::Mutex};
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;
use workspace::{Document, Entry, Format, Preview, Workspace, WorkspaceInfo};

#[derive(Default)]
struct AppState(Mutex<Option<Workspace>>);

fn with_workspace<T>(
    state: &State<'_, AppState>,
    action: impl FnOnce(&mut Workspace) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "工作空间忙，请重试".to_string())?;
    action(guard.as_mut().ok_or("请先选择工作空间")?)
}

#[tauri::command]
fn recent_workspace(app: tauri::AppHandle) -> Option<String> {
    let path = app.path().app_config_dir().ok()?.join("workspace.json");
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

#[tauri::command]
fn open_workspace(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<WorkspaceInfo, String> {
    let mut workspace = Workspace::open(&path)?;
    // Validate the index before replacing the currently active workspace.
    workspace.list()?;
    let info = workspace.info();
    let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&config).map_err(|e| e.to_string())?;
    fs::write(
        config.join("workspace.json"),
        serde_json::to_string(&info.path).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    *state.0.lock().map_err(|_| "工作空间忙，请重试")? = Some(workspace);
    Ok(info)
}

#[tauri::command]
fn list_entries(state: State<'_, AppState>) -> Result<Vec<Entry>, String> {
    with_workspace(&state, Workspace::list)
}

#[tauri::command]
fn list_previews(state: State<'_, AppState>, dates: Vec<String>) -> Result<Vec<Preview>, String> {
    with_workspace(&state, |workspace| workspace.previews(&dates))
}

#[tauri::command]
fn open_workspace_directory(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = with_workspace(&state, |workspace| Ok(workspace.info().path))?;
    if !std::path::Path::new(&path).is_dir() {
        return Err("工作空间目录已不存在，请在设置中重新选择".into());
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|error| format!("无法打开工作空间目录：{error}"))
}

#[tauri::command]
fn read_entry(
    state: State<'_, AppState>,
    date: String,
    format: Format,
) -> Result<Document, String> {
    with_workspace(&state, |workspace| workspace.read(&date, format))
}

#[tauri::command]
fn create_entry(
    state: State<'_, AppState>,
    date: String,
    format: Format,
    content: String,
) -> Result<Document, String> {
    with_workspace(&state, |workspace| {
        workspace.create_with_content(&date, format, &content)
    })
}

#[tauri::command]
fn save_entry(
    state: State<'_, AppState>,
    date: String,
    content: String,
    format: Format,
    expected_revision: String,
) -> Result<Document, String> {
    with_workspace(&state, |workspace| {
        workspace.save(&date, format, &content, &expected_revision)
    })
}

#[tauri::command]
fn switch_entry_format(
    state: State<'_, AppState>,
    date: String,
    from: Format,
    to: Format,
    expected_revision: String,
) -> Result<Document, String> {
    with_workspace(&state, |workspace| {
        workspace.switch_format(&date, from, to, &expected_revision)
    })
}
#[tauri::command]
fn copy_content(app: tauri::AppHandle, content: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .write_text(content)
        .map_err(|_| "无法写入剪贴板".into())
}
#[tauri::command]
fn load_ai_settings(app: tauri::AppHandle) -> Result<ai::Settings, String> {
    ai::load(&app.path().app_config_dir().map_err(|e| e.to_string())?)
}
#[tauri::command]
fn save_ai_settings(app: tauri::AppHandle, settings: ai::Settings) -> Result<(), String> {
    ai::save(
        &app.path().app_config_dir().map_err(|e| e.to_string())?,
        &settings,
    )
}
#[tauri::command]
async fn detect_ai_clis() -> Vec<ai::DetectedCli> {
    ai::detect().await
}
#[tauri::command]
async fn list_ai_models(base_url: String, api_key: String) -> Result<Vec<ai::ModelInfo>, String> {
    ai::list_models(&base_url, &api_key).await
}
#[tauri::command]
async fn list_cli_models(
    kind: ai::ProviderKind,
    executable: String,
) -> Result<Vec<ai::ModelInfo>, String> {
    ai::list_cli_models(&kind, &executable).await
}
#[tauri::command]
fn cancel_ai(state: State<'_, ai::Runtime>, request_id: String) {
    state.cancel(&request_id);
}
#[tauri::command]
async fn polish_entry(
    app: tauri::AppHandle,
    state: State<'_, ai::Runtime>,
    content: String,
    format: Format,
    request_id: String,
) -> Result<String, String> {
    let settings = load_ai_settings(app)?;
    let provider = settings
        .providers
        .iter()
        .find(|provider| Some(&provider.id) == settings.default_provider_id.as_ref())
        .ok_or("请先在设置中选择默认 AI")?
        .clone();
    let token = state.begin(request_id.clone())?;
    let result = ai::polish(
        &provider,
        &content,
        format.extension(),
        &settings.generation_preference,
        token,
    )
    .await;
    state.finish(&request_id);
    result
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(ai::Runtime::default())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            recent_workspace,
            open_workspace,
            open_workspace_directory,
            list_entries,
            list_previews,
            read_entry,
            create_entry,
            save_entry,
            switch_entry_format,
            copy_content,
            load_ai_settings,
            save_ai_settings,
            detect_ai_clis,
            list_ai_models,
            list_cli_models,
            cancel_ai,
            polish_entry
        ])
        .run(tauri::generate_context!())
        .expect("无法启动 WorkReport");
}
