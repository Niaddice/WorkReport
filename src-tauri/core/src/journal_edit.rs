use std::path::{Path, PathBuf};

use crate::{
    file_rules, history,
    workspace::{Document, Format, Workspace, FILE_REVISION_CONFLICT},
};

/// An editing session pinned to a local journal path and storage configuration.
pub struct JournalEdit {
    workspace: Workspace,
    path: PathBuf,
    date: String,
    format: Format,
    revision: String,
    new_draft: bool,
}

impl JournalEdit {
    pub fn begin(
        root: impl AsRef<Path>,
        date: &str,
        format: Format,
        expected_revision: &str,
    ) -> Result<(Self, Document), String> {
        let workspace = Workspace::open_file_preview(root.as_ref())?;
        let path = workspace.entry_path(date, format)?;
        let document = workspace.read_file_at(date, format, &path)?;
        if document.revision != expected_revision {
            return Err(FILE_REVISION_CONFLICT.into());
        }
        let session = Self {
            workspace,
            path,
            date: date.into(),
            format,
            revision: document.revision.clone(),
            new_draft: false,
        };
        Ok((session, document))
    }

    /// Prepare an empty journal without creating files, history or a database.
    pub fn begin_new(
        root: impl AsRef<Path>,
        date: &str,
        format: Format,
    ) -> Result<(Self, Document), String> {
        let workspace = Workspace::open_file_preview(root.as_ref())?;
        let path = workspace.entry_path(date, format)?;
        if workspace
            .managed_journal_files()?
            .iter()
            .any(|record| record.date == date)
        {
            return Err("这一天已有日报，请刷新并打开现有日报。".into());
        }
        let session = Self {
            workspace,
            path,
            date: date.into(),
            format,
            revision: String::new(),
            new_draft: true,
        };
        let document = session.empty_document("")?;
        Ok((session, document))
    }

    pub fn save(&mut self, content: &str) -> Result<Document, String> {
        self.save_with_mode(content, false)
    }

    pub fn save_automatic(&mut self, content: &str) -> Result<Document, String> {
        self.save_with_mode(content, true)
    }

    /// Record the last saved revision without rewriting the journal or its backup.
    pub fn checkpoint_history(&self) -> Result<(), String> {
        self.validate_scope()?;
        if self.new_draft {
            return Ok(());
        }
        let current = self
            .workspace
            .read_file_at(&self.date, self.format, &self.path)?;
        if current.revision != self.revision {
            return Err(FILE_REVISION_CONFLICT.into());
        }
        history::record(
            self.workspace.root(),
            &self.date,
            self.format,
            &current.content,
            self.workspace.database_settings().history_limit_bytes,
        )
    }

    fn save_with_mode(&mut self, content: &str, automatic: bool) -> Result<Document, String> {
        self.validate_scope()?;
        let saved = if self.new_draft {
            if content.trim().is_empty() {
                return self.empty_document(content);
            }
            self.workspace.create_local_at(
                &self.date,
                self.format,
                content,
                automatic,
                &self.path,
            )?
        } else {
            self.workspace.save_local_at(
                &self.date,
                self.format,
                content,
                &self.revision,
                automatic,
                &self.path,
            )?
        };
        self.revision.clone_from(&saved.revision);
        self.new_draft = false;
        Ok(saved)
    }

    fn empty_document(&self, content: &str) -> Result<Document, String> {
        Ok(Document {
            date: self.date.clone(),
            filename: self
                .path
                .file_name()
                .ok_or("日报文件名无效")?
                .to_string_lossy()
                .into_owned(),
            format: self.format,
            content: content.into(),
            revision: String::new(),
            backup_warning: None,
        })
    }

    fn validate_scope(&self) -> Result<(), String> {
        // Recheck the current mapping even when the writer still has an indexed path.
        let current = Workspace::open_file_preview(self.workspace.root())?;
        if current.root() != self.workspace.root()
            || current.journal_file_rule() != self.workspace.journal_file_rule()
            || current.database_settings() != self.workspace.database_settings()
            || current.entry_path(&self.date, self.format)? != self.path
        {
            return Err(
                "工作空间的存储配置或日报路径已变化。请先保留草稿，再重新打开日报。".into(),
            );
        }
        if self.new_draft
            && current
                .managed_journal_files()?
                .iter()
                .any(|record| record.date == self.date)
        {
            return Err(FILE_REVISION_CONFLICT.into());
        }
        file_rules::safe_file(current.root(), &format!("{}/{}", history::DIR, self.date))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{database, settings, workspace_preview::WorkspacePreview};
    use tempfile::tempdir;

    const DATE: &str = "20261119";
    const ORIGINAL: &str = "原始日报\r\n第二行\r\n";

    fn existing_file(root: &Path, format: Format) -> PathBuf {
        let year = root.join("2026");
        fs::create_dir(&year).unwrap();
        let file = year.join(format!("{DATE}.{}", format.extension()));
        fs::write(&file, ORIGINAL).unwrap();
        file
    }

    fn begin(root: &Path, format: Format) -> (JournalEdit, Document) {
        let preview = WorkspacePreview::open(root).unwrap();
        let document = preview.read(DATE, format).unwrap();
        JournalEdit::begin(root, DATE, format, &document.revision).unwrap()
    }

    #[test]
    fn new_journals_stay_empty_until_first_nonblank_save_and_keep_custom_names() {
        for format in [Format::Md, Format::Txt] {
            let directory = tempdir().unwrap();
            let root = directory.path();
            let mut files = file_rules::Settings::default();
            files.journal.directory = "日报".into();
            files.journal.filename_template = "工作_{date}".into();
            let stored = settings::StoredSettings {
                files: Some(files),
                ..Default::default()
            };
            settings::save(&root.join(settings::FILE_NAME), &stored).unwrap();
            let config = fs::read(root.join(settings::FILE_NAME)).unwrap();
            let (mut edit, empty) = JournalEdit::begin_new(root, DATE, format).unwrap();
            assert!(empty.content.is_empty());
            assert!(empty.revision.is_empty());
            assert_eq!(
                empty.filename,
                format!("工作_{DATE}.{}", format.extension())
            );

            assert!(edit.save_automatic(" \r\n\t").unwrap().revision.is_empty());
            edit.checkpoint_history().unwrap();
            assert_eq!(fs::read_dir(root).unwrap().count(), 1);

            let content = "中文新日报 📝\r\n保留 **标记**\r\n";
            let saved = edit.save_automatic(content).unwrap();
            assert!(!saved.revision.is_empty());
            let path = root.join(format!("日报/2026/工作_{DATE}.{}", format.extension()));
            assert_eq!(fs::read_to_string(&path).unwrap(), content);
            assert_eq!(fs::read(root.join(settings::FILE_NAME)).unwrap(), config);
            let mut reopened = Workspace::open(root).unwrap();
            assert_eq!(reopened.list().unwrap().len(), 1);
            assert_eq!(
                reopened.read(DATE, format).unwrap().revision,
                saved.revision
            );
            assert_eq!(history::list(root, Some(DATE)).unwrap().len(), 1);

            let updated = edit.save("后续编辑\n").unwrap();
            assert_ne!(updated.revision, saved.revision);
            assert_eq!(fs::read_to_string(&path).unwrap(), updated.content);
        }
    }

    #[test]
    fn new_drafts_do_not_initialize_a_dual_storage_database_for_blank_content() {
        let directory = tempdir().unwrap();
        let root = directory.path();
        let stored = settings::StoredSettings {
            database: database::Settings {
                enabled: true,
                workspace_id: "550e8400-e29b-41d4-a716-446655440000".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        settings::save(&root.join(settings::FILE_NAME), &stored).unwrap();
        let (mut edit, _) = JournalEdit::begin_new(root, DATE, Format::Txt).unwrap();
        assert!(edit.save_automatic(" \n").unwrap().revision.is_empty());
        assert_eq!(fs::read_dir(root).unwrap().count(), 1);
        assert!(!root.join(".workreport-backup-pending").exists());
    }

    #[test]
    fn new_journal_refuses_existing_dates_in_either_format() {
        for existing_format in [Format::Md, Format::Txt] {
            let directory = tempdir().unwrap();
            let root = directory.path();
            let original = existing_file(root, existing_format);
            for requested_format in [Format::Md, Format::Txt] {
                assert!(JournalEdit::begin_new(root, DATE, requested_format).is_err());
                assert_eq!(fs::read_to_string(&original).unwrap(), ORIGINAL);
                assert_eq!(fs::read_dir(root.join("2026")).unwrap().count(), 1);
                assert!(!root.join(history::DIR).exists());
            }
        }
    }

    #[test]
    fn new_journal_refuses_an_external_file_created_while_the_draft_was_open() {
        for external_format in [Format::Md, Format::Txt] {
            let directory = tempdir().unwrap();
            let root = directory.path();
            let (mut edit, _) = JournalEdit::begin_new(root, DATE, Format::Md).unwrap();
            let original = existing_file(root, external_format);
            assert!(edit.save_automatic("不能覆盖的草稿\n").is_err());
            assert_eq!(fs::read_to_string(&original).unwrap(), ORIGINAL);
            assert_eq!(fs::read_dir(root.join("2026")).unwrap().count(), 1);
            assert!(edit.revision.is_empty());
            assert!(!root.join(history::DIR).exists());
            assert!(!root.join(file_rules::CATALOG_FILE).exists());
        }
    }

    #[test]
    fn new_journal_retries_an_identity_registered_before_interrupted_publication() {
        let directory = tempdir().unwrap();
        let root = directory.path();
        let relative = format!("2026/{DATE}.md");
        let record = file_rules::ManagedFile::journal(&relative, DATE).unwrap();
        file_rules::register(root, record).unwrap();
        assert!(!root.join(&relative).exists());

        let (mut edit, empty) = JournalEdit::begin_new(root, DATE, Format::Md).unwrap();
        assert!(empty.revision.is_empty());
        let saved = edit.save_automatic("# 重试后的日报\n").unwrap();

        assert_eq!(
            fs::read_to_string(root.join(&relative)).unwrap(),
            saved.content
        );
        let catalog = file_rules::catalog(root).unwrap();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].relative_path, relative);
        let mut reopened = WorkspacePreview::open(root).unwrap();
        assert_eq!(reopened.list().unwrap().len(), 1);
        assert_eq!(
            reopened.read(DATE, Format::Md).unwrap().revision,
            saved.revision
        );
        assert_eq!(reopened.list_history(Some(DATE)).unwrap().len(), 1);
    }

    #[test]
    fn competing_new_sessions_reject_the_later_save_without_adopting_the_winner() {
        for first_format in [Format::Md, Format::Txt] {
            for second_format in [Format::Md, Format::Txt] {
                let directory = tempdir().unwrap();
                let root = directory.path();
                let (mut first, _) = JournalEdit::begin_new(root, DATE, first_format).unwrap();
                let (mut second, _) = JournalEdit::begin_new(root, DATE, second_format).unwrap();
                let saved = first.save("首先保存的日报\n").unwrap();

                assert!(second.save_automatic("稍后提交的草稿\n").is_err());
                assert!(second.new_draft);
                assert!(second.revision.is_empty());
                assert_eq!(fs::read_dir(root.join("2026")).unwrap().count(), 1);
                let mut reopened = WorkspacePreview::open(root).unwrap();
                let entries = reopened.list().unwrap();
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].format, first_format);
                assert_eq!(
                    reopened.read(DATE, first_format).unwrap().content,
                    saved.content
                );
                let versions = reopened.list_history(Some(DATE)).unwrap();
                assert_eq!(versions.len(), 1);
                assert_eq!(
                    reopened.read_history(DATE, &versions[0].id).unwrap(),
                    saved.content
                );
                assert_eq!(file_rules::catalog(root).unwrap().len(), 1);
            }
        }
    }

    #[test]
    fn simultaneous_markdown_and_txt_creation_publish_only_one_journal() {
        use std::sync::{Arc, Barrier};

        for _ in 0..8 {
            let directory = tempdir().unwrap();
            let root = directory.path();
            let (mut markdown, _) = JournalEdit::begin_new(root, DATE, Format::Md).unwrap();
            let (mut text, _) = JournalEdit::begin_new(root, DATE, Format::Txt).unwrap();
            let barrier = Arc::new(Barrier::new(2));
            let outcomes = std::thread::scope(|scope| {
                let markdown_barrier = barrier.clone();
                let first = scope.spawn(move || {
                    markdown_barrier.wait();
                    markdown.save("# Markdown 创建请求\n")
                });
                let second = scope.spawn(move || {
                    barrier.wait();
                    text.save("TXT 创建请求\n")
                });
                [first.join().unwrap(), second.join().unwrap()]
            });

            assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
            let winner = outcomes.into_iter().find_map(Result::ok).unwrap();
            let mut reopened = WorkspacePreview::open(root).unwrap();
            let entries = reopened.list().unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].format, winner.format);
            assert_eq!(
                reopened.read(DATE, winner.format).unwrap().content,
                winner.content
            );
            assert_eq!(fs::read_dir(root.join("2026")).unwrap().count(), 1);
            assert_eq!(file_rules::catalog(root).unwrap().len(), 1);
            assert_eq!(history::list(root, Some(DATE)).unwrap().len(), 1);
        }
    }

    #[test]
    fn round_trips_markdown_and_txt_with_history_and_successive_revisions() {
        for (format, saved_body) in [
            (Format::Md, "# 中文日报\n\n**进度**\n"),
            (Format::Txt, "保留 CRLF\r\n中文更新\r\n"),
        ] {
            let directory = tempdir().unwrap();
            let root = directory.path();
            let file = existing_file(root, format);
            let (mut edit, document) = begin(root, format);
            assert_eq!(document.content, ORIGINAL);
            assert!(!root.join(history::DIR).exists());
            assert_eq!(fs::read(&file).unwrap(), ORIGINAL.as_bytes());

            let first = edit.save(saved_body).unwrap();
            assert_ne!(first.revision, document.revision);
            let second = edit.save(&format!("{saved_body}再次编辑\n")).unwrap();
            assert_ne!(second.revision, first.revision);
            drop(edit);

            let restored = WorkspacePreview::open(root).unwrap();
            assert_eq!(restored.read(DATE, format).unwrap().content, second.content);
            assert_eq!(fs::read(&file).unwrap(), second.content.as_bytes());
            let versions = restored.list_history(Some(DATE)).unwrap();
            assert_eq!(versions.len(), 2);
            assert_eq!(
                restored.read_history(DATE, &versions[0].id).unwrap(),
                second.content
            );
        }
    }

    #[test]
    fn automatic_saves_update_the_file_and_checkpoint_only_the_committed_revision() {
        let directory = tempdir().unwrap();
        let root = directory.path();
        let file = existing_file(root, Format::Md);
        let (mut edit, _) = begin(root, Format::Md);

        let first = edit.save_automatic("# 自动保存一\n").unwrap();
        let latest = edit.save_automatic("# 自动保存二 📝\n").unwrap();
        assert_ne!(first.revision, latest.revision);
        assert_eq!(fs::read_to_string(&file).unwrap(), latest.content);
        let reopened = WorkspacePreview::open(root).unwrap();
        assert_eq!(
            reopened.read(DATE, Format::Md).unwrap().revision,
            latest.revision
        );
        let versions = reopened.list_history(Some(DATE)).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(
            reopened.read_history(DATE, &versions[0].id).unwrap(),
            first.content
        );

        edit.checkpoint_history().unwrap();
        edit.checkpoint_history().unwrap();
        let versions = reopened.list_history(Some(DATE)).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(
            reopened.read_history(DATE, &versions[0].id).unwrap(),
            latest.content
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), latest.content);

        fs::write(&file, "其他程序写入").unwrap();
        assert_eq!(
            edit.checkpoint_history().unwrap_err(),
            FILE_REVISION_CONFLICT
        );
        assert_eq!(reopened.list_history(Some(DATE)).unwrap(), versions);
        assert_eq!(fs::read_to_string(&file).unwrap(), "其他程序写入");
    }

    #[test]
    fn a_stale_preview_cannot_start_editing() {
        let directory = tempdir().unwrap();
        let root = directory.path();
        let file = existing_file(root, Format::Txt);
        let preview = WorkspacePreview::open(root).unwrap();
        let document = preview.read(DATE, Format::Txt).unwrap();
        fs::write(&file, "外部更新").unwrap();
        let error = JournalEdit::begin(root, DATE, Format::Txt, &document.revision)
            .err()
            .unwrap();
        assert_eq!(error, FILE_REVISION_CONFLICT);
        assert_eq!(fs::read_to_string(&file).unwrap(), "外部更新");
        assert!(!root.join(history::DIR).exists());
    }

    #[test]
    fn an_external_change_never_advances_the_revision_or_marks_a_backup_pending() {
        let directory = tempdir().unwrap();
        let root = directory.path();
        let file = existing_file(root, Format::Txt);
        let stored = settings::StoredSettings {
            database: database::Settings {
                enabled: true,
                workspace_id: "550e8400-e29b-41d4-a716-446655440000".into(),
                sqlite_path: "blocked/store.sqlite".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        settings::save(&root.join(settings::FILE_NAME), &stored).unwrap();
        fs::write(root.join("blocked"), "数据库目录离线").unwrap();

        let (mut edit, preview) = begin(root, Format::Txt);
        fs::write(&file, "外部更新").unwrap();
        assert_eq!(edit.save("本地草稿").unwrap_err(), FILE_REVISION_CONFLICT);
        assert_eq!(edit.revision, preview.revision);
        assert_eq!(fs::read_to_string(&file).unwrap(), "外部更新");
        assert!(!root.join(".workreport-backup-pending").exists());
        assert!(!root.join(history::DIR).exists());

        fs::write(&file, ORIGINAL).unwrap();
        let saved = edit.save("本地草稿").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "本地草稿");
        assert!(saved.backup_warning.is_some());
        assert!(root.join(".workreport-backup-pending").is_file());
    }

    #[test]
    fn dual_storage_only_connects_on_save_and_backs_up_the_committed_revision() {
        let directory = tempdir().unwrap();
        let root = directory.path();
        let file = existing_file(root, Format::Txt);
        let stored = settings::StoredSettings {
            database: database::Settings {
                enabled: true,
                workspace_id: "550e8400-e29b-41d4-a716-446655440000".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        settings::save(&root.join(settings::FILE_NAME), &stored).unwrap();
        let sqlite = root.join(&stored.database.sqlite_path);

        let (mut edit, _) = begin(root, Format::Txt);
        assert!(!sqlite.exists());

        let saved = edit.save("保存在文件与数据库\n").unwrap();
        assert_eq!(fs::read_to_string(file).unwrap(), saved.content);
        assert!(saved.backup_warning.is_none());
        assert!(!root.join(".workreport-backup-pending").exists());
        assert!(sqlite.is_file());
        let db = database::run_blocking(|| {
            database::Database::connect(root, &stored.database, &stored.database.workspace_id)
        })
        .unwrap();
        let backup = database::run_blocking(|| db.read(DATE)).unwrap().unwrap();
        assert_eq!(backup.content, saved.content);
        assert_eq!(backup.revision, saved.revision);
    }

    #[test]
    fn edits_reject_a_catalog_remap_even_when_the_new_file_has_the_same_bytes() {
        let directory = tempdir().unwrap();
        let root = directory.path();
        let file = existing_file(root, Format::Md);
        let (mut edit, _) = begin(root, Format::Md);

        let renamed = "2026/自定义文件名.md";
        fs::rename(&file, root.join(renamed)).unwrap();
        let record = file_rules::ManagedFile::journal(renamed, DATE).unwrap();
        file_rules::write_catalog(root, &[record]).unwrap();

        assert!(edit.save("不可覆盖").unwrap_err().contains("路径已变化"));
        assert_eq!(fs::read_to_string(root.join(renamed)).unwrap(), ORIGINAL);
        assert!(!file.exists());
        assert!(!root.join(history::DIR).exists());
    }

    #[test]
    fn edits_reject_a_switch_to_database_only_without_creating_a_database() {
        let directory = tempdir().unwrap();
        let root = directory.path();
        let file = existing_file(root, Format::Md);
        let (mut edit, _) = begin(root, Format::Md);

        let stored = settings::StoredSettings {
            database: database::Settings {
                enabled: true,
                mode: database::Mode::Database,
                workspace_id: "550e8400-e29b-41d4-a716-446655440000".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        settings::save(&root.join(settings::FILE_NAME), &stored).unwrap();

        assert!(edit.save("不可覆盖").unwrap_err().contains("仅数据库存储"));
        assert_eq!(fs::read_to_string(&file).unwrap(), ORIGINAL);
        assert!(!root.join("workreport.sqlite").exists());
        assert!(!root.join(history::DIR).exists());
    }

    #[cfg(unix)]
    #[test]
    fn edits_reject_history_directory_links_before_any_write() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let root = directory.path();
        let file = existing_file(root, Format::Md);
        let outside = tempdir().unwrap();
        let (mut edit, _) = begin(root, Format::Md);

        symlink(outside.path(), root.join(history::DIR)).unwrap();

        assert!(edit.save("不可覆盖").unwrap_err().contains("符号链接"));
        assert_eq!(fs::read_to_string(&file).unwrap(), ORIGINAL);
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
