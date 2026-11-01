use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

fn default_scale() -> u16 {
    100
}

/// 功能栏（模块侧边栏）位置默认在右侧。
fn default_rail() -> String {
    "right".into()
}

fn default_rail_hold_ms() -> u32 {
    2000
}

pub const RAIL_SIDES: [&str; 2] = ["left", "right"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_scale")]
    pub scale: u16,
    /// 功能栏位置：left 或 right；缺省视为 right，兼容旧 appearance.json。
    #[serde(default = "default_rail")]
    pub rail: String,
    #[serde(default = "default_rail_hold_ms")]
    pub rail_hold_delay_ms: u32,
    #[serde(default = "default_rail_hold_ms")]
    pub rail_hold_duration_ms: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scale: default_scale(),
            rail: default_rail(),
            rail_hold_delay_ms: default_rail_hold_ms(),
            rail_hold_duration_ms: default_rail_hold_ms(),
        }
    }
}

pub fn validate(settings: &Settings) -> Result<(), String> {
    if !matches!(settings.scale, 80 | 90 | 100 | 110 | 120) {
        return Err("界面缩放仅支持 80%、90%、100%、110% 或 120%".into());
    }
    if !RAIL_SIDES.contains(&settings.rail.as_str()) {
        return Err("功能栏位置仅支持左侧或右侧".into());
    }
    for duration in [settings.rail_hold_delay_ms, settings.rail_hold_duration_ms] {
        if !(500..=10_000).contains(&duration) || duration % 100 != 0 {
            return Err("长按关闭的两个时长须在 0.5–10 秒之间，步长为 0.1 秒".into());
        }
    }
    Ok(())
}

pub fn load(root: &Path) -> Result<Settings, String> {
    match fs::read(root.join("appearance.json")) {
        Ok(bytes) => {
            let settings: Settings = serde_json::from_slice(&bytes).map_err(|_| {
                "界面配置文件无效，请检查应用配置目录中的 appearance.json".to_string()
            })?;
            validate(&settings)
                .map_err(|error| format!("界面配置文件 appearance.json 无效：{error}"))?;
            Ok(settings)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(_) => Err("无法读取应用配置目录中的 appearance.json".into()),
    }
}

/// Kept for tests: production settings are written to the workspace by `settings`.
#[cfg(test)]
pub fn save(root: &Path, settings: &Settings) -> Result<(), String> {
    use std::io::Write;
    use tempfile::NamedTempFile;

    validate(settings)?;
    fs::create_dir_all(root).map_err(|_| "无法创建应用配置目录")?;
    let mut file = NamedTempFile::new_in(root).map_err(|_| "无法保存界面配置")?;
    file.write_all(&serde_json::to_vec_pretty(settings).map_err(|_| "界面配置无效")?)
        .map_err(|_| "无法写入界面配置")?;
    file.as_file().sync_all().map_err(|_| "无法写入界面配置")?;
    file.persist(root.join("appearance.json"))
        .map_err(|_| "无法保存界面配置")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_use_default_without_creating_files() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("config");
        assert_eq!(load(&root).unwrap(), Settings { scale: 100, ..Default::default() });
        assert!(!root.exists());
    }

    #[test]
    fn absent_scale_uses_default() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("appearance.json"), "{}").unwrap();
        assert_eq!(load(root.path()).unwrap().scale, 100);
    }

    #[test]
    fn absent_rail_defaults_to_the_right_side() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("appearance.json"), r#"{"scale":110}"#).unwrap();
        let settings = load(root.path()).unwrap();
        assert_eq!(settings.scale, 110);
        assert_eq!(settings.rail, "right");
        assert_eq!(settings.rail_hold_delay_ms, 2000);
        assert_eq!(settings.rail_hold_duration_ms, 2000);
    }

    #[test]
    fn hold_durations_round_trip_independently_in_camel_case() {
        let root = tempfile::tempdir().unwrap();
        let settings = Settings {
            rail_hold_delay_ms: 500,
            rail_hold_duration_ms: 10_000,
            ..Default::default()
        };
        save(root.path(), &settings).unwrap();
        assert_eq!(load(root.path()).unwrap(), settings);
        let json: serde_json::Value = serde_json::from_slice(
            &fs::read(root.path().join("appearance.json")).unwrap(),
        ).unwrap();
        assert_eq!(json["railHoldDelayMs"], 500);
        assert_eq!(json["railHoldDurationMs"], 10_000);
    }

    #[test]
    fn invalid_hold_durations_do_not_overwrite_settings() {
        let root = tempfile::tempdir().unwrap();
        save(root.path(), &Settings::default()).unwrap();
        let original = fs::read(root.path().join("appearance.json")).unwrap();
        for duration in [0, 499, 501, 2050, 10_100, u32::MAX] {
            for settings in [
                Settings { rail_hold_delay_ms: duration, ..Default::default() },
                Settings { rail_hold_duration_ms: duration, ..Default::default() },
            ] {
                assert!(save(root.path(), &settings).unwrap_err().contains("长按关闭"));
                assert_eq!(fs::read(root.path().join("appearance.json")).unwrap(), original);
            }
        }
    }

    #[test]
    fn rail_accepts_only_left_and_right() {
        for rail in ["left", "right"] {
            let settings = Settings {
                rail: rail.into(),
                ..Default::default()
            };
            assert!(validate(&settings).is_ok());
        }
        for rail in ["", "top", "Right", " middle "] {
            let settings = Settings {
                rail: rail.into(),
                ..Default::default()
            };
            assert!(validate(&settings).is_err());
        }
    }

    #[test]
    fn supported_steps_round_trip_and_replace_previous_settings() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("config");
        for scale in [80, 90, 100, 110, 120] {
            let settings = Settings { scale, ..Default::default() };
            save(&root, &settings).unwrap();
            assert_eq!(load(&root).unwrap(), settings);
            assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        }
    }

    #[test]
    fn unsupported_values_never_replace_saved_settings() {
        let root = tempfile::tempdir().unwrap();
        save(root.path(), &Settings::default()).unwrap();
        let original = fs::read(root.path().join("appearance.json")).unwrap();
        for scale in [0, 79, 81, 99, 101, 119, 121, u16::MAX] {
            assert!(save(root.path(), &Settings { scale, ..Default::default() }).is_err());
            assert_eq!(
                fs::read(root.path().join("appearance.json")).unwrap(),
                original
            );
        }
    }

    #[test]
    fn invalid_saved_values_and_types_report_errors() {
        let root = tempfile::tempdir().unwrap();
        for value in [
            "79", "85", "121", "-1", "65536", "100.5", "\"100\"", "null", "true",
        ] {
            fs::write(
                root.path().join("appearance.json"),
                format!("{{\"scale\":{value}}}"),
            )
            .unwrap();
            assert!(load(root.path()).unwrap_err().contains("appearance.json"));
        }
    }

    #[test]
    fn corrupted_and_unreadable_settings_report_errors() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("appearance.json");
        fs::write(&path, "{ invalid json").unwrap();
        assert!(load(root.path()).unwrap_err().contains("appearance.json"));
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(load(root.path()).unwrap_err().contains("appearance.json"));
    }
}
