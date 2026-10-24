use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File},
    future::Future,
    io::{self, Read, Write},
    path::Path,
    time::Duration,
};
use tempfile::NamedTempFile;

const FILE_NAME: &str = "holiday-data.json";
const MAX_DOWNLOAD_BYTES: usize = 256 * 1024;
const MAX_CACHE_BYTES: usize = 8 * 1024 * 1024;
const MIN_YEAR: i32 = 2007;
const MAX_YEAR: i32 = 2100;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub date: String,
    pub name: String,
    pub is_off_day: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Year {
    pub year: i32,
    pub days: Vec<Day>,
    pub synced_at: String,
    pub source_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub year: i32,
    pub message: String,
    pub attempted_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HolidayState {
    #[serde(default)]
    pub years: Vec<Year>,
    #[serde(default)]
    pub failures: Vec<Failure>,
}

#[derive(Deserialize)]
struct Download {
    year: i32,
    days: Vec<Day>,
}

/// Holiday metadata belongs to the app, independently of the active workspace.
/// This lock also prevents an older request from overwriting a newer result.
#[derive(Default)]
pub struct Runtime {
    lock: tokio::sync::Mutex<()>,
}

impl Runtime {
    pub async fn load(&self, directory: &Path) -> Result<HolidayState, String> {
        let _guard = self.lock.lock().await;
        load(&directory.join(FILE_NAME))
    }

    /// Called only by the explicit manual-sync command, never on app startup.
    pub async fn sync(&self, directory: &Path, year: i32) -> Result<HolidayState, String> {
        validate_year(year)?;
        let _guard = self.lock.lock().await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("WorkReport/0.1 holiday-sync")
            .build()
            .map_err(|_| "无法初始化放假信息同步，请稍后重试".to_string());
        synchronize_with(&directory.join(FILE_NAME), year, |url| {
            let client = client.clone();
            async move {
                let client = client?;
                download(&client, &url).await
            }
        })
        .await
    }
}

fn validate_year(year: i32) -> Result<(), String> {
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
        return Err(format!("仅支持同步 {MIN_YEAR} 至 {MAX_YEAR} 年的放假信息"));
    }
    Ok(())
}

fn source_urls(year: i32) -> [String; 2] {
    [
        format!("https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/{year}.json"),
        format!("https://cdn.jsdelivr.net/gh/NateScarlet/holiday-cn@master/{year}.json"),
    ]
}

fn validate_days(year: i32, days: &[Day]) -> Result<(), String> {
    validate_year(year)?;
    if days.is_empty() || days.len() > 400 {
        return Err("放假数据缺少日期或日期数量异常".into());
    }
    // A notice for the following year can include dates in the previous December.
    let first = NaiveDate::from_ymd_opt(year - 1, 12, 1).unwrap();
    let last = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    let mut seen = HashSet::new();
    let mut has_current_year = false;
    for day in days {
        let date = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d")
            .map_err(|_| "放假数据包含无效日期".to_string())?;
        if day.date.len() != 10 || date.format("%Y-%m-%d").to_string() != day.date {
            return Err("放假数据的日期格式无效".into());
        }
        if date < first || date > last {
            return Err("放假数据包含超出该年度范围的日期".into());
        }
        if !seen.insert(&day.date) {
            return Err("放假数据包含重复日期".into());
        }
        if day.name.trim().is_empty()
            || day.name.trim().chars().count() > 100
            || day.name.chars().any(char::is_control)
        {
            return Err("放假数据包含无效的节假日名称".into());
        }
        has_current_year |= date.year() == year;
    }
    if !has_current_year {
        return Err("放假数据未包含该年度的日期".into());
    }
    Ok(())
}

fn parse_download(year: i32, bytes: &[u8]) -> Result<Vec<Day>, String> {
    if bytes.len() > MAX_DOWNLOAD_BYTES {
        return Err("放假数据过大，已停止同步".into());
    }
    let downloaded: Download = serde_json::from_slice(bytes)
        .map_err(|_| "放假数据格式无效，请稍后重试".to_string())?;
    if downloaded.year != year {
        return Err("数据源返回的放假年份与请求年份不一致".into());
    }
    validate_days(year, &downloaded.days)?;
    let mut days = downloaded.days;
    for day in &mut days {
        day.name = day.name.trim().to_string();
    }
    days.sort_by(|left, right| left.date.cmp(&right.date));
    Ok(days)
}

async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let request_error = |error: reqwest::Error| {
        if error.is_timeout() {
            "连接放假数据源超时，请检查网络后重试".to_string()
        } else {
            "无法连接放假数据源，请检查网络后重试".to_string()
        }
    };
    let mut response = client.get(url).send().await.map_err(request_error)?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("该年度放假数据尚未发布，请稍后再同步".into());
    }
    if !response.status().is_success() {
        return Err(format!("放假数据源返回 HTTP {}，请稍后重试", response.status().as_u16()));
    }
    if response.content_length().is_some_and(|size| size > MAX_DOWNLOAD_BYTES as u64) {
        return Err("放假数据过大，已停止同步".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(request_error)? {
        if bytes.len().saturating_add(chunk.len()) > MAX_DOWNLOAD_BYTES {
            return Err("放假数据过大，已停止同步".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn validate_state(state: &HolidayState) -> Result<(), String> {
    let mut years = HashSet::new();
    for item in &state.years {
        if !years.insert(item.year) {
            return Err("放假数据缓存包含重复年份".into());
        }
        validate_days(item.year, &item.days)?;
        if DateTime::parse_from_rfc3339(&item.synced_at).is_err()
            || !source_urls(item.year).contains(&item.source_url)
        {
            return Err("放假数据缓存的同步记录无效".into());
        }
    }
    let mut failures = HashSet::new();
    for failure in &state.failures {
        validate_year(failure.year)?;
        if !failures.insert(failure.year)
            || failure.message.trim().is_empty()
            || failure.message.chars().count() > 500
            || DateTime::parse_from_rfc3339(&failure.attempted_at).is_err()
        {
            return Err("放假数据缓存的失败记录无效".into());
        }
    }
    Ok(())
}

enum CacheError {
    Read(String),
    Invalid(String),
}

impl CacheError {
    fn message(self) -> String {
        match self {
            Self::Read(message) | Self::Invalid(message) => message,
        }
    }
}

fn load(path: &Path) -> Result<HolidayState, String> {
    read_state(path).map_err(CacheError::message)
}

fn read_state(path: &Path) -> Result<HolidayState, CacheError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(HolidayState::default()),
        Err(error) => return Err(CacheError::Read(format!("无法读取放假数据缓存：{error}"))),
    };
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_CACHE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| CacheError::Read(format!("无法读取放假数据缓存：{error}")))?;
    if bytes.len() > MAX_CACHE_BYTES {
        return Err(CacheError::Invalid("放假数据缓存过大，请检查 holiday-data.json".into()));
    }
    let state: HolidayState = serde_json::from_slice(&bytes)
        .map_err(|_| CacheError::Invalid("放假数据缓存内容无效，请检查 holiday-data.json".into()))?;
    validate_state(&state)
        .map_err(|error| CacheError::Invalid(format!("放假数据缓存内容无效：{error}；请检查 holiday-data.json")))?;
    Ok(state)
}

fn load_for_manual_sync(path: &Path) -> Result<HolidayState, String> {
    match read_state(path) {
        Ok(state) => Ok(state),
        Err(CacheError::Read(error)) => Err(error),
        Err(CacheError::Invalid(_)) => {
            // The user explicitly requested a fresh download. Keep every byte of
            // the damaged cache before creating a replacement; startup never does this.
            let metadata = fs::symlink_metadata(path)
                .map_err(|error| format!("无法检查放假数据缓存：{error}"))?;
            if !metadata.is_file() {
                return Err("放假数据缓存路径异常，无法自动修复".into());
            }
            let directory = path.parent().ok_or("无法确定放假数据缓存目录")?;
            let backup = directory.join(format!("holiday-data.invalid-{}.json", uuid::Uuid::new_v4()));
            fs::rename(path, backup)
                .map_err(|error| format!("无法保留损坏的放假数据缓存，原数据保持不变：{error}"))?;
            Ok(HolidayState::default())
        }
    }
}

fn save(path: &Path, state: &HolidayState) -> Result<(), String> {
    validate_state(state)?;
    let directory = path.parent().ok_or("无法确定放假数据缓存目录")?;
    fs::create_dir_all(directory).map_err(|error| format!("无法创建放假数据缓存目录：{error}"))?;
    let bytes = serde_json::to_vec_pretty(state).map_err(|_| "无法整理放假数据缓存")?;
    if bytes.len() > MAX_CACHE_BYTES {
        return Err("放假数据缓存过大，原数据保持不变".into());
    }
    let mut temporary = NamedTempFile::new_in(directory)
        .map_err(|error| format!("无法创建放假数据临时文件：{error}"))?;
    temporary.write_all(&bytes).map_err(|error| format!("无法写入放假数据缓存：{error}"))?;
    temporary.as_file().sync_all().map_err(|error| format!("无法同步放假数据缓存：{error}"))?;
    // 带上目标路径：失败时能直接判断是权限、磁盘还是目录问题。
    temporary.persist(path).map(|_| ()).map_err(|error| {
        format!("无法保存放假数据（{}），原数据保持不变：{}", path.display(), error.error)
    })
}

async fn synchronize_with<F, Fut>(path: &Path, year: i32, fetch: F) -> Result<HolidayState, String>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, String>>,
{
    validate_year(year)?;
    let mut state = load_for_manual_sync(path)?;
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut failure_message = "无法获取放假数据，请稍后重试".to_string();
    for url in source_urls(year) {
        let result = match fetch(url.clone()).await {
            Ok(bytes) => parse_download(year, &bytes),
            Err(error) => Err(error),
        };
        match result {
            Ok(days) => {
                state.years.retain(|item| item.year != year);
                state.years.push(Year {
                    year,
                    days,
                    synced_at: timestamp,
                    source_url: url,
                });
                state.years.sort_by_key(|item| item.year);
                state.failures.retain(|item| item.year != year);
                save(path, &state)?;
                return Ok(state);
            }
            Err(error) => failure_message = error,
        }
    }
    // Keep the last successful data usable even when a later manual refresh fails.
    state.failures.retain(|item| item.year != year);
    state.failures.push(Failure {
        year,
        message: failure_message.clone(),
        attempted_at: timestamp,
    });
    state.failures.sort_by_key(|item| item.year);
    save(path, &state)
        .map_err(|error| format!("{failure_message}；同步失败记录未能保存：{error}"))?;
    Err(failure_message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    fn bytes(year: i32) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "year": year,
            "papers": ["https://www.gov.cn/holiday-notice"],
            "days": [
                { "date": format!("{year}-01-04"), "name": "元旦", "isOffDay": false },
                { "date": format!("{year}-01-01"), "name": " 元旦 ", "isOffDay": true }
            ]
        })).unwrap()
    }

    fn saved_year(year: i32) -> Year {
        Year {
            year,
            days: parse_download(year, &bytes(year)).unwrap(),
            synced_at: "2026-01-01T00:00:00Z".into(),
            source_url: source_urls(year)[0].clone(),
        }
    }

    #[test]
    fn missing_cache_is_empty_but_corrupt_cache_is_an_explicit_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        assert_eq!(load(&path).unwrap(), HolidayState::default());
        fs::write(&path, "{truncated").unwrap();
        assert!(load(&path).unwrap_err().contains("缓存内容无效"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{truncated");
    }

    #[test]
    fn response_uses_exact_year_trims_names_and_orders_dates() {
        let days = parse_download(2026, &bytes(2026)).unwrap();
        assert_eq!(days[0].date, "2026-01-01");
        assert_eq!(days[0].name, "元旦");
        assert!(days[0].is_off_day);
        assert!(!days[1].is_off_day);
        assert!(parse_download(2027, &bytes(2026)).unwrap_err().contains("年份"));
    }

    #[test]
    fn accepts_previous_december_but_rejects_other_year_boundaries() {
        let mut value: serde_json::Value = serde_json::from_slice(&bytes(2027)).unwrap();
        value["days"].as_array_mut().unwrap().push(json!({
            "date": "2026-12-31", "name": "元旦", "isOffDay": true
        }));
        assert!(parse_download(2027, &serde_json::to_vec(&value).unwrap()).is_ok());
        for date in ["2026-11-30", "2028-01-01", "2027-02-29", "2027-1-01"] {
            value["days"][2]["date"] = json!(date);
            assert!(parse_download(2027, &serde_json::to_vec(&value).unwrap()).is_err(), "{date}");
        }
        value["days"] = json!([{ "date": "2026-12-31", "name": "元旦", "isOffDay": true }]);
        assert!(parse_download(2027, &serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn rejects_duplicate_empty_invalid_and_oversized_downloads() {
        for days in [
            json!([]),
            json!([{ "date": "2026-01-01", "name": "", "isOffDay": true }]),
            json!([{ "date": "2026-01-01", "name": "元旦\n", "isOffDay": true }]),
            json!([{ "date": "2026-01-01", "name": "元旦", "isOffDay": "true" }]),
            json!([
                { "date": "2026-01-01", "name": "元旦", "isOffDay": true },
                { "date": "2026-01-01", "name": "补班", "isOffDay": false }
            ]),
        ] {
            let value = json!({ "year": 2026, "days": days });
            assert!(parse_download(2026, &serde_json::to_vec(&value).unwrap()).is_err());
        }
        assert!(parse_download(2026, &vec![b' '; MAX_DOWNLOAD_BYTES + 1]).is_err());
        assert!(validate_year(2006).is_err());
        assert!(validate_year(2101).is_err());
    }

    #[tokio::test]
    async fn successful_sync_persists_replaces_one_year_and_clears_only_its_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        let earlier = saved_year(2025);
        let mut stale = saved_year(2026);
        stale.days[0].name = "旧名称".into();
        save(&path, &HolidayState {
            years: vec![earlier.clone(), stale],
            failures: vec![
                Failure { year: 2026, message: "上次失败".into(), attempted_at: "2026-01-01T00:00:00Z".into() },
                Failure { year: 2027, message: "未发布".into(), attempted_at: "2026-01-01T00:00:00Z".into() },
            ],
        }).unwrap();
        let state = synchronize_with(&path, 2026, |_| async { Ok(bytes(2026)) }).await.unwrap();
        assert_eq!(state, load(&path).unwrap());
        assert_eq!(state.years.len(), 2);
        assert_eq!(state.years[0], earlier);
        assert_eq!(state.years[1].days[0].name, "元旦");
        assert_eq!(state.failures.len(), 1);
        assert_eq!(state.failures[0].year, 2027);
        assert!(DateTime::parse_from_rfc3339(&state.years[1].synced_at).is_ok());
        let json = serde_json::to_value(&state).unwrap();
        assert!(json["years"][1].get("syncedAt").is_some());
        assert_eq!(json["years"][1]["days"][0]["isOffDay"], true);
    }

    #[tokio::test]
    async fn fallback_is_fixed_and_records_the_source_that_supplied_valid_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        let requested = Arc::new(Mutex::new(Vec::new()));
        let state = synchronize_with(&path, 2026, |url| {
            requested.lock().unwrap().push(url.clone());
            async move {
                if url.contains("raw.githubusercontent.com") {
                    Ok(b"invalid JSON".to_vec())
                } else {
                    Ok(bytes(2026))
                }
            }
        }).await.unwrap();
        assert_eq!(*requested.lock().unwrap(), source_urls(2026).to_vec());
        assert_eq!(state.years[0].source_url, source_urls(2026)[1]);
    }

    #[tokio::test]
    async fn failed_sync_keeps_last_valid_data_and_persists_only_latest_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        let earlier = saved_year(2026);
        save(&path, &HolidayState { years: vec![earlier.clone()], failures: vec![] }).unwrap();
        for message in ["连接超时", "放假年份不一致"] {
            assert_eq!(synchronize_with(&path, 2026, |_| async { Err(message.to_string()) }).await.unwrap_err(), message);
            let state = load(&path).unwrap();
            assert_eq!(state.years, vec![earlier.clone()]);
            assert_eq!(state.failures.len(), 1);
            assert_eq!(state.failures[0].message, message);
            assert!(DateTime::parse_from_rfc3339(&state.failures[0].attempted_at).is_ok());
        }
    }

    #[tokio::test]
    async fn rejected_download_does_not_turn_existing_cache_into_success() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        assert!(synchronize_with(&path, 2026, |_| async { Ok(bytes(2027)) }).await.is_err());
        let state = load(&path).unwrap();
        assert!(state.years.is_empty());
        assert_eq!(state.failures.len(), 1);
        assert_eq!(state.failures[0].year, 2026);
    }

    #[tokio::test]
    async fn manual_sync_repairs_corrupt_cache_and_preserves_the_original() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        fs::write(&path, "invalid cache").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);

        let state = synchronize_with(&path, 2026, |_| async { Ok(bytes(2026)) }).await.unwrap();
        assert_eq!(state, load(&path).unwrap());
        assert_eq!(state.years.len(), 1);
        let files: Vec<_> = fs::read_dir(directory.path()).unwrap().map(|item| item.unwrap().path()).collect();
        assert_eq!(files.len(), 2);
        let backup = files.iter().find(|file| file.file_name().unwrap().to_string_lossy().starts_with("holiday-data.invalid-")).unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), "invalid cache");
    }

    #[tokio::test]
    async fn failed_manual_repair_keeps_invalid_backup_and_persists_a_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        fs::write(&path, "invalid cache").unwrap();
        assert!(synchronize_with(&path, 2026, |_| async { Err("连接超时".into()) }).await.is_err());
        let state = load(&path).unwrap();
        assert!(state.years.is_empty());
        assert_eq!(state.failures[0].message, "连接超时");
        let files: Vec<_> = fs::read_dir(directory.path()).unwrap().map(|item| item.unwrap().path()).collect();
        assert_eq!(files.len(), 2);
        let backup = files.iter().find(|file| file.file_name().unwrap().to_string_lossy().starts_with("holiday-data.invalid-")).unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), "invalid cache");
    }

    #[test]
    fn validates_cached_years_sources_and_timestamps() {
        let mut state = HolidayState { years: vec![saved_year(2026)], failures: vec![] };
        assert!(validate_state(&state).is_ok());
        state.years[0].source_url = "https://untrusted.example/data".into();
        assert!(validate_state(&state).is_err());
        state.years[0] = saved_year(2026);
        state.years[0].synced_at = "not a time".into();
        assert!(validate_state(&state).is_err());
        state.years[0] = saved_year(2026);
        state.years.push(saved_year(2026));
        assert!(validate_state(&state).is_err());
    }

    #[test]
    fn failed_atomic_replace_keeps_destination_and_removes_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep.txt"), "original destination").unwrap();
        assert!(save(&path, &HolidayState::default()).is_err());
        assert_eq!(fs::read_to_string(path.join("keep.txt")).unwrap(), "original destination");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn runtime_load_does_not_create_a_cache_or_contact_the_network() {
        let directory = tempfile::tempdir().unwrap();
        let state = Runtime::default().load(directory.path()).await.unwrap();
        assert_eq!(state, HolidayState::default());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
