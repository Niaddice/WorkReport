use std::collections::{BTreeMap, BTreeSet};

use workreport_core::{workspace::Entry, workspace_preview::WorkspaceSession};

/// Optional, per-day summaries. A failed day does not discard other previews.
#[derive(Debug, Default)]
pub(crate) struct CalendarPreviews {
    pub(crate) values: BTreeMap<String, Result<String, String>>,
}

impl CalendarPreviews {
    pub(crate) fn read(reader: &WorkspaceSession, entries: &[Entry], dates: &[String]) -> Self {
        let requested: BTreeSet<_> = dates.iter().map(String::as_str).collect();
        let values = entries
            .iter()
            .filter(|entry| requested.contains(entry.date.as_str()))
            .map(|entry| {
                (
                    entry.date.clone(),
                    reader.preview(&entry.date, entry.format),
                )
            })
            .collect();
        Self { values }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;
    use workreport_core::{file_rules, settings};

    fn add_file(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn keys(dates: &[&str]) -> Vec<String> {
        dates.iter().map(|date| (*date).to_owned()).collect()
    }

    #[test]
    fn requested_previews_keep_custom_paths_and_the_legacy_summary_rules_without_writes() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let custom = "日报/2026/旧版交接.md";
        let markdown = "# 迁移日报\r\n\r\n> - **完成** 中文工作 📝\r\n第二行\r\n";
        let plain = "每日工作日报\r\n## *TXT 标记仍保留*\r\n未取第二行\r\n";
        add_file(root, custom, markdown);
        add_file(root, "2026/20261118.txt", plain);
        add_file(root, "2026/20261119.md", "未请求的日期\n");
        file_rules::register(
            root,
            file_rules::ManagedFile::journal(custom, "20261117").unwrap(),
        )
        .unwrap();
        settings::save(
            &root.join(settings::FILE_NAME),
            &settings::StoredSettings::default(),
        )
        .unwrap();
        let config = fs::read(root.join(settings::FILE_NAME)).unwrap();
        let catalog = fs::read(root.join(file_rules::CATALOG_FILE)).unwrap();
        let reader = WorkspaceSession::open(root).unwrap();
        let entries = reader.list().unwrap();

        let previews = CalendarPreviews::read(
            &reader,
            &entries,
            &keys(&["20261117", "20261118", "20261117", "20261120"]),
        );
        assert_eq!(previews.values.len(), 2);
        assert_eq!(previews.values["20261117"], Ok("完成** 中文工作 📝".into()));
        // The legacy summary trims Markdown-like prefixes for both formats;
        // internal markup stays literal and is never rendered as Markdown.
        assert_eq!(previews.values["20261118"], Ok("TXT 标记仍保留*".into()));
        assert!(!previews.values.contains_key("20261119"));
        assert!(!previews.values.contains_key("20261120"));
        assert_eq!(fs::read(root.join(custom)).unwrap(), markdown.as_bytes());
        assert_eq!(
            fs::read(root.join("2026/20261118.txt")).unwrap(),
            plain.as_bytes()
        );
        assert_eq!(fs::read(root.join(settings::FILE_NAME)).unwrap(), config);
        assert_eq!(
            fs::read(root.join(file_rules::CATALOG_FILE)).unwrap(),
            catalog
        );
        assert!(!root.join(".history").exists());
        assert!(!root.join(".workreport-create.lock").exists());
    }

    #[test]
    fn failed_or_removed_files_do_not_prevent_other_days_from_returning() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        add_file(root, "2026/20261117.md", "有效内容 📝\n");
        add_file(root, "2026/20261118.txt", b"\xff\xfe\x00\x01");
        add_file(root, "2026/20261119.md", "待删除\n");
        let reader = WorkspaceSession::open(root).unwrap();
        let entries = reader.list().unwrap();
        fs::remove_file(root.join("2026/20261119.md")).unwrap();

        let previews = CalendarPreviews::read(
            &reader,
            &entries,
            &keys(&["20261117", "20261118", "20261119"]),
        );
        assert_eq!(previews.values["20261117"], Ok("有效内容 📝".into()));
        assert_eq!(
            previews.values["20261118"],
            Ok("无法预览（文件不是 UTF-8 编码）".into())
        );
        assert_eq!(previews.values["20261119"], Ok("无法读取预览".into()));
        assert_eq!(
            fs::read_to_string(root.join("2026/20261117.md")).unwrap(),
            "有效内容 📝\n"
        );
    }

    #[test]
    fn unicode_summary_uses_eighty_characters_and_a_bounded_utf8_prefix() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let long = "中文📝".repeat(40);
        add_file(root, "2026/20261117.txt", format!("# 日报\n\n{long}\n"));
        let mut split_prefix = vec![b'a'; 16 * 1024 - 1];
        split_prefix.extend_from_slice("你".as_bytes());
        add_file(root, "2026/20261118.md", &split_prefix);
        add_file(root, "2026/20261119.txt", "# 日报\n\n \r\n");
        let reader = WorkspaceSession::open(root).unwrap();
        let entries = reader.list().unwrap();

        let previews = CalendarPreviews::read(
            &reader,
            &entries,
            &keys(&["20261117", "20261118", "20261119"]),
        );
        assert_eq!(
            previews.values["20261117"],
            Ok(long.chars().take(80).collect())
        );
        assert_eq!(previews.values["20261118"], Ok("a".repeat(80)));
        assert_eq!(previews.values["20261119"], Ok("还没有内容".into()));
        assert_eq!(
            fs::read(root.join("2026/20261118.md")).unwrap(),
            split_prefix
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_replaced_symlink_is_rejected_independently_of_other_days() {
        use std::os::unix::fs::symlink;

        let folder = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let root = folder.path();
        add_file(root, "2026/20261117.md", "有效日报内容\n");
        add_file(root, "2026/20261118.txt", "原本在工作空间内\n");
        let reader = WorkspaceSession::open(root).unwrap();
        let entries = reader.list().unwrap();
        let replaced = root.join("2026/20261118.txt");
        fs::remove_file(&replaced).unwrap();
        fs::write(external.path().join("private.txt"), "外部正文").unwrap();
        symlink(external.path().join("private.txt"), &replaced).unwrap();

        let previews = CalendarPreviews::read(&reader, &entries, &keys(&["20261117", "20261118"]));
        assert_eq!(previews.values["20261117"], Ok("有效日报内容".into()));
        assert!(previews.values["20261118"].is_err());
        assert_eq!(
            fs::read_to_string(external.path().join("private.txt")).unwrap(),
            "外部正文"
        );
    }
}
