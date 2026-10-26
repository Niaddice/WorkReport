use chrono::NaiveDate;
use crate::database::{self, Database, JournalRecord, Mode, Settings as DatabaseSettings};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_warning: Option<String>,
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
    database_settings: DatabaseSettings,
    database_pool: Option<Database>,
    database_error: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferResult {
    pub copied: usize,
    pub skipped: usize,
    pub mode: Mode,
    pub enabled: bool,
    pub settings: DatabaseSettings,
    pub message: String,
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
        let root = fs::canonicalize(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "工作空间目录已不存在，请重新选择文件夹。".to_string()
            } else {
                io_error(error)
            }
        })?;
        if !root.is_dir() {
            return Err("请选择一个文件夹作为工作空间".into());
        }
        NamedTempFile::new_in(&root).map_err(io_error)?;
        let database_settings = crate::settings::load(&root.join(crate::settings::FILE_NAME))?
            .unwrap_or_default().database;
        let connection = if database_settings.enabled {
            Some(database::run_blocking(|| Database::connect(&root, &database_settings, &database_settings.workspace_id)))
        } else { None };
        let (database_pool, database_error) = match connection {
            Some(Ok(pool)) => (Some(pool), None),
            Some(Err(error)) => (None, Some(error)),
            None => (None, None),
        };
        Ok(Self {
            root,
            index: HashMap::new(),
            database_settings,
            database_pool,
            database_error,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// 历史版本失败不应阻断保存，这里只做尽力而为的记录。
    fn record_history(&self, date: &str, format: Format, content: &str) {
        let limit = self.database_settings.history_limit_bytes;
        if let Err(error) = crate::history::record(self.root(), date, format, content, limit) {
            #[cfg(debug_assertions)]
            eprintln!("[history] {error}");
        }
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
    fn list_files(&mut self) -> Result<Vec<Entry>, String> {
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
    fn previews_files(&mut self, dates: &[String]) -> Result<Vec<Preview>, String> {
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
    fn read_file(&self, date: &str, format: Format) -> Result<Document, String> {
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
            backup_warning: self.backup_warning(),
        })
    }
    #[cfg(test)]
    pub fn create(&mut self, date: &str, format: Format) -> Result<Document, String> {
        let parsed = valid_date(date)?;
        let heading = if format == Format::Md { "# " } else { "" };
        let content = format!("{heading}{} 日报\n\n", parsed.format("%Y年%m月%d日"));
        self.create_with_content(date, format, &content)
    }
    fn create_file_with_content(
        &mut self,
        date: &str,
        format: Format,
        content: &str,
    ) -> Result<Document, String> {
        let path = self.entry_path(date, format)?;
        if path.exists() {
            let document = self.read_file(date, format)?;
            self.remember(&document);
            return Ok(document);
        }
        let other = match format {
            Format::Md => Format::Txt,
            Format::Txt => Format::Md,
        };
        // Keep one canonical report per day: never add a second extension.
        if self.entry_path(date, other)?.exists() {
            let document = self.read_file(date, other)?;
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
        let document = self.read_file(date, format)?;
        self.remember(&document);
        Ok(document)
    }
    fn save_file(
        &mut self,
        date: &str,
        format: Format,
        content: &str,
        expected_revision: &str,
    ) -> Result<Document, String> {
        let path = self.entry_path(date, format)?;
        let current = self.read_file(date, format)?;
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
        let document = self.read_file(date, format)?;
        self.remember(&document);
        Ok(document)
    }
    fn switch_file_format(
        &mut self,
        date: &str,
        from: Format,
        to: Format,
        expected_revision: &str,
    ) -> Result<Document, String> {
        let current = self.read_file(date, from)?;
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
        if self.read_file(date, from)?.revision != expected_revision {
            return Err("原文件在切换时被外部修改，两份文件均已保留，请刷新后检查".into());
        }
        fs::remove_file(source).map_err(|_| {
            "新格式文件已创建，原文件未能移除。两份内容均已保留，请刷新后检查。".to_string()
        })?;
        let document = self.read_file(date, to)?;
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
const BACKUP_PENDING: &str = ".workreport-backup-pending";
const BACKUP_WARNING: &str = "日报文件已保存，但数据库备份尚未完成。请在数据存储设置中重试备份。";

impl Workspace {
    fn database_only(&self) -> bool {
        self.database_settings.enabled && self.database_settings.mode == Mode::Database
    }

    pub fn database_settings(&self) -> &DatabaseSettings { &self.database_settings }

    /// A disconnected database workspace must still open so its settings can be repaired.
    /// Document operations remain unavailable rather than silently creating local files.
    pub fn validate_open(&mut self) -> Result<(), String> {
        if self.database_only() { return Ok(()); }
        self.list_files().map(|_| ())
    }

    fn connected_database(&self) -> Result<Database, String> {
        if let Some(database) = &self.database_pool { return Ok(database.clone()); }
        database::run_blocking(|| Database::connect(&self.root, &self.database_settings, &self.database_settings.workspace_id))
            .map_err(|error| format!("无法访问日报数据库：{error}。请进入数据存储设置修复连接。"))
    }

    fn database_record(&self, date: &str) -> Result<Option<JournalRecord>, String> {
        valid_date(date)?;
        let database = self.connected_database()?;
        database::run_blocking(|| database.read(date))
    }

    fn document_from_record(record: JournalRecord) -> Document {
        Document {
            filename: format!("{}.{}", record.date, record.format.extension()),
            date: record.date, format: record.format, content: record.content,
            revision: record.revision, backup_warning: None,
        }
    }

    pub fn list(&mut self) -> Result<Vec<Entry>, String> {
        if !self.database_only() { return self.list_files(); }
        let database = self.connected_database()?;
        let records = database::run_blocking(|| database.list())?;
        Ok(records.into_iter().map(|record| Entry {
            filename: format!("{}.{}", record.date, record.format.extension()),
            date: record.date, format: record.format,
        }).collect())
    }

    pub fn previews(&mut self, dates: &[String]) -> Result<Vec<Preview>, String> {
        if !self.database_only() { return self.previews_files(dates); }
        let database = self.connected_database()?;
        database::run_blocking(|| async {
            let mut previews = Vec::new();
            for date in dates {
                valid_date(date)?;
                if let Some(record) = database.read(date).await? {
                    previews.push(Preview { date: date.clone(), preview: preview_text(&record.content) });
                }
            }
            Ok(previews)
        })
    }

    pub fn read(&self, date: &str, format: Format) -> Result<Document, String> {
        if !self.database_only() { return self.read_file(date, format); }
        let record = self.database_record(date)?.ok_or("数据库中没有这一天的日报，请刷新后重试")?;
        if record.format != format { return Err("数据库中的日报格式已变更，请刷新后重试".into()); }
        Ok(Self::document_from_record(record))
    }

    pub fn create_with_content(&mut self, date: &str, format: Format, content: &str) -> Result<Document, String> {
        valid_date(date)?;
        if self.database_only() {
            let database = self.connected_database()?;
            if let Some(record) = database::run_blocking(|| database.read(date))? {
                return Ok(Self::document_from_record(record));
            }
            let record = JournalRecord::new(date, format, content);
            database::run_blocking(|| database.save(&record, None))?;
            self.record_history(date, format, content);
            return Ok(Self::document_from_record(record));
        }
        let was_pending = self.begin_backup();
        let document = self.create_file_with_content(date, format, content)?;
        self.record_history(date, format, content);
        Ok(self.backup_document(document, was_pending))
    }

    pub fn save(&mut self, date: &str, format: Format, content: &str, expected_revision: &str) -> Result<Document, String> {
        if self.database_only() {
            self.read(date, format)?;
            let record = JournalRecord::new(date, format, content);
            let database = self.connected_database()?;
            database::run_blocking(|| database.save(&record, Some(expected_revision)))?;
            self.record_history(date, format, content);
            return Ok(Self::document_from_record(record));
        }
        let was_pending = self.begin_backup();
        let document = self.save_file(date, format, content, expected_revision)?;
        self.record_history(date, format, content);
        Ok(self.backup_document(document, was_pending))
    }

    pub fn switch_format(&mut self, date: &str, from: Format, to: Format, expected_revision: &str) -> Result<Document, String> {
        if self.database_only() {
            let current = self.read(date, from)?;
            if current.revision != expected_revision { return Err("数据库日报已被其他程序修改，请重新载入后再切换格式".into()); }
            let record = JournalRecord::new(date, to, current.content);
            let database = self.connected_database()?;
            database::run_blocking(|| database.save(&record, Some(expected_revision)))?;
            return Ok(Self::document_from_record(record));
        }
        let was_pending = self.begin_backup();
        let document = self.switch_file_format(date, from, to, expected_revision)?;
        Ok(self.backup_document(document, was_pending))
    }

    fn pending_backup(&self) -> bool {
        fs::symlink_metadata(self.root.join(BACKUP_PENDING)).is_ok()
    }

    pub fn backup_warning(&self) -> Option<String> {
        if self.database_settings.enabled && self.database_settings.mode == Mode::Dual {
            if self.pending_backup() { return Some(BACKUP_WARNING.into()); }
            if let Some(error) = &self.database_error { return Some(format!("{BACKUP_WARNING} {error}")); }
        }
        None
    }

    fn mark_backup_pending(&self) -> Result<(), String> {
        let mut temp = NamedTempFile::new_in(&self.root).map_err(io_error)?;
        temp.write_all(b"Database backup is pending. Retry from storage settings.\n").map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        temp.persist(self.root.join(BACKUP_PENDING)).map_err(|error| io_error(error.error))?;
        Ok(())
    }

    fn clear_backup_pending(&self) -> Result<(), String> {
        match fs::remove_file(self.root.join(BACKUP_PENDING)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }

    /// Mark before writing, so a process interruption cannot silently lose a queued backup.
    fn begin_backup(&mut self) -> bool {
        if !self.database_settings.enabled || self.database_settings.mode != Mode::Dual { return false; }
        let previous = self.pending_backup() || self.database_error.is_some();
        if let Err(error) = self.mark_backup_pending() { self.database_error = Some(error); }
        previous
    }

    fn backup_document(&mut self, mut document: Document, was_pending: bool) -> Document {
        if !self.database_settings.enabled || self.database_settings.mode != Mode::Dual { return document; }
        let result = (|| {
            let database = self.connected_database()?;
            let record = JournalRecord::new(&document.date, document.format, &document.content);
            // The file is authoritative in dual storage. Explicit transfer operations use
            // conflict detection; normal journal writes keep the backup equal to the file.
            database::run_blocking(|| async { database.upsert_many(&[record], true).await })?;
            self.database_pool = Some(database);
            if !was_pending { self.clear_backup_pending()?; }
            Ok::<(), String>(())
        })();
        match result {
            Ok(()) => { self.database_error = None; }
            Err(error) => {
                self.database_error = Some(error);
                let _ = self.mark_backup_pending();
            }
        }
        document.backup_warning = self.backup_warning();
        document
    }

    fn file_snapshot(&self) -> Result<Vec<JournalRecord>, String> {
        let mut files: Vec<_> = self.scan()?.into_iter().collect();
        files.sort_by(|left, right| left.0.cmp(&right.0));
        files.into_iter().map(|(date, found)| {
            let document = self.read_file(&date, found.format)?;
            Ok(JournalRecord::new(date, found.format, document.content))
        }).collect()
    }

    fn primary_snapshot(&self) -> Result<Vec<JournalRecord>, String> {
        if self.database_only() {
            let database = self.connected_database()?;
            database::run_blocking(|| database.list())
        } else { self.file_snapshot() }
    }

    fn verify_database(database: &Database, records: &[JournalRecord]) -> Result<(), String> {
        database::run_blocking(|| async {
            for record in records {
                if database.read(&record.date).await?.as_ref() != Some(record) {
                    return Err(format!("{} 的数据库内容校验失败，未切换存储方式，源数据已保留", record.date));
                }
            }
            Ok(())
        })
    }

    fn ensure_primary_unchanged(&self, snapshot: &[JournalRecord]) -> Result<(), String> {
        if self.primary_snapshot()? != snapshot { return Err("操作过程中源日报被其他程序修改，未切换存储方式。请检查后重试。".into()); }
        Ok(())
    }

    /// Prepare data and connection first; the caller commits settings atomically before
    /// accept_database makes a new primary visible to the editor.
    pub fn prepare_database_settings(&self, proposed: &DatabaseSettings) -> Result<(DatabaseSettings, Option<Database>), String> {
        let mut candidate = database::validate(proposed)?;
        if !self.database_settings.workspace_id.is_empty() { candidate.workspace_id = self.database_settings.workspace_id.clone(); }
        if candidate.enabled && candidate.workspace_id.is_empty() { candidate.workspace_id = uuid::Uuid::new_v4().to_string(); }
        if candidate == self.database_settings && (!candidate.enabled || self.database_pool.is_some()) {
            return Ok((candidate, self.database_pool.clone()));
        }
        if !candidate.enabled && !self.database_only() { return Ok((candidate, None)); }
        // Repairing authentication for the same destination must not require the old,
        // broken credentials to work. It also must not overwrite an older dual backup
        // during a settings save: retain the pending warning until explicit retry.
        let repaired = if candidate.enabled && self.database_settings.enabled && same_database_destination(&candidate, &self.database_settings) {
            let pool = database::run_blocking(|| Database::connect(&self.root, &candidate, &candidate.workspace_id))?;
            database::run_blocking(|| pool.list())?;
            if candidate.mode == self.database_settings.mode { return Ok((candidate, Some(pool))); }
            Some(pool)
        } else { None };
        let source = if self.database_only() {
            if let Some(pool) = &repaired { database::run_blocking(|| pool.list())? }
            else { self.primary_snapshot()? }
        } else { self.file_snapshot()? };
        if self.database_only() && (!candidate.enabled || candidate.mode == Mode::Dual) {
            let source_database = if let Some(pool) = &repaired { pool.clone() } else { self.connected_database()? };
            self.copy_records_to_files_guarded(&source, false, || {
                if database::run_blocking(|| source_database.list())? != source { return Err("复制过程中源数据库被其他程序修改，未切换存储方式".into()); }
                Ok(())
            })?;
        }
        let pool = if candidate.enabled {
            let database = if let Some(pool) = repaired.clone() { pool } else { database::run_blocking(|| Database::connect(&self.root, &candidate, &candidate.workspace_id))? };
            database::run_blocking(|| database.upsert_many(&source, false))?;
            Self::verify_database(&database, &source)?;
            Some(database)
        } else { None };
        if self.database_only() {
            if let Some(pool) = repaired { if database::run_blocking(|| pool.list())? != source { return Err("操作过程中源数据库被其他程序修改，未切换存储方式".into()); } }
            else { self.ensure_primary_unchanged(&source)?; }
        } else { self.ensure_primary_unchanged(&source)?; }
        Ok((candidate, pool))
    }

    pub fn accept_database(&mut self, settings: DatabaseSettings, database: Option<Database>) {
        let preserve_pending = self.database_settings.enabled && self.database_settings.mode == Mode::Dual
            && settings.enabled && settings.mode == Mode::Dual && same_database_destination(&settings, &self.database_settings);
        self.database_settings = settings;
        self.database_pool = database;
        self.database_error = None;
        self.index.clear();
        // Successful preparation has checked the complete primary snapshot.
        // Preserve a pending marker on an unchanged dual configuration until retry_backup.
        if !preserve_pending {
            let _ = self.clear_backup_pending();
        }
    }

    fn transfer_result(settings: &DatabaseSettings, copied: usize, skipped: usize, message: String) -> TransferResult {
        TransferResult { copied, skipped, mode: settings.mode, enabled: settings.enabled, settings: settings.clone(), message }
    }

    pub fn prepare_transfer(&mut self, direction: &str, action: &str, conflict_policy: &str) -> Result<(TransferResult, DatabaseSettings, Option<Database>), String> {
        if !matches!(direction, "filesToDatabase" | "databaseToFiles") || !matches!(action, "copy" | "migrate") || !matches!(conflict_policy, "abort" | "overwrite") {
            return Err("复制或迁移参数无效，请重新选择操作".into());
        }
        let mut candidate = self.database_settings.clone();
        if candidate.workspace_id.is_empty() { candidate.workspace_id = uuid::Uuid::new_v4().to_string(); }
        let database = database::run_blocking(|| Database::connect(&self.root, &candidate, &candidate.workspace_id))?;
        let overwrite = conflict_policy == "overwrite";
        let (copied, skipped) = if direction == "filesToDatabase" {
            let source = self.file_snapshot()?;
            let counts = database::run_blocking(|| database.upsert_many(&source, overwrite))?;
            Self::verify_database(&database, &source)?;
            if self.file_snapshot()? != source { return Err("目标已保存复制时的快照，源日报发生变化，未切换存储方式，请重试。源数据已保留。".into()); }
            // A verified full copy to the current dual-store destination has also
            // completed its pending backup, even if a later mode-setting commit fails.
            if self.database_settings.enabled && self.database_settings.mode == Mode::Dual
                && same_database_destination(&candidate, &self.database_settings) {
                self.clear_backup_pending().map_err(|error| format!("数据库复制已完成，但无法清理待备份标记：{error}"))?;
                self.database_error = None;
            }
            if action == "migrate" { candidate.enabled = true; candidate.mode = Mode::Database; }
            counts
        } else {
            let source = database::run_blocking(|| database.list())?;
            let counts = self.copy_records_to_files_guarded(&source, overwrite, || {
                if database::run_blocking(|| database.list())? != source { return Err("复制过程中源数据库日报被其他程序修改，请检查后重试".into()); }
                Ok(())
            })?;
            if action == "migrate" { candidate.enabled = false; candidate.mode = Mode::Dual; }
            counts
        };
        let pool = if candidate.enabled { Some(database) } else { None };
        let target = if direction == "filesToDatabase" { "数据库" } else { "日报文件" };
        let operation = if action == "migrate" { "迁移" } else { "复制" };
        let message = format!("已{operation}到{target}：写入 {copied} 篇，跳过 {skipped} 篇相同日报。源数据已保留。");
        Ok((Self::transfer_result(&candidate, copied, skipped, message), candidate, pool))
    }

    pub fn retry_backup(&mut self) -> Result<TransferResult, String> {
        if !self.database_settings.enabled || self.database_settings.mode != Mode::Dual { return Err("当前未启用文件与数据库双存储".into()); }
        let source = self.file_snapshot()?;
        let database = self.connected_database()?;
        let (copied, skipped) = database::run_blocking(|| database.upsert_many(&source, true))?;
        Self::verify_database(&database, &source)?;
        if self.file_snapshot()? != source { return Err("备份期间日报文件被其他程序修改，请再次重试备份。".into()); }
        self.clear_backup_pending()?;
        self.database_pool = Some(database);
        self.database_error = None;
        Ok(Self::transfer_result(&self.database_settings, copied, skipped, format!("数据库备份已完成：写入 {copied} 篇，核对 {skipped} 篇相同日报。")))
    }
}

#[derive(Clone)]
struct FileChange {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}

fn same_database_destination(left: &DatabaseSettings, right: &DatabaseSettings) -> bool {
    if left.engine != right.engine || left.workspace_id != right.workspace_id { return false; }
    match left.engine {
        database::Engine::Sqlite => left.sqlite_path == right.sqlite_path,
        database::Engine::Mysql | database::Engine::Postgres => {
            let default_port = if left.engine == database::Engine::Postgres { 5432 } else { 3306 };
            let port = |value: u16| if value == 0 { default_port } else { value };
            left.host.eq_ignore_ascii_case(&right.host) && port(left.port) == port(right.port) && left.database == right.database
        }
    }
}

fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => return Err("复制目标必须是工作空间内的普通日报文件".into()),
        Ok(_) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
    }
    fs::read(path).map(Some).map_err(io_error)
}

fn write_file_change(change: &FileChange) -> Result<(), String> {
    let parent = change.path.parent().ok_or("无法确定复制目标目录")?;
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => return Err("复制目标年份目录必须是工作空间内的普通目录".into()),
        Ok(_) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
        Err(error) => return Err(io_error(error)),
    }
    if read_optional_bytes(&change.path)? != change.before { return Err("目标日报在操作过程中被其他程序修改，已停止复制".into()); }
    if let Some(content) = &change.after {
        match fs::create_dir(parent) {
            Ok(()) => {},
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {},
            Err(error) => return Err(io_error(error)),
        }
        let metadata = fs::symlink_metadata(parent).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() { return Err("复制目标年份目录必须是工作空间内的普通目录".into()); }
        let mut temp = NamedTempFile::new_in(parent).map_err(io_error)?;
        temp.write_all(content).map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        if change.before.is_none() { temp.persist_noclobber(&change.path).map_err(|error| io_error(error.error))?; }
        else { temp.persist(&change.path).map_err(|error| io_error(error.error))?; }
    } else { fs::remove_file(&change.path).map_err(io_error)?; }
    Ok(())
}

fn apply_file_changes(changes: &[FileChange]) -> Result<(), String> {
    let mut applied: Vec<&FileChange> = Vec::new();
    for change in changes {
        if let Err(error) = write_file_change(change) {
            let mut rollback_failed = false;
            for previous in applied.into_iter().rev() {
                let rollback = FileChange { path: previous.path.clone(), before: previous.after.clone(), after: previous.before.clone() };
                if write_file_change(&rollback).is_err() { rollback_failed = true; }
            }
            return Err(if rollback_failed { format!("{error}。部分目标文件无法恢复，请检查目标文件；源日报数据均已保留。") } else { format!("{error}。本次目标变更已撤回，源日报数据均已保留。") });
        }
        applied.push(change);
    }
    Ok(())
}

fn rollback_file_changes(changes: &[FileChange]) -> bool {
    let mut failed = false;
    for change in changes.iter().rev() {
        let rollback = FileChange { path: change.path.clone(), before: change.after.clone(), after: change.before.clone() };
        if write_file_change(&rollback).is_err() { failed = true; }
    }
    !failed
}

impl Workspace {
    fn copy_records_to_files_guarded(&self, records: &[JournalRecord], overwrite: bool, verify_source: impl FnOnce() -> Result<(), String>) -> Result<(usize, usize), String> {
        let mut changes = Vec::new();
        let mut copied = 0;
        let mut skipped = 0;
        let mut dates = std::collections::HashSet::new();
        // Preflight the whole target set before touching any destination file.
        for record in records {
            valid_date(&record.date)?;
            if !dates.insert(&record.date) { return Err("复制来源包含重复的日报日期".into()); }
            let path = self.entry_path(&record.date, record.format)?;
            let other_format = if record.format == Format::Md { Format::Txt } else { Format::Md };
            let other_path = self.entry_path(&record.date, other_format)?;
            let before = read_optional_bytes(&path)?;
            let other = read_optional_bytes(&other_path)?;
            let after = record.content.as_bytes().to_vec();
            let same = before.as_ref() == Some(&after) && other.is_none();
            if same { skipped += 1; continue; }
            if !overwrite && (before.is_some() || other.is_some()) {
                return Err(format!("{} 已有不同内容或格式的日报文件，未覆盖任何日报。请检查两份内容后选择操作。", record.date));
            }
            if before.as_ref() != Some(&after) {
                changes.push(FileChange { path, before, after: Some(after) });
            }
            if let Some(content) = other { changes.push(FileChange { path: other_path, before: Some(content), after: None }); }
            copied += 1;
        }
        apply_file_changes(&changes)?;
        let verification = (|| {
            for record in records {
                let document = self.read_file(&record.date, record.format)?;
                let other_format = if record.format == Format::Md { Format::Txt } else { Format::Md };
                if document.content != record.content || document.revision != record.revision || read_optional_bytes(&self.entry_path(&record.date, other_format)?)?.is_some() {
                    return Err(format!("{} 的目标文件校验失败，未切换存储方式", record.date));
                }
            }
            verify_source()
        })();
        if let Err(error) = verification {
            let rolled_back = rollback_file_changes(&changes);
            return Err(if rolled_back { format!("{error}。本次目标变更已撤回，源数据已保留。") } else { format!("{error}。目标文件有外部修改，部分目标变更无法撤回，请检查；源数据已保留。") });
        }
        Ok((copied, skipped))
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
    fn missing_workspace_prompts_for_another_directory() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("missing-workspace");
        let error = Workspace::open(missing.to_str().unwrap()).err().unwrap();
        assert_eq!(error, "工作空间目录已不存在，请重新选择文件夹。");
        assert!(!missing.exists());
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
    fn saving_records_a_history_version() {
        let (dir, mut workspace) = setup();
        workspace.create_with_content("20261018", Format::Md, "第一版").unwrap();
        let current = workspace.read("20261018", Format::Md).unwrap();
        workspace.save("20261018", Format::Md, "第二版", &current.revision).unwrap();
        let versions = crate::history::list(dir.path(), Some("20261018")).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(crate::history::read(dir.path(), "20261018", &versions[0].id).unwrap(), "第二版");
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

    fn enable_database(workspace: &mut Workspace, mode: Mode) -> DatabaseSettings {
        let proposed = DatabaseSettings { enabled: true, mode, ..DatabaseSettings::default() };
        let (settings, connection) = workspace.prepare_database_settings(&proposed).unwrap();
        persist_database_settings(workspace, settings.clone(), connection);
        settings
    }

    fn persist_database_settings(workspace: &mut Workspace, settings: DatabaseSettings, connection: Option<Database>) {
        let mut stored = crate::settings::load(&workspace.root.join(crate::settings::FILE_NAME)).unwrap().unwrap_or_default();
        stored.database = settings.clone();
        crate::settings::save(&workspace.root.join(crate::settings::FILE_NAME), &stored).unwrap();
        workspace.accept_database(settings, connection);
    }

    #[test]
    fn enabling_database_copies_all_existing_reports_before_accepting_the_settings() {
        let (dir, mut workspace) = setup();
        workspace.create_with_content("20261018", Format::Md, "现有日报一").unwrap();
        workspace.create_with_content("20261019", Format::Txt, "现有日报二").unwrap();
        let proposed = DatabaseSettings { enabled: true, ..DatabaseSettings::default() };
        let (settings, connection) = workspace.prepare_database_settings(&proposed).unwrap();
        assert!(!workspace.database_settings().enabled);
        assert!(!dir.path().join(crate::settings::FILE_NAME).exists());
        assert!(dir.path().join("workreport.sqlite").is_file());
        assert!(!settings.workspace_id.is_empty());
        let records = database::run_blocking(|| connection.as_ref().unwrap().list()).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].format, Format::Txt);
        persist_database_settings(&mut workspace, settings, connection);
        let mut reopened = Workspace::open(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(reopened.list().unwrap().len(), 2);
        assert!(reopened.database_settings().enabled);
    }

    #[test]
    fn dual_storage_creates_saves_and_changes_format_in_both_stores() {
        let (dir, mut workspace) = setup();
        enable_database(&mut workspace, Mode::Dual);
        let created = workspace.create_with_content("20261018", Format::Md, "初始日报").unwrap();
        assert!(created.backup_warning.is_none());
        let saved = workspace.save("20261018", Format::Md, "完成双存储", &created.revision).unwrap();
        let converted = workspace.switch_format("20261018", Format::Md, Format::Txt, &saved.revision).unwrap();
        let database = workspace.connected_database().unwrap();
        let records = database::run_blocking(|| database.list()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].format, Format::Txt);
        assert_eq!(records[0].content, converted.content);
        assert!(!entry_file(&dir, "20261018", Format::Md).exists());
        assert_eq!(fs::read_to_string(entry_file(&dir, "20261018", Format::Txt)).unwrap(), "完成双存储");
        assert!(!workspace.pending_backup());
    }

    #[test]
    fn database_only_routes_listing_previews_editing_and_format_without_creating_diary_files() {
        let (dir, mut workspace) = setup();
        enable_database(&mut workspace, Mode::Database);
        let created = workspace.create_with_content("20261018", Format::Md, "# 日报\n\n数据库正文").unwrap();
        assert!(!dir.path().join("2026").exists());
        assert_eq!(workspace.list().unwrap()[0].format, Format::Md);
        assert_eq!(preview_texts(&mut workspace, &["20261018"]), vec!["数据库正文"]);
        assert_eq!(workspace.create_with_content("20261018", Format::Txt, "禁止覆盖").unwrap().content, created.content);
        let saved = workspace.save("20261018", Format::Md, "新内容", &created.revision).unwrap();
        assert!(workspace.save("20261018", Format::Md, "错误覆盖", &created.revision).is_err());
        let switched = workspace.switch_format("20261018", Format::Md, Format::Txt, &saved.revision).unwrap();
        assert_eq!(switched.format, Format::Txt);
        assert!(workspace.read("20261018", Format::Md).is_err());
        assert_eq!(workspace.read("20261018", Format::Txt).unwrap().content, "新内容");
        assert!(!dir.path().join("2026").exists());
    }

    #[test]
    fn disconnected_database_only_workspace_remains_openable_but_never_falls_back_to_files() {
        let (dir, mut workspace) = setup();
        let mut settings = enable_database(&mut workspace, Mode::Database);
        settings.sqlite_path = "blocked/store.sqlite".into();
        fs::write(dir.path().join("blocked"), "not a directory").unwrap();
        persist_database_settings(&mut workspace, settings, None);
        let mut reopened = Workspace::open(dir.path().to_str().unwrap()).unwrap();
        assert!(reopened.validate_open().is_ok());
        assert!(reopened.list().is_err());
        assert!(reopened.read("20261018", Format::Md).is_err());
        assert!(reopened.create_with_content("20261018", Format::Md, "未落盘草稿").is_err());
        assert!(!dir.path().join("2026").exists());
    }

    #[test]
    fn dual_database_failure_keeps_file_success_and_pending_warning_until_full_retry() {
        let (dir, mut workspace) = setup();
        let proposed = DatabaseSettings { enabled: true, sqlite_path: "backup/store.sqlite".into(), ..DatabaseSettings::default() };
        let (settings, pool) = workspace.prepare_database_settings(&proposed).unwrap();
        persist_database_settings(&mut workspace, settings, pool);
        workspace.database_pool = None;
        fs::rename(dir.path().join("backup"), dir.path().join("backup-offline")).unwrap();
        fs::write(dir.path().join("backup"), "offline").unwrap();
        let document = workspace.create_with_content("20261018", Format::Txt, "文件保存成功").unwrap();
        assert!(document.backup_warning.is_some());
        assert_eq!(fs::read_to_string(entry_file(&dir, "20261018", Format::Txt)).unwrap(), "文件保存成功");
        assert!(workspace.pending_backup());
        let reopened = Workspace::open(dir.path().to_str().unwrap()).unwrap();
        assert!(reopened.read("20261018", Format::Txt).unwrap().backup_warning.is_some());
        drop(reopened);
        fs::remove_file(dir.path().join("backup")).unwrap();
        fs::rename(dir.path().join("backup-offline"), dir.path().join("backup")).unwrap();
        workspace.retry_backup().unwrap();
        assert!(!workspace.pending_backup());
        assert!(workspace.read("20261018", Format::Txt).unwrap().backup_warning.is_none());
        assert_eq!(workspace.database_record("20261018").unwrap().unwrap().content, "文件保存成功");
    }

    #[test]
    fn same_destination_credentials_repair_preserves_pending_backup_without_overwriting_it() {
        let (dir, mut workspace) = setup();
        let original = workspace.create_with_content("20261018", Format::Md, "旧备份").unwrap();
        let settings = enable_database(&mut workspace, Mode::Dual);
        fs::write(entry_file(&dir, "20261018", Format::Md), "当前文件").unwrap();
        workspace.mark_backup_pending().unwrap();
        workspace.database_pool = None;
        workspace.database_error = Some("旧连接失效".into());
        let proposed = DatabaseSettings { password: "修复连接凭据".into(), ..settings };
        let (settings, connection) = workspace.prepare_database_settings(&proposed).unwrap();
        persist_database_settings(&mut workspace, settings, connection);
        assert!(workspace.pending_backup());
        assert_eq!(workspace.database_record("20261018").unwrap().unwrap().revision, original.revision);
        workspace.retry_backup().unwrap();
        assert_eq!(workspace.database_record("20261018").unwrap().unwrap().content, "当前文件");
    }

    #[test]
    fn leaving_database_only_refuses_stale_files_until_explicit_overwrite_migration() {
        let (dir, mut workspace) = setup();
        let file = workspace.create_with_content("20261018", Format::Md, "原文件").unwrap();
        let settings = enable_database(&mut workspace, Mode::Database);
        let newer = workspace.save("20261018", Format::Md, "数据库新内容", &file.revision).unwrap();
        workspace.switch_format("20261018", Format::Md, Format::Txt, &newer.revision).unwrap();
        let proposed = DatabaseSettings { enabled: false, ..settings };
        assert!(workspace.prepare_database_settings(&proposed).is_err());
        assert_eq!(fs::read_to_string(entry_file(&dir, "20261018", Format::Md)).unwrap(), "原文件");
        assert!(workspace.prepare_transfer("databaseToFiles", "migrate", "abort").is_err());
        let (result, settings, connection) = workspace.prepare_transfer("databaseToFiles", "migrate", "overwrite").unwrap();
        assert_eq!(result.copied, 1);
        assert!(!settings.enabled);
        assert!(workspace.database_only());
        assert!(!entry_file(&dir, "20261018", Format::Md).exists());
        assert_eq!(fs::read_to_string(entry_file(&dir, "20261018", Format::Txt)).unwrap(), "数据库新内容");
        assert_eq!(workspace.database_record("20261018").unwrap().unwrap().content, "数据库新内容");
        persist_database_settings(&mut workspace, settings, connection);
        assert!(!workspace.database_only());
        assert_eq!(workspace.list().unwrap()[0].format, Format::Txt);
    }

    #[test]
    fn database_copy_conflict_preflights_entire_target_set_before_any_file_write() {
        let (dir, mut workspace) = setup();
        enable_database(&mut workspace, Mode::Database);
        workspace.create_with_content("20261018", Format::Md, "数据库第一篇").unwrap();
        workspace.create_with_content("20261019", Format::Txt, "数据库第二篇").unwrap();
        fs::create_dir_all(dir.path().join("2026")).unwrap();
        fs::write(entry_file(&dir, "20261019", Format::Md), "外部文件").unwrap();
        assert!(workspace.prepare_transfer("databaseToFiles", "copy", "abort").is_err());
        assert!(!entry_file(&dir, "20261018", Format::Md).exists());
        assert_eq!(fs::read_to_string(entry_file(&dir, "20261019", Format::Md)).unwrap(), "外部文件");
        let (result, settings, connection) = workspace.prepare_transfer("databaseToFiles", "copy", "overwrite").unwrap();
        assert_eq!(result.copied, 2);
        assert!(settings.enabled);
        assert_eq!(settings.mode, Mode::Database);
        assert!(connection.is_some());
        assert!(!entry_file(&dir, "20261019", Format::Md).exists());
        assert!(entry_file(&dir, "20261019", Format::Txt).exists());
        assert_eq!(workspace.list().unwrap().len(), 2);
    }

    #[test]
    fn file_batch_rolls_back_prior_writes_when_later_target_changed_externally() {
        let (dir, _workspace) = setup();
        let first = dir.path().join("first.txt");
        let second = dir.path().join("second.txt");
        fs::write(&first, b"original").unwrap();
        fs::write(&second, b"external change").unwrap();
        let changes = [
            FileChange { path: first.clone(), before: Some(b"original".to_vec()), after: Some(b"new".to_vec()) },
            FileChange { path: second.clone(), before: Some(b"stale".to_vec()), after: Some(b"bad overwrite".to_vec()) },
        ];
        assert!(apply_file_changes(&changes).unwrap_err().contains("已撤回"));
        assert_eq!(fs::read(&first).unwrap(), b"original");
        assert_eq!(fs::read(&second).unwrap(), b"external change");
    }

    #[test]
    fn source_verification_failure_rolls_back_file_copy_and_preserves_external_target_edits() {
        let (dir, workspace) = setup();
        let records = [JournalRecord::new("20261018", Format::Txt, "copied")];
        let error = workspace.copy_records_to_files_guarded(&records, false, || Err("source changed".into())).unwrap_err();
        assert!(error.contains("已撤回"));
        assert!(!entry_file(&dir, "20261018", Format::Txt).exists());
        let error = workspace.copy_records_to_files_guarded(&records, false, || {
            fs::write(entry_file(&dir, "20261018", Format::Txt), "external target edit").unwrap();
            Err("source changed".into())
        }).unwrap_err();
        assert!(error.contains("部分目标变更无法撤回"));
        assert_eq!(fs::read_to_string(entry_file(&dir, "20261018", Format::Txt)).unwrap(), "external target edit");
    }

    #[test]
    fn moving_to_an_existing_conflicting_database_never_switches_primary_or_loses_source() {
        let (dir, mut workspace) = setup();
        workspace.create_with_content("20261018", Format::Md, "文件来源").unwrap();
        let first_settings = enable_database(&mut workspace, Mode::Database);
        let proposed = DatabaseSettings { sqlite_path: "another.sqlite".into(), ..first_settings.clone() };
        let target = database::run_blocking(|| Database::connect(dir.path(), &proposed, &proposed.workspace_id)).unwrap();
        let conflicting = JournalRecord::new("20261018", Format::Txt, "目标中不同日报");
        database::run_blocking(|| target.save(&conflicting, None)).unwrap();
        assert!(workspace.prepare_database_settings(&proposed).is_err());
        assert_eq!(workspace.database_settings().sqlite_path, first_settings.sqlite_path);
        assert_eq!(workspace.read("20261018", Format::Md).unwrap().content, "文件来源");
        assert_eq!(database::run_blocking(|| target.read("20261018")).unwrap().unwrap(), conflicting);
    }

    #[test]
    fn verified_full_copy_or_migration_to_current_dual_database_clears_pending_backup() {
        for action in ["copy", "migrate"] {
            let (dir, mut workspace) = setup();
            workspace.create_with_content("20261018", Format::Md, "原备份").unwrap();
            enable_database(&mut workspace, Mode::Dual);
            fs::write(entry_file(&dir, "20261018", Format::Md), "当前文件").unwrap();
            workspace.mark_backup_pending().unwrap();
            workspace.database_error = Some("先前备份连接失败".into());
            assert!(workspace.backup_warning().is_some());
            let (result, candidate, _) = workspace.prepare_transfer("filesToDatabase", action, "overwrite").unwrap();
            assert_eq!(result.copied, 1);
            assert_eq!(workspace.database_settings().mode, Mode::Dual);
            assert_eq!(candidate.mode, if action == "migrate" { Mode::Database } else { Mode::Dual });
            assert!(!workspace.pending_backup());
            assert!(workspace.database_error.is_none());
            assert!(workspace.backup_warning().is_none());
            assert_eq!(workspace.database_record("20261018").unwrap().unwrap().content, "当前文件");
        }
    }
}
