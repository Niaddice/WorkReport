use std::{
    collections::HashMap,
    fs,
    path::Path,
};

use chrono::Utc;
use serde_json::{Value, json};
use workreport_core::{
    settings::{self, StoredSettings},
    summary,
    workspace::{Format, Workspace},
    workspace_preview::WorkspaceSession,
};

const MAX_BACKUP_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BackupJournalEntry {
    pub(crate) date: String,
    pub(crate) format: Format,
    pub(crate) content: String,
}

#[derive(Clone)]
pub(crate) struct WorkspaceBackupBundle {
    pub(crate) version: u64,
    pub(crate) exported_at: String,
    pub(crate) workspace_name: String,
    pub(crate) workspace_path: String,
    pub(crate) journals: Vec<BackupJournalEntry>,
    pub(crate) settings: Option<StoredSettings>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestorePlan {
    pub(crate) total_in_backup: usize,
    pub(crate) new_entries_count: usize,
    pub(crate) conflicting_entries_count: usize,
    pub(crate) has_settings: bool,
    pub(crate) new_dates: Vec<String>,
    pub(crate) conflicting_dates: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestorePolicy {
    Skip,
    Overwrite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestoreResult {
    pub(crate) total_restored: usize,
    pub(crate) created_count: usize,
    pub(crate) overwritten_count: usize,
    pub(crate) skipped_count: usize,
    pub(crate) settings_restored: bool,
}

pub(crate) fn export_backup_to_file(
    session: &WorkspaceSession,
    include_settings: bool,
    destination: &Path,
) -> Result<usize, String> {
    let entries = session.list()?;
    let mut journals = Vec::new();
    for entry in entries {
        if let Ok(doc) = session.read(&entry.date, entry.format) {
            journals.push(json!({
                "date": doc.date,
                "format": doc.format.extension(),
                "content": doc.content,
            }));
        }
    }
    let settings_value = if include_settings {
        session
            .load_stored_settings()
            .ok()
            .and_then(|s| serde_json::to_value(s).ok())
    } else {
        None
    };

    let info = session.info();
    let count = journals.len();
    let mut root = json!({
        "version": 1,
        "exportedAt": Utc::now().to_rfc3339(),
        "workspaceName": info.name,
        "workspacePath": info.path,
        "totalJournals": count,
        "journals": journals,
    });
    if let (Some(obj), Some(settings)) = (root.as_object_mut(), settings_value) {
        obj.insert("settings".into(), settings);
    }

    let bytes = serde_json::to_vec_pretty(&root)
        .map_err(|error| format!("无法序列化备份文件：{error}"))?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建备份目录：{error}"))?;
    }
    fs::write(destination, bytes).map_err(|error| format!("无法写入备份文件：{error}"))?;
    Ok(count)
}

pub(crate) fn load_backup_file(path: &Path) -> Result<WorkspaceBackupBundle, String> {
    let meta = fs::metadata(path).map_err(|_| "无法读取备份文件信息".to_string())?;
    if !meta.is_file() {
        return Err("请选择有效的 .json 备份文件".into());
    }
    if meta.len() > MAX_BACKUP_BYTES {
        return Err("备份文件过大（超过 32 MB 限制）".into());
    }
    let bytes = fs::read(path).map_err(|error| format!("无法读取备份文件：{error}"))?;
    parse_backup_bytes(&bytes)
}

pub(crate) fn parse_backup_bytes(bytes: &[u8]) -> Result<WorkspaceBackupBundle, String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("备份 JSON 解析失败：{error}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| "无效的备份文件结构".to_string())?;
    let journal_items = obj
        .get("journals")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "备份文件中缺少 journals 日报列表".to_string())?;

    let mut journals = Vec::new();
    for item in journal_items {
        let Some(entry) = item.as_object() else {
            continue;
        };
        let Some(date) = entry.get("date").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(content) = entry.get("content").and_then(|v| v.as_str()) else {
            continue;
        };
        let format = match entry.get("format").and_then(|v| v.as_str()) {
            Some("txt") => Format::Txt,
            _ => Format::Md,
        };
        journals.push(BackupJournalEntry {
            date: date.to_string(),
            format,
            content: content.to_string(),
        });
    }

    let settings = obj
        .get("settings")
        .filter(|v| v.is_object())
        .and_then(|v| serde_json::from_value::<StoredSettings>(v.clone()).ok());

    Ok(WorkspaceBackupBundle {
        version: obj.get("version").and_then(|v| v.as_u64()).unwrap_or(1),
        exported_at: obj
            .get("exportedAt")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        workspace_name: obj
            .get("workspaceName")
            .and_then(|v| v.as_str())
            .unwrap_or("备份工作空间")
            .to_string(),
        workspace_path: obj
            .get("workspacePath")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        journals,
        settings,
    })
}

pub(crate) fn plan_restore(
    session: &WorkspaceSession,
    bundle: &WorkspaceBackupBundle,
) -> Result<RestorePlan, String> {
    let existing = session.list()?;
    let existing_dates: HashMap<&str, Format> = existing
        .iter()
        .map(|entry| (entry.date.as_str(), entry.format))
        .collect();
    let mut new_dates = Vec::new();
    let mut conflicting_dates = Vec::new();
    for item in &bundle.journals {
        if existing_dates.contains_key(item.date.as_str()) {
            conflicting_dates.push(item.date.clone());
        } else {
            new_dates.push(item.date.clone());
        }
    }
    Ok(RestorePlan {
        total_in_backup: bundle.journals.len(),
        new_entries_count: new_dates.len(),
        conflicting_entries_count: conflicting_dates.len(),
        has_settings: bundle.settings.is_some(),
        new_dates,
        conflicting_dates,
    })
}

pub(crate) fn execute_restore(
    root: &Path,
    expected_path: &str,
    bundle: &WorkspaceBackupBundle,
    policy: RestorePolicy,
    restore_settings: bool,
) -> Result<RestoreResult, String> {
    summary::validate_workspace(root, expected_path)?;
    let mut workspace = Workspace::open(root)?;
    let existing = workspace.list()?;
    let existing_map: HashMap<String, Format> = existing
        .into_iter()
        .map(|entry| (entry.date, entry.format))
        .collect();

    let mut created_count = 0usize;
    let mut overwritten_count = 0usize;
    let mut skipped_count = 0usize;

    for item in &bundle.journals {
        if let Some(&existing_format) = existing_map.get(&item.date) {
            match policy {
                RestorePolicy::Skip => {
                    skipped_count += 1;
                }
                RestorePolicy::Overwrite => {
                    if let Ok(current) = workspace.read(&item.date, existing_format) {
                        if workspace
                            .save(&item.date, existing_format, &item.content, &current.revision)
                            .is_ok()
                        {
                            overwritten_count += 1;
                        }
                    }
                }
            }
        } else if workspace
            .create_with_content(&item.date, item.format, &item.content)
            .is_ok()
        {
            created_count += 1;
        }
    }

    let mut settings_restored = false;
    if restore_settings {
        if let Some(candidate) = &bundle.settings {
            if settings::save_workspace_settings_confirmed(
                &mut workspace,
                expected_path,
                candidate.clone(),
                None,
                false,
            )
            .is_ok()
            {
                settings_restored = true;
            }
        }
    }

    Ok(RestoreResult {
        total_restored: created_count + overwritten_count,
        created_count,
        overwritten_count,
        skipped_count,
        settings_restored,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use workreport_core::database;

    #[test]
    fn backup_export_plan_and_restore_work_across_file_and_sqlite_modes() {
        let source_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(source_dir.path().join("2026")).unwrap();
        fs::write(source_dir.path().join("2026/20261118.md"), "Day 18 body").unwrap();
        fs::write(source_dir.path().join("2026/20261119.txt"), "Day 19 text").unwrap();

        let source_session = WorkspaceSession::open(source_dir.path()).unwrap();
        let backup_file = source_dir.path().join("backup-export.json");
        let count = export_backup_to_file(&source_session, true, &backup_file).unwrap();
        assert_eq!(count, 2);

        // Parse JSON compatible with Vue WorkspaceBackupBundle
        let bundle = load_backup_file(&backup_file).unwrap();
        assert_eq!(bundle.version, 1);
        assert_eq!(bundle.journals.len(), 2);

        // Restore into a SQLite Database-only target workspace with 1 existing conflict
        let target_dir = tempfile::tempdir().unwrap();
        let mut target_settings = StoredSettings::default();
        target_settings.database = database::Settings {
            enabled: true,
            engine: database::Engine::Sqlite,
            mode: database::Mode::Database,
            workspace_id: "550e8400-e29b-41d4-a716-446655440000".into(),
            sqlite_path: "workreport.sqlite".into(),
            ..Default::default()
        };
        settings::save(&target_dir.path().join(settings::FILE_NAME), &target_settings).unwrap();
        {
            let mut ws = Workspace::open(target_dir.path()).unwrap();
            ws.create_with_content("20261118", Format::Md, "Existing body")
                .unwrap();
        }

        let target_session = WorkspaceSession::open(target_dir.path()).unwrap();
        let plan = plan_restore(&target_session, &bundle).unwrap();
        assert_eq!(plan.total_in_backup, 2);
        assert_eq!(plan.new_entries_count, 1);
        assert_eq!(plan.conflicting_entries_count, 1);

        let target_path = target_session.info().path;
        let skip_result = execute_restore(
            target_session.root(),
            &target_path,
            &bundle,
            RestorePolicy::Skip,
            false,
        )
        .unwrap();
        assert_eq!(skip_result.created_count, 1);
        assert_eq!(skip_result.skipped_count, 1);
        assert_eq!(skip_result.overwritten_count, 0);

        let reopened = WorkspaceSession::open(target_dir.path()).unwrap();
        assert_eq!(reopened.read("20261118", Format::Md).unwrap().content, "Existing body");
        assert_eq!(reopened.read("20261119", Format::Txt).unwrap().content, "Day 19 text");

        let overwrite_result = execute_restore(
            target_session.root(),
            &target_path,
            &bundle,
            RestorePolicy::Overwrite,
            false,
        )
        .unwrap();
        assert_eq!(overwrite_result.overwritten_count, 2);
        let after = WorkspaceSession::open(target_dir.path()).unwrap();
        assert_eq!(after.read("20261118", Format::Md).unwrap().content, "Day 18 body");
    }
}
