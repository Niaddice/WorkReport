use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use chrono::{Datelike, NaiveDate, Weekday};
use tempfile::NamedTempFile;
use workreport_core::holidays::{self, HolidayState};

const FILE_NAME: &str = "holiday-data.json";
const LOCAL_DIRECTORY: &str = "com.workreport.gpui-prototype";
const LEGACY_DIRECTORY: &str = "com.workreport.desktop";
const MAX_CACHE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DayKind {
    Holiday,
    Workday,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DayMeta {
    pub(crate) name: String,
    pub(crate) kind: DayKind,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct HolidayCalendar {
    state: HolidayState,
    synced_years: BTreeSet<i32>,
    dates: BTreeMap<NaiveDate, DayMeta>,
}

impl HolidayCalendar {
    pub(crate) fn from_state(state: HolidayState) -> Self {
        let mut synced_years = BTreeSet::new();
        let mut dates = BTreeMap::new();
        let mut years: Vec<_> = state.years.iter().collect();
        // A following year's notice can revise dates in this December.
        years.sort_by_key(|entry| entry.year);
        for entry in years {
            synced_years.insert(entry.year);
            for day in &entry.days {
                if let Ok(date) = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") {
                    dates.insert(
                        date,
                        DayMeta {
                            name: if day.is_off_day {
                                day.name.trim().to_owned()
                            } else {
                                "补班".to_owned()
                            },
                            kind: if day.is_off_day {
                                DayKind::Holiday
                            } else {
                                DayKind::Workday
                            },
                        },
                    );
                }
            }
        }
        Self {
            state,
            synced_years,
            dates,
        }
    }

    pub(crate) fn metadata(&self, date: NaiveDate) -> Option<DayMeta> {
        if let Some(day) = self.dates.get(&date) {
            return Some(day.clone());
        }
        // A synced year is authoritative even when an old fallback date is absent.
        if self.synced_years.contains(&date.year()) {
            return None;
        }
        let fallback = fallback_calendar();
        if let Some(day) = fallback.overrides.get(&date.format("%Y-%m-%d").to_string()) {
            return Some(day.clone());
        }
        fallback
            .fixed
            .get(&date.format("%m-%d").to_string())
            .map(|name| DayMeta {
                name: name.clone(),
                kind: DayKind::Holiday,
            })
    }

    pub(crate) fn is_weekend(&self, date: NaiveDate) -> bool {
        matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
            && self
                .metadata(date)
                .is_none_or(|day| day.kind != DayKind::Workday)
    }

    pub(crate) fn state(&self) -> &HolidayState {
        &self.state
    }
}

struct FallbackCalendar {
    fixed: BTreeMap<String, String>,
    overrides: BTreeMap<String, DayMeta>,
}

fn fallback_calendar() -> &'static FallbackCalendar {
    static FALLBACK: OnceLock<FallbackCalendar> = OnceLock::new();
    FALLBACK.get_or_init(|| {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../../shared/calendar/holidays.json"))
                .expect("bundled holiday calendar must be valid JSON");
        let fixed = data["fixedFestivals"]
            .as_object()
            .expect("bundled holiday calendar must contain fixedFestivals")
            .iter()
            .map(|(date, name)| {
                (
                    date.clone(),
                    name.as_str().expect("bundled festival name").to_owned(),
                )
            })
            .collect();
        let overrides = data["holidayOverrides"]
            .as_object()
            .expect("bundled holiday calendar must contain holidayOverrides")
            .iter()
            .map(|(date, day)| {
                let kind = match day["kind"].as_str() {
                    Some("holiday") => DayKind::Holiday,
                    Some("workday") => DayKind::Workday,
                    _ => panic!("bundled holiday kind"),
                };
                (
                    date.clone(),
                    DayMeta {
                        name: day["name"]
                            .as_str()
                            .expect("bundled holiday name")
                            .to_owned(),
                        kind,
                    },
                )
            })
            .collect();
        FallbackCalendar { fixed, overrides }
    })
}

pub(crate) struct HolidayUpdate {
    pub(crate) state: Option<HolidayState>,
    pub(crate) error: Option<String>,
}

pub(crate) trait HolidaySource: Send + Sync {
    fn load(&self) -> Result<HolidayState, String>;
    fn sync(&self, year: i32) -> HolidayUpdate;
}

pub(crate) struct HolidayService {
    directory: PathBuf,
    legacy_directory: Option<PathBuf>,
    runtime: holidays::Runtime,
    gate: Mutex<()>,
}

impl HolidayService {
    pub(crate) fn in_config_dir() -> Result<Self, String> {
        let config = dirs::config_dir().ok_or("无法确定放假信息配置目录")?;
        Ok(Self::in_directory(
            config.join(LOCAL_DIRECTORY),
            Some(config.join(LEGACY_DIRECTORY)),
        ))
    }

    pub(crate) fn in_directory(directory: PathBuf, legacy_directory: Option<PathBuf>) -> Self {
        Self {
            directory,
            legacy_directory,
            runtime: holidays::Runtime::default(),
            gate: Mutex::new(()),
        }
    }

    /// Keep filesystem work and the Tokio reactor on a worker even when a caller
    /// already runs inside a Tokio runtime. GPUI calls this from its executor.
    fn run<T: Send>(
        &self,
        operation: impl FnOnce(&tokio::runtime::Runtime) -> Result<T, String> + Send,
    ) -> Result<T, String> {
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let reactor = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|_| "无法启动放假信息运行环境".to_owned())?;
                    operation(&reactor)
                })
                .join()
                .map_err(|_| "放假信息任务未能完成，请重试".to_owned())?
        })
    }

    fn read(
        &self,
        reactor: &tokio::runtime::Runtime,
        directory: &Path,
    ) -> Result<HolidayState, String> {
        reactor.block_on(self.runtime.load(directory))
    }

    /// Seeding happens only after a manual sync request. It never changes the
    /// legacy cache and cannot replace a cache another process has published.
    fn seed_legacy(&self, reactor: &tokio::runtime::Runtime) -> Result<(), String> {
        if cache_exists(&self.directory)? {
            return Ok(());
        }
        let Some(legacy) = &self.legacy_directory else {
            return Ok(());
        };
        if !cache_exists(legacy)? {
            return Ok(());
        }
        // Explicit sync can recover from an invalid legacy cache into the
        // prototype's own directory. Startup still reports the invalid cache.
        let state = match self.read(reactor, legacy) {
            Ok(state) => state,
            Err(error)
                if error.starts_with("放假数据缓存内容无效")
                    || error.starts_with("放假数据缓存过大") =>
            {
                return Ok(());
            }
            // An unreadable cache may still contain valid years. Preserve the
            // error instead of publishing a new cache that would hide them.
            Err(error) => return Err(error),
        };
        self.publish_seed(&state)
    }

    fn publish_seed(&self, state: &HolidayState) -> Result<(), String> {
        let bytes = serde_json::to_vec(state).map_err(|_| "无法整理旧版放假信息".to_owned())?;
        if bytes.len() > MAX_CACHE_BYTES {
            return Err("旧版放假信息过大，原数据保持不变".into());
        }
        fs::create_dir_all(&self.directory)
            .map_err(|error| format!("无法创建原型放假信息目录：{error}"))?;
        let mut file = NamedTempFile::new_in(&self.directory)
            .map_err(|error| format!("无法创建放假信息迁移临时文件：{error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("无法写入原型放假信息：{error}"))?;
        file.as_file()
            .sync_all()
            .map_err(|error| format!("无法同步原型放假信息：{error}"))?;
        match file.persist_noclobber(self.directory.join(FILE_NAME)) {
            Ok(_) => Ok(()),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(format!("无法迁移放假信息，原数据保持不变：{}", error.error)),
        }
    }
}

impl HolidaySource for HolidayService {
    fn load(&self) -> Result<HolidayState, String> {
        let _guard = self.gate.lock().map_err(|_| "放假信息任务不可用，请重试")?;
        self.run(|reactor| {
            if cache_exists(&self.directory)? {
                return self.read(reactor, &self.directory);
            }
            match &self.legacy_directory {
                Some(legacy) => self.read(reactor, legacy),
                None => Ok(HolidayState::default()),
            }
        })
    }

    fn sync(&self, year: i32) -> HolidayUpdate {
        if !(2007..=2100).contains(&year) {
            return HolidayUpdate {
                state: None,
                error: Some("仅支持同步 2007 至 2100 年的放假信息".into()),
            };
        }
        let result = (|| {
            let _guard = self.gate.lock().map_err(|_| "放假信息任务不可用，请重试")?;
            self.run(|reactor| {
                self.seed_legacy(reactor)?;
                match reactor.block_on(self.runtime.sync(&self.directory, year)) {
                    Ok(state) => Ok(HolidayUpdate {
                        state: Some(state),
                        error: None,
                    }),
                    Err(error) => Ok(HolidayUpdate {
                        state: self.read(reactor, &self.directory).ok(),
                        error: Some(error),
                    }),
                }
            })
        })();
        result.unwrap_or_else(|error| HolidayUpdate {
            state: None,
            error: Some(error),
        })
    }
}

fn cache_exists(directory: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(directory.join(FILE_NAME)) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("无法检查放假信息缓存：{error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use workreport_core::holidays::{Day, Failure, Year};

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn year(year: i32, days: &[(&str, &str, bool)]) -> Year {
        Year {
            year,
            days: days
                .iter()
                .map(|(date, name, is_off_day)| Day {
                    date: (*date).to_owned(),
                    name: (*name).to_owned(),
                    is_off_day: *is_off_day,
                })
                .collect(),
            synced_at: "2026-01-01T00:00:00Z".into(),
            source_url: format!(
                "https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/{year}.json"
            ),
        }
    }

    fn cache(directory: &Path, state: &HolidayState) -> Vec<u8> {
        fs::create_dir_all(directory).unwrap();
        let bytes = serde_json::to_vec_pretty(state).unwrap();
        fs::write(directory.join(FILE_NAME), &bytes).unwrap();
        bytes
    }

    #[test]
    fn offline_fallback_and_makeup_weekends_match_the_existing_calendar() {
        let calendar = HolidayCalendar::default();
        assert_eq!(
            calendar.metadata(date("2026-02-17")),
            Some(DayMeta {
                name: "春节".into(),
                kind: DayKind::Holiday
            })
        );
        assert_eq!(
            calendar.metadata(date("2026-10-10")),
            Some(DayMeta {
                name: "补班".into(),
                kind: DayKind::Workday
            })
        );
        assert!(!calendar.is_weekend(date("2026-10-10")));
        assert!(calendar.is_weekend(date("2026-10-11")));
        assert_eq!(calendar.metadata(date("2028-01-01")).unwrap().name, "元旦");
        assert!(calendar.metadata(date("2026-11-19")).is_none());
    }

    #[test]
    fn a_synced_year_is_authoritative_and_newer_notices_override_previous_december() {
        let previous = year(2026, &[("2026-12-31", "旧安排", false)]);
        let next = year(
            2027,
            &[
                ("2027-01-01", "元旦", true),
                ("2026-12-31", "元旦假期", true),
            ],
        );
        let state = HolidayState {
            years: vec![next, previous],
            failures: vec![],
        };
        let calendar = HolidayCalendar::from_state(state.clone());
        assert_eq!(calendar.state(), &state);
        assert_eq!(
            calendar.metadata(date("2026-12-31")).unwrap().name,
            "元旦假期"
        );
        assert!(calendar.metadata(date("2026-10-10")).is_none());
        assert!(calendar.metadata(date("2026-10-01")).is_none());
        assert!(calendar.is_weekend(date("2026-10-10")));
        assert_eq!(calendar.metadata(date("2028-01-01")).unwrap().name, "元旦");
    }

    #[test]
    fn synced_makeup_days_use_the_shared_workday_name_and_remove_weekend_styling() {
        let calendar = HolidayCalendar::from_state(HolidayState {
            years: vec![year(
                2026,
                &[
                    ("2026-10-10", "调整后的假期", true),
                    ("2026-10-11", "国庆节", false),
                ],
            )],
            failures: vec![],
        });
        assert_eq!(
            calendar.metadata(date("2026-10-10")).unwrap().name,
            "调整后的假期"
        );
        assert!(calendar.is_weekend(date("2026-10-10")));
        assert_eq!(calendar.metadata(date("2026-10-11")).unwrap().name, "补班");
        assert!(!calendar.is_weekend(date("2026-10-11")));
    }

    #[test]
    fn startup_load_is_read_only_prefers_local_and_preserves_a_broken_local_cache() {
        let temporary = tempfile::tempdir().unwrap();
        let legacy = temporary.path().join(LEGACY_DIRECTORY);
        let local = temporary.path().join(LOCAL_DIRECTORY);
        let old = HolidayState {
            years: vec![year(2026, &[("2026-01-01", "旧数据", true)])],
            failures: vec![],
        };
        let old_bytes = cache(&legacy, &old);
        let service = HolidayService::in_directory(local.clone(), Some(legacy.clone()));
        assert_eq!(service.load().unwrap(), old);
        assert!(!local.exists());
        assert_eq!(fs::read(legacy.join(FILE_NAME)).unwrap(), old_bytes);

        let new = HolidayState {
            years: vec![year(2027, &[("2027-01-01", "原型数据", true)])],
            failures: vec![],
        };
        cache(&local, &new);
        assert_eq!(service.load().unwrap(), new);
        fs::write(local.join(FILE_NAME), "{broken").unwrap();
        assert!(service.load().unwrap_err().contains("缓存内容无效"));
        assert_eq!(
            fs::read_to_string(local.join(FILE_NAME)).unwrap(),
            "{broken"
        );
        assert_eq!(fs::read(legacy.join(FILE_NAME)).unwrap(), old_bytes);
        assert_eq!(fs::read_dir(&local).unwrap().count(), 1);
    }

    #[test]
    fn missing_caches_and_invalid_sync_years_create_nothing() {
        let temporary = tempfile::tempdir().unwrap();
        let service = HolidayService::in_directory(
            temporary.path().join(LOCAL_DIRECTORY),
            Some(temporary.path().join(LEGACY_DIRECTORY)),
        );
        assert_eq!(service.load().unwrap(), HolidayState::default());
        for year in [2006, 2101] {
            let result = service.sync(year);
            assert!(result.state.is_none());
            assert!(result.error.unwrap().contains("2007 至 2100"));
        }
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    }

    #[test]
    fn manual_seed_preserves_every_valid_legacy_year_and_the_original_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let legacy = temporary.path().join(LEGACY_DIRECTORY);
        let local = temporary.path().join(LOCAL_DIRECTORY);
        let state = HolidayState {
            years: vec![
                year(2026, &[("2026-01-01", "元旦", true)]),
                year(2027, &[("2027-01-01", "元旦", true)]),
            ],
            failures: vec![Failure {
                year: 2028,
                message: "尚未发布".into(),
                attempted_at: "2026-01-01T00:00:00Z".into(),
            }],
        };
        let before = cache(&legacy, &state);
        let service = HolidayService::in_directory(local.clone(), Some(legacy.clone()));
        assert_eq!(service.load().unwrap(), state);
        assert!(!local.exists());
        service.run(|reactor| service.seed_legacy(reactor)).unwrap();
        assert_eq!(service.load().unwrap(), state);
        assert_eq!(fs::read(legacy.join(FILE_NAME)).unwrap(), before);
        assert_eq!(fs::read_dir(&local).unwrap().count(), 1);
    }

    #[test]
    fn seeding_does_not_replace_a_cache_another_process_has_already_published() {
        let temporary = tempfile::tempdir().unwrap();
        let local = temporary.path().join(LOCAL_DIRECTORY);
        let winner = HolidayState {
            years: vec![year(2027, &[("2027-01-01", "新安排", true)])],
            failures: vec![],
        };
        let before = cache(&local, &winner);
        let service = HolidayService::in_directory(local.clone(), None);
        service.publish_seed(&HolidayState::default()).unwrap();
        assert_eq!(fs::read(local.join(FILE_NAME)).unwrap(), before);
        assert_eq!(service.load().unwrap(), winner);
        assert_eq!(fs::read_dir(local).unwrap().count(), 1);
    }

    #[test]
    fn damaged_legacy_cache_is_reported_at_startup_and_skipped_only_for_manual_seed() {
        let temporary = tempfile::tempdir().unwrap();
        let legacy = temporary.path().join(LEGACY_DIRECTORY);
        let local = temporary.path().join(LOCAL_DIRECTORY);
        fs::create_dir(&legacy).unwrap();
        fs::write(legacy.join(FILE_NAME), "{broken legacy").unwrap();
        let service = HolidayService::in_directory(local.clone(), Some(legacy.clone()));
        assert!(service.load().is_err());
        service.run(|reactor| service.seed_legacy(reactor)).unwrap();
        assert!(!local.exists());
        assert_eq!(
            fs::read_to_string(legacy.join(FILE_NAME)).unwrap(),
            "{broken legacy"
        );
        assert_eq!(fs::read_dir(legacy).unwrap().count(), 1);
    }

    #[test]
    fn an_unreadable_legacy_cache_is_not_treated_as_invalid_data_during_seeding() {
        let temporary = tempfile::tempdir().unwrap();
        let legacy = temporary.path().join(LEGACY_DIRECTORY);
        let local = temporary.path().join(LOCAL_DIRECTORY);
        fs::create_dir_all(legacy.join(FILE_NAME)).unwrap();
        fs::write(legacy.join(FILE_NAME).join("keep"), "original").unwrap();
        let service = HolidayService::in_directory(local.clone(), Some(legacy.clone()));
        assert!(service.load().is_err());
        assert!(service.run(|reactor| service.seed_legacy(reactor)).is_err());
        assert!(!local.exists());
        assert_eq!(
            fs::read_to_string(legacy.join(FILE_NAME).join("keep")).unwrap(),
            "original"
        );
    }

    #[test]
    fn a_nested_tokio_caller_can_load_without_a_runtime_panic() {
        let temporary = tempfile::tempdir().unwrap();
        let service = HolidayService::in_directory(temporary.path().join(LOCAL_DIRECTORY), None);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let state = runtime.block_on(async { service.load() }).unwrap();
        assert_eq!(state, HolidayState::default());
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    }
}
