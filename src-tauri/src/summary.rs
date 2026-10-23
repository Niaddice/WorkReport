use chrono::{Datelike, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;

use crate::workspace::Format;

fn default_output_directory() -> String {
    "总结".into()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub output_directory: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_directory: default_output_directory(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub filename: String,
    pub path: String,
}

/// A delayed frontend action must never be redirected into a new workspace.
pub fn validate_workspace(root: &Path, requested: &str) -> Result<(), String> {
    let requested = Path::new(requested);
    if !requested.is_absolute() || fs::canonicalize(requested).ok().as_deref() != Some(root) {
        return Err("工作空间已切换，请重新打开总结后重试".into());
    }
    Ok(())
}

/// Store portable, workspace-relative paths. Reject parent components rather
/// than silently normalizing them, and reserve the journal's year namespaces.
pub fn normalize_settings(root: &Path, settings: &Settings) -> Result<Settings, String> {
    let raw = settings.output_directory.trim().replace('\\', "/");
    if raw.is_empty() {
        return Err("请设置总结输出目录".into());
    }
    if raw.len() > 4096 || raw.contains('\0') {
        return Err("总结输出目录路径无效或过长".into());
    }
    let path = Path::new(&raw);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("总结输出目录不能包含上级路径 ..".into());
    }
    let relative = if path.is_absolute() {
        path.strip_prefix(root)
            .map_err(|_| "总结输出目录必须位于当前工作空间内".to_string())?
    } else {
        path
    };
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => {
                let name = name.to_str().ok_or("总结输出目录名称必须为有效文字")?;
                // Keep saved workspace paths usable on Windows as well as macOS.
                let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
                let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                    || (stem.len() == 4
                        && (stem.starts_with("COM") || stem.starts_with("LPT"))
                        && matches!(stem.as_bytes()[3], b'1'..=b'9'));
                if name.contains(['<', '>', ':', '"', '|', '?', '*'])
                    || name.chars().any(char::is_control)
                    || name.ends_with(['.', ' '])
                    || reserved
                {
                    return Err("总结输出目录包含不支持的目录名称".into());
                }
                components.push(name.to_owned());
            }
            _ => return Err("总结输出目录必须位于当前工作空间内".into()),
        }
    }
    let first = components
        .first()
        .ok_or("请选择工作空间内的总结子目录，不能使用工作空间根目录")?;
    if first.len() == 4 && first.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("总结输出目录不能使用日报年份目录，请另选总结子目录".into());
    }
    let settings = Settings {
        output_directory: components.join("/"),
    };
    inspect_directory(root, &settings.output_directory, false)?;
    Ok(settings)
}

/// Inspect every component, including existing ancestors, so a symlink cannot
/// redirect a saved path out of the workspace. Creation is deferred until use.
fn inspect_directory(root: &Path, relative: &str, create: bool) -> Result<PathBuf, String> {
    let metadata =
        fs::symlink_metadata(root).map_err(|error| format!("无法访问工作空间：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("工作空间目录已改变，请重新选择工作空间".into());
    }
    let mut current = root.to_path_buf();
    for component in Path::new(relative).components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                return Err(
                    "总结输出路径必须由工作空间内的普通目录组成，不能使用文件或符号链接".into(),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if create {
                    match fs::create_dir(&current) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(format!("无法创建总结输出目录：{error}")),
                    }
                    let meta = fs::symlink_metadata(&current)
                        .map_err(|error| format!("无法访问总结输出目录：{error}"))?;
                    if meta.file_type().is_symlink() || !meta.is_dir() {
                        return Err("总结输出目录已改变，请重新选择目录".into());
                    }
                }
            }
            Err(error) => return Err(format!("无法访问总结输出目录：{error}")),
        }
    }
    Ok(current)
}

pub fn prepare_directory(root: &Path, settings: &Settings) -> Result<PathBuf, String> {
    let normalized = normalize_settings(root, settings)?;
    inspect_directory(root, &normalized.output_directory, true)
}

fn date_range(dates: &[String]) -> Result<(String, String), String> {
    if dates.is_empty() {
        return Err("请先选择需要总结的日期".into());
    }
    for value in dates {
        if value.len() != 8 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("总结日期格式无效，请使用 YYYYMMDD".into());
        }
        let date = NaiveDate::parse_from_str(value, "%Y%m%d")
            .map_err(|_| "总结日期无效，请重新选择日历范围".to_string())?;
        if date.year() < 1 || date.format("%Y%m%d").to_string() != *value {
            return Err("总结日期无效，请重新选择日历范围".into());
        }
    }
    Ok((
        dates.iter().min().unwrap().clone(),
        dates.iter().max().unwrap().clone(),
    ))
}

pub fn export(
    root: &Path,
    settings: &Settings,
    dates: &[String],
    content: &str,
    format: Format,
) -> Result<ExportResult, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "系统时间无效，无法生成总结文件名")?
        .as_secs();
    let stamp = export_timestamp(seconds, &Local)?;
    export_at(root, settings, dates, content, format, &stamp)
}

fn export_timestamp<Tz: TimeZone>(seconds: u64, time_zone: &Tz) -> Result<String, String>
where
    Tz::Offset: std::fmt::Display,
{
    let seconds = i64::try_from(seconds).map_err(|_| "系统时间无效，无法生成总结文件名")?;
    Ok(chrono::DateTime::<Utc>::from_timestamp(seconds, 0)
        .ok_or("系统时间无效，无法生成总结文件名")?
        .with_timezone(time_zone)
        .format("%Y%m%d_%H%M%S")
        .to_string())
}

fn export_at(
    root: &Path,
    settings: &Settings,
    dates: &[String],
    content: &str,
    format: Format,
    stamp: &str,
) -> Result<ExportResult, String> {
    let (start, end) = date_range(dates)?;
    if content.trim().is_empty() {
        return Err("总结内容为空，无法导出".into());
    }
    let directory = prepare_directory(root, settings)?;
    let stem = format!("总结_{start}-{end}_{stamp}");
    let mut temporary =
        NamedTempFile::new_in(&directory).map_err(|error| format!("无法创建总结文件：{error}"))?;
    temporary
        .write_all(content.as_bytes())
        .map_err(|error| format!("无法写入总结文件：{error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("无法同步总结文件：{error}"))?;
    for sequence in 0_u32..10_000 {
        let suffix = if sequence == 0 {
            String::new()
        } else {
            format!("_{sequence}")
        };
        let filename = format!("{stem}{suffix}.{}", format.extension());
        let path = directory.join(&filename);
        match temporary.persist_noclobber(&path) {
            Ok(_) => {
                return Ok(ExportResult {
                    filename,
                    path: path.to_string_lossy().into_owned(),
                })
            }
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                temporary = error.file;
            }
            Err(error) => return Err(format!("无法保存总结文件：{}", error.error)),
        }
    }
    Err("相同时间的总结文件过多，请稍后重新导出".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;

    fn settings(path: &str) -> Settings {
        Settings {
            output_directory: path.into(),
        }
    }

    fn dates() -> Vec<String> {
        vec!["20261022".into(), "20261014".into(), "20261022".into()]
    }

    #[test]
    fn accepts_portable_paths_without_creating_directories() {
        let root = tempfile::tempdir().unwrap();
        for path in [
            "总结/每周",
            "./总结/每周",
            "总结\\每周",
            root.path().join("总结/每周").to_str().unwrap(),
        ] {
            assert_eq!(
                normalize_settings(root.path(), &settings(path))
                    .unwrap()
                    .output_directory,
                "总结/每周"
            );
        }
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        let nested = prepare_directory(root.path(), &settings("总结/每周")).unwrap();
        assert!(nested.is_dir());
        assert_eq!(
            prepare_directory(root.path(), &settings("总结/每周")).unwrap(),
            nested
        );
    }

    #[test]
    fn rejects_root_traversal_year_namespace_and_cross_platform_invalid_paths() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        for path in [
            "",
            " ",
            ".",
            "..",
            "总结/../2026",
            "2026",
            "2026/总结",
            "总结/NUL",
            "总结/LPT1.txt",
            "总结/a:",
            "总结/a.",
            "总结/a?",
            "总结/\0",
            "C:\\outside",
            root.path().to_str().unwrap(),
            outside.path().to_str().unwrap(),
        ] {
            assert!(
                normalize_settings(root.path(), &settings(path)).is_err(),
                "accepted {path:?}"
            );
        }
        fs::write(root.path().join("existing-file"), "keep").unwrap();
        assert!(prepare_directory(root.path(), &settings("existing-file/nested")).is_err());
        assert_eq!(
            fs::read(root.path().join("existing-file")).unwrap(),
            b"keep"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_even_if_settings_were_saved_before_path_changed() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let saved = normalize_settings(root.path(), &settings("总结/nested")).unwrap();
        symlink(outside.path(), root.path().join("总结")).unwrap();
        assert!(prepare_directory(root.path(), &saved).is_err());
        assert!(export(root.path(), &saved, &dates(), "private", Format::Md).is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
        fs::remove_file(root.path().join("总结")).unwrap();
        fs::create_dir(root.path().join("real")).unwrap();
        symlink(root.path().join("real"), root.path().join("总结")).unwrap();
        assert!(normalize_settings(root.path(), &saved).is_err());
    }

    #[test]
    fn exports_exact_utf8_both_formats_without_overwrite_or_journal_index_entries() {
        let temporary = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::open(temporary.path().to_str().unwrap()).unwrap();
        let root = workspace.root().to_path_buf();
        let original = "  # 周总结\n\n- 完成项目 A。\n\n";
        let first = export_at(
            &root,
            &Settings::default(),
            &dates(),
            original,
            Format::Md,
            "20261022_083000",
        )
        .unwrap();
        let second = export_at(
            &root,
            &Settings::default(),
            &dates(),
            "第二次",
            Format::Md,
            "20261022_083000",
        )
        .unwrap();
        let text = export_at(
            &root,
            &Settings::default(),
            &dates(),
            "工作总结\n保留纯文本",
            Format::Txt,
            "20261022_083000",
        )
        .unwrap();
        assert_eq!(first.filename, "总结_20261014-20261022_20261022_083000.md");
        assert_eq!(
            second.filename,
            "总结_20261014-20261022_20261022_083000_1.md"
        );
        assert!(text.filename.ends_with(".txt"));
        assert_eq!(fs::read_to_string(&first.path).unwrap(), original);
        assert_eq!(fs::read_to_string(&second.path).unwrap(), "第二次");
        assert_eq!(
            fs::read_to_string(&text.path).unwrap(),
            "工作总结\n保留纯文本"
        );
        assert!(workspace.list().unwrap().is_empty());
        assert_eq!(fs::read_dir(root.join("总结")).unwrap().count(), 3);
    }

    #[test]
    fn export_names_use_local_dates_on_both_sides_of_the_utc_day_boundary() {
        let root = tempfile::tempdir().unwrap();
        for (utc_time, offset_seconds, expected_stamp) in [
            ("2026-10-22T22:53:35Z", 8 * 3600, "20261023_065335"),
            ("2026-10-23T02:53:35Z", -7 * 3600, "20261022_195335"),
        ] {
            let seconds = chrono::DateTime::parse_from_rfc3339(utc_time)
                .unwrap()
                .timestamp() as u64;
            let time_zone = chrono::FixedOffset::east_opt(offset_seconds).unwrap();
            let stamp = export_timestamp(seconds, &time_zone).unwrap();
            assert_eq!(stamp, expected_stamp);
            let output = export_at(
                root.path(),
                &Settings::default(),
                &dates(),
                "本地时间命名",
                Format::Txt,
                &stamp,
            )
            .unwrap();
            assert_eq!(
                output.filename,
                format!("总结_20261014-20261022_{expected_stamp}.txt")
            );
            assert_eq!(fs::read_to_string(output.path).unwrap(), "本地时间命名");
        }
    }

    #[test]
    fn validates_dates_and_content_before_creating_any_output() {
        let root = tempfile::tempdir().unwrap();
        for invalid in [
            vec![],
            vec!["20260229".into()],
            vec!["20261301".into()],
            vec!["2026-10-22".into()],
            vec!["00000101".into()],
            vec!["20261022".into(), "bad".into()],
        ] {
            assert!(export(
                root.path(),
                &Settings::default(),
                &invalid,
                "content",
                Format::Md
            )
            .is_err());
        }
        assert!(export(
            root.path(),
            &Settings::default(),
            &dates(),
            " \n\t",
            Format::Md
        )
        .is_err());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn rejects_late_actions_after_workspace_switch() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let canonical = fs::canonicalize(first.path()).unwrap();
        validate_workspace(&canonical, first.path().to_str().unwrap()).unwrap();
        assert!(validate_workspace(&canonical, second.path().to_str().unwrap()).is_err());
        assert!(validate_workspace(&canonical, "relative").is_err());
        assert!(
            validate_workspace(&canonical, first.path().join("missing").to_str().unwrap()).is_err()
        );
    }
}
