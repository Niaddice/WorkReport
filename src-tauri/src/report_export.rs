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
    mut temporary: NamedTempFile,
    directory: &Path,
    start: &str,
    end: &str,
    stamp: &str,
    extension: &str,
) -> Result<(String, PathBuf), String> {
    let stem = format!("总结_{start}-{end}_{stamp}");
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
    Err("相同时间的导出文件过多，请稍后重新导出".into())
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
}
