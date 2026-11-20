use std::path::Path;

use crate::{
    file_rules,
    history::{self, Version},
    workspace::{Document, Entry, Format, Workspace, WorkspaceInfo},
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
