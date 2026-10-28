use chrono::NaiveDate;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;

use crate::workspace::Format;

/// 版本目录放在工作空间根目录，随工作空间一起备份和迁移。
pub const DIR: &str = ".history";
/// 同一天最多保留 100 个版本，避免单日版本无限增长。
const MAX_VERSIONS_PER_DAY: usize = 100;
const MAX_CONTENT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    /// 版本所属的日报日期（yyyymmdd）。
    pub date: String,
    pub saved_at: u64,
    pub format: Format,
    pub bytes: usize,
}

/// 每次成功保存后记录一份快照；内容没变化时不重复写入。
pub fn record(
    root: &Path,
    date: &str,
    format: Format,
    content: &str,
    limit_bytes: u64,
) -> Result<(), String> {
    record_at(root, date, format, content, limit_bytes, now_millis())
}

fn record_at(
    root: &Path,
    date: &str,
    format: Format,
    content: &str,
    limit_bytes: u64,
    timestamp: u64,
) -> Result<(), String> {
    validate_date(date)?;
    if content.trim().is_empty() {
        return Ok(());
    }
    if content.len() > MAX_CONTENT_BYTES {
        return Err("日报内容超过历史记录上限".into());
    }
    let directory = root.join(DIR).join(date);
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建历史记录目录：{error}"))?;
    let hash = short_hash(content);
    let latest = newest(&directory)?;
    if let Some((_, id)) = &latest {
        if id.contains(&format!("-{hash}.")) {
            return Ok(());
        }
    }
    // fsync 在不同平台耗时不同，连续保存可能发生在同一毫秒。
    // 从磁盘最新版本递增，兼顾时钟回拨和重新打开工作空间，保留原文件名格式。
    let timestamp = timestamp.max(latest.as_ref().map_or(0, |(stamp, _)| stamp + 1));
    let id = format!("{timestamp}-{hash}.{}", format.extension());
    if parse_id(&id).is_none() {
        return Err("无法保存历史记录：版本时间超出支持范围".into());
    }
    let mut file = NamedTempFile::new_in(&directory)
        .map_err(|error| format!("无法创建历史记录临时文件：{error}"))?;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("无法写入历史记录：{error}"))?;
    file.as_file()
        .sync_all()
        .map_err(|error| format!("无法同步历史记录：{error}"))?;
    file.persist(directory.join(&id))
        .map_err(|error| format!("无法保存历史记录：{}", error.error))?;
    trim_day(&directory);
    enforce_limit(root, limit_bytes, date);
    Ok(())
}

/// 不传日期时返回全部版本，按日期倒序、同日按保存时间倒序。
pub fn list(root: &Path, date: Option<&str>) -> Result<Vec<Version>, String> {
    let mut versions = Vec::new();
    match date {
        Some(date) => {
            validate_date(date)?;
            collect_day(&root.join(DIR).join(date), date, &mut versions);
        }
        None => {
            let Ok(days) = fs::read_dir(root.join(DIR)) else {
                return Ok(versions);
            };
            for day in days.flatten() {
                if !day.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                    continue;
                }
                let name = day.file_name().to_string_lossy().into_owned();
                if validate_date(&name).is_ok() {
                    collect_day(&day.path(), &name, &mut versions);
                }
            }
        }
    }
    versions.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then(right.saved_at.cmp(&left.saved_at))
            .then(right.id.cmp(&left.id))
    });
    Ok(versions)
}

fn collect_day(directory: &Path, date: &str, versions: &mut Vec<Version>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let meta = match entry.metadata() {
            Ok(meta) if meta.is_file() => meta,
            _ => continue,
        };
        let id = entry.file_name().to_string_lossy().into_owned();
        let Some((saved_at, format)) = parse_id(&id) else {
            continue;
        };
        versions.push(Version {
            id,
            date: date.to_string(),
            saved_at,
            format,
            bytes: meta.len() as usize,
        });
    }
}

pub fn read(root: &Path, date: &str, id: &str) -> Result<String, String> {
    validate_date(date)?;
    if parse_id(id).is_none() {
        return Err("历史版本标识无效".into());
    }
    let path = root.join(DIR).join(date).join(id);
    let meta = fs::metadata(&path).map_err(|_| "历史版本不存在".to_string())?;
    if !meta.is_file() || meta.len() > MAX_CONTENT_BYTES as u64 {
        return Err("历史版本内容无效".into());
    }
    fs::read_to_string(&path).map_err(|error| format!("无法读取历史版本：{error}"))
}

fn validate_date(date: &str) -> Result<(), String> {
    if date.len() != 8 || !date.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("日报日期必须为 yyyymmdd 格式".into());
    }
    NaiveDate::parse_from_str(date, "%Y%m%d")
        .map(|_| ())
        .map_err(|_| "日报日期无效".into())
}

/// 文件名格式：毫秒时间戳-8 位内容哈希.扩展名。
fn parse_id(id: &str) -> Option<(u64, Format)> {
    let (stamp, rest) = id.split_once('-')?;
    if stamp.len() != 13 || !stamp.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (hash, extension) = rest.rsplit_once('.')?;
    if hash.len() != 8 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let format = match extension {
        "md" => Format::Md,
        "txt" => Format::Txt,
        _ => return None,
    };
    Some((stamp.parse().ok()?, format))
}

fn newest(directory: &Path) -> Result<Option<(u64, String)>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("无法读取历史记录目录：{error}")),
    };
    let mut latest: Option<(u64, String)> = None;
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().into_owned();
        let Some((saved_at, _)) = parse_id(&id) else {
            continue;
        };
        // 兼容旧版本中相同毫秒的快照，与 list 的排序保持一致。
        if latest.as_ref().is_none_or(|(stamp, latest_id)| {
            saved_at > *stamp || (saved_at == *stamp && id > *latest_id)
        }) {
            latest = Some((saved_at, id));
        }
    }
    Ok(latest)
}

/// 设置保存后立即按新上限整理，避免等下一次写日报才清理。
pub fn enforce(root: &Path, limit_bytes: u64) {
    enforce_limit(root, limit_bytes, "");
}

/// 总量超过设置的上限时，按日期从最旧开始整天清理；
/// 只剩当前日期时退化为删除当天最旧的版本，避免刚写入的内容被整天空目录清掉。
fn enforce_limit(root: &Path, limit_bytes: u64, keep_date: &str) {
    let history = root.join(DIR);
    let limit = limit_bytes.max(1);
    while total_bytes(&history) > limit {
        let mut days: Vec<(String, PathBuf)> = match fs::read_dir(&history) {
            Ok(entries) => entries
                .flatten()
                .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
                .map(|entry| (entry.file_name().to_string_lossy().into_owned(), entry.path()))
                .collect(),
            Err(_) => return,
        };
        days.sort_by(|left, right| left.0.cmp(&right.0));
        if let Some(path) = days.iter().find(|(date, _)| date != keep_date).map(|(_, path)| path) {
            if fs::remove_dir_all(path).is_err() {
                return;
            }
            continue;
        }
        if !trim_oldest_file(&history.join(keep_date)) {
            return;
        }
    }
}

fn total_bytes(history: &Path) -> u64 {
    let Ok(days) = fs::read_dir(history) else {
        return 0;
    };
    let mut total = 0;
    for day in days.flatten() {
        let Ok(files) = fs::read_dir(day.path()) else {
            continue;
        };
        for file in files.flatten() {
            if let Ok(meta) = file.metadata() {
                if meta.is_file() {
                    total += meta.len();
                }
            }
        }
    }
    total
}

fn trim_oldest_file(directory: &Path) -> bool {
    let Ok(entries) = fs::read_dir(directory) else {
        return false;
    };
    let oldest = entries
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().into_owned();
            parse_id(&id).map(|(saved_at, _)| (saved_at, entry.path()))
        })
        .min();
    match oldest {
        Some((_, path)) => fs::remove_file(path).is_ok(),
        None => false,
    }
}

fn trim_day(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut versions: Vec<(u64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().into_owned();
            parse_id(&id).map(|(saved_at, _)| (saved_at, entry.path()))
        })
        .collect();
    if versions.len() <= MAX_VERSIONS_PER_DAY {
        return;
    }
    versions.sort();
    let excess = versions.len() - MAX_VERSIONS_PER_DAY;
    for (_, path) in versions.iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

fn short_hash(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    digest.iter().take(4).map(|byte| format!("{byte:02x}")).collect()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNLIMITED: u64 = u64::MAX;

    fn root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn text(fill: char, len: usize) -> String {
        fill.to_string().repeat(len)
    }

    fn day_list(dir: &tempfile::TempDir, date: &str) -> Result<Vec<Version>, String> {
        list(dir.path(), Some(date))
    }

    #[test]
    fn same_millisecond_saves_keep_every_snapshot_in_save_order() {
        let dir = root();
        let timestamp = 1_700_000_000_000;
        record_at(dir.path(), "20261024", Format::Md, "第一版", UNLIMITED, timestamp).unwrap();
        record_at(dir.path(), "20261024", Format::Txt, "第二版", UNLIMITED, timestamp).unwrap();
        record_at(dir.path(), "20261024", Format::Md, "第一版", UNLIMITED, timestamp).unwrap();

        let versions = day_list(&dir, "20261024").unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].format, Format::Md);
        assert_eq!(versions[1].format, Format::Txt);
        assert_eq!(read(dir.path(), "20261024", &versions[0].id).unwrap(), "第一版");
        assert_eq!(read(dir.path(), "20261024", &versions[1].id).unwrap(), "第二版");
        assert_eq!(read(dir.path(), "20261024", &versions[2].id).unwrap(), "第一版");
        assert!(versions[0].saved_at > versions[1].saved_at);
        assert!(versions[1].saved_at > versions[2].saved_at);

        record_at(dir.path(), "20261024", Format::Md, "第一版", UNLIMITED, timestamp).unwrap();
        assert_eq!(day_list(&dir, "20261024").unwrap(), versions);
    }

    #[test]
    fn clock_rollback_keeps_new_snapshots_after_existing_versions() {
        let dir = root();
        let timestamp = 1_700_000_000_000;
        record_at(dir.path(), "20261024", Format::Md, "时钟调整前", UNLIMITED, timestamp).unwrap();
        record_at(dir.path(), "20261024", Format::Txt, "时钟调整后", UNLIMITED, timestamp - 60_000).unwrap();

        let versions = day_list(&dir, "20261024").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].saved_at, timestamp + 1);
        assert_eq!(read(dir.path(), "20261024", &versions[0].id).unwrap(), "时钟调整后");
        assert_eq!(read(dir.path(), "20261024", &versions[1].id).unwrap(), "时钟调整前");
    }

    #[test]
    fn legacy_timestamp_ties_use_consistent_listing_deduplication_and_cleanup_order() {
        let dir = root();
        let directory = dir.path().join(DIR).join("20261024");
        fs::create_dir_all(&directory).unwrap();
        let latest_content = "旧版快照";
        let latest_id = format!("1700000000000-{}.md", short_hash(latest_content));
        let older_id = "1700000000000-00000000.txt";
        fs::write(directory.join(&latest_id), latest_content).unwrap();
        fs::write(directory.join(older_id), "较旧的同毫秒快照").unwrap();
        let before = day_list(&dir, "20261024").unwrap();
        assert_eq!(before[0].id, latest_id);
        assert_eq!(newest(&directory).unwrap().unwrap().1, latest_id);

        record_at(dir.path(), "20261024", Format::Md, latest_content, UNLIMITED, 1_700_000_000_000).unwrap();
        assert_eq!(day_list(&dir, "20261024").unwrap(), before);
        assert!(trim_oldest_file(&directory));
        assert!(!directory.join(older_id).exists());
        assert!(directory.join(&latest_id).exists());

        record_at(dir.path(), "20261024", Format::Txt, "新的快照", UNLIMITED, 1_700_000_000_000).unwrap();
        let after = day_list(&dir, "20261024").unwrap();
        assert_eq!(after[0].saved_at, 1_700_000_000_001);
        assert_eq!(read(dir.path(), "20261024", &after[0].id).unwrap(), "新的快照");
    }

    #[test]
    fn timestamp_overflow_never_publishes_an_unreadable_version() {
        let dir = root();
        record_at(dir.path(), "20261024", Format::Md, "最后可读版本", UNLIMITED, 9_999_999_999_999).unwrap();
        let before = day_list(&dir, "20261024").unwrap();
        assert!(record_at(dir.path(), "20261024", Format::Txt, "新的内容", UNLIMITED, 1_700_000_000_000).is_err());
        assert_eq!(day_list(&dir, "20261024").unwrap(), before);
        assert_eq!(fs::read_dir(dir.path().join(DIR).join("20261024")).unwrap().count(), 1);
    }

    #[test]
    fn records_lists_and_reads_versions_newest_first() {
        let dir = root();
        record(dir.path(), "20261024", Format::Md, "第一版", UNLIMITED).unwrap();
        record(dir.path(), "20261024", Format::Txt, "第二版", UNLIMITED).unwrap();
        let versions = day_list(&dir, "20261024").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].format, Format::Txt);
        assert_eq!(read(dir.path(), "20261024", &versions[1].id).unwrap(), "第一版");
        assert_eq!(read(dir.path(), "20261024", &versions[0].id).unwrap(), "第二版");
    }

    #[test]
    fn lists_every_day_by_date_newest_first() {
        let dir = root();
        record(dir.path(), "20261022", Format::Md, "前天", UNLIMITED).unwrap();
        record(dir.path(), "20261024", Format::Md, "今天一", UNLIMITED).unwrap();
        record(dir.path(), "20261024", Format::Md, "今天二", UNLIMITED).unwrap();
        let versions = list(dir.path(), None).unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].date, "20261024");
        assert_eq!(versions[1].date, "20261024");
        assert_eq!(versions[2].date, "20261022");
        assert!(versions[0].saved_at >= versions[1].saved_at);
    }

    #[test]
    fn skips_identical_content_and_keeps_days_separate() {
        let dir = root();
        record(dir.path(), "20261024", Format::Md, "相同内容", UNLIMITED).unwrap();
        record(dir.path(), "20261024", Format::Md, "相同内容", UNLIMITED).unwrap();
        record(dir.path(), "20261023", Format::Md, "相同内容", UNLIMITED).unwrap();
        assert_eq!(day_list(&dir, "20261024").unwrap().len(), 1);
        assert_eq!(day_list(&dir, "20261023").unwrap().len(), 1);
    }

    #[test]
    fn removes_the_oldest_day_when_over_the_configured_limit() {
        let dir = root();
        record(dir.path(), "20261020", Format::Md, &text('a', 100), UNLIMITED).unwrap();
        record(dir.path(), "20261021", Format::Md, &text('b', 100), UNLIMITED).unwrap();
        record(dir.path(), "20261022", Format::Md, &text('c', 100), 250).unwrap();
        assert!(day_list(&dir, "20261020").unwrap().is_empty());
        assert_eq!(day_list(&dir, "20261021").unwrap().len(), 1);
        assert_eq!(day_list(&dir, "20261022").unwrap().len(), 1);
    }

    #[test]
    fn keeps_the_current_day_within_the_limit_by_dropping_its_oldest_version() {
        let dir = root();
        record_at(dir.path(), "20261024", Format::Md, &text('a', 80), UNLIMITED, 1_700_000_000_000).unwrap();
        record_at(dir.path(), "20261024", Format::Md, &text('b', 80), 100, 1_700_000_000_000).unwrap();
        let versions = day_list(&dir, "20261024").unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(read(dir.path(), "20261024", &versions[0].id).unwrap(), text('b', 80));
    }

    #[test]
    fn enforce_can_clean_every_day_when_the_limit_shrinks() {
        let dir = root();
        record(dir.path(), "20261020", Format::Md, &text('a', 100), UNLIMITED).unwrap();
        record(dir.path(), "20261024", Format::Md, &text('b', 100), UNLIMITED).unwrap();
        enforce(dir.path(), 100);
        assert!(day_list(&dir, "20261020").unwrap().is_empty());
        assert_eq!(day_list(&dir, "20261024").unwrap().len(), 1);
    }

    #[test]
    fn rejects_invalid_dates_and_ids() {
        let dir = root();
        assert!(record(dir.path(), "2026-10-24", Format::Md, "x", UNLIMITED).is_err());
        assert!(day_list(&dir, "20261340").is_err());
        record(dir.path(), "20261024", Format::Md, "x", UNLIMITED).unwrap();
        assert!(read(dir.path(), "20261024", "../workreport.json").is_err());
        assert!(read(dir.path(), "20261024", "1700000000000-zzzzzzzz.md").is_err());
    }

    #[test]
    fn trims_a_day_to_the_version_cap() {
        let dir = root();
        for index in 0..105 {
            record_at(dir.path(), "20261024", Format::Md, &format!("版本{index}"), UNLIMITED, 1_700_000_000_000).unwrap();
        }
        let versions = day_list(&dir, "20261024").unwrap();
        assert_eq!(versions.len(), MAX_VERSIONS_PER_DAY);
        assert_eq!(read(dir.path(), "20261024", &versions[0].id).unwrap(), "版本104");
        assert_eq!(read(dir.path(), "20261024", &versions[MAX_VERSIONS_PER_DAY - 1].id).unwrap(), "版本5");
    }
}
