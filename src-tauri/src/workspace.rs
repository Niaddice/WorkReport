use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::PathBuf};
use tempfile::NamedTempFile;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Md,
    Txt,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Md => "md",
            Self::Txt => "txt",
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub path: String,
    pub name: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub date: String,
    pub filename: String,
    pub format: Format,
    pub preview: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub date: String,
    pub filename: String,
    pub format: Format,
    pub content: String,
    pub revision: String,
}
pub struct Workspace {
    root: PathBuf,
}
fn io_error(error: std::io::Error) -> String {
    format!("无法访问工作空间文件：{error}")
}
fn valid_date(date: &str) -> Result<NaiveDate, String> {
    if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
        return Err("日报日期必须为 yyyymmdd 格式".into());
    }
    NaiveDate::parse_from_str(date, "%Y%m%d").map_err(|_| "日报日期无效".into())
}
fn revision(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}
impl Workspace {
    pub fn open(path: &str) -> Result<Self, String> {
        let root = fs::canonicalize(path).map_err(io_error)?;
        if !root.is_dir() {
            return Err("请选择一个文件夹作为工作空间".into());
        }
        NamedTempFile::new_in(&root).map_err(io_error)?;
        Ok(Self { root })
    }
    pub fn info(&self) -> WorkspaceInfo {
        WorkspaceInfo {
            path: self.root.to_string_lossy().into_owned(),
            name: self
                .root
                .file_name()
                .unwrap_or(self.root.as_os_str())
                .to_string_lossy()
                .into_owned(),
        }
    }
    fn entry_path(&self, date: &str, format: Format) -> Result<PathBuf, String> {
        valid_date(date)?;
        let path = self.root.join(format!("{date}.{}", format.extension()));
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
                return Err("日报文件必须是工作空间内的普通文件".into())
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(io_error(error))
            }
            _ => {}
        }
        Ok(path)
    }
    pub fn list(&self) -> Result<Vec<Entry>, String> {
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.root).map_err(io_error)? {
            let item = item.map_err(io_error)?;
            if !item.file_type().map_err(io_error)?.is_file() {
                continue;
            }
            let filename = item.file_name().to_string_lossy().into_owned();
            let (date, format) = if let Some(date) = filename.strip_suffix(".md") {
                (date, Format::Md)
            } else if let Some(date) = filename.strip_suffix(".txt") {
                (date, Format::Txt)
            } else {
                continue;
            };
            if valid_date(date).is_err() {
                continue;
            }
            let content = fs::read_to_string(item.path()).map_err(io_error)?;
            let preview = content
                .lines()
                .map(|line| line.trim().trim_start_matches(['#', '-', '*', ' ', '>']))
                .find(|line| !line.is_empty() && !line.ends_with("日报"))
                .unwrap_or("还没有内容")
                .chars()
                .take(80)
                .collect();
            entries.push(Entry {
                date: date.into(),
                filename,
                format,
                preview,
            });
        }
        entries.sort_by(|a, b| a.filename.cmp(&b.filename));
        Ok(entries)
    }
    pub fn read(&self, date: &str, format: Format) -> Result<Document, String> {
        let content = fs::read_to_string(self.entry_path(date, format)?).map_err(io_error)?;
        Ok(Document {
            date: date.into(),
            filename: format!("{date}.{}", format.extension()),
            format,
            revision: revision(&content),
            content,
        })
    }
    pub fn create(&self, date: &str, format: Format) -> Result<Document, String> {
        let parsed = valid_date(date)?;
        let path = self.entry_path(date, format)?;
        if path.exists() {
            return self.read(date, format);
        }
        let heading = if format == Format::Md { "# " } else { "" };
        let content = format!("{heading}{} 日报\n\n", parsed.format("%Y年%m月%d日"));
        let mut temp = NamedTempFile::new_in(&self.root).map_err(io_error)?;
        temp.write_all(content.as_bytes()).map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        match temp.persist_noclobber(path) {
            Ok(_) => self.read(date, format),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                self.read(date, format)
            }
            Err(error) => Err(io_error(error.error)),
        }
    }
    pub fn save(
        &self,
        date: &str,
        format: Format,
        content: &str,
        expected_revision: &str,
    ) -> Result<Document, String> {
        let path = self.entry_path(date, format)?;
        let current = self.read(date, format)?;
        if current.revision != expected_revision {
            return Err(
                "文件已被其他程序修改。请先复制当前草稿，再重新载入磁盘内容，以免覆盖修改。".into(),
            );
        }
        let mut temp = NamedTempFile::new_in(&self.root).map_err(io_error)?;
        temp.write_all(content.as_bytes()).map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        temp.persist(path).map_err(|error| io_error(error.error))?;
        self.read(date, format)
    }
    pub fn switch_format(
        &self,
        date: &str,
        from: Format,
        to: Format,
        expected_revision: &str,
    ) -> Result<Document, String> {
        let current = self.read(date, from)?;
        if current.revision != expected_revision {
            return Err("文件已被其他程序修改，请重新载入后再切换格式".into());
        }
        if from == to {
            return Ok(current);
        }
        let target = self.entry_path(date, to)?;
        if target.exists() {
            return Err(format!(
                "{}.{} 已存在，未覆盖文件。请先在工作空间中处理同名日报。",
                date,
                to.extension()
            ));
        }
        // Write the new name without overwriting. Only remove the source after durable creation.
        let mut temp = NamedTempFile::new_in(&self.root).map_err(io_error)?;
        temp.write_all(current.content.as_bytes())
            .map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        temp.persist_noclobber(&target)
            .map_err(|error| io_error(error.error))?;
        let source = self.entry_path(date, from)?;
        if self.read(date, from)?.revision != expected_revision {
            return Err("原文件在切换时被外部修改，两份文件均已保留，请刷新后检查".into());
        }
        fs::remove_file(source).map_err(|_| {
            "新格式文件已创建，原文件未能移除。两份内容均已保留，请刷新后检查。".to_string()
        })?;
        self.read(date, to)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    fn setup() -> (TempDir, Workspace) {
        let dir = TempDir::new().unwrap();
        let workspace = Workspace::open(dir.path().to_str().unwrap()).unwrap();
        (dir, workspace)
    }
    #[test]
    fn create_edit_reopen_and_index_both_formats() {
        let (dir, workspace) = setup();
        for format in [Format::Md, Format::Txt] {
            let doc = workspace.create("20261018", format).unwrap();
            let saved = workspace
                .save(
                    "20261018",
                    format,
                    "# 日报\n\n完成工作空间功能\n",
                    &doc.revision,
                )
                .unwrap();
            assert_eq!(
                fs::read_to_string(dir.path().join(&saved.filename)).unwrap(),
                saved.content
            );
            assert_eq!(
                workspace.create("20261018", format).unwrap().content,
                saved.content
            );
        }
        let reopened = Workspace::open(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(reopened.list().unwrap().len(), 2);
        assert_eq!(reopened.list().unwrap()[0].preview, "完成工作空间功能");
    }
    #[test]
    fn switching_format_preserves_text_and_removes_previous_name() {
        let (dir, workspace) = setup();
        let doc = workspace.create("20261018", Format::Md).unwrap();
        let converted = workspace
            .switch_format("20261018", Format::Md, Format::Txt, &doc.revision)
            .unwrap();
        assert_eq!(converted.content, doc.content);
        assert!(!dir.path().join("20261018.md").exists());
        assert!(dir.path().join("20261018.txt").exists());
        assert_eq!(
            workspace
                .switch_format("20261018", Format::Txt, Format::Md, &converted.revision)
                .unwrap()
                .content,
            doc.content
        );
    }
    #[test]
    fn switching_never_overwrites_existing_target_or_external_edits() {
        let (_dir, workspace) = setup();
        let doc = workspace.create("20261018", Format::Md).unwrap();
        let txt = workspace.create("20261018", Format::Txt).unwrap();
        assert!(workspace
            .switch_format("20261018", Format::Md, Format::Txt, &doc.revision)
            .is_err());
        assert_eq!(
            workspace.read("20261018", Format::Txt).unwrap().content,
            txt.content
        );
        assert!(workspace
            .switch_format("20261018", Format::Md, Format::Txt, "stale")
            .is_err());
    }
    #[test]
    fn rejects_traversal_invalid_dates_and_missing_files() {
        let (dir, workspace) = setup();
        for date in [
            "../secrets",
            "20260230",
            "20261301",
            "2026101",
            "２０２６１０１８",
        ] {
            assert!(workspace.create(date, Format::Md).is_err());
        }
        assert!(workspace
            .save("20261018", Format::Txt, "draft", "")
            .is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    #[test]
    fn external_changes_are_never_silently_overwritten() {
        let (dir, workspace) = setup();
        let doc = workspace.create("20261018", Format::Md).unwrap();
        fs::write(dir.path().join("20261018.md"), "external edit").unwrap();
        assert!(workspace
            .save("20261018", Format::Md, "app edit", &doc.revision)
            .is_err());
        assert_eq!(
            workspace.read("20261018", Format::Md).unwrap().content,
            "external edit"
        );
        fs::write(dir.path().join("notes.txt"), "unrelated").unwrap();
        assert_eq!(workspace.list().unwrap().len(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlink_diaries() {
        let (dir, workspace) = setup();
        let outside = TempDir::new().unwrap();
        let target = outside.path().join("private.txt");
        fs::write(&target, "private").unwrap();
        for format in [Format::Md, Format::Txt] {
            std::os::unix::fs::symlink(
                &target,
                dir.path().join(format!("20261018.{}", format.extension())),
            )
            .unwrap();
            assert!(workspace.read("20261018", format).is_err());
            assert!(workspace.create("20261018", format).is_err());
        }
        assert!(workspace.list().unwrap().is_empty());
    }
}
