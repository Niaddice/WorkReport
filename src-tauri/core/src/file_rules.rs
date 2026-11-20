use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
use tempfile::NamedTempFile;

pub const CATALOG_FILE: &str = ".workreport-files.json";
pub const REPORT_TEMPLATE: &str = "总结_{startDate}-{endDate}_{date}_{time}";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Feature {
    Journal,
    Excel,
    Summary,
}
impl Feature {
    pub fn label(self) -> &'static str {
        match self {
            Self::Journal => "日报",
            Self::Excel => "Excel报告",
            Self::Summary => "总结",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub directory: String,
    pub filename_template: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub journal: Rule,
    pub excel: Rule,
    pub summary: Rule,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            journal: Rule {
                directory: String::new(),
                filename_template: "{date}".into(),
            },
            excel: Rule {
                directory: "excel".into(),
                filename_template: REPORT_TEMPLATE.into(),
            },
            summary: Rule {
                directory: "summary".into(),
                filename_template: REPORT_TEMPLATE.into(),
            },
        }
    }
}
impl Settings {
    pub fn rule(&self, feature: Feature) -> &Rule {
        match feature {
            Feature::Journal => &self.journal,
            Feature::Excel => &self.excel,
            Feature::Summary => &self.summary,
        }
    }
}
pub fn effective(stored: &crate::settings::StoredSettings) -> Settings {
    stored.files.clone().unwrap_or_else(|| {
        let mut settings = Settings::default();
        if !stored.summary.output_directory.trim().is_empty() {
            settings.summary.directory = stored.summary.output_directory.clone();
        }
        settings
    })
}

fn valid_date(date: &str) -> Result<(), String> {
    if date.len() != 8
        || !date.bytes().all(|value| value.is_ascii_digit())
        || NaiveDate::parse_from_str(date, "%Y%m%d").is_err()
    {
        return Err("文件日期无效，应为 YYYYMMDD".into());
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct Context {
    pub date: String,
    pub start_date: String,
    pub end_date: String,
    pub stamp: String,
}
impl Context {
    pub fn journal(date: &str) -> Result<Self, String> {
        valid_date(date)?;
        Ok(Self {
            date: date.into(),
            start_date: date.into(),
            end_date: date.into(),
            stamp: format!("{date}_000000"),
        })
    }
    pub fn report(start: &str, end: &str, stamp: &str) -> Result<Self, String> {
        valid_date(start)?;
        valid_date(end)?;
        if start > end
            || stamp.len() != 15
            || NaiveDateTime::parse_from_str(stamp, "%Y%m%d_%H%M%S").is_err()
        {
            return Err("报告日期或生成时间无效".into());
        }
        Ok(Self {
            date: stamp[..8].into(),
            start_date: start.into(),
            end_date: end.into(),
            stamp: stamp.into(),
        })
    }
    fn value(&self, token: &str) -> Option<&str> {
        Some(match token {
            "date" => &self.date,
            "YYYY" => &self.date[..4],
            "MM" => &self.date[4..6],
            "DD" => &self.date[6..],
            "time" => &self.stamp[9..],
            "HH" => &self.stamp[9..11],
            "mm" => &self.stamp[11..13],
            "ss" => &self.stamp[13..],
            "startDate" => &self.start_date,
            "endDate" => &self.end_date,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
enum Part {
    Literal(String),
    Token(String),
}
fn pattern(template: &str) -> Result<Vec<Part>, String> {
    if template.trim().is_empty() || template.len() > 240 {
        return Err("请填写有效的文件名规则（不含扩展名）".into());
    }
    let mut parts = Vec::new();
    let mut remaining = template;
    while !remaining.is_empty() {
        if let Some(index) = remaining.find(['{', '}']) {
            if index > 0 {
                parts.push(Part::Literal(remaining[..index].into()));
                remaining = &remaining[index..];
            }
            if remaining.starts_with('}') {
                return Err("文件名日期占位符的大括号不匹配".into());
            }
            let end = remaining
                .find('}')
                .ok_or("文件名日期占位符的大括号不匹配")?;
            let token = &remaining[1..end];
            if ![
                "date",
                "YYYY",
                "MM",
                "DD",
                "time",
                "HH",
                "mm",
                "ss",
                "startDate",
                "endDate",
            ]
            .contains(&token)
            {
                return Err(format!("不支持的日期占位符：{{{token}}}"));
            }
            parts.push(Part::Token(token.into()));
            remaining = &remaining[end + 1..];
        } else {
            parts.push(Part::Literal(remaining.into()));
            break;
        }
    }
    Ok(parts)
}
fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.starts_with('.')
        && !value.ends_with(['.', ' '])
        && !value
            .chars()
            .any(|ch| ch.is_control() || "<>:\"/\\|?*".contains(ch))
        && value.len() <= 240
        && ![
            "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
            "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
        ]
        .contains(
            &value
                .split('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
        )
}
fn directory_name(value: &str, allow_root: bool, reserve_years: bool) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return if allow_root {
            Ok(String::new())
        } else {
            Err("目录名称不能为空".into())
        };
    }
    if value.contains('\\') || value.starts_with('/') || value.ends_with('/') || value.len() > 500 {
        return Err("目录只能使用工作空间内的相对名称".into());
    }
    let parts: Vec<_> = value.split('/').collect();
    if parts
        .iter()
        .any(|part| !valid_component(part) || part.eq_ignore_ascii_case(crate::settings::FILE_NAME))
    {
        return Err("目录名称不能包含 ..、隐藏目录或非法字符".into());
    }
    if reserve_years && parts[0].len() == 4 && parts[0].bytes().all(|ch| ch.is_ascii_digit()) {
        return Err("工作空间根目录的年份目录保留给旧版日报，请使用其他目录名称".into());
    }
    Ok(value.into())
}
pub fn relative_directory(value: &str, allow_root: bool) -> Result<String, String> {
    directory_name(value, allow_root, true)
}
pub fn safe_directory(root: &Path, relative: &str, create: bool) -> Result<PathBuf, String> {
    let relative = directory_name(relative, true, false)?;
    let base = fs::canonicalize(root).map_err(|error| format!("无法访问工作空间：{error}"))?;
    let mut current = base.clone();
    for part in relative.split('/').filter(|part| !part.is_empty()) {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("保存目录不能使用符号链接".into())
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!("目录位置已被文件占用：{relative}"))
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if create {
                    fs::create_dir(&current)
                        .map_err(|error| format!("无法创建保存目录：{error}"))?;
                }
            }
            Err(error) => return Err(format!("无法检查保存目录：{error}")),
        }
        if let Ok(actual) = fs::canonicalize(&current) {
            if !actual.starts_with(&base) {
                return Err("保存目录必须在工作空间中".into());
            }
        }
    }
    Ok(current)
}
pub fn safe_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() || relative.contains('\\') || relative.starts_with('/') {
        return Err("文件路径必须位于工作空间中".into());
    }
    let path = Path::new(relative);
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("文件路径不能越出工作空间".into());
    }
    let mut current = fs::canonicalize(root).map_err(|error| error.to_string())?;
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("文件路径不能使用符号链接".into())
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法检查文件路径：{error}")),
        }
    }
    Ok(current)
}
pub fn render_filename(rule: &Rule, context: &Context, extension: &str) -> Result<String, String> {
    if !["md", "txt", "xlsx"].contains(&extension) {
        return Err("不支持的文件格式".into());
    }
    let mut stem = String::new();
    for part in pattern(&rule.filename_template)? {
        match part {
            Part::Literal(value) => stem.push_str(&value),
            Part::Token(token) => stem.push_str(context.value(&token).ok_or("无效的日期占位符")?),
        }
    }
    if !valid_component(&stem) || stem.len() + extension.len() + 1 > 240 {
        return Err("生成的文件名过长或含有非法字符".into());
    }
    if [".md", ".txt", ".xlsx"]
        .iter()
        .any(|extension| rule.filename_template.to_lowercase().ends_with(extension))
    {
        return Err("文件名规则不需要填写扩展名".into());
    }
    Ok(format!("{stem}.{extension}"))
}
pub fn normalize(root: &Path, value: &Settings) -> Result<Settings, String> {
    let mut settings = value.clone();
    for feature in [Feature::Journal, Feature::Excel, Feature::Summary] {
        let rule = match feature {
            Feature::Journal => &mut settings.journal,
            Feature::Excel => &mut settings.excel,
            Feature::Summary => &mut settings.summary,
        };
        rule.directory = relative_directory(&rule.directory, feature == Feature::Journal)?;
        rule.filename_template = rule.filename_template.trim().into();
        safe_directory(root, &rule.directory, false)?;
        let parts = pattern(&rule.filename_template)?;
        if feature == Feature::Journal {
            let tokens: BTreeSet<_> = parts
                .iter()
                .filter_map(|part| {
                    if let Part::Token(value) = part {
                        Some(value.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if tokens
                .iter()
                .any(|token| !["date", "YYYY", "MM", "DD"].contains(token))
            {
                return Err("日报文件名只支持日期占位符".into());
            }
            if !tokens.contains("date")
                && !["YYYY", "MM", "DD"]
                    .iter()
                    .all(|token| tokens.contains(token))
            {
                return Err("日报文件名必须包含完整日期：{date} 或 {YYYY}{MM}{DD}".into());
            }
        }
        render_filename(
            rule,
            &Context::report("20261001", "20261031", "20261031_203000")?,
            if feature == Feature::Excel {
                "xlsx"
            } else {
                "md"
            },
        )?;
    }
    let directories: Vec<_> = [Feature::Journal, Feature::Excel, Feature::Summary]
        .into_iter()
        .filter_map(|feature| {
            let directory = &settings.rule(feature).directory;
            (!directory.is_empty()).then(|| (feature, directory.to_lowercase()))
        })
        .collect();
    for (index, (feature, directory)) in directories.iter().enumerate() {
        for (other, target) in directories.iter().skip(index + 1) {
            if directory == target
                || directory.starts_with(&format!("{target}/"))
                || target.starts_with(&format!("{directory}/"))
            {
                return Err(format!(
                    "{}和{}的保存目录不能相同或互相包含",
                    feature.label(),
                    other.label()
                ));
            }
        }
    }
    Ok(settings)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedFile {
    pub feature: Feature,
    pub relative_path: String,
    pub date: String,
    pub start_date: String,
    pub end_date: String,
    pub stamp: String,
}
impl ManagedFile {
    pub fn journal(relative_path: &str, date: &str) -> Result<Self, String> {
        let context = Context::journal(date)?;
        Ok(Self {
            feature: Feature::Journal,
            relative_path: relative_path.into(),
            date: context.date,
            start_date: context.start_date,
            end_date: context.end_date,
            stamp: context.stamp,
        })
    }
    pub fn report(
        feature: Feature,
        relative_path: &str,
        start: &str,
        end: &str,
        stamp: &str,
    ) -> Result<Self, String> {
        let context = Context::report(start, end, stamp)?;
        Ok(Self {
            feature,
            relative_path: relative_path.into(),
            date: context.date,
            start_date: context.start_date,
            end_date: context.end_date,
            stamp: context.stamp,
        })
    }
    pub fn context(&self) -> Result<Context, String> {
        if self.feature == Feature::Journal {
            Context::journal(&self.date)
        } else {
            Context::report(&self.start_date, &self.end_date, &self.stamp)
        }
    }
}
#[derive(Default, Deserialize, Serialize)]
struct Catalog {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    files: Vec<ManagedFile>,
}
pub fn catalog(root: &Path) -> Result<Vec<ManagedFile>, String> {
    let path = root.join(CATALOG_FILE);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            return Err("文件索引不是普通文件".into())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("无法读取文件索引：{error}")),
        _ => {}
    }
    let saved: Catalog =
        serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
            .map_err(|_| "文件索引内容无效，请保留索引并检查工作空间备份")?;
    if saved.version != 1 {
        return Err("文件索引版本不支持".into());
    }
    validate_records(root, &saved.files)?;
    Ok(saved.files)
}
fn validate_records(root: &Path, files: &[ManagedFile]) -> Result<(), String> {
    let mut paths = BTreeSet::new();
    for record in files {
        safe_file(root, &record.relative_path)?;
        record.context()?;
        let path = Path::new(&record.relative_path);
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let allowed = if record.feature == Feature::Excel {
            extension == "xlsx"
        } else {
            ["md", "txt"].contains(&extension)
        };
        if !allowed
            || path.components().any(|part| {
                part.as_os_str()
                    .to_str()
                    .is_none_or(|part| !valid_component(part))
            })
            || !paths.insert(&record.relative_path)
        {
            return Err("文件索引包含无效路径、格式或重复记录，请检查工作空间备份".into());
        }
    }
    Ok(())
}
pub fn write_catalog(root: &Path, files: &[ManagedFile]) -> Result<(), String> {
    // Validate an existing index before replacing it, including a symlink at its location.
    catalog(root)?;
    validate_records(root, files)?;
    atomic_bytes(
        &root.join(CATALOG_FILE),
        &serde_json::to_vec_pretty(&Catalog {
            version: 1,
            files: files.to_vec(),
        })
        .map_err(|error| error.to_string())?,
    )
}
pub fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut temporary = NamedTempFile::new_in(path.parent().ok_or("文件目录无效")?)
        .map_err(|error| error.to_string())?;
    temporary
        .write_all(bytes)
        .map_err(|error| error.to_string())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    temporary
        .persist(path)
        .map_err(|error| format!("无法保存文件设置或索引：{}", error.error))?;
    Ok(())
}
pub fn register(root: &Path, record: ManagedFile) -> Result<(), String> {
    safe_file(root, &record.relative_path)?;
    record.context()?;
    let mut files = catalog(root)?;
    files.retain(|previous| {
        previous.feature != record.feature || previous.relative_path != record.relative_path
    });
    files.push(record);
    files.sort_by(|a, b| (&a.relative_path, a.feature).cmp(&(&b.relative_path, b.feature)));
    write_catalog(root, &files)
}
pub fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let canonical = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let path = path
        .strip_prefix(root)
        .or_else(|_| path.strip_prefix(&canonical))
        .map_err(|_| "文件必须位于工作空间中")?
        .to_str()
        .ok_or("文件路径不是有效文本")?
        .replace('\\', "/");
    safe_file(root, &path)?;
    Ok(path)
}
pub fn journal_target(
    root: &Path,
    settings: &Settings,
    date: &str,
    extension: &str,
) -> Result<PathBuf, String> {
    let context = Context::journal(date)?;
    let directory = safe_directory(root, &settings.journal.directory, false)?;
    let year = directory.join(&date[..4]);
    if let Ok(metadata) = fs::symlink_metadata(&year) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("日报年份目录不能是文件或符号链接".into());
        }
    }
    let path = year.join(render_filename(&settings.journal, &context, extension)?);
    safe_file(root, &relative_path(root, &path)?)
}

fn parse_name(template: &str, stem: &str) -> Option<Context> {
    let mut remaining = stem;
    let mut values = BTreeMap::<String, String>::new();
    for part in pattern(template).ok()? {
        match part {
            Part::Literal(value) => remaining = remaining.strip_prefix(&value)?,
            Part::Token(token) => {
                let length = match token.as_str() {
                    "YYYY" => 4,
                    "MM" | "DD" | "HH" | "mm" | "ss" => 2,
                    "time" => 6,
                    _ => 8,
                };
                let value = remaining.get(..length)?;
                if !value.bytes().all(|ch| ch.is_ascii_digit()) {
                    return None;
                }
                if let Some(previous) = values.insert(token, value.into()) {
                    if previous != value {
                        return None;
                    }
                }
                remaining = &remaining[length..];
            }
        }
    }
    if !remaining.is_empty() {
        return None;
    }
    let date = values.get("date").cloned().or_else(|| {
        Some(format!(
            "{}{}{}",
            values.get("YYYY")?,
            values.get("MM")?,
            values.get("DD")?
        ))
    })?;
    valid_date(&date).ok()?;
    let time = values.get("time").cloned().unwrap_or_else(|| {
        format!(
            "{}{}{}",
            values.get("HH").map(String::as_str).unwrap_or("00"),
            values.get("mm").map(String::as_str).unwrap_or("00"),
            values.get("ss").map(String::as_str).unwrap_or("00")
        )
    });
    Context::report(
        values.get("startDate").unwrap_or(&date),
        values.get("endDate").unwrap_or(&date),
        &format!("{date}_{time}"),
    )
    .ok()
}
/// Recognize a newly created file without publishing it first.
pub fn journal_date_for_path(settings: &Settings, relative: &str) -> Option<String> {
    let path = Path::new(relative);
    if !["md", "txt"].contains(&path.extension()?.to_str()?) { return None; }
    let parent = path.parent()?;
    let year = parent.file_name()?.to_str()?;
    if year.len() != 4 || !year.bytes().all(|ch| ch.is_ascii_digit()) { return None; }
    let base = parent.parent()?.to_str()?;
    if !base.is_empty() && base != settings.journal.directory { return None; }
    let stem = path.file_stem()?.to_str()?;
    let date = if valid_date(stem).is_ok() { stem.to_string() }
        else { parse_name(&settings.journal.filename_template, stem)?.date };
    date.starts_with(year).then_some(date)
}
pub fn journal_records(root: &Path, settings: &Settings) -> Result<Vec<ManagedFile>, String> {
    let mut records: BTreeMap<String, ManagedFile> = catalog(root)?
        .into_iter()
        .filter(|record| record.feature == Feature::Journal)
        .filter(|record| root.join(&record.relative_path).is_file())
        .map(|record| (record.relative_path.clone(), record))
        .collect();
    let mut bases = vec![root.to_path_buf()];
    let configured = safe_directory(root, &settings.journal.directory, false)?;
    if configured != root {
        bases.push(configured);
    }
    for base in bases {
        let years = match fs::read_dir(&base) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        for year in years {
            let year = year.map_err(|error| error.to_string())?;
            let name = year.file_name().to_string_lossy().into_owned();
            if name.len() != 4 || !name.bytes().all(|ch| ch.is_ascii_digit()) {
                continue;
            }
            if !year
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                continue;
            }
            for entry in fs::read_dir(year.path()).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                if !entry
                    .file_type()
                    .map_err(|error| error.to_string())?
                    .is_file()
                {
                    continue;
                }
                let path = entry.path();
                let extension = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("");
                if !["md", "txt"].contains(&extension) {
                    continue;
                }
                let stem = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("");
                let date = if valid_date(stem).is_ok() {
                    Some(stem.to_string())
                } else {
                    parse_name(&settings.journal.filename_template, stem)
                        .map(|context| context.date)
                };
                if let Some(date) = date.filter(|date| date.starts_with(&name)) {
                    let relative = relative_path(root, &path)?;
                    records
                        .entry(relative.clone())
                        .or_insert(ManagedFile::journal(&relative, &date)?);
                }
            }
        }
    }
    Ok(records.into_values().collect())
}
pub fn managed_records(root: &Path, settings: &Settings) -> Result<Vec<ManagedFile>, String> {
    let mut records: BTreeMap<String, ManagedFile> = catalog(root)?
        .into_iter()
        .filter(|record| root.join(&record.relative_path).is_file())
        .map(|record| (record.relative_path.clone(), record))
        .collect();
    for record in journal_records(root, settings)? {
        records
            .entry(record.relative_path.clone())
            .or_insert(record);
    }
    for feature in [Feature::Excel, Feature::Summary] {
        let rule = settings.rule(feature);
        let directory = safe_directory(root, &rule.directory, false)?;
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_file()
            {
                continue;
            }
            let path = entry.path();
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if if feature == Feature::Excel {
                extension != "xlsx"
            } else {
                !["md", "txt"].contains(&extension)
            } {
                continue;
            }
            let relative = relative_path(root, &path)?;
            if records.contains_key(&relative) {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            // A collision suffix does not change a report's dates.
            let context = parse_name(&rule.filename_template, stem)
                .or_else(|| parse_name(REPORT_TEMPLATE, stem))
                .or_else(|| {
                    let (base, suffix) = stem.rsplit_once('_')?;
                    if suffix.is_empty() || !suffix.bytes().all(|ch| ch.is_ascii_digit()) {
                        return None;
                    }
                    parse_name(&rule.filename_template, base)
                        .or_else(|| parse_name(REPORT_TEMPLATE, base))
                });
            if let Some(context) = context {
                records.insert(
                    relative.clone(),
                    ManagedFile::report(
                        feature,
                        &relative,
                        &context.start_date,
                        &context.end_date,
                        &context.stamp,
                    )?,
                );
            }
        }
    }
    Ok(records.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn templates_allow_literal_dates_and_preserve_default_names() {
        let context = Context::report("20261001", "20261031", "20261031_203045").unwrap();
        assert_eq!(
            render_filename(&Settings::default().excel, &context, "xlsx").unwrap(),
            "总结_20261001-20261031_20261031_203045.xlsx"
        );
        let rule = Rule {
            directory: "日报".into(),
            filename_template: "工作日报_{YYYY}年{MM}月{DD}日".into(),
        };
        assert_eq!(
            render_filename(&rule, &context, "md").unwrap(),
            "工作日报_2026年10月31日.md"
        );
    }
    #[test]
    fn directories_and_patterns_cannot_escape_or_overlap() {
        let root = tempfile::tempdir().unwrap();
        for directory in [
            "../outside",
            "/tmp/outside",
            "C:\\outside",
            ".history",
            "2026",
            "a/../b",
            "a//b",
        ] {
            let mut settings = Settings::default();
            settings.excel.directory = directory.into();
            assert!(normalize(root.path(), &settings).is_err(), "{directory}");
        }
        let mut settings = Settings::default();
        settings.summary.directory = "excel/summary".into();
        assert!(normalize(root.path(), &settings).is_err());
        for pattern in ["../{date}", "{unknown}", "日报_{YYYY}", "{date}.md"] {
            let mut settings = Settings::default();
            settings.journal.filename_template = pattern.into();
            assert!(normalize(root.path(), &settings).is_err(), "{pattern}");
        }
    }
    #[test]
    fn catalog_keeps_old_custom_journals_after_a_template_change() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("2026")).unwrap();
        fs::write(root.path().join("2026/旧日报_20261031.md"), "old content").unwrap();
        register(
            root.path(),
            ManagedFile::journal("2026/旧日报_20261031.md", "20261031").unwrap(),
        )
        .unwrap();
        let records = journal_records(root.path(), &Settings::default()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].date, "20261031");
    }
    #[cfg(unix)]
    #[test]
    fn symlink_directories_and_catalogs_are_rejected() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.path().join("linked")).unwrap();
        assert!(safe_directory(root.path(), "linked/subdir", true).is_err());
        symlink(
            outside.path().join("index.json"),
            root.path().join(CATALOG_FILE),
        )
        .unwrap();
        assert!(catalog(root.path()).is_err());
    }
}
