use chrono::{Local, TimeZone, Utc};
use std::{
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;

pub fn current_timestamp() -> Result<String, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "系统时间无效，无法生成导出文件名")?
        .as_secs();
    timestamp(seconds, &Local)
}

pub fn timestamp<Tz: TimeZone>(seconds: u64, time_zone: &Tz) -> Result<String, String>
where
    Tz::Offset: std::fmt::Display,
{
    let seconds = i64::try_from(seconds).map_err(|_| "系统时间无效，无法生成导出文件名")?;
    Ok(chrono::DateTime::<Utc>::from_timestamp(seconds, 0)
        .ok_or("系统时间无效，无法生成导出文件名")?
        .with_timezone(time_zone)
        .format("%Y%m%d_%H%M%S")
        .to_string())
}

/// Publish complete files atomically, sharing names and collision handling across all report formats.
pub fn publish(
    temporary: NamedTempFile,
    directory: &Path,
    start: &str,
    end: &str,
    stamp: &str,
    extension: &str,
) -> Result<(String, PathBuf), String> {
    let stem = format!("总结_{start}-{end}_{stamp}");
    publish_named(temporary, directory, &format!("{stem}.{extension}"))
}

/// Use the approved naming rule and register date metadata only after the
/// complete file exists. A failed catalog update rolls back this new output.
pub fn publish_rule(
    temporary: NamedTempFile,
    root: &Path,
    directory: &Path,
    rule: &crate::file_rules::Rule,
    feature: crate::file_rules::Feature,
    start: &str,
    end: &str,
    stamp: &str,
    extension: &str,
) -> Result<(String, PathBuf), String> {
    let root = std::fs::canonicalize(root).map_err(|_| "工作空间目录不存在")?;
    let directory = std::fs::canonicalize(directory).map_err(|_| "导出目录不存在")?;
    if directory == root || !directory.starts_with(&root) {
        return Err("导出目录必须位于工作空间内".into());
    }
    let context = crate::file_rules::Context::report(start, end, stamp)?;
    let filename = crate::file_rules::render_filename(rule, &context, extension)?;
    let (filename, path) = publish_named(temporary, &directory, &filename)?;
    let registration = (|| {
        let relative = path.strip_prefix(&root).map_err(|_| "导出路径必须位于工作空间内")?;
        let relative = relative.components().map(|part| part.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        let record = crate::file_rules::ManagedFile::report(feature, &relative, start, end, stamp)?;
        crate::file_rules::register(&root, record)
    })();
    if let Err(error) = registration {
        return match std::fs::remove_file(&path) {
            Ok(()) => Err(format!("登记导出文件失败，已撤回本次输出：{error}")),
            Err(rollback) => Err(format!("登记导出文件失败：{error}；无法撤回 {}：{rollback}", path.display())),
        };
    }
    Ok((filename, path))
}

fn publish_named(
    mut temporary: NamedTempFile,
    directory: &Path,
    filename: &str,
) -> Result<(String, PathBuf), String> {
    let path = Path::new(filename);
    if path.components().count() != 1 || path.file_name().and_then(|name| name.to_str()) != Some(filename) {
        return Err("导出文件名不能包含目录".into());
    }
    let stem = path.file_stem().and_then(|name| name.to_str()).ok_or("导出文件名无效")?;
    let extension = path.extension().and_then(|name| name.to_str()).ok_or("导出文件扩展名无效")?;
    for sequence in 0_u32..10_000 {
        let suffix = if sequence == 0 {
            String::new()
        } else {
            format!("_{sequence}")
        };
        let filename = format!("{stem}{suffix}.{extension}");
        let path = directory.join(&filename);
        match temporary.persist_noclobber(&path) {
            Ok(_) => return Ok((filename, path)),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                temporary = error.file;
            }
            Err(error) => return Err(format!("无法保存导出文件：{}", error.error)),
        }
    }
    Err("同名导出文件过多，请调整命名规则后重试".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write};

    #[test]
    fn all_report_formats_share_names_and_preserve_same_second_exports() {
        let root = tempfile::tempdir().unwrap();
        for extension in ["xlsx", "md", "txt"] {
            for sequence in 0..2 {
                let content = format!("{extension}-{sequence}");
                let mut temporary = NamedTempFile::new_in(root.path()).unwrap();
                temporary.write_all(content.as_bytes()).unwrap();
                let (filename, path) = publish(
                    temporary,
                    root.path(),
                    "20261019",
                    "20261025",
                    "20261028_160500",
                    extension,
                )
                .unwrap();
                let suffix = if sequence == 0 { "" } else { "_1" };
                assert_eq!(
                    filename,
                    format!("总结_20261019-20261025_20261028_160500{suffix}.{extension}")
                );
                assert_eq!(fs::read_to_string(path).unwrap(), content);
            }
        }
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 6);
    }

    #[test]
    fn configured_names_keep_collisions_and_date_metadata_for_every_report_format() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("报告");
        fs::create_dir(&directory).unwrap();
        let rule = crate::file_rules::Rule {
            directory: "报告".into(),
            filename_template: "项目_{YYYY}-{MM}-{DD}_{startDate}_{endDate}".into(),
        };
        for extension in ["xlsx", "md", "txt"] {
            for sequence in 0..2 {
                let mut temporary = NamedTempFile::new_in(&directory).unwrap();
                temporary.write_all(format!("{extension}-{sequence}").as_bytes()).unwrap();
                let feature = if extension == "xlsx" { crate::file_rules::Feature::Excel } else { crate::file_rules::Feature::Summary };
                let (filename, path) = publish_rule(temporary, root.path(), &directory, &rule, feature, "20261019", "20261025", "20261028_160500", extension).unwrap();
                let suffix = if sequence == 0 { "" } else { "_1" };
                assert_eq!(filename, format!("项目_2026-10-28_20261019_20261025{suffix}.{extension}"));
                assert_eq!(fs::read_to_string(path).unwrap(), format!("{extension}-{sequence}"));
            }
        }
        let records = crate::file_rules::catalog(root.path()).unwrap();
        assert_eq!(records.len(), 6);
        assert!(records.iter().all(|record| record.relative_path.starts_with("报告/") && record.start_date == "20261019" && record.end_date == "20261025" && record.stamp == "20261028_160500"));
    }

    #[test]
    fn catalog_failure_rolls_back_only_the_new_report() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("summary");
        fs::create_dir(&directory).unwrap();
        fs::write(root.path().join(crate::file_rules::CATALOG_FILE), "invalid catalog").unwrap();
        let original = directory.join("固定总结.md");
        fs::write(&original, "original").unwrap();
        let rule = crate::file_rules::Rule { directory: "summary".into(), filename_template: "固定总结".into() };
        let mut temporary = NamedTempFile::new_in(&directory).unwrap();
        temporary.write_all(b"new report").unwrap();
        let error = publish_rule(temporary, root.path(), &directory, &rule, crate::file_rules::Feature::Summary, "20261019", "20261025", "20261028_160500", "md").unwrap_err();
        assert!(error.contains("已撤回"));
        assert_eq!(fs::read_to_string(original).unwrap(), "original");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        assert_eq!(fs::read_to_string(root.path().join(crate::file_rules::CATALOG_FILE)).unwrap(), "invalid catalog");
    }
}
