mod ai;
mod appearance;
mod database;
mod excel;
mod holidays;
mod settings;
mod summary;
mod workspace;

use std::{fs, sync::{Arc, Mutex}};
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use workspace::{Document, Entry, Format, Preview, Workspace, WorkspaceInfo};

#[derive(Default)]
struct AppState(Arc<Mutex<Option<Workspace>>>);

fn with_workspace<T>(
    state: &State<'_, AppState>,
    action: impl FnOnce(&mut Workspace) -> Result<T, String>,
) -> Result<T, String> {
    with_locked_workspace(&state.0, action)
}

fn with_locked_workspace<T>(
    state: &Mutex<Option<Workspace>>,
    action: impl FnOnce(&mut Workspace) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "工作空间忙，请重试".to_string())?;
    action(guard.as_mut().ok_or("请先选择工作空间")?)
}

/// Database operations run away from the webview/UI thread.
async fn run_workspace<T: Send + 'static>(
    state: Arc<Mutex<Option<Workspace>>>,
    action: impl FnOnce(&mut Workspace) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || with_locked_workspace(&state, action))
        .await.map_err(|_| "存储操作未能完成，请重试".to_string())?
}

async fn run_optional_workspace<T: Send + 'static>(
    state: Arc<Mutex<Option<Workspace>>>,
    action: impl FnOnce(Option<&Workspace>) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let guard = state.lock().map_err(|_| "工作空间忙，请重试".to_string())?;
        action(guard.as_ref())
    }).await.map_err(|_| "读取工作空间设置失败".to_string())?
}

fn workspace_root(state: &State<'_, AppState>) -> Result<std::path::PathBuf, String> {
    with_workspace(state, |workspace| Ok(workspace.root().to_path_buf()))
}

fn settings_path(state: &State<'_, AppState>) -> Result<std::path::PathBuf, String> {
    Ok(workspace_root(state)?.join(settings::FILE_NAME))
}

fn legacy_config_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|error| error.to_string())
}

/// 设置保存在工作空间根目录；未打开工作空间或文件尚未创建时，
/// 回退读取旧版应用配置目录，保证升级后原有配置仍然可用。
fn read_ai_settings(
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
) -> Result<ai::Settings, String> {
    if let Ok(path) = settings_path(state) {
        if let Some(stored) = settings::load(&path)? {
            return Ok(stored.ai);
        }
    }
    ai::load(&legacy_config_dir(app)?)
}

/// 首次保存时以旧版配置为底稿，避免只保存其中一段就把另一段清空。
fn stored_settings_at(
    app: &tauri::AppHandle,
    root: &std::path::Path,
) -> Result<settings::StoredSettings, String> {
    let path = root.join(settings::FILE_NAME);
    if let Some(stored) = settings::load(&path)? {
        return Ok(stored);
    }
    let legacy = legacy_config_dir(app)?;
    Ok(settings::StoredSettings {
        appearance: appearance::load(&legacy)?,
        ai: ai::load(&legacy)?,
        excel: excel::Settings::default(),
        summary: summary::Settings::default(),
        database: database::Settings::default(),
    })
}

/// Keep the workspace selection and settings read/modify/write under the same
/// lock, so concurrent section saves cannot overwrite one another's changes.
fn update_settings(
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
    update: impl FnOnce(&mut settings::StoredSettings) -> Result<(), String>,
) -> Result<(), String> {
    with_workspace(state, |workspace| {
        let mut stored = stored_settings_at(app, workspace.root())?;
        update(&mut stored)?;
        settings::save(&workspace.root().join(settings::FILE_NAME), &stored)
    })
}

#[tauri::command]
fn recent_workspace(app: tauri::AppHandle) -> Option<String> {
    let path = app.path().app_config_dir().ok()?.join("workspace.json");
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

#[tauri::command]
async fn load_holiday_data(
    app: tauri::AppHandle,
    runtime: State<'_, holidays::Runtime>,
) -> Result<holidays::HolidayState, String> {
    runtime.load(&legacy_config_dir(&app)?).await
}

#[tauri::command]
async fn sync_holiday_year(
    app: tauri::AppHandle,
    runtime: State<'_, holidays::Runtime>,
    year: i32,
) -> Result<holidays::HolidayState, String> {
    runtime.sync(&legacy_config_dir(&app)?, year).await
}

#[tauri::command]
async fn open_workspace(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<WorkspaceInfo, String> {
    let shared = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
    let mut workspace = Workspace::open(&path)?;
    // Validate the index before replacing the currently active workspace.
    workspace.validate_open()?;
    let info = workspace.info();
    let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&config).map_err(|e| e.to_string())?;
    fs::write(
        config.join("workspace.json"),
        serde_json::to_string(&info.path).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    *shared.lock().map_err(|_| "工作空间忙，请重试")? = Some(workspace);
    Ok(info)
    }).await.map_err(|_| "无法打开工作空间".to_string())?
}

#[tauri::command]
async fn list_entries(state: State<'_, AppState>) -> Result<Vec<Entry>, String> {
    run_workspace(state.0.clone(), Workspace::list).await
}

#[tauri::command]
async fn list_previews(state: State<'_, AppState>, dates: Vec<String>) -> Result<Vec<Preview>, String> {
    run_workspace(state.0.clone(), move |workspace| workspace.previews(&dates)).await
}

#[tauri::command]
async fn open_workspace_directory(
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
async fn read_entry(
    state: State<'_, AppState>,
    date: String,
    format: Format,
) -> Result<Document, String> {
    run_workspace(state.0.clone(), move |workspace| workspace.read(&date, format)).await
}

#[tauri::command]
async fn create_entry(
    state: State<'_, AppState>,
    date: String,
    format: Format,
    content: String,
) -> Result<Document, String> {
    run_workspace(state.0.clone(), move |workspace| {
        workspace.create_with_content(&date, format, &content)
    }).await
}

#[tauri::command]
async fn save_entry(
    state: State<'_, AppState>,
    date: String,
    content: String,
    format: Format,
    expected_revision: String,
) -> Result<Document, String> {
    run_workspace(state.0.clone(), move |workspace| {
        workspace.save(&date, format, &content, &expected_revision)
    }).await
}

#[tauri::command]
async fn switch_entry_format(
    state: State<'_, AppState>,
    date: String,
    from: Format,
    to: Format,
    expected_revision: String,
) -> Result<Document, String> {
    run_workspace(state.0.clone(), move |workspace| {
        workspace.switch_format(&date, from, to, &expected_revision)
    }).await
}
#[tauri::command]
fn copy_content(app: tauri::AppHandle, content: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .write_text(content)
        .map_err(|_| "无法写入剪贴板".into())
}
#[tauri::command]
async fn load_ai_settings(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ai::Settings, String> {
    run_optional_workspace(state.0.clone(), move |workspace| {
        if let Some(workspace) = workspace {
            if let Some(stored) = settings::load(&workspace.root().join(settings::FILE_NAME))? { return Ok(stored.ai); }
        }
        ai::load(&legacy_config_dir(&app)?)
    }).await
}

#[tauri::command]
async fn save_ai_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: ai::Settings,
) -> Result<(), String> {
    ai::validate(&settings)?;
    update_settings(&app, &state, |stored| {
        stored.ai = settings;
        Ok(())
    })
}
#[tauri::command]
async fn load_appearance_settings(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<appearance::Settings, String> {
    run_optional_workspace(state.0.clone(), move |workspace| {
        if let Some(workspace) = workspace {
            if let Some(stored) = settings::load(&workspace.root().join(settings::FILE_NAME))? { return Ok(stored.appearance); }
        }
        appearance::load(&legacy_config_dir(&app)?)
    }).await
}

#[tauri::command]
async fn save_appearance_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: appearance::Settings,
) -> Result<(), String> {
    appearance::validate(&settings)?;
    update_settings(&app, &state, |stored| {
        stored.appearance = settings;
        Ok(())
    })
}
#[tauri::command]
async fn load_summary_settings(state: State<'_, AppState>) -> Result<summary::Settings, String> {
    run_optional_workspace(state.0.clone(), |workspace| {
        let Some(workspace) = workspace else { return Ok(summary::Settings::default()); };
        let stored = settings::load(&workspace.root().join(settings::FILE_NAME))?.unwrap_or_default();
        summary::normalize_settings(workspace.root(), &stored.summary)
    }).await
}

#[tauri::command]
async fn save_summary_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: summary::Settings,
) -> Result<(), String> {
    with_workspace(&state, |workspace| {
        let normalized = summary::normalize_settings(workspace.root(), &settings)?;
        let mut stored = stored_settings_at(&app, workspace.root())?;
        stored.summary = normalized;
        settings::save(&workspace.root().join(settings::FILE_NAME), &stored)
    })
}

#[tauri::command]
async fn open_summary_output_directory(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    workspace_path: String,
) -> Result<(), String> {
    let directory = with_workspace(&state, |workspace| {
        summary::validate_workspace(workspace.root(), &workspace_path)?;
        let stored =
            settings::load(&workspace.root().join(settings::FILE_NAME))?.unwrap_or_default();
        summary::prepare_directory(workspace.root(), &stored.summary)
    })?;
    app.opener()
        .open_path(directory.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|error| format!("无法打开总结输出目录：{error}"))
}

#[tauri::command]
async fn export_summary(
    state: State<'_, AppState>,
    workspace_path: String,
    dates: Vec<String>,
    content: String,
    format: Format,
) -> Result<summary::ExportResult, String> {
    with_workspace(&state, |workspace| {
        summary::validate_workspace(workspace.root(), &workspace_path)?;
        let stored =
            settings::load(&workspace.root().join(settings::FILE_NAME))?.unwrap_or_default();
        summary::export(workspace.root(), &stored.summary, &dates, &content, format)
    })
}

#[tauri::command]
async fn load_excel_settings(state: State<'_, AppState>) -> Result<excel::Settings, String> {
    run_optional_workspace(state.0.clone(), |workspace| {
        let Some(workspace) = workspace else { return Ok(excel::Settings::default()); };
        Ok(settings::load(&workspace.root().join(settings::FILE_NAME))?.unwrap_or_default().excel)
    }).await
}

fn validate_excel_settings(
    settings: &excel::Settings,
    ai_settings: &ai::Settings,
) -> Result<(), String> {
    if settings.enabled {
        ai::default_provider(ai_settings)
            .map_err(|error| format!("启用 Excel 输出需要先配置默认 AI：{error}"))?;
    }
    excel::validate_settings(settings)?;
    if settings.enabled {
        let template = std::path::Path::new(settings.template_path.trim());
        if !template.is_absolute() || !template.is_file() {
            return Err("Excel 模板不存在，请重新选择模板文件".into());
        }
        let directory = std::path::Path::new(settings.output_directory.trim());
        if !directory.is_absolute() || !directory.is_dir() {
            return Err("Excel 输出目录不存在，请重新选择目录".into());
        }
    }
    Ok(())
}

#[tauri::command]
async fn save_excel_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: excel::Settings,
) -> Result<(), String> {
    update_settings(&app, &state, |stored| {
        validate_excel_settings(&settings, &stored.ai)?;
        stored.excel = settings;
        Ok(())
    })
}

fn save_workspace_settings_at(
    workspace: &mut Workspace,
    workspace_path: &str,
    mut stored: settings::StoredSettings,
) -> Result<settings::StoredSettings, String> {
    summary::validate_workspace(workspace.root(), workspace_path)
        .map_err(|_| "工作空间已切换，请重新打开设置后重试".to_string())?;
    appearance::validate(&stored.appearance)?;
    ai::validate(&stored.ai)?;
    // Validate the dependency against this same save, so enabling Excel and
    // configuring its AI provider succeeds or fails as one transaction.
    validate_excel_settings(&stored.excel, &stored.ai)?;
    stored.summary = summary::normalize_settings(workspace.root(), &stored.summary)?;
    let path = workspace.root().join(settings::FILE_NAME);
    // Match section saves: a damaged existing file must not be silently replaced.
    if let Some(current) = settings::load(&path)? {
        if current.database != *workspace.database_settings() {
            return Err("数据库配置已被外部修改，请重新打开工作空间后重试".into());
        }
    }
    let (database_settings, connection) = workspace.prepare_database_settings(&stored.database)?;
    stored.database = database_settings;
    settings::save(&path, &stored)?;
    workspace.accept_database(stored.database.clone(), connection);
    Ok(stored)
}

#[tauri::command]
async fn save_workspace_settings(
    state: State<'_, AppState>,
    workspace_path: String,
    settings: settings::StoredSettings,
) -> Result<settings::StoredSettings, String> {
    run_workspace(state.0.clone(), move |workspace| {
        save_workspace_settings_at(workspace, &workspace_path, settings)
    }).await
}

#[tauri::command]
async fn load_database_settings(state: State<'_, AppState>) -> Result<database::Settings, String> {
    run_optional_workspace(state.0.clone(), |workspace| {
        Ok(workspace.map(|workspace| workspace.database_settings().clone()).unwrap_or_default())
    }).await
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseConnectionResult { message: String, record_count: usize }

#[tauri::command]
async fn test_database_connection(
    state: State<'_, AppState>, workspace_path: String, mut settings: database::Settings,
) -> Result<DatabaseConnectionResult, String> {
    run_workspace(state.0.clone(), move |workspace| {
        summary::validate_workspace(workspace.root(), &workspace_path)?;
        if let Some(stored) = settings::load(&workspace.root().join(settings::FILE_NAME))? {
            if stored.database != *workspace.database_settings() {
                return Err("数据库配置已被外部修改，请重新打开工作空间后重试".into());
            }
        }
        settings.enabled = true;
        settings = database::validate(&settings)?;
        // A test never saves settings or joins another workspace's namespace.
        let saved_id = &workspace.database_settings().workspace_id;
        settings.workspace_id = if saved_id.is_empty() { uuid::Uuid::new_v4().to_string() } else { saved_id.clone() };
        let connection = database::run_blocking(|| database::Database::connect(workspace.root(), &settings, &settings.workspace_id))?;
        let records = database::run_blocking(|| connection.list())?;
        Ok(DatabaseConnectionResult { message: "连接成功".into(), record_count: records.len() })
    }).await
}

#[tauri::command]
async fn transfer_workspace_storage(
    state: State<'_, AppState>, workspace_path: String, direction: String, action: String, conflict_policy: String,
) -> Result<workspace::TransferResult, String> {
    run_workspace(state.0.clone(), move |workspace| {
        summary::validate_workspace(workspace.root(), &workspace_path)?;
        let path = workspace.root().join(settings::FILE_NAME);
        let mut stored = settings::load(&path)?.unwrap_or_default();
        // Use only the saved connection; drafts cannot redirect a transfer.
        if stored.database != *workspace.database_settings() {
            return Err("数据库设置已被外部修改，请重新打开工作空间后重试".into());
        }
        let (result, next_settings, connection) = workspace.prepare_transfer(&direction, &action, &conflict_policy)?;
        if action == "migrate" || next_settings != stored.database {
            stored.database = next_settings.clone();
            settings::save(&path, &stored)?;
        }
        workspace.accept_database(next_settings, connection);
        Ok(result)
    }).await
}

#[tauri::command]
async fn database_backup_warning(state: State<'_, AppState>) -> Result<Option<String>, String> {
    run_workspace(state.0.clone(), |workspace| Ok(workspace.backup_warning())).await
}

#[tauri::command]
async fn retry_database_backup(state: State<'_, AppState>, workspace_path: String) -> Result<workspace::TransferResult, String> {
    run_workspace(state.0.clone(), move |workspace| {
        summary::validate_workspace(workspace.root(), &workspace_path)?;
        workspace.retry_backup()
    }).await
}

#[tauri::command]
async fn open_excel_output_directory(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let directory = with_workspace(&state, |workspace| {
        let settings = settings::load(&workspace.root().join(settings::FILE_NAME))?
            .unwrap_or_default()
            .excel;
        let path = std::path::PathBuf::from(settings.output_directory.trim());
        if !path.is_absolute() || !path.is_dir() {
            return Err("Excel 输出目录已不存在，请在设置中重新选择".into());
        }
        Ok(path)
    })?;
    app.opener()
        .open_path(directory.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|error| format!("无法打开 Excel 输出目录：{error}"))
}

struct ExcelSnapshot {
    settings: excel::Settings,
    provider: ai::Provider,
    content: String,
    source_revision: String,
    start: String,
    end: String,
}

/// Read only the selected workspace reports for the Excel AI preview.
/// Check every selected date before filtering empty days, so an empty day from
/// another week cannot bypass the range restriction.
fn excel_snapshot(
    workspace: &mut Workspace,
    stored: settings::StoredSettings,
    dates: &[String],
    workspace_path: &str,
) -> Result<ExcelSnapshot, String> {
    summary::validate_workspace(workspace.root(), workspace_path)?;
    let (start, end) = excel::validate_dates(dates)?;
    if !stored.excel.enabled {
        return Err("请先在设置中启用 Excel 输出".into());
    }
    validate_excel_settings(&stored.excel, &stored.ai)?;
    let provider = ai::default_provider(&stored.ai)?;
    let entries = workspace.list()?;
    let mut selected = dates.to_vec();
    selected.sort();
    selected.dedup();
    let mut reports = Vec::new();
    let mut sources = Vec::new();
    let mut content_bytes = 0;
    for date in &selected {
        let Some(entry) = entries.iter().find(|entry| entry.date == *date) else {
            sources.push(serde_json::json!({ "date": date, "report": null }));
            continue;
        };
        let document = workspace.read(date, entry.format)?;
        sources.push(serde_json::json!({
            "date": document.date, "format": document.format, "content": document.content,
        }));
        if document.content.trim().is_empty() {
            continue;
        }
        content_bytes += document.content.len();
        if content_bytes > 500_000 {
            return Err("所选日报内容超过 500 KB，请缩小范围后重试".into());
        }
        reports.push(serde_json::json!({
            "date": document.date,
            "format": document.format,
            "content": document.content,
        }));
    }
    if reports.is_empty() {
        return Err("所选日期没有可输出的日报内容".into());
    }
    let content = serde_json::to_string(&serde_json::json!({
        "selectedDates": selected,
        "start": start,
        "end": end,
        "reports": reports,
    }))
    .map_err(|_| "无法准备 Excel 输出内容")?;
    if content.len() > 500_000 {
        return Err("所选日报内容超过 500 KB，请缩小范围后重试".into());
    }
    Ok(ExcelSnapshot {
        settings: stored.excel,
        provider,
        content,
        source_revision: {
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(serde_json::to_vec(&sources).map_err(|_| "无法检查 Excel 日报内容")?))
        },
        start,
        end,
    })
}

#[tauri::command]
async fn prepare_excel(
    app: tauri::AppHandle,
    state: State<'_, ai::Runtime>,
    runtime: State<'_, excel::Runtime>,
    workspace_state: State<'_, AppState>,
    dates: Vec<String>,
    workspace_path: String,
    request_id: String,
) -> Result<excel::ExcelPreview, String> {
    let token = state.begin(request_id.clone())?;
    let result = async {
        let snapshot_path = workspace_path.clone();
        let snapshot_dates = dates.clone();
        let snapshot = run_workspace(workspace_state.0.clone(), move |workspace| {
            summary::validate_workspace(workspace.root(), &snapshot_path)?;
            let stored = stored_settings_at(&app, workspace.root())?;
            excel_snapshot(workspace, stored, &snapshot_dates, &snapshot_path)
        }).await?;
        if token.is_cancelled() {
            return Err("已取消 Excel 预览".into());
        }
        let template_path = snapshot.settings.template_path.trim().to_string();
        let template = tauri::async_runtime::spawn_blocking(move || {
            excel::read_template(std::path::Path::new(&template_path))
        }).await.map_err(|_| "无法读取 Excel 模板")??;
        let template_fingerprint = template.fingerprint().to_string();
        let context = template.context()?;
        let response = ai::fill_excel(
            &snapshot.provider,
            &context,
            &snapshot.content,
            &snapshot.settings.prompt,
            token.clone(),
        ).await?;
        if token.is_cancelled() {
            return Err("已取消 Excel 预览".into());
        }
        let provenance = excel::Provenance {
            workspace_path, dates, settings: snapshot.settings,
            source_revision: snapshot.source_revision, template_fingerprint,
        };
        let render_token = token.clone();
        let pending = tauri::async_runtime::spawn_blocking(move || {
            excel::prepare(template, &response, provenance, snapshot.start, snapshot.end, render_token)
        }).await.map_err(|_| "无法生成 Excel 预览")??;
        let preview = runtime.insert(pending)?;
        if token.is_cancelled() {
            runtime.discard(&preview.id);
            return Err("已取消 Excel 预览".into());
        }
        Ok(preview)
    }.await;
    state.finish(&request_id);
    result
}

fn validate_excel_confirmation(
    workspace: &mut Workspace,
    stored: settings::StoredSettings,
    workspace_path: &str,
    provenance: &excel::Provenance,
) -> Result<(), String> {
    let regenerate = |reason: String| format!("{reason}。请重新生成 Excel 预览");
    if workspace_path != provenance.workspace_path {
        return Err("工作空间已切换，请重新生成 Excel 预览".into());
    }
    summary::validate_workspace(workspace.root(), workspace_path).map_err(regenerate)?;
    if stored.excel != provenance.settings {
        return Err("Excel 输出设置已修改，请重新生成 Excel 预览".into());
    }
    let current = excel_snapshot(workspace, stored, &provenance.dates, workspace_path).map_err(regenerate)?;
    if current.source_revision != provenance.source_revision {
        return Err("所选日报内容已修改，请重新生成 Excel 预览".into());
    }
    let fingerprint = excel::template_fingerprint(std::path::Path::new(provenance.settings.template_path.trim()))
        .map_err(regenerate)?;
    if fingerprint != provenance.template_fingerprint {
        return Err("Excel 模板已修改，请重新生成 Excel 预览".into());
    }
    Ok(())
}

#[tauri::command]
async fn confirm_excel(
    app: tauri::AppHandle,
    runtime: State<'_, excel::Runtime>,
    workspace_state: State<'_, AppState>,
    workspace_path: String,
    preview_id: String,
) -> Result<excel::ExportResult, String> {
    let runtime = runtime.inner().clone();
    run_workspace(workspace_state.0.clone(), move |workspace| {
        runtime.confirm(&preview_id, |provenance| {
            let stored = stored_settings_at(&app, workspace.root())
                .map_err(|error| format!("{error}。请重新生成 Excel 预览"))?;
            validate_excel_confirmation(workspace, stored, &workspace_path, provenance)
        })
    }).await
}

#[tauri::command]
fn discard_excel_preview(runtime: State<'_, excel::Runtime>, preview_id: String) {
    runtime.discard(&preview_id);
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
    workspace_state: State<'_, AppState>,
    content: String,
    format: Format,
    request_id: String,
) -> Result<String, String> {
    let settings = read_ai_settings(&app, &workspace_state)?;
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
#[tauri::command]
async fn summarize_entries(
    app: tauri::AppHandle,
    state: State<'_, ai::Runtime>,
    workspace_state: State<'_, AppState>,
    content: String,
    request_id: String,
) -> Result<String, String> {
    let settings = read_ai_settings(&app, &workspace_state)?;
    let provider = settings
        .providers
        .iter()
        .find(|provider| Some(&provider.id) == settings.default_provider_id.as_ref())
        .ok_or("请先在设置中选择默认 AI")?
        .clone();
    let token = state.begin(request_id.clone())?;
    let result = ai::summarize(&provider, &content, &settings.summary_prompt, token).await;
    state.finish(&request_id);
    result
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                // Native menu key equivalents are handled before WebKit keydown.
                // Replace the predefined Undo/Redo entries with menu items that
                // forward the action to the editor's own history.
                if let Some(menu) = app.menu() {
                    if let Ok(items) = menu.items() {
                        if let Some(edit) = items.into_iter().find_map(|item| match item {
                            tauri::menu::MenuItemKind::Submenu(submenu)
                                if submenu.text().ok().as_deref() == Some("Edit") =>
                            {
                                Some(submenu)
                            }
                            _ => None,
                        }) {
                            let _ = edit.remove_at(0);
                            let _ = edit.remove_at(0);
                            let undo = tauri::menu::MenuItem::with_id(
                                app,
                                "history-undo",
                                "撤销",
                                true,
                                Some("CmdOrCtrl+Z"),
                            )?;
                            let redo = tauri::menu::MenuItem::with_id(
                                app,
                                "history-redo",
                                "重做",
                                true,
                                Some("CmdOrCtrl+Shift+Z"),
                            )?;
                            edit.insert_items(&[&undo, &redo], 0)?;
                        }
                    }
                }
            }
            Ok(())
        })
        .on_menu_event(|app, event| {
            let action = match event.id().as_ref() {
                "history-undo" => "undo",
                "history-redo" => "redo",
                _ => return,
            };
            let _ = app.emit("history-action", action);
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(ai::Runtime::default())
        .manage(excel::Runtime::default())
        .manage(holidays::Runtime::default())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            recent_workspace,
            load_holiday_data,
            sync_holiday_year,
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
            load_appearance_settings,
            save_appearance_settings,
            load_summary_settings,
            save_summary_settings,
            open_summary_output_directory,
            export_summary,
            load_excel_settings,
            save_excel_settings,
            save_workspace_settings,
            load_database_settings,
            test_database_connection,
            transfer_workspace_storage,
            retry_database_backup,
            database_backup_warning,
            open_excel_output_directory,
            prepare_excel,
            confirm_excel,
            discard_excel_preview,
            detect_ai_clis,
            list_ai_models,
            list_cli_models,
            cancel_ai,
            polish_entry,
            summarize_entries
        ])
        .run(tauri::generate_context!())
        .expect("无法启动 WorkReport");
}

#[cfg(test)]
mod excel_integration_tests {
    use super::*;

    fn configured_settings(root: &std::path::Path) -> settings::StoredSettings {
        let template = root.join("weekly.xlsx");
        fs::write(&template, "template parsed only during export").unwrap();
        let provider = ai::Provider {
            id: "default".into(),
            name: "HTTP".into(),
            kind: ai::ProviderKind::Http,
            executable: String::new(),
            model: "model".into(),
            base_url: "https://example.com/v1".into(),
            api_key: String::new(),
        };
        settings::StoredSettings {
            excel: excel::Settings {
                enabled: true,
                template_path: template.to_string_lossy().into_owned(),
                prompt: "将工作内容填写到日报模板".into(),
                output_directory: root.to_string_lossy().into_owned(),
            },
            ai: ai::Settings {
                providers: vec![provider],
                default_provider_id: Some("default".into()),
                ..ai::Settings::default()
            },
            ..settings::StoredSettings::default()
        }
    }

    #[test]
    fn workspace_settings_save_all_sections_and_validate_excel_against_new_ai() {
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        let path = workspace.root().join(settings::FILE_NAME);
        settings::save(&path, &settings::StoredSettings::default()).unwrap();
        let mut proposed = configured_settings(root.path());
        proposed.appearance.scale = 120;
        proposed.ai.generation_preference = "简洁、突出结果".into();
        proposed.ai.summary_prompt = "按项目归类".into();
        proposed.summary.output_directory = workspace
            .root()
            .join("总结/每周")
            .to_string_lossy()
            .into_owned();

        let saved = save_workspace_settings_at(
            &mut workspace,
            root.path().to_str().unwrap(),
            proposed,
        )
        .unwrap();

        assert_eq!(saved.appearance.scale, 120);
        assert_eq!(saved.ai.generation_preference, "简洁、突出结果");
        assert_eq!(saved.ai.summary_prompt, "按项目归类");
        assert!(saved.excel.enabled);
        assert_eq!(saved.summary.output_directory, "总结/每周");
        let restored = settings::load(&path).unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(saved).unwrap(),
            serde_json::to_value(restored).unwrap()
        );
        assert!(!workspace.root().join("总结").exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[test]
    fn any_invalid_section_leaves_every_saved_section_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        let path = workspace.root().join(settings::FILE_NAME);
        let original = configured_settings(root.path());
        settings::save(&path, &original).unwrap();
        let original_bytes = fs::read(&path).unwrap();

        for invalid_section in ["appearance", "ai", "excel", "excelAi", "summary", "database"] {
            let mut proposed = original.clone();
            proposed.appearance.scale = 110;
            proposed.ai.summary_prompt = "尚未保存的新提示词".into();
            proposed.summary.output_directory = "新总结目录".into();
            match invalid_section {
                "appearance" => proposed.appearance.scale = 105,
                "ai" => proposed.ai.providers[0].base_url = "invalid-url".into(),
                "excel" => proposed.excel.template_path = "/missing-template.xlsx".into(),
                "excelAi" => proposed.ai.default_provider_id = None,
                "summary" => proposed.summary.output_directory = "../outside".into(),
                "database" => { proposed.database.enabled = true; proposed.database.sqlite_path = "../outside.sqlite".into(); },
                _ => unreachable!(),
            }

            assert!(
                save_workspace_settings_at(
                    &mut workspace,
                    root.path().to_str().unwrap(),
                    proposed,
                )
                .is_err(),
                "{invalid_section} must fail before any settings are saved"
            );
            assert_eq!(fs::read(&path).unwrap(), original_bytes);
            assert!(!workspace.root().join("新总结目录").exists());
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
        }
    }

    #[test]
    fn workspace_settings_reject_stale_workspace_without_writing_either_workspace() {
        let current = tempfile::tempdir().unwrap();
        let previous = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(current.path().to_str().unwrap()).unwrap();
        let current_path = current.path().join(settings::FILE_NAME);
        let previous_path = previous.path().join(settings::FILE_NAME);
        settings::save(&current_path, &settings::StoredSettings::default()).unwrap();
        settings::save(&previous_path, &settings::StoredSettings::default()).unwrap();
        let current_bytes = fs::read(&current_path).unwrap();
        let previous_bytes = fs::read(&previous_path).unwrap();

        let error = save_workspace_settings_at(
            &mut workspace,
            previous.path().to_str().unwrap(),
            settings::StoredSettings::default(),
        )
        .err()
        .unwrap();

        assert!(error.contains("工作空间已切换"));
        assert_eq!(fs::read(&current_path).unwrap(), current_bytes);
        assert_eq!(fs::read(&previous_path).unwrap(), previous_bytes);
        assert_eq!(fs::read_dir(current.path()).unwrap().count(), 1);
        assert_eq!(fs::read_dir(previous.path()).unwrap().count(), 1);
    }

    #[test]
    fn workspace_settings_do_not_silently_replace_a_damaged_existing_file() {
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        let path = workspace.root().join(settings::FILE_NAME);
        fs::write(&path, "{ broken configuration").unwrap();

        assert!(save_workspace_settings_at(
            &mut workspace,
            root.path().to_str().unwrap(),
            settings::StoredSettings::default(),
        )
        .is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "{ broken configuration");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn unified_settings_save_backfills_sqlite_and_persists_its_generated_identity() {
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        workspace.create_with_content("20261022", Format::Md, "需要备份的日报").unwrap();
        let mut proposed = settings::StoredSettings::default();
        proposed.database.enabled = true;
        let saved = save_workspace_settings_at(&mut workspace, root.path().to_str().unwrap(), proposed).unwrap();
        database::validate_workspace_id(&saved.database.workspace_id).unwrap();
        assert!(root.path().join("workreport.sqlite").is_file());
        let restored = settings::load(&root.path().join(settings::FILE_NAME)).unwrap().unwrap();
        assert_eq!(restored.database, saved.database);
        assert_eq!(workspace.database_settings(), &saved.database);
        let connection = database::run_blocking(|| database::Database::connect(root.path(), &saved.database, &saved.database.workspace_id)).unwrap();
        let records = database::run_blocking(|| connection.list()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].content, "需要备份的日报");
    }

    #[test]
    fn unified_settings_reject_external_database_redirection_before_copying_or_overwriting() {
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        let mut externally_changed = settings::StoredSettings::default();
        externally_changed.database.sqlite_path = "external.sqlite".into();
        let path = root.path().join(settings::FILE_NAME);
        settings::save(&path, &externally_changed).unwrap();
        let original = fs::read(&path).unwrap();
        let error = save_workspace_settings_at(&mut workspace, root.path().to_str().unwrap(), settings::StoredSettings::default()).err().unwrap();
        assert!(error.contains("外部修改"));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(!root.path().join("external.sqlite").exists());
        assert!(!root.path().join("workreport.sqlite").exists());
    }

    #[test]
    fn enabling_excel_requires_a_valid_default_ai() {
        let root = tempfile::tempdir().unwrap();
        let mut stored = configured_settings(root.path());
        assert!(validate_excel_settings(&stored.excel, &stored.ai).is_ok());
        stored.ai.default_provider_id = None;
        let error = validate_excel_settings(&stored.excel, &stored.ai).unwrap_err();
        assert!(error.contains("默认 AI"));
        stored.excel.enabled = false;
        stored.excel.template_path.clear();
        stored.excel.output_directory.clear();
        stored.excel.prompt.clear();
        assert!(validate_excel_settings(&stored.excel, &stored.ai).is_ok());
    }

    #[test]
    fn excel_source_reads_only_selected_reports_without_requiring_a_summary() {
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        workspace
            .create_with_content("20261020", Format::Md, "完成接口测试")
            .unwrap();
        workspace
            .create_with_content("20261021", Format::Txt, "整理周报")
            .unwrap();
        workspace
            .create_with_content("20261022", Format::Md, "未选择的机密内容")
            .unwrap();
        let snapshot = excel_snapshot(
            &mut workspace,
            configured_settings(root.path()),
            &["20261021".into(), "20261020".into(), "20261021".into()],
            root.path().to_str().unwrap(),
        )
        .unwrap();
        let content: serde_json::Value = serde_json::from_str(&snapshot.content).unwrap();
        assert_eq!(snapshot.start, "20261020");
        assert_eq!(snapshot.end, "20261021");
        assert!(content.get("summary").is_none());
        assert_eq!(
            content["selectedDates"],
            serde_json::json!(["20261020", "20261021"])
        );
        assert_eq!(content["reports"].as_array().unwrap().len(), 2);
        assert_eq!(content["reports"][0]["date"], "20261020");
        assert_eq!(content["reports"][0]["format"], "md");
        assert_eq!(content["reports"][0]["content"], "完成接口测试");
        assert_eq!(content["reports"][1]["date"], "20261021");
        assert_eq!(content["reports"][1]["format"], "txt");
        assert_eq!(content["reports"][1]["content"], "整理周报");
        assert!(!snapshot.content.contains("未选择的机密内容"));
        assert!(!snapshot.content.contains(root.path().to_str().unwrap()));
    }

    #[test]
    fn excel_checks_empty_selected_days_before_filtering_report_content() {
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        workspace
            .create_with_content("20261020", Format::Md, "工作记录")
            .unwrap();
        let error = excel_snapshot(
            &mut workspace,
            configured_settings(root.path()),
            &["20261020".into(), "20261026".into()],
            root.path().to_str().unwrap(),
        )
        .err()
        .unwrap();
        assert!(error.contains("同一周"));
        let error = excel_snapshot(
            &mut workspace,
            configured_settings(root.path()),
            &["20261023".into()],
            root.path().to_str().unwrap(),
        )
        .err()
        .unwrap();
        assert!(error.contains("没有可输出"));
    }

    #[test]
    fn excel_requires_nonempty_bounded_selected_reports() {
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        workspace
            .create_with_content("20261020", Format::Md, " \n\t")
            .unwrap();
        let error = excel_snapshot(
            &mut workspace,
            configured_settings(root.path()),
            &["20261020".into()],
            root.path().to_str().unwrap(),
        )
        .err()
        .unwrap();
        assert!(error.contains("没有可输出"));
        workspace
            .create_with_content("20261021", Format::Md, &"字".repeat(166_667))
            .unwrap();
        let error = excel_snapshot(
            &mut workspace,
            configured_settings(root.path()),
            &["20261021".into()],
            root.path().to_str().unwrap(),
        )
        .err()
        .unwrap();
        assert!(error.contains("500 KB"));
    }

    #[test]
    fn excel_rejects_export_after_workspace_switch() {
        let root = tempfile::tempdir().unwrap();
        let previous = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        workspace
            .create_with_content("20261020", Format::Md, "新工作空间日报")
            .unwrap();
        let error = excel_snapshot(
            &mut workspace,
            configured_settings(root.path()),
            &["20261020".into()],
            previous.path().to_str().unwrap(),
        )
        .err()
        .unwrap();
        assert!(error.contains("工作空间已切换"));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    fn prepared_preview(
        workspace: &mut Workspace,
        stored: &settings::StoredSettings,
    ) -> (excel::Runtime, excel::ExcelPreview) {
        let workspace_path = workspace.root().to_string_lossy().into_owned();
        let dates = vec!["20261019".into(), "20261020".into(), "20261021".into()];
        let snapshot = excel_snapshot(workspace, stored.clone(), &dates, &workspace_path).unwrap();
        let template = excel::read_template(std::path::Path::new(&stored.excel.template_path)).unwrap();
        let provenance = excel::Provenance {
            workspace_path, dates,
            settings: stored.excel.clone(), source_revision: snapshot.source_revision,
            template_fingerprint: template.fingerprint().into(),
        };
        let pending = excel::prepare(template,
            &serde_json::json!({"cells":[{"sheet":"Sheet1","cell":"B2","value":"用户确认的内容"}]}).to_string(),
            provenance, snapshot.start, snapshot.end, tokio_util::sync::CancellationToken::new()).unwrap();
        let runtime = excel::Runtime::default();
        let preview = runtime.insert(pending).unwrap();
        (runtime, preview)
    }

    fn preview_fixture(root: &std::path::Path) -> (Workspace, settings::StoredSettings) {
        let mut workspace = Workspace::open(root.to_str().unwrap()).unwrap();
        workspace.create_with_content("20261020", Format::Md, "生成时的日报正文").unwrap();
        workspace.create_with_content("20261021", Format::Txt, " ").unwrap();
        let mut stored = configured_settings(root);
        let output = root.join("output");
        fs::create_dir(&output).unwrap();
        stored.excel.output_directory = output.to_string_lossy().into_owned();
        umya_spreadsheet::writer::xlsx::write(&umya_spreadsheet::new_file(), std::path::Path::new(&stored.excel.template_path)).unwrap();
        settings::save(&root.join(settings::FILE_NAME), &stored).unwrap();
        (workspace, stored)
    }

    #[test]
    fn excel_confirmation_revalidates_selected_sources_settings_template_and_active_workspace_before_writing() {
        for mutation in ["settings", "source", "empty-source", "missing-source", "template", "workspace"] {
            let root = tempfile::tempdir().unwrap();
            let switched = tempfile::tempdir().unwrap();
            let (mut workspace, mut stored) = preview_fixture(root.path());
            let (runtime, preview) = prepared_preview(&mut workspace, &stored);
            let workspace_path = workspace.root().to_string_lossy().into_owned();
            let output = root.path().join("output");
            assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
            match mutation {
                "settings" => {
                    stored.excel.prompt.push_str("修改后的提示词");
                    settings::save(&root.path().join(settings::FILE_NAME), &stored).unwrap();
                }
                "source" | "empty-source" => {
                    let (date, format, content) = if mutation == "source" { ("20261020", Format::Md, "修改后的正文") }
                        else { ("20261021", Format::Txt, "  ") };
                    let document = workspace.read(date, format).unwrap();
                    workspace.save(date, format, content, &document.revision).unwrap();
                }
                "missing-source" => { workspace.create_with_content("20261019", Format::Md, "新建选中日报").unwrap(); }
                "template" => {
                    let mut replacement = umya_spreadsheet::new_file();
                    replacement.sheet_by_name_mut("Sheet1").unwrap().cell_mut("A1").set_value_string("模板已修改");
                    umya_spreadsheet::writer::xlsx::write(&replacement, std::path::Path::new(&stored.excel.template_path)).unwrap();
                }
                "workspace" => { workspace = Workspace::open(switched.path().to_str().unwrap()).unwrap(); }
                _ => unreachable!(),
            }
            let current = settings::load(&root.path().join(settings::FILE_NAME)).unwrap().unwrap();
            let error = runtime.confirm(&preview.id, |provenance| {
                validate_excel_confirmation(&mut workspace, current, &workspace_path, provenance)
            }).unwrap_err();
            assert!(error.contains("重新生成"), "{mutation}: {error}");
            let expected = match mutation { "settings" => "设置", "template" => "模板", "workspace" => "工作空间", _ => "日报" };
            assert!(error.contains(expected), "{mutation}: {error}");
            assert_eq!(fs::read_dir(&output).unwrap().count(), 0, "{mutation}");
            assert!(runtime.confirm(&preview.id, |_| Ok(())).is_err(), "stale token must be discarded: {mutation}");
        }
    }

    #[test]
    fn excel_confirmation_keeps_approved_plan_when_only_unselected_reports_change() {
        let root = tempfile::tempdir().unwrap();
        let (mut workspace, stored) = preview_fixture(root.path());
        let (runtime, preview) = prepared_preview(&mut workspace, &stored);
        workspace.create_with_content("20261022", Format::Md, "未选中的新增日报").unwrap();
        let workspace_path = workspace.root().to_string_lossy().into_owned();
        let result = runtime.confirm(&preview.id, |provenance| {
            validate_excel_confirmation(&mut workspace, stored, &workspace_path, provenance)
        }).unwrap();
        assert_eq!(fs::read_dir(root.path().join("output")).unwrap().count(), 1);
        assert!(std::path::Path::new(&result.path).is_file());
        assert_eq!(result.cells, 1);
    }

}
