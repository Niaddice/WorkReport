use chrono::NaiveDate;
use workreport_core::{
    workspace::{Document, Entry},
    workspace_preview::WorkspaceSession,
};

/// A read-only range keeps each document's format and exact source text.
pub(crate) struct WorkspaceRange {
    pub(crate) start: NaiveDate,
    pub(crate) end: NaiveDate,
    pub(crate) documents: Vec<Document>,
    pub(crate) source: String,
}

impl WorkspaceRange {
    pub(crate) fn read(
        reader: &WorkspaceSession,
        entries: &[Entry],
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Self, String> {
        let (start, end) = (start.min(end), start.max(end));
        let start_key = start.format("%Y%m%d").to_string();
        let end_key = end.format("%Y%m%d").to_string();
        let mut selected: Vec<_> = entries
            .iter()
            .filter(|entry| entry.date >= start_key && entry.date <= end_key)
            .collect();
        selected.sort_by(|left, right| right.date.cmp(&left.date));

        let mut documents = Vec::with_capacity(selected.len());
        let mut sections = Vec::with_capacity(selected.len());
        for entry in selected {
            let date = NaiveDate::parse_from_str(&entry.date, "%Y%m%d")
                .map_err(|_| "所选日报的日期无效，请刷新工作空间".to_string())?;
            let label = date.format("%Y-%m-%d");
            let document = reader
                .read(&entry.date, entry.format)
                .map_err(|error| format!("无法读取 {label} 的日报。{error}"))?;
            sections.push(format!("## {label}\n\n{}", document.content));
            documents.push(document);
        }

        Ok(Self {
            start,
            end,
            documents,
            source: sections.join("\n\n---\n\n"),
        })
    }

    pub(crate) fn label(&self) -> String {
        if self.start == self.end {
            self.start.format("%Y-%m-%d").to_string()
        } else {
            format!(
                "{} — {}",
                self.start.format("%Y-%m-%d"),
                self.end.format("%Y-%m-%d")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;
    use workreport_core::{file_rules, settings, workspace::Format};

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y%m%d").unwrap()
    }

    fn add_file(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn reads_reversed_cross_year_range_with_inclusive_bounds_and_exact_sources() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let markdown = "---\r\ntitle: 保留头部\r\n---\r\n\r\n# 新年 📝\r\n";
        let plain = "## 这是纯文本\r\n保留 **字面标记**\r\n";
        add_file(root, "日报/2027/自定义新年.md", markdown);
        add_file(root, "2026/20261231.txt", plain);
        add_file(root, "2027/20270103.md", "范围之外");
        file_rules::register(
            root,
            file_rules::ManagedFile::journal("日报/2027/自定义新年.md", "20270101").unwrap(),
        )
        .unwrap();
        let config = settings::StoredSettings::default();
        settings::save(&root.join(settings::FILE_NAME), &config).unwrap();
        let config_before = fs::read(root.join(settings::FILE_NAME)).unwrap();
        let catalog_before = fs::read(root.join(file_rules::CATALOG_FILE)).unwrap();

        let reader = WorkspaceSession::open(root).unwrap();
        let mut entries = reader.list().unwrap();
        entries.reverse();
        let range =
            WorkspaceRange::read(&reader, &entries, date("20270102"), date("20261231")).unwrap();

        assert_eq!(range.label(), "2026-12-31 — 2027-01-02");
        assert_eq!(range.documents.len(), 2);
        assert_eq!(range.documents[0].date, "20270101");
        assert_eq!(range.documents[0].format, Format::Md);
        assert_eq!(range.documents[0].filename, "自定义新年.md");
        assert_eq!(range.documents[0].content, markdown);
        assert_eq!(range.documents[1].date, "20261231");
        assert_eq!(range.documents[1].format, Format::Txt);
        assert_eq!(range.documents[1].content, plain);
        assert_eq!(
            range.source,
            format!("## 2027-01-01\n\n{markdown}\n\n---\n\n## 2026-12-31\n\n{plain}")
        );
        assert_eq!(
            fs::read(root.join(settings::FILE_NAME)).unwrap(),
            config_before
        );
        assert_eq!(
            fs::read(root.join(file_rules::CATALOG_FILE)).unwrap(),
            catalog_before
        );
        assert_eq!(
            fs::read_to_string(root.join("日报/2027/自定义新年.md")).unwrap(),
            markdown
        );
        assert_eq!(
            fs::read_to_string(root.join("2026/20261231.txt")).unwrap(),
            plain
        );
        assert!(!root.join(".history").exists());
        assert!(!root.join(".workreport-create.lock").exists());
    }

    #[test]
    fn empty_range_keeps_selected_bounds_without_creating_journal_files() {
        let folder = tempfile::tempdir().unwrap();
        let reader = WorkspaceSession::open(folder.path()).unwrap();
        let entries = reader.list().unwrap();
        let range =
            WorkspaceRange::read(&reader, &entries, date("20261120"), date("20261122")).unwrap();

        assert_eq!(range.start, date("20261120"));
        assert_eq!(range.end, date("20261122"));
        assert!(range.documents.is_empty());
        assert!(range.source.is_empty());
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 0);
    }

    #[test]
    fn unavailable_selected_document_returns_its_date_and_preserves_other_files() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        add_file(root, "2026/20261120.txt", "有效日报");
        add_file(root, "2026/20261121.md", "即将移除");
        let reader = WorkspaceSession::open(root).unwrap();
        let entries = reader.list().unwrap();
        fs::remove_file(root.join("2026/20261121.md")).unwrap();

        let error = WorkspaceRange::read(&reader, &entries, date("20261120"), date("20261121"))
            .err()
            .unwrap();

        assert!(error.contains("2026-11-21"));
        assert_eq!(
            fs::read_to_string(root.join("2026/20261120.txt")).unwrap(),
            "有效日报"
        );
        let range =
            WorkspaceRange::read(&reader, &entries, date("20261120"), date("20261120")).unwrap();
        assert_eq!(range.label(), "2026-11-20");
        assert_eq!(range.documents.len(), 1);
    }
}
