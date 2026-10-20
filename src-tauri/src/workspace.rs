use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};
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
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub date: String,
    pub preview: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub date: String,
    pub filename: String,
    pub format: Format,
    pub content: String,
    pub revision: String,
}
#[derive(Clone)]
struct CachedEntry {
    path: PathBuf,
    filename: String,
    format: Format,
    modified: Option<SystemTime>,
    size: u64,
    preview: Option<String>,
}
struct FoundFile {
    path: PathBuf,
    filename: String,
    format: Format,
    modified: Option<SystemTime>,
    size: u64,
}
pub struct Workspace {
    root: PathBuf,
    index: HashMap<String, CachedEntry>,
}
fn io_error(error: std::io::Error) -> String {
    format!("无法访问工作空间文件：{error}")
}
const PREVIEW_BYTES: u64 = 16 * 1024;
fn valid_date(date: &str) -> Result<NaiveDate, String> {
    if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
        return Err("日报日期必须为 yyyymmdd 格式".into());
    }
    NaiveDate::parse_from_str(date, "%Y%m%d").map_err(|_| "日报日期无效".into())
}
fn revision(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}
fn preview_text(content: &str) -> String {
    content
        .lines()
        .map(|line| line.trim().trim_start_matches(['#', '-', '*', ' ', '>']))
        .find(|line| !line.is_empty() && !line.ends_with("日报"))
        .unwrap_or("还没有内容")
        .chars()
        .take(80)
        .collect()
}
fn entry_preview(path: &Path) -> String {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return "无法读取预览".into(),
    };
    let mut buffer = Vec::new();
    let mut prefix = file.take(PREVIEW_BYTES);
    if prefix.read_to_end(&mut buffer).is_err() {
        return "无法读取预览".into();
    }
    let content = match std::str::from_utf8(&buffer) {
        Ok(content) => content,
        // The bounded prefix can end in the middle of a multi-byte character.
        Err(error) if error.error_len().is_none() && buffer.len() as u64 == PREVIEW_BYTES => {
            std::str::from_utf8(&buffer[..error.valid_up_to()]).unwrap_or("")
        }
        Err(_) => return "无法预览（文件不是 UTF-8 编码）".into(),
    };
    preview_text(content)
}
impl Workspace {
    pub fn open(path: &str) -> Result<Self, String> {
        let root = fs::canonicalize(path).map_err(io_error)?;
        if !root.is_dir() {
            return Err("请选择一个文件夹作为工作空间".into());
        }
        NamedTempFile::new_in(&root).map_err(io_error)?;
        Ok(Self {
            root,
            index: HashMap::new(),
        })
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
    fn year_directory(&self, date: &str) -> Result<PathBuf, String> {
        valid_date(date)?;
        let directory = self.root.join(&date[..4]);
        match fs::symlink_metadata(&directory) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                return Err("日报年份目录必须是工作空间内的普通目录".into())
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(io_error(error))
            }
            _ => {}
        }
        Ok(directory)
    }
    fn entry_path(&self, date: &str, format: Format) -> Result<PathBuf, String> {
        let path = self
            .year_directory(date)?
            .join(format!("{date}.{}", format.extension()));
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
    fn scan(&self) -> Result<HashMap<String, FoundFile>, String> {
        let mut found: HashMap<String, FoundFile> = HashMap::new();
        for year in fs::read_dir(&self.root).map_err(io_error)? {
            let year = year.map_err(io_error)?;
            if !year.file_type().map_err(io_error)?.is_dir() {
                continue;
            }
            let year_name = year.file_name().to_string_lossy().into_owned();
            if year_name.len() != 4 || !year_name.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            for item in fs::read_dir(year.path()).map_err(io_error)? {
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
                let date = date.to_string();
                if valid_date(&date).is_err() || &date[..4] != year_name.as_str() {
                    continue;
                }
                let metadata = match item.metadata() {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(io_error(error)),
                };
                let file = FoundFile {
                    path: item.path(),
                    filename,
                    format,
                    modified: metadata.modified().ok(),
                    size: metadata.len(),
                };
                match found.get(&date) {
                    Some(existing) if existing.format == Format::Md || format == Format::Txt => {}
                    _ => {
                        found.insert(date, file);
                    }
                }
            }
        }
        Ok(found)
    }
    pub fn list(&mut self) -> Result<Vec<Entry>, String> {
        let found = self.scan()?;
        self.index.retain(|date, _| found.contains_key(date));
        let mut entries = Vec::with_capacity(found.len());
        for (date, file) in found {
            let unchanged = self.index.get(&date).is_some_and(|cached| {
                cached.path == file.path
                    && cached.format == file.format
                    && cached.modified == file.modified
                    && cached.size == file.size
            });
            if !unchanged {
                self.index.insert(
                    date.clone(),
                    CachedEntry {
                        path: file.path,
                        filename: file.filename.clone(),
                        format: file.format,
                        modified: file.modified,
                        size: file.size,
                        preview: None,
                    },
                );
            }
            entries.push(Entry {
                date,
                filename: file.filename,
                format: file.format,
            });
        }
        entries.sort_by(|left, right| left.date.cmp(&right.date));
        Ok(entries)
    }
    pub fn previews(&mut self, dates: &[String]) -> Result<Vec<Preview>, String> {
        let mut previews = Vec::with_capacity(dates.len());
        for date in dates {
            let Some(cached) = self.index.get(date).cloned() else {
                continue;
            };
            let metadata = match fs::metadata(&cached.path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    self.index.remove(date);
                    continue;
                }
            };
            let modified = metadata.modified().ok();
            let size = metadata.len();
            let cached_preview = if cached.modified == modified && cached.size == size {
                cached.preview
            } else {
                None
            };
            let preview = match cached_preview {
                Some(preview) => preview,
                None => {
                    let preview = entry_preview(&cached.path);
                    self.index.insert(
                        date.clone(),
                        CachedEntry {
                            path: cached.path,
                            filename: cached.filename,
                            format: cached.format,
                            modified,
                            size,
                            preview: Some(preview.clone()),
                        },
                    );
                    preview
                }
            };
            previews.push(Preview {
                date: date.clone(),
                preview,
            });
        }
        Ok(previews)
    }
    pub fn read(&self, date: &str, format: Format) -> Result<Document, String> {
        let content = fs::read_to_string(self.entry_path(date, format)?).map_err(|error| {
            if error.kind() == std::io::ErrorKind::InvalidData {
                "日报文件不是 UTF-8 编码，请在外部编辑器中转换后再打开".into()
            } else {
                io_error(error)
            }
        })?;
        Ok(Document {
            date: date.into(),
            filename: format!("{date}.{}", format.extension()),
            format,
            revision: revision(&content),
            content,
        })
    }
    #[cfg(test)]
    pub fn create(&mut self, date: &str, format: Format) -> Result<Document, String> {
        let parsed = valid_date(date)?;
        let heading = if format == Format::Md { "# " } else { "" };
        let content = format!("{heading}{} 日报\n\n", parsed.format("%Y年%m月%d日"));
        self.create_with_content(date, format, &content)
    }
    pub fn create_with_content(
        &mut self,
        date: &str,
        format: Format,
        content: &str,
    ) -> Result<Document, String> {
        let path = self.entry_path(date, format)?;
        if path.exists() {
            let document = self.read(date, format)?;
            self.remember(&document);
            return Ok(document);
        }
        let other = match format {
            Format::Md => Format::Txt,
            Format::Txt => Format::Md,
        };
        // Keep one canonical report per day: never add a second extension.
        if self.entry_path(date, other)?.exists() {
            let document = self.read(date, other)?;
            self.remember(&document);
            return Ok(document);
        }
        let directory = self.year_directory(date)?;
        fs::create_dir_all(&directory).map_err(io_error)?;
        let mut temp = NamedTempFile::new_in(&directory).map_err(io_error)?;
        temp.write_all(content.as_bytes()).map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        match temp.persist_noclobber(&path) {
            Ok(_) => {}
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error(error.error)),
        }
        let document = self.read(date, format)?;
        self.remember(&document);
        Ok(document)
    }
    pub fn save(
        &mut self,
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
        let directory = path
            .parent()
            .ok_or_else(|| "无法确定日报目录".to_string())?
            .to_path_buf();
        let mut temp = NamedTempFile::new_in(&directory).map_err(io_error)?;
        temp.write_all(content.as_bytes()).map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        temp.persist(path).map_err(|error| io_error(error.error))?;
        let document = self.read(date, format)?;
        self.remember(&document);
        Ok(document)
    }
    pub fn switch_format(
        &mut self,
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
        let directory = target
            .parent()
            .ok_or_else(|| "无法确定日报目录".to_string())?
            .to_path_buf();
        // Write the new name without overwriting. Only remove the source after durable creation.
        let mut temp = NamedTempFile::new_in(&directory).map_err(io_error)?;
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
        let document = self.read(date, to)?;
        self.remember(&document);
        Ok(document)
    }
    fn remember(&mut self, document: &Document) {
        let path = match self.entry_path(&document.date, document.format) {
            Ok(path) => path,
            Err(_) => {
                self.index.remove(&document.date);
                return;
            }
        };
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                self.index.remove(&document.date);
                return;
            }
        };
        self.index.insert(
            document.date.clone(),
            CachedEntry {
                path,
                filename: document.filename.clone(),
                format: document.format,
                modified: metadata.modified().ok(),
                size: metadata.len(),
                preview: Some(preview_text(&document.content)),
            },
        );
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
    fn entry_file(dir: &TempDir, date: &str, format: Format) -> std::path::PathBuf {
        dir.path()
            .join(&date[..4])
            .join(format!("{date}.{}", format.extension()))
    }
    fn preview_texts(workspace: &mut Workspace, dates: &[&str]) -> Vec<String> {
        let dates: Vec<String> = dates.iter().map(|date| (*date).to_string()).collect();
        workspace
            .previews(&dates)
            .unwrap()
            .into_iter()
            .map(|preview| preview.preview)
            .collect()
    }
    #[test]
    fn create_edit_reopen_and_index_both_formats() {
        let (dir, mut workspace) = setup();
        for (date, format) in [("20261018", Format::Md), ("20261019", Format::Txt)] {
            let doc = workspace.create(date, format).unwrap();
            let saved = workspace
                .save(date, format, "# 日报\n\n完成工作空间功能\n", &doc.revision)
                .unwrap();
            assert_eq!(
                fs::read_to_string(entry_file(&dir, date, format)).unwrap(),
                saved.content
            );
            assert_eq!(
                workspace.create(date, format).unwrap().content,
                saved.content
            );
        }
        let mut reopened = Workspace::open(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(reopened.list().unwrap().len(), 2);
        assert_eq!(
            preview_texts(&mut reopened, &["20261018", "20261019"]),
            vec![
                "完成工作空间功能".to_string(),
                "完成工作空间功能".to_string()
            ]
        );
    }
    #[test]
    fn new_workspace_stores_reports_in_year_directories() {
        let (dir, mut workspace) = setup();
        workspace.create("20261018", Format::Md).unwrap();
        assert!(entry_file(&dir, "20261018", Format::Md).exists());
        assert!(!dir.path().join("20261018.md").exists());
        let entries = workspace.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].filename, "20261018.md");
    }
    #[test]
    fn create_with_content_writes_literal_report_without_template() {
        let (dir, mut workspace) = setup();
        let content = "用户输入的第一行\n第二行\n";
        let document = workspace
            .create_with_content("20261018", Format::Txt, content)
            .unwrap();
        assert_eq!(document.content, content);
        assert_eq!(
            fs::read_to_string(entry_file(&dir, "20261018", Format::Txt)).unwrap(),
            content
        );
        assert_eq!(workspace.list().unwrap()[0].filename, "20261018.txt");
    }
    #[test]
    fn create_never_adds_a_second_format_for_the_same_date() {
        let (dir, mut workspace) = setup();
        workspace.create("20261018", Format::Md).unwrap();
        let existing = workspace.create("20261018", Format::Txt).unwrap();
        assert_eq!(existing.format, Format::Md);
        assert!(!entry_file(&dir, "20261018", Format::Txt).exists());
    }
    #[test]
    fn list_prefers_markdown_and_reports_the_first_preview() {
        let (dir, mut workspace) = setup();
        fs::create_dir_all(dir.path().join("2026")).unwrap();
        fs::write(
            entry_file(&dir, "20261018", Format::Md),
            "# 日报\n\nMarkdown 内容\n",
        )
        .unwrap();
        fs::write(entry_file(&dir, "20261018", Format::Txt), "旧文本内容\n").unwrap();
        let entries = workspace.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].format, Format::Md);
        assert_eq!(
            preview_texts(&mut workspace, &["20261018"]),
            vec!["Markdown 内容".to_string()]
        );
    }
    #[test]
    fn switching_format_preserves_text_and_removes_previous_name() {
        let (dir, mut workspace) = setup();
        let doc = workspace.create("20261018", Format::Md).unwrap();
        let converted = workspace
            .switch_format("20261018", Format::Md, Format::Txt, &doc.revision)
            .unwrap();
        assert_eq!(converted.content, doc.content);
        assert!(!entry_file(&dir, "20261018", Format::Md).exists());
        assert!(entry_file(&dir, "20261018", Format::Txt).exists());
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
        let (dir, mut workspace) = setup();
        let doc = workspace.create("20261018", Format::Md).unwrap();
        fs::create_dir_all(dir.path().join("2026")).unwrap();
        fs::write(entry_file(&dir, "20261018", Format::Txt), "external txt").unwrap();
        assert!(workspace
            .switch_format("20261018", Format::Md, Format::Txt, &doc.revision)
            .is_err());
        assert_eq!(
            workspace.read("20261018", Format::Txt).unwrap().content,
            "external txt"
        );
        assert!(workspace
            .switch_format("20261018", Format::Md, Format::Txt, "stale")
            .is_err());
    }
    #[test]
    fn rejects_traversal_invalid_dates_and_missing_files() {
        let (dir, mut workspace) = setup();
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
        let (dir, mut workspace) = setup();
        let doc = workspace.create("20261018", Format::Md).unwrap();
        fs::write(entry_file(&dir, "20261018", Format::Md), "external edit").unwrap();
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
    #[test]
    fn ignores_unrelated_roots_and_legacy_flat_files() {
        let (dir, mut workspace) = setup();
        fs::write(dir.path().join("20261018.md"), "legacy").unwrap();
        fs::write(dir.path().join("notes.txt"), "unrelated").unwrap();
        assert!(workspace.list().unwrap().is_empty());
    }
    #[test]
    fn listing_survives_non_utf8_files_and_reports_a_clear_read_error() {
        let (dir, mut workspace) = setup();
        fs::create_dir_all(dir.path().join("2026")).unwrap();
        fs::write(
            entry_file(&dir, "20261018", Format::Md),
            b"# \xc4\xe3\xba\xc3\n",
        )
        .unwrap();
        fs::write(
            entry_file(&dir, "20261019", Format::Md),
            "# 日报\n\n完成工作\n",
        )
        .unwrap();
        let entries = workspace.list().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            preview_texts(&mut workspace, &["20261018", "20261019"]),
            vec![
                "无法预览（文件不是 UTF-8 编码）".to_string(),
                "完成工作".to_string()
            ]
        );
        assert!(workspace
            .read("20261018", Format::Md)
            .unwrap_err()
            .contains("UTF-8"));
    }
    #[test]
    fn preview_handles_a_utf8_character_split_by_the_read_limit() {
        let (dir, mut workspace) = setup();
        fs::create_dir_all(dir.path().join("2026")).unwrap();
        let mut content = vec![b'a'; PREVIEW_BYTES as usize - 1];
        content.extend_from_slice("你".as_bytes());
        fs::write(entry_file(&dir, "20261018", Format::Txt), content).unwrap();
        assert_eq!(workspace.list().unwrap().len(), 1);
        assert_eq!(
            preview_texts(&mut workspace, &["20261018"]),
            vec!["a".repeat(80)]
        );
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlink_diaries() {
        let (dir, mut workspace) = setup();
        let outside = TempDir::new().unwrap();
        let target = outside.path().join("private.txt");
        fs::write(&target, "private").unwrap();
        fs::create_dir_all(dir.path().join("2026")).unwrap();
        for format in [Format::Md, Format::Txt] {
            std::os::unix::fs::symlink(&target, entry_file(&dir, "20261018", format)).unwrap();
            assert!(workspace.read("20261018", format).is_err());
            assert!(workspace.create("20261018", format).is_err());
        }
        assert!(workspace.list().unwrap().is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlink_year_directories() {
        let (dir, mut workspace) = setup();
        let outside = TempDir::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("2026")).unwrap();
        assert!(workspace.create("20261018", Format::Md).is_err());
        assert!(workspace.list().unwrap().is_empty());
    }
}
