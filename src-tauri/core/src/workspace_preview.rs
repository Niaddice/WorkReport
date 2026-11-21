use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::{
    ai, database, directory_ops, excel, file_migration, file_rules, git,
    history::{self, Version},
    settings, summary,
    workspace::{
        DailyActivity, Document, Entry, FileNode, Format, GitReportAppend, GitReportAppendResult,
        SearchResultItem, TransferResult, Workspace, WorkspaceInfo,
    },
    workspace_document,
};

/// Read-only access to local journals and history without connecting to a database.
pub struct WorkspacePreview(Workspace);

impl WorkspacePreview {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        Workspace::open_file_preview(path.as_ref()).map(Self)
    }

    pub fn info(&self) -> WorkspaceInfo {
        self.0.info()
    }

    pub fn root(&self) -> &Path {
        self.0.root()
    }

    pub fn default_format(&self) -> Format {
        self.0.database_settings().default_format
    }

    pub fn list(&mut self) -> Result<Vec<Entry>, String> {
        self.0.list()
    }

    pub fn read(&self, date: &str, format: Format) -> Result<Document, String> {
        self.0.read(date, format)
    }

    pub fn preview(&self, date: &str, format: Format) -> Result<String, String> {
        let path = self.0.entry_path(date, format)?;
        Ok(crate::workspace::entry_preview(&path))
    }

    pub fn list_history(&self, date: Option<&str>) -> Result<Vec<Version>, String> {
        if let Some(date) = date {
            history::validate_date(date)?;
            file_rules::safe_file(self.0.root(), &format!("{}/{date}", history::DIR))?;
        } else {
            file_rules::safe_file(self.0.root(), history::DIR)?;
        }
        history::list(self.0.root(), date)
    }

    pub fn read_history(&self, date: &str, id: &str) -> Result<String, String> {
        history::validate_date(date)?;
        if history::parse_id(id).is_none() {
            return Err("历史版本标识无效".into());
        }
        file_rules::safe_file(self.0.root(), &format!("{}/{date}/{id}", history::DIR))?;
        history::read(self.0.root(), date, id)
    }

    pub fn ui_preferences(&self) -> Result<Option<crate::ui::Preferences>, String> {
        let path = file_rules::safe_file(self.0.root(), crate::settings::FILE_NAME)?;
        Ok(crate::settings::load(&path)?.and_then(|stored| stored.ui))
    }

    pub fn appearance_settings(&self) -> Result<crate::appearance::Settings, String> {
        let path = file_rules::safe_file(self.0.root(), crate::settings::FILE_NAME)?;
        let appearance = crate::settings::load(&path)?
            .map(|stored| stored.appearance)
            .unwrap_or_default();
        if crate::appearance::validate(&appearance).is_ok() {
            Ok(appearance)
        } else {
            Ok(crate::appearance::Settings::default())
        }
    }
}

enum WorkspaceSessionInner {
    FilePreview(Mutex<WorkspacePreview>),
    Database(Mutex<Workspace>),
}

/// Unified workspace access for GPUI: keeps file/dual workspaces read-only on open
/// while also supporting database-only workspaces.
pub struct WorkspaceSession {
    root: PathBuf,
    info: WorkspaceInfo,
    default_format: Format,
    database_only: bool,
    inner: WorkspaceSessionInner,
}

impl WorkspaceSession {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        match WorkspacePreview::open(path) {
            Ok(preview) => {
                let root = preview.root().to_path_buf();
                let info = preview.info();
                let default_format = preview.default_format();
                Ok(Self {
                    root,
                    info,
                    default_format,
                    database_only: false,
                    inner: WorkspaceSessionInner::FilePreview(Mutex::new(preview)),
                })
            }
            Err(preview_error) => {
                let Ok(canonical) = fs::canonicalize(path) else {
                    return Err(preview_error);
                };
                let settings_path = canonical.join(settings::FILE_NAME);
                let Ok(meta) = fs::symlink_metadata(&settings_path) else {
                    return Err(preview_error);
                };
                if !meta.is_file() || meta.file_type().is_symlink() {
                    return Err(preview_error);
                }
                let Ok(Some(stored)) = settings::load(&settings_path) else {
                    return Err(preview_error);
                };
                if !stored.database.enabled || stored.database.mode != database::Mode::Database {
                    return Err(preview_error);
                }
                let mut workspace = Workspace::open(canonical)?;
                workspace.validate_open()?;
                let root = workspace.root().to_path_buf();
                let info = workspace.info();
                let default_format = workspace.database_settings().default_format;
                Ok(Self {
                    root,
                    info,
                    default_format,
                    database_only: true,
                    inner: WorkspaceSessionInner::Database(Mutex::new(workspace)),
                })
            }
        }
    }

    pub fn info(&self) -> WorkspaceInfo {
        self.info.clone()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn default_format(&self) -> Format {
        self.default_format
    }

    pub fn is_database_only(&self) -> bool {
        self.database_only
    }

    pub fn list(&self) -> Result<Vec<Entry>, String> {
        match &self.inner {
            WorkspaceSessionInner::FilePreview(preview) => preview
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .list(),
            WorkspaceSessionInner::Database(workspace) => workspace
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .list(),
        }
    }

    pub fn read(&self, date: &str, format: Format) -> Result<Document, String> {
        match &self.inner {
            WorkspaceSessionInner::FilePreview(preview) => preview
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .read(date, format),
            WorkspaceSessionInner::Database(workspace) => workspace
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .read(date, format),
        }
    }

    pub fn preview(&self, date: &str, format: Format) -> Result<String, String> {
        match &self.inner {
            WorkspaceSessionInner::FilePreview(preview) => preview
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .preview(date, format),
            WorkspaceSessionInner::Database(workspace) => workspace
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .preview_database_entry(date),
        }
    }

    pub fn list_history(&self, date: Option<&str>) -> Result<Vec<Version>, String> {
        if let Some(date) = date {
            history::validate_date(date)?;
            file_rules::safe_file(&self.root, &format!("{}/{date}", history::DIR))?;
        } else {
            file_rules::safe_file(&self.root, history::DIR)?;
        }
        history::list(&self.root, date)
    }

    pub fn read_history(&self, date: &str, id: &str) -> Result<String, String> {
        history::validate_date(date)?;
        if history::parse_id(id).is_none() {
            return Err("历史版本标识无效".into());
        }
        file_rules::safe_file(&self.root, &format!("{}/{date}/{id}", history::DIR))?;
        history::read(&self.root, date, id)
    }

    pub fn ui_preferences(&self) -> Result<Option<crate::ui::Preferences>, String> {
        let path = file_rules::safe_file(&self.root, settings::FILE_NAME)?;
        Ok(settings::load(&path)?.and_then(|stored| stored.ui))
    }

    pub fn appearance_settings(&self) -> Result<crate::appearance::Settings, String> {
        let path = file_rules::safe_file(&self.root, settings::FILE_NAME)?;
        let appearance = settings::load(&path)?
            .map(|stored| stored.appearance)
            .unwrap_or_default();
        if crate::appearance::validate(&appearance).is_ok() {
            Ok(appearance)
        } else {
            Ok(crate::appearance::Settings::default())
        }
    }

    pub fn summary_settings(&self) -> Result<summary::Settings, String> {
        let path = file_rules::safe_file(&self.root, settings::FILE_NAME)?;
        Ok(settings::load(&path)?
            .map(|stored| stored.summary)
            .unwrap_or_default())
    }

    pub fn ai_settings(&self) -> Result<Option<ai::Settings>, String> {
        let path = file_rules::safe_file(&self.root, settings::FILE_NAME)?;
        Ok(settings::load(&path)?.map(|stored| stored.ai))
    }

    pub fn load_stored_settings(&self) -> Result<settings::StoredSettings, String> {
        let path = file_rules::safe_file(&self.root, settings::FILE_NAME)?;
        Ok(settings::load(&path)?.unwrap_or_default())
    }

    pub fn excel_settings(&self) -> Result<excel::Settings, String> {
        Ok(self.load_stored_settings()?.excel)
    }

    pub fn git_settings(&self) -> Result<git::Settings, String> {
        Ok(self.load_stored_settings()?.git)
    }

    pub fn editor_settings(&self) -> Result<settings::EditorSettings, String> {
        Ok(self.load_stored_settings()?.editor)
    }

    pub fn database_settings(&self) -> Result<database::Settings, String> {
        Ok(self.load_stored_settings()?.database)
    }

    pub fn effective_file_settings(&self) -> Result<file_rules::Settings, String> {
        let stored = self.load_stored_settings()?;
        settings::effective_file_settings(&self.root, &stored)
    }

    pub fn excel_snapshot(
        &self,
        expected_path: &str,
        dates: &[String],
        content_prompt_override: Option<String>,
        fill_prompt_override: Option<String>,
    ) -> Result<(excel::Snapshot, excel::Settings), String> {
        let mut stored = self.load_stored_settings()?;
        let saved_settings = stored.excel.clone();
        if let Some(prompt) = content_prompt_override {
            stored.excel.content_prompt = prompt;
        }
        if let Some(prompt) = fill_prompt_override {
            stored.excel.prompt = prompt;
        }
        let entries = self.list()?;
        let snapshot = excel::build_snapshot_with_reader(
            &self.root,
            expected_path,
            stored,
            dates,
            &entries,
            |date, format| self.read(date, format),
        )?;
        Ok((snapshot, saved_settings))
    }

    pub fn validate_excel_confirmation(
        &self,
        expected_path: &str,
        provenance: &excel::Provenance,
    ) -> Result<(), String> {
        let stored = self.load_stored_settings().map_err(|error| {
            format!("{error}。请重新生成 Excel 预览")
        })?;
        let entries = self.list()?;
        excel::validate_confirmation_with_reader(
            &self.root,
            stored,
            expected_path,
            provenance,
            &entries,
            |date, format| self.read(date, format),
        )
    }

    pub fn daily_activity(&self, dates: &[String]) -> Result<Vec<DailyActivity>, String> {
        match &self.inner {
            WorkspaceSessionInner::FilePreview(preview) => preview
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .0
                .daily_activity(dates),
            WorkspaceSessionInner::Database(workspace) => workspace
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .daily_activity(dates),
        }
    }

    pub fn list_workspace_files(&self) -> Result<Vec<FileNode>, String> {
        match &self.inner {
            WorkspaceSessionInner::FilePreview(preview) => preview
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .0
                .list_workspace_files(),
            WorkspaceSessionInner::Database(workspace) => workspace
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .list_workspace_files(),
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResultItem>, String> {
        match &self.inner {
            WorkspaceSessionInner::FilePreview(preview) => preview
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .0
                .search(query, limit),
            WorkspaceSessionInner::Database(workspace) => workspace
                .lock()
                .map_err(|_| "工作空间忙，请重试".to_string())?
                .search(query, limit),
        }
    }

    pub fn read_document(&self, relative: &str) -> Result<workspace_document::Document, String> {
        match &self.inner {
            WorkspaceSessionInner::FilePreview(preview) => {
                let guard = preview
                    .lock()
                    .map_err(|_| "工作空间忙，请重试".to_string())?;
                workspace_document::read(&guard.0, relative)
            }
            WorkspaceSessionInner::Database(workspace) => {
                let guard = workspace
                    .lock()
                    .map_err(|_| "工作空间忙，请重试".to_string())?;
                workspace_document::read(&guard, relative)
            }
        }
    }
}

/// Explicitly mutates a non-journal workspace document after verifying `expected_path`.
pub fn save_document_at(
    root: &Path,
    expected_path: &str,
    relative: &str,
    content: &str,
    expected_revision: &str,
) -> Result<workspace_document::Document, String> {
    summary::validate_workspace(root, expected_path)?;
    let workspace = Workspace::open(root)?;
    workspace_document::save(&workspace, relative, content, expected_revision)
}

/// Previews a directory operation (create/rename/delete/restore) after verifying `expected_path`.
pub fn preview_directory_change_at(
    root: &Path,
    expected_path: &str,
    request: &directory_ops::Request,
) -> Result<directory_ops::Plan, String> {
    summary::validate_workspace(root, expected_path)?;
    let workspace = Workspace::open(root)?;
    directory_ops::preview(&workspace, request)
}

/// Applies a confirmed directory operation after verifying `expected_path`.
pub fn apply_directory_change_at(
    root: &Path,
    expected_path: &str,
    request: &directory_ops::Request,
    token: &str,
) -> Result<directory_ops::ChangeResult, String> {
    summary::validate_workspace(root, expected_path)?;
    let mut workspace = Workspace::open(root)?;
    directory_ops::apply(&mut workspace, request, token)
}

/// Lists items in `.trash` after verifying `expected_path`.
pub fn list_workspace_trash_at(
    root: &Path,
    expected_path: &str,
) -> Result<Vec<directory_ops::TrashEntry>, String> {
    summary::validate_workspace(root, expected_path)?;
    let canonical = fs::canonicalize(root).map_err(|_| "工作空间目录不存在".to_string())?;
    directory_ops::list_trash(&canonical)
}

/// Appends Git commit daily logs into the matching workspace journal(s).
pub fn append_git_reports_at(
    root: &Path,
    expected_path: &str,
    reports: Vec<GitReportAppend>,
) -> Result<GitReportAppendResult, String> {
    summary::validate_workspace(root, expected_path)?;
    let mut workspace = Workspace::open(root)?;
    workspace.append_git_reports(reports)
}

/// Previews file rule migration changes before committing settings.
pub fn preview_file_settings_at(
    root: &Path,
    expected_path: &str,
    files: &file_rules::Settings,
) -> Result<file_migration::Plan, String> {
    summary::validate_workspace(root, expected_path)?;
    let stored = settings::load(&root.join(settings::FILE_NAME))?.unwrap_or_default();
    let before = settings::effective_file_settings(root, &stored)?;
    file_migration::preview(root, &before, files)
}

/// Tests database connectivity without mutating workspace settings or creating a DB row.
pub fn test_database_connection_at(
    root: &Path,
    expected_path: &str,
    mut proposed: database::Settings,
) -> Result<(String, usize), String> {
    summary::validate_workspace(root, expected_path)?;
    let workspace = Workspace::open(root)?;
    if let Some(stored) = settings::load(&workspace.root().join(settings::FILE_NAME))? {
        if stored.database != *workspace.database_settings() {
            return Err("数据库配置已被外部修改，请重新打开工作空间后重试".into());
        }
    }
    proposed.enabled = true;
    proposed = database::validate(&proposed)?;
    let saved_id = &workspace.database_settings().workspace_id;
    proposed.workspace_id = if saved_id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        saved_id.clone()
    };
    let connection = database::run_blocking(|| {
        database::Database::connect(workspace.root(), &proposed, &proposed.workspace_id)
    })?;
    let records = database::run_blocking(|| connection.list())?;
    Ok(("连接成功".into(), records.len()))
}

/// Transfers or migrates workspace storage between files and database.
pub fn transfer_workspace_storage_at(
    root: &Path,
    expected_path: &str,
    direction: &str,
    action: &str,
    conflict_policy: &str,
) -> Result<TransferResult, String> {
    summary::validate_workspace(root, expected_path)?;
    let mut workspace = Workspace::open(root)?;
    let settings_path = workspace.root().join(settings::FILE_NAME);
    let mut stored = settings::load(&settings_path)?.unwrap_or_default();
    if stored.database != *workspace.database_settings() {
        return Err("数据库设置已被外部修改，请重新打开工作空间后重试".into());
    }
    let (result, next_settings, _) =
        workspace.prepare_transfer(direction, action, conflict_policy)?;
    if action == "migrate" || next_settings != stored.database {
        stored.database = next_settings;
        settings::save(&settings_path, &stored)?;
    }
    Ok(result)
}

/// Saves full workspace settings atomically (with optional file migration confirmation).
pub fn save_workspace_settings_at(
    root: &Path,
    expected_path: &str,
    stored: settings::StoredSettings,
    confirmation: Option<&file_migration::Confirmation>,
    migrate_files: bool,
) -> Result<settings::StoredSettings, String> {
    summary::validate_workspace(root, expected_path)?;
    let mut workspace = Workspace::open(root)?;
    settings::save_workspace_settings_confirmed(
        &mut workspace,
        expected_path,
        stored,
        confirmation,
        migrate_files,
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{database, settings};
    use tempfile::tempdir;

    #[test]
    fn reads_legacy_workspace_journals_and_history_without_rewriting() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gpui-migration/workspace");
        let copy = tempdir().unwrap();
        let journals = [
            ("20261117", Format::Md, "日报/2026/旧版交接.md"),
            ("20261118", Format::Txt, "2026/20261118.txt"),
            ("20261119", Format::Md, "日报/2026/日报_20261119.md"),
        ];
        let history = [
            ".history/20261119/1795068300000-68a5053c.md",
            ".history/20261119/1795051200000-cd476556.md",
        ];
        let files: Vec<_> = [settings::FILE_NAME, crate::file_rules::CATALOG_FILE]
            .into_iter()
            .chain(journals.iter().map(|(_, _, path)| *path))
            .chain(history)
            .map(|relative| {
                let destination = copy.path().join(relative);
                let bytes = fs::read(fixture.join(relative)).unwrap();
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::write(&destination, &bytes).unwrap();
                (destination, bytes)
            })
            .collect();

        let mut preview = WorkspacePreview::open(copy.path()).unwrap();
        assert_eq!(
            Path::new(&preview.info().path),
            copy.path().canonicalize().unwrap()
        );
        let entries = preview.list().unwrap();
        assert_eq!(entries.len(), journals.len());
        for (entry, (date, format, relative)) in entries.iter().zip(journals) {
            let filename = Path::new(relative).file_name().unwrap().to_str().unwrap();
            assert_eq!(
                (entry.date.as_str(), entry.format, entry.filename.as_str()),
                (date, format, filename)
            );
            let document = preview.read(date, format).unwrap();
            assert_eq!(document.filename, filename);
            assert_eq!(
                document.content,
                fs::read_to_string(fixture.join(relative)).unwrap()
            );
        }
        let versions = preview.list_history(Some("20261119")).unwrap();
        assert_eq!(versions.len(), history.len());
        for (version, relative) in versions.iter().zip(history) {
            let filename = Path::new(relative).file_name().unwrap().to_str().unwrap();
            assert_eq!(version.id, filename);
            assert_eq!(
                preview.read_history(&version.date, &version.id).unwrap(),
                fs::read_to_string(fixture.join(relative)).unwrap()
            );
        }

        for (path, bytes) in files {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }

    fn configure_sqlite(root: &Path, mode: database::Mode) {
        let stored = settings::StoredSettings {
            database: database::Settings {
                enabled: true,
                mode,
                workspace_id: "550e8400-e29b-41d4-a716-446655440000".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        settings::save(&root.join(settings::FILE_NAME), &stored).unwrap();
    }

    #[test]
    fn reads_dual_storage_journal_without_creating_sqlite() {
        let folder = tempdir().unwrap();
        let root = folder.path();
        configure_sqlite(root, database::Mode::Dual);
        fs::create_dir(root.join("2026")).unwrap();
        fs::write(root.join("2026/20261118.txt"), "只读测试\r\n").unwrap();

        let mut preview = WorkspacePreview::open(root).unwrap();
        let entries = preview.list().unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.date, "20261118");
        assert_eq!(entry.format, Format::Txt);
        assert_eq!(
            preview.read(&entry.date, entry.format).unwrap().content,
            "只读测试\r\n"
        );
        assert_eq!(fs::read_dir(root).unwrap().count(), 2);
    }

    #[test]
    fn rejects_database_only_without_creating_sqlite() {
        let folder = tempdir().unwrap();
        let root = folder.path();
        configure_sqlite(root, database::Mode::Database);

        let error = WorkspacePreview::open(root).err().unwrap();
        assert!(error.contains("仅数据库存储"));
        assert_eq!(fs::read_dir(root).unwrap().count(), 1);
    }

    #[test]
    fn workspace_session_opens_reads_and_previews_database_only_sqlite() {
        let folder = tempdir().unwrap();
        let root = folder.path();
        configure_sqlite(root, database::Mode::Database);

        let mut workspace = Workspace::open(root).unwrap();
        workspace.create_with_content("20261119", Format::Md, "# 数据库正文\n摘要内容").unwrap();

        let session = WorkspaceSession::open(root).unwrap();
        assert!(session.is_database_only());
        let entries = session.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].date, "20261119");
        assert_eq!(entries[0].format, Format::Md);
        assert_eq!(
            session.read("20261119", Format::Md).unwrap().content,
            "# 数据库正文\n摘要内容"
        );
        assert_eq!(
            session.preview("20261119", Format::Md).unwrap(),
            "数据库正文"
        );
        assert!(!root.join("2026").exists());
    }

    #[cfg(unix)]
    #[test]
    fn preview_does_not_follow_settings_symlinks() {
        use std::os::unix::fs::symlink;

        let copy = tempdir().unwrap();
        let external = tempdir().unwrap();
        let external_settings = external.path().join(settings::FILE_NAME);
        fs::write(&external_settings, "{}").unwrap();
        symlink(external_settings, copy.path().join(settings::FILE_NAME)).unwrap();

        let error = WorkspacePreview::open(copy.path()).err().unwrap();
        assert!(error.contains("workreport.json 不是普通文件"));
    }

    #[cfg(unix)]
    #[test]
    fn preview_does_not_follow_history_symlinks() {
        use std::os::unix::fs::symlink;

        let copy = tempdir().unwrap();
        let external = tempdir().unwrap();
        let date = "20261119";
        let id = "1795068300000-68a5053c.md";
        let external_day = external.path().join(date);
        fs::create_dir(&external_day).unwrap();
        fs::write(external_day.join(id), "外部文件").unwrap();

        let history_root = copy.path().join(history::DIR);
        let preview = WorkspacePreview::open(copy.path()).unwrap();
        symlink(external.path(), &history_root).unwrap();
        assert!(preview.list_history(None).is_err());
        assert!(preview.list_history(Some(date)).is_err());
        assert!(preview.read_history(date, id).is_err());
        fs::remove_file(&history_root).unwrap();

        fs::create_dir(&history_root).unwrap();
        let history_day = history_root.join(date);
        symlink(&external_day, &history_day).unwrap();
        assert!(preview.list_history(None).unwrap().is_empty());
        assert!(preview.list_history(Some(date)).is_err());
        assert!(preview.read_history(date, id).is_err());
        fs::remove_file(&history_day).unwrap();

        fs::create_dir(&history_day).unwrap();
        symlink(external_day.join(id), history_day.join(id)).unwrap();
        assert!(preview.list_history(Some(date)).unwrap().is_empty());
        assert!(preview.read_history(date, id).is_err());
    }
}
