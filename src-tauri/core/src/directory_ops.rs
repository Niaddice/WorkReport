use crate::{
    file_rules::{self, Feature, ManagedFile},
    settings,
    workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const TRASH: &str = ".workreport-trash";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    CreateFile,
    CreateDirectory,
    Rename,
    Delete,
    Restore,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub action: Action,
    #[serde(default)]
    pub path: String,
    pub name: Option<String>,
    pub trash_id: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub token: String,
    pub file_count: usize,
    pub directory_count: usize,
    pub journal_count: usize,
    pub settings_changed: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeResult {
    pub files: file_rules::Settings,
    pub path: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub id: String,
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub deleted_at: u64,
    pub file_count: usize,
    pub directory_count: usize,
}
// Records are persisted in info.json, but never sent to the webview.
#[derive(Deserialize, Serialize)]
struct TrashInfo {
    entry: TrashEntry,
    records: Vec<ManagedFile>,
}

fn valid_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.contains(['/', '\\']) {
        return Err("请填写文件或文件夹名称，不能包含路径分隔符".into());
    }
    let stem = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    if value.starts_with('.')
        || value.ends_with(['.', ' '])
        || value.len() > 255
        || value
            .chars()
            .any(|ch| ch.is_control() || "<>:\"|?*".contains(ch))
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        return Err("名称包含非法字符、隐藏名称或系统保留名称".into());
    }
    if matches!(
        value.to_ascii_lowercase().as_str(),
        "thumbs.db" | "desktop.ini"
    ) {
        return Err("不能使用系统保留名称".into());
    }
    Ok(value.into())
}
pub fn visible_path(root: &Path, relative: &str, allow_root: bool) -> Result<PathBuf, String> {
    if relative.is_empty() {
        return if allow_root {
            Ok(root.into())
        } else {
            Err("不能修改工作空间根目录".into())
        };
    }
    if relative.contains('\\')
        || Path::new(relative)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("只能操作工作空间内的相对路径".into());
    }
    for part in relative.split('/') {
        valid_name(part)?;
    }
    if relative.eq_ignore_ascii_case(settings::FILE_NAME) {
        return Err("工作空间设置文件由设置页面管理".into());
    }
    file_rules::safe_file(root, relative)
}
pub fn is_protected(workspace: &Workspace, relative: &str) -> bool {
    let database = workspace.database_settings();
    if !database.enabled || database.engine != crate::database::Engine::Sqlite {
        return false;
    }
    let path = relative.to_lowercase();
    let database = database.sqlite_path.to_lowercase();
    [
        database.clone(),
        format!("{database}-wal"),
        format!("{database}-shm"),
        format!("{database}-journal"),
    ]
    .iter()
    .any(|file| *file == path || file.starts_with(&format!("{path}/")))
}
fn check_protected(workspace: &Workspace, relative: &str) -> Result<(), String> {
    if is_protected(workspace, relative) {
        Err("此项目包含正在使用的 SQLite 数据库，请在数据存储设置中管理".into())
    } else {
        Ok(())
    }
}
fn within(path: &str, directory: &str) -> bool {
    path == directory || path.starts_with(&format!("{directory}/"))
}
fn replace_prefix(path: &str, from: &str, to: &str) -> String {
    if path == from {
        to.into()
    } else if let Some(rest) = path.strip_prefix(&format!("{from}/")) {
        format!("{to}/{rest}")
    } else {
        path.into()
    }
}
fn snapshot(path: &Path, hash: &mut Sha256, counts: &mut (usize, usize)) -> Result<(), String> {
    hash.update(path.to_string_lossy().as_bytes());
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            hash.update(b"missing");
            return Ok(());
        }
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        // Preserve contained links as links; never read their destinations.
        hash.update(b"link");
        hash.update(
            fs::read_link(path)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .as_bytes(),
        );
        counts.0 += 1;
    } else if metadata.is_dir() {
        hash.update(b"directory");
        counts.1 += 1;
        let mut entries = fs::read_dir(path)
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort();
        for entry in entries {
            snapshot(&entry, hash, counts)?;
        }
    } else if metadata.is_file() {
        hash.update(b"file");
        hash.update(metadata.len().to_le_bytes());
        counts.0 += 1;
        let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut buffer = [0u8; 32768];
        loop {
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
    } else {
        return Err("不支持操作此类型的文件".into());
    }
    Ok(())
}
fn trash_directory(root: &Path, create: bool) -> Result<PathBuf, String> {
    let directory = root.join(TRASH);
    if create && !directory.exists() {
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
    }
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err("工作空间回收站不是普通目录".into())
        }
        Ok(_) => Ok(directory),
        Err(error) if error.kind() == io::ErrorKind::NotFound && !create => Ok(directory),
        Err(error) => Err(error.to_string()),
    }
}
fn read_trash(root: &Path, id: &str) -> Result<(PathBuf, TrashInfo), String> {
    uuid::Uuid::parse_str(id).map_err(|_| "无效的回收站项目")?;
    let directory = trash_directory(root, false)?.join(id);
    let metadata = fs::symlink_metadata(&directory).map_err(|_| "回收站项目不存在")?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("回收站项目目录无效".into());
    }
    let info_path = directory.join("info.json");
    let metadata = fs::symlink_metadata(&info_path).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("回收站项目索引无效".into());
    }
    let info: TrashInfo = serde_json::from_slice(&fs::read(info_path).map_err(|e| e.to_string())?)
        .map_err(|_| "回收站项目索引损坏，请保留目录并检查备份")?;
    if info.entry.id != id {
        return Err("回收站项目标识不匹配".into());
    }
    visible_path(root, &info.entry.path, false)?;
    for record in &info.records {
        if !within(&record.relative_path, &info.entry.path) {
            return Err("回收站文件身份索引无效".into());
        }
        record.context()?;
    }
    let payload = directory.join("item");
    let metadata = fs::symlink_metadata(&payload).map_err(|_| "回收站文件已恢复或不存在")?;
    if metadata.file_type().is_symlink() || metadata.is_dir() != info.entry.is_dir {
        return Err("回收站文件类型已变化".into());
    }
    Ok((payload, info))
}
pub fn list_trash(root: &Path) -> Result<Vec<TrashEntry>, String> {
    let directory = trash_directory(root, false)?;
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        if uuid::Uuid::parse_str(&id).is_err() || !entry.path().join("item").exists() {
            continue;
        }
        entries.push(read_trash(root, &id)?.1.entry);
    }
    entries.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    Ok(entries)
}
struct Prepared {
    plan: Plan,
    source: PathBuf,
    target: PathBuf,
    target_relative: String,
    stored: settings::StoredSettings,
    files: file_rules::Settings,
    records: Vec<ManagedFile>,
    affected: Vec<ManagedFile>,
    restore: Option<TrashInfo>,
}
fn prepare(workspace: &Workspace, request: &Request) -> Result<Prepared, String> {
    let root = workspace.root();
    let mut stored = settings::load(&root.join(settings::FILE_NAME))?.unwrap_or_default();
    let mut files = file_rules::effective(&stored);
    if stored.files.is_none() {
        files.summary.directory =
            crate::summary::normalize_settings(root, &stored.summary)?.output_directory;
    }
    let old_files = files.clone();
    let records: BTreeMap<_, _> = file_rules::managed_records(root, &files)?
        .into_iter()
        .map(|record| (record.relative_path.clone(), record))
        .collect();
    let mut restore = None;
    let (source, target, target_relative) = if request.action == Action::Restore {
        let (source, info) =
            read_trash(root, request.trash_id.as_deref().ok_or("请选择回收站项目")?)?;
        for restored in info
            .records
            .iter()
            .filter(|record| record.feature == Feature::Journal)
        {
            if records
                .values()
                .any(|record| record.feature == Feature::Journal && record.date == restored.date)
            {
                return Err(format!(
                    "{} 已有日报，请先处理现有日报后再恢复，回收站内容已保留",
                    restored.date
                ));
            }
        }
        check_protected(workspace, &info.entry.path)?;
        let target = visible_path(root, &info.entry.path, false)?;
        let relative = info.entry.path.clone();
        restore = Some(info);
        (source, target, relative)
    } else {
        let source = visible_path(
            root,
            &request.path,
            matches!(request.action, Action::CreateFile | Action::CreateDirectory),
        )?;
        if matches!(request.action, Action::Rename | Action::Delete) {
            check_protected(workspace, &request.path)?;
        }
        let target = match request.action {
            Action::CreateFile | Action::CreateDirectory => {
                if !source.is_dir() {
                    return Err("请选择新项目的上级目录".into());
                }
                source.join(valid_name(request.name.as_deref().unwrap_or(""))?)
            }
            Action::Rename => source
                .parent()
                .ok_or("不能重命名工作空间根目录")?
                .join(valid_name(request.name.as_deref().unwrap_or(""))?),
            _ => source.clone(),
        };
        let relative = file_rules::relative_path(root, &target)?;
        visible_path(root, &relative, false)?;
        check_protected(workspace, &relative)?;
        (source, target, relative)
    };
    if matches!(
        request.action,
        Action::Rename | Action::Delete | Action::Restore
    ) {
        let metadata =
            fs::symlink_metadata(&source).map_err(|_| "文件或目录已不存在，请刷新后重试")?;
        if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
            return Err("只能操作普通文件或目录，不能使用符号链接".into());
        }
    }
    let same = request.action == Action::Rename
        && (source == target
            || matches!((fs::canonicalize(&source), fs::canonicalize(&target)), (Ok(a), Ok(b)) if a == b));
    if request.action != Action::Delete && target.exists() && !same {
        return Err("同名文件或目录已存在，未覆盖任何内容".into());
    }
    if request.action == Action::Rename && source.is_dir() {
        for feature in [Feature::Journal, Feature::Excel, Feature::Summary] {
            let rule = match feature {
                Feature::Journal => &mut files.journal,
                Feature::Excel => &mut files.excel,
                Feature::Summary => &mut files.summary,
            };
            if !rule.directory.is_empty() && within(&rule.directory, &request.path) {
                rule.directory = replace_prefix(&rule.directory, &request.path, &target_relative);
            }
        }
        files = file_rules::normalize(root, &files)?;
    }
    let affected: Vec<_> = records
        .values()
        .filter(|record| {
            within(&record.relative_path, &request.path)
                && root.join(&record.relative_path).is_file()
        })
        .cloned()
        .collect();
    if request.action == Action::Rename && !source.is_dir() {
        if let Some(record) = affected
            .iter()
            .find(|record| record.relative_path == request.path)
        {
            let extension = target
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if if record.feature == Feature::Excel {
                extension != "xlsx"
            } else {
                !["md", "txt"].contains(&extension)
            } {
                return Err(
                    "请保留此功能支持的文件扩展名：日报和总结使用 .md / .txt，Excel 报告使用 .xlsx"
                        .into(),
                );
            }
            if record.feature == Feature::Journal
                && file_rules::journal_date_for_path(&files, &target_relative)
                    .is_some_and(|date| date != record.date)
            {
                return Err(format!(
                    "重命名保留原日报日期 {}，请勿改成另一天的日期文件名",
                    record.date
                ));
            }
        }
    }
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(request).map_err(|e| e.to_string())?);
    let mut counts = (0, 0);
    if !matches!(request.action, Action::CreateFile | Action::CreateDirectory) {
        snapshot(&source, &mut hash, &mut counts)?;
    }
    if request.action != Action::Delete {
        snapshot(&target, &mut hash, &mut (0, 0))?;
    }
    snapshot(&root.join(settings::FILE_NAME), &mut hash, &mut (0, 0))?;
    snapshot(&root.join(file_rules::CATALOG_FILE), &mut hash, &mut (0, 0))?;
    if files != old_files {
        stored.files = Some(files.clone());
        stored.summary.output_directory = files.summary.directory.clone();
    }
    let plan = Plan {
        token: format!("{:x}", hash.finalize()),
        file_count: counts.0,
        directory_count: counts.1,
        journal_count: affected
            .iter()
            .filter(|record| record.feature == Feature::Journal)
            .count(),
        settings_changed: files != old_files,
    };
    Ok(Prepared {
        plan,
        source,
        target,
        target_relative,
        stored,
        files,
        records: records.into_values().collect(),
        affected,
        restore,
    })
}
pub fn preview(workspace: &Workspace, request: &Request) -> Result<Plan, String> {
    Ok(prepare(workspace, request)?.plan)
}

fn no_clobber(from: &Path, to: &Path) -> Result<(), String> {
    if fs::symlink_metadata(to).is_ok() {
        return Err("目标已存在，未覆盖文件".into());
    }
    if from.is_dir() {
        fs::rename(from, to).map_err(|e| e.to_string())
    } else {
        fs::hard_link(from, to).map_err(|e| e.to_string())?;
        if let Err(error) = fs::remove_file(from) {
            let _ = fs::remove_file(to);
            return Err(error.to_string());
        }
        Ok(())
    }
}
fn saved_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err("设置或文件索引不是普通文件".into())
        }
        Ok(_) => fs::read(path).map(Some).map_err(|e| e.to_string()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}
fn restore_bytes(path: &Path, bytes: &Option<Vec<u8>>) -> Result<(), String> {
    if let Some(bytes) = bytes {
        file_rules::atomic_bytes(path, bytes)
    } else if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}
pub fn apply(
    workspace: &mut Workspace,
    request: &Request,
    token: &str,
) -> Result<ChangeResult, String> {
    let prepared = prepare(workspace, request)?;
    if prepared.plan.token != token {
        return Err("文件或目录内容已变化，请重新打开操作窗口后确认".into());
    }
    let root = workspace.root().to_path_buf();
    let catalog_path = root.join(file_rules::CATALOG_FILE);
    let settings_path = root.join(settings::FILE_NAME);
    let catalog_before = saved_bytes(&catalog_path)?;
    let settings_before = saved_bytes(&settings_path)?;
    let mut records = prepared.records.clone();
    let mut moved: Option<(PathBuf, PathBuf)> = None;
    let mut created: Option<PathBuf> = None;
    let mut parents = Vec::new();
    let mut trash_entry = None;
    let mut metadata_written = false;
    let result = (|| {
        match request.action {
            Action::CreateFile => {
                let temporary =
                    tempfile::NamedTempFile::new_in(&prepared.source).map_err(|e| e.to_string())?;
                temporary.as_file().sync_all().map_err(|e| e.to_string())?;
                temporary
                    .persist_noclobber(&prepared.target)
                    .map_err(|e| e.error.to_string())?;
                created = Some(prepared.target.clone());
            }
            Action::CreateDirectory => {
                fs::create_dir(&prepared.target).map_err(|e| e.to_string())?;
                created = Some(prepared.target.clone());
            }
            Action::Rename => {
                if prepared.source != prepared.target {
                    // Stage even case-only names; never rename onto an alias.
                    let stage = prepared
                        .source
                        .parent()
                        .unwrap()
                        .join(format!(".workreport-rename-{}", uuid::Uuid::new_v4()));
                    no_clobber(&prepared.source, &stage)?;
                    moved = Some((stage.clone(), prepared.source.clone()));
                    no_clobber(&stage, &prepared.target)?;
                    moved = Some((prepared.target.clone(), prepared.source.clone()));
                }
                for record in &mut records {
                    record.relative_path = replace_prefix(
                        &record.relative_path,
                        &request.path,
                        &prepared.target_relative,
                    );
                }
            }
            Action::Delete => {
                let id = uuid::Uuid::new_v4().to_string();
                let entry_dir = trash_directory(&root, true)?.join(&id);
                fs::create_dir(&entry_dir).map_err(|e| e.to_string())?;
                trash_entry = Some(entry_dir.clone());
                let entry = TrashEntry {
                    id,
                    path: request.path.clone(),
                    name: prepared
                        .source
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    is_dir: prepared.source.is_dir(),
                    deleted_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|e| e.to_string())?
                        .as_secs(),
                    file_count: prepared.plan.file_count,
                    directory_count: prepared.plan.directory_count,
                };
                file_rules::atomic_bytes(
                    &entry_dir.join("info.json"),
                    &serde_json::to_vec_pretty(&TrashInfo {
                        entry,
                        records: prepared.affected.clone(),
                    })
                    .map_err(|e| e.to_string())?,
                )?;
                let target = entry_dir.join("item");
                no_clobber(&prepared.source, &target)?;
                moved = Some((target, prepared.source.clone()));
                records.retain(|record| !within(&record.relative_path, &request.path));
            }
            Action::Restore => {
                let parent = prepared.target.parent().ok_or("恢复目录无效")?;
                let mut missing = Vec::new();
                let mut cursor = parent;
                while !cursor.exists() {
                    missing.push(cursor.to_path_buf());
                    cursor = cursor.parent().ok_or("恢复目录无效")?;
                }
                for path in missing.into_iter().rev() {
                    fs::create_dir(&path).map_err(|e| e.to_string())?;
                    parents.push(path);
                }
                visible_path(&root, &prepared.target_relative, false)?;
                no_clobber(&prepared.source, &prepared.target)?;
                moved = Some((prepared.target.clone(), prepared.source.clone()));
                for record in &prepared.restore.as_ref().unwrap().records {
                    records.retain(|existing| existing.relative_path != record.relative_path);
                    records.push(record.clone());
                }
            }
        }
        // Discover generic files that became journals after creating or renaming
        // a year folder; preserve all previously registered dates first.
        let paths = if request.action == Action::CreateFile {
            vec![prepared.target_relative.clone()]
        } else if matches!(request.action, Action::Rename | Action::Restore) {
            let mut paths = Vec::new();
            collect_files(&root, &prepared.target, &mut paths)?;
            paths
        } else {
            Vec::new()
        };
        for relative in paths {
            if records
                .iter()
                .any(|record| record.relative_path == relative)
            {
                continue;
            }
            if let Some(date) = file_rules::journal_date_for_path(&prepared.files, &relative) {
                if records.iter().any(|record| {
                    record.feature == Feature::Journal
                        && record.date == date
                        && root.join(&record.relative_path).is_file()
                }) {
                    return Err(format!("{date} 已有日报，未创建重复日报"));
                }
                records.push(ManagedFile::journal(&relative, &date)?);
            }
        }
        // Deleted records may share the destination name with a current file.
        let mut unique = BTreeMap::new();
        for record in records {
            unique.insert(record.relative_path.clone(), record);
        }
        metadata_written = true;
        file_rules::write_catalog(&root, &unique.into_values().collect::<Vec<_>>())?;
        if prepared.plan.settings_changed {
            settings::save(&settings_path, &prepared.stored)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        if let Some((from, to)) = &moved {
            if let Err(error) = no_clobber(from, to) {
                failures.push(format!("文件保留在 {}：{error}", from.display()));
            }
        }
        if let Some(path) = &created {
            let result =
                if path.is_file() && fs::metadata(path).is_ok_and(|metadata| metadata.len() > 0) {
                    Err(io::Error::other(format!(
                        "新文件已有外部修改，已保留：{}",
                        path.display()
                    )))
                } else if path.is_dir() {
                    fs::remove_dir(path)
                } else {
                    fs::remove_file(path)
                };
            if let Err(error) = result {
                failures.push(error.to_string());
            }
        }
        for parent in parents.iter().rev() {
            if let Err(error) = fs::remove_dir(parent) {
                failures.push(error.to_string());
            }
        }
        if metadata_written {
            if let Err(error) = restore_bytes(&catalog_path, &catalog_before) {
                failures.push(error);
            }
            if let Err(error) = restore_bytes(&settings_path, &settings_before) {
                failures.push(error);
            }
        }
        if let Some(path) = &trash_entry {
            if !path.join("item").exists() {
                let _ = fs::remove_file(path.join("info.json"));
                let _ = fs::remove_dir(path);
            }
        }
        return Err(if failures.is_empty() {
            format!("操作未完成，已恢复原文件：{error}")
        } else {
            format!("操作失败：{error}；{}", failures.join("；"))
        });
    }
    if request.action == Action::Restore {
        let directory = prepared.source.parent().unwrap();
        let _ = fs::remove_file(directory.join("info.json"));
        let _ = fs::remove_dir(directory);
    }
    workspace.accept_file_settings(prepared.files.clone());
    Ok(ChangeResult {
        files: prepared.files,
        path: prepared.target_relative,
    })
}
fn collect_files(root: &Path, path: &Path, files: &mut Vec<String>) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        files.push(file_rules::relative_path(root, path)?);
    } else if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            collect_files(root, &entry.path(), files)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Format;
    fn request(action: Action, path: &str, name: Option<&str>) -> Request {
        Request {
            action,
            path: path.into(),
            name: name.map(str::to_string),
            trash_id: None,
        }
    }
    fn execute(workspace: &mut Workspace, request: Request) -> Result<ChangeResult, String> {
        let token = preview(workspace, &request)?.token;
        apply(workspace, &request, &token)
    }
    fn fixture() -> (tempfile::TempDir, Workspace) {
        let root = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        (root, workspace)
    }
    #[test]
    fn creates_files_and_folders_without_overwrite_and_registers_new_journals() {
        let (root, mut workspace) = fixture();
        execute(
            &mut workspace,
            request(Action::CreateDirectory, "", Some("资料")),
        )
        .unwrap();
        execute(
            &mut workspace,
            request(Action::CreateFile, "资料", Some("notes.txt")),
        )
        .unwrap();
        fs::write(root.path().join("资料/notes.txt"), "keep").unwrap();
        assert!(execute(
            &mut workspace,
            request(Action::CreateFile, "资料", Some("notes.txt"))
        )
        .is_err());
        execute(
            &mut workspace,
            request(Action::CreateDirectory, "", Some("2026")),
        )
        .unwrap();
        execute(
            &mut workspace,
            request(Action::CreateFile, "2026", Some("20261101.md")),
        )
        .unwrap();
        assert_eq!(workspace.list().unwrap()[0].date, "20261101");
        assert!(execute(
            &mut workspace,
            request(Action::CreateFile, "2026", Some("20261101.txt"))
        )
        .is_err());
        assert!(!root.path().join("2026/20261101.txt").exists());
        assert_eq!(
            fs::read_to_string(root.path().join("资料/notes.txt")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn renamed_journals_keep_date_and_remain_editable_after_reopen() {
        let (root, mut workspace) = fixture();
        workspace
            .create_with_content("20261101", Format::Md, "original")
            .unwrap();
        execute(
            &mut workspace,
            request(Action::Rename, "2026/20261101.md", Some("项目日报.md")),
        )
        .unwrap();
        let mut reopened = Workspace::open(root.path().to_str().unwrap()).unwrap();
        let document = reopened.read("20261101", Format::Md).unwrap();
        assert_eq!(document.filename, "项目日报.md");
        reopened
            .save("20261101", Format::Md, "edited", &document.revision)
            .unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("2026/项目日报.md")).unwrap(),
            "edited"
        );
        assert!(execute(
            &mut reopened,
            request(Action::Rename, "2026/项目日报.md", Some("20261102.md"))
        )
        .is_err());
        assert!(execute(
            &mut reopened,
            request(Action::Rename, "2026/项目日报.md", Some("项目日报.pdf"))
        )
        .is_err());
    }
    #[test]
    fn renaming_configured_parent_updates_all_rules_and_file_identity() {
        let (root, _) = fixture();
        let mut files = file_rules::Settings::default();
        files.journal.directory = "输出/日报".into();
        files.excel.directory = "输出/Excel".into();
        files.summary.directory = "输出/总结".into();
        let stored = settings::StoredSettings {
            files: Some(files),
            ..Default::default()
        };
        settings::save(&root.path().join(settings::FILE_NAME), &stored).unwrap();
        let mut workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        workspace
            .create_with_content("20261101", Format::Md, "content")
            .unwrap();
        execute(
            &mut workspace,
            request(Action::Rename, "输出", Some("成果")),
        )
        .unwrap();
        let saved = settings::load(&root.path().join(settings::FILE_NAME))
            .unwrap()
            .unwrap();
        assert_eq!(saved.files.unwrap().journal.directory, "成果/日报");
        assert_eq!(saved.summary.output_directory, "成果/总结");
        assert_eq!(
            workspace.read("20261101", Format::Md).unwrap().content,
            "content"
        );
        workspace
            .create_with_content("20261102", Format::Md, "next")
            .unwrap();
        assert!(root.path().join("成果/日报/2026/20261102.md").is_file());
        assert!(!root.path().join("输出").exists());
    }
    #[test]
    fn trash_and_restore_preserve_directory_contents_and_custom_dates() {
        let (root, mut workspace) = fixture();
        workspace
            .create_with_content("20261101", Format::Md, "keep journal")
            .unwrap();
        execute(
            &mut workspace,
            request(Action::Rename, "2026/20261101.md", Some("日报.md")),
        )
        .unwrap();
        fs::write(root.path().join("2026/other.txt"), "other").unwrap();
        execute(&mut workspace, request(Action::Delete, "2026", None)).unwrap();
        assert!(!root.path().join("2026").exists());
        assert!(workspace.list().unwrap().is_empty());
        let entries = list_trash(workspace.root()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_count, 2);
        let restore = Request {
            action: Action::Restore,
            path: String::new(),
            name: None,
            trash_id: Some(entries[0].id.clone()),
        };
        execute(&mut workspace, restore).unwrap();
        assert_eq!(
            workspace.read("20261101", Format::Md).unwrap().content,
            "keep journal"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("2026/other.txt")).unwrap(),
            "other"
        );
        assert!(list_trash(workspace.root()).unwrap().is_empty());
    }
    #[test]
    fn stale_delete_and_restore_collisions_preserve_both_copies() {
        let (root, mut workspace) = fixture();
        fs::write(root.path().join("file.txt"), "old").unwrap();
        let delete = request(Action::Delete, "file.txt", None);
        let token = preview(&workspace, &delete).unwrap().token;
        fs::write(root.path().join("file.txt"), "latest").unwrap();
        assert!(apply(&mut workspace, &delete, &token)
            .err()
            .unwrap()
            .contains("已变化"));
        execute(&mut workspace, delete).unwrap();
        let entries = list_trash(workspace.root()).unwrap();
        fs::write(root.path().join("file.txt"), "new file").unwrap();
        assert!(execute(
            &mut workspace,
            Request {
                action: Action::Restore,
                path: String::new(),
                name: None,
                trash_id: Some(entries[0].id.clone())
            }
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("file.txt")).unwrap(),
            "new file"
        );
        assert_eq!(
            fs::read_to_string(root.path().join(TRASH).join(&entries[0].id).join("item")).unwrap(),
            "latest"
        );
    }
    #[test]
    fn restore_metadata_failure_returns_payload_to_trash_and_keeps_its_manifest() {
        let (root, mut workspace) = fixture();
        workspace
            .create_with_content("20261101", Format::Md, "journal")
            .unwrap();
        execute(
            &mut workspace,
            request(Action::Delete, "2026/20261101.md", None),
        )
        .unwrap();
        let entry = list_trash(workspace.root()).unwrap().remove(0);
        let info_path = root.path().join(TRASH).join(&entry.id).join("info.json");
        let mut info: TrashInfo = serde_json::from_slice(&fs::read(&info_path).unwrap()).unwrap();
        info.records[0].feature = Feature::Excel; // Invalid .md identity fails catalog validation after the move.
        fs::write(&info_path, serde_json::to_vec(&info).unwrap()).unwrap();
        let error = execute(
            &mut workspace,
            Request {
                action: Action::Restore,
                path: String::new(),
                name: None,
                trash_id: Some(entry.id.clone()),
            },
        )
        .err()
        .unwrap();
        assert!(error.contains("已恢复原文件"), "{error}");
        assert!(!root.path().join("2026/20261101.md").exists());
        assert_eq!(
            fs::read_to_string(root.path().join(TRASH).join(&entry.id).join("item")).unwrap(),
            "journal"
        );
        assert!(info_path.is_file());
        assert!(file_rules::catalog(workspace.root()).unwrap().is_empty());
    }
    #[test]
    fn adoption_conflicts_rollback_the_renamed_folder() {
        let (root, mut workspace) = fixture();
        fs::create_dir(root.path().join("archive")).unwrap();
        fs::write(root.path().join("archive/old.md"), "existing journal").unwrap();
        file_rules::register(
            workspace.root(),
            ManagedFile::journal("archive/old.md", "20261101").unwrap(),
        )
        .unwrap();
        fs::create_dir(root.path().join("notes")).unwrap();
        fs::write(root.path().join("notes/20261101.md"), "generic file").unwrap();
        assert!(execute(
            &mut workspace,
            request(Action::Rename, "notes", Some("2026"))
        )
        .is_err());
        assert!(root.path().join("notes/20261101.md").is_file());
        assert!(!root.path().join("2026").exists());
        assert_eq!(
            workspace.read("20261101", Format::Md).unwrap().content,
            "existing journal"
        );
    }
    #[test]
    fn paths_settings_and_active_database_files_are_protected() {
        let (root, workspace) = fixture();
        for path in [
            "",
            "../outside",
            "/tmp/outside",
            ".history",
            ".workreport-trash",
            "workreport.json",
            "a/../b",
        ] {
            assert!(
                preview(&workspace, &request(Action::Delete, path, None)).is_err(),
                "{path}"
            );
        }
        for name in ["../outside", ".hidden", "CON", "a/b", ""] {
            assert!(preview(&workspace, &request(Action::CreateFile, "", Some(name))).is_err());
        }
        let mut stored = settings::StoredSettings::default();
        stored.database.enabled = true;
        stored.database.sqlite_path = "database/work.sqlite".into();
        settings::save(&root.path().join(settings::FILE_NAME), &stored).unwrap();
        let workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        assert!(is_protected(&workspace, "database"));
        assert!(is_protected(&workspace, "database/work.sqlite-wal"));
        assert!(preview(&workspace, &request(Action::Delete, "database", None)).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn root_symlinks_are_rejected_and_contained_links_move_without_following_them() {
        use std::os::unix::fs::symlink;
        let (root, mut workspace) = fixture();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("keep.txt"), "outside").unwrap();
        symlink(outside.path(), root.path().join("linked")).unwrap();
        assert!(preview(
            &workspace,
            &request(Action::CreateFile, "linked", Some("bad.txt"))
        )
        .is_err());
        fs::create_dir(root.path().join("folder")).unwrap();
        symlink(outside.path(), root.path().join("folder/link")).unwrap();
        execute(&mut workspace, request(Action::Delete, "folder", None)).unwrap();
        let entries = list_trash(workspace.root()).unwrap();
        execute(
            &mut workspace,
            Request {
                action: Action::Restore,
                path: String::new(),
                name: None,
                trash_id: Some(entries[0].id.clone()),
            },
        )
        .unwrap();
        assert!(fs::symlink_metadata(root.path().join("folder/link"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_to_string(outside.path().join("keep.txt")).unwrap(),
            "outside"
        );
    }
}
