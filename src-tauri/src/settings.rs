use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::Path,
};
use tempfile::NamedTempFile;

/// 设置文件固定放在工作空间根目录，随工作空间一起迁移和备份。
pub const FILE_NAME: &str = "workreport.json";

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSettings {
    #[serde(default)]
    pub appearance: crate::appearance::Settings,
    #[serde(default)]
    pub ai: crate::ai::Settings,
    #[serde(default)]
    pub excel: crate::excel::Settings,
    #[serde(default)]
    pub summary: crate::summary::Settings,
    #[serde(default)]
    pub database: crate::database::Settings,
    /// 主界面模块布局；只影响界面，不参与业务校验。
    /// 统一保存设置时前端可能不带 ui 字段，因此这里用 Option 区分“未提交”与“已清空”。
    #[serde(default)]
    pub ui: Option<crate::ui::Preferences>,
}

pub fn load(path: &Path) -> Result<Option<StoredSettings>, String> {
    match fs::read(path) {
        Ok(bytes) => {
            let mut stored: StoredSettings = serde_json::from_slice(&bytes).map_err(|_| {
                "设置文件 workreport.json 内容无效，请修正或删除该文件后重试".to_string()
            })?;
            // 旧文件的 ui 是 axis/order 结构，读入时立即迁移成新的模块化布局，
            // 这样任何“读取后整体写回”的保存路径都不会丢掉既有布局。
            stored.ui = stored.ui.map(crate::ui::normalize);
            Ok(Some(stored))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("无法读取设置文件 workreport.json：{error}")),
    }
}

pub fn save(path: &Path, settings: &StoredSettings) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| "无法确定设置文件所在目录".to_string())?;
    fs::create_dir_all(directory).map_err(|error| format!("无法创建设置文件目录：{error}"))?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|_| "设置内容无法序列化")?;
    // Write through a temporary file so a crash never leaves a truncated workreport.json.
    let mut file = NamedTempFile::new_in(directory)
        .map_err(|error| format!("无法创建临时设置文件：{error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("无法写入设置文件：{error}"))?;
    file.as_file()
        .sync_all()
        .map_err(|error| format!("无法同步设置文件：{error}"))?;
    // Never fall back to truncating the destination: if the atomic replacement
    // fails, the previously saved settings must remain intact.
    file.persist(path)
        .map(|_| ())
        .map_err(|error| format!("无法保存设置文件，原设置保持不变：{}", error.error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{Provider, ProviderKind};

    fn provider(id: &str) -> Provider {
        Provider {
            id: id.into(),
            name: "Codex".into(),
            kind: ProviderKind::Codex,
            executable: "/usr/local/bin/codex".into(),
            model: String::new(),
            base_url: String::new(),
            api_key: String::new(),
        }
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let directory = tempfile::tempdir().unwrap();
        assert!(load(&directory.path().join(FILE_NAME)).unwrap().is_none());
    }

    #[test]
    fn failed_atomic_replace_preserves_destination_and_removes_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep.txt"), "keep existing destination").unwrap();

        assert!(save(&path, &StoredSettings::default()).is_err());

        assert_eq!(
            fs::read_to_string(path.join("keep.txt")).unwrap(),
            "keep existing destination"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn round_trips_both_sections_in_one_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        let settings = StoredSettings {
            ui: None,
            database: crate::database::Settings::default(),
            appearance: crate::appearance::Settings { scale: 110, ..Default::default() },
            ai: crate::ai::Settings {
                providers: vec![provider("one")],
                default_provider_id: Some("one".into()),
                generation_preference: "简洁客观".into(),
                summary_prompt: "按项目归类".into(),
            },
            excel: crate::excel::Settings {
                enabled: false,
                template_path: "/templates/weekly.xlsx".into(),
                prompt: "填写本周工作进展".into(),
                output_directory: "/reports".into(),
            },
            summary: crate::summary::Settings {
                output_directory: "总结/每周".into(),
            },
        };
        save(&path, &settings).unwrap();
        let restored = load(&path).unwrap().expect("settings file");
        assert_eq!(restored.appearance.scale, 110);
        assert_eq!(restored.ai.summary_prompt, "按项目归类");
        assert_eq!(restored.ai.default_provider_id.as_deref(), Some("one"));
        assert_eq!(restored.ai.providers[0].executable, "/usr/local/bin/codex");
        assert_eq!(restored.excel.template_path, "/templates/weekly.xlsx");
        assert_eq!(restored.excel.prompt, "填写本周工作进展");
        assert_eq!(restored.summary.output_directory, "总结/每周");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn updating_ai_keeps_the_saved_appearance() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        save(
            &path,
            &StoredSettings {
                ui: None,
                database: crate::database::Settings::default(),
                appearance: crate::appearance::Settings { scale: 120, ..Default::default() },
                ai: crate::ai::Settings::default(),
                excel: crate::excel::Settings {
                    prompt: "保留输出提示词".into(),
                    ..crate::excel::Settings::default()
                },
                summary: crate::summary::Settings {
                    output_directory: "报告/总结".into(),
                },
            },
        )
        .unwrap();
        let mut stored = load(&path).unwrap().unwrap();
        stored.ai.generation_preference = "突出进展".into();
        save(&path, &stored).unwrap();
        let restored = load(&path).unwrap().unwrap();
        assert_eq!(restored.appearance.scale, 120);
        assert_eq!(restored.ai.generation_preference, "突出进展");
        assert_eq!(restored.excel.prompt, "保留输出提示词");
        assert_eq!(restored.summary.output_directory, "报告/总结");
    }

    #[test]
    fn missing_sections_fall_back_to_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        fs::write(&path, "{}").unwrap();
        let restored = load(&path).unwrap().unwrap();
        assert_eq!(restored.appearance.scale, 100);
        assert!(restored.ai.providers.is_empty());
        assert!(!restored.excel.enabled);
        assert!(restored.excel.template_path.is_empty());
        assert_eq!(restored.summary.output_directory, "总结");
        // 旧文件没有 ui 段：读取后是 None，由界面层回退默认布局。
        assert!(restored.ui.is_none());
    }

    #[test]
    fn corrupted_files_report_a_clear_error_and_are_not_replaced_by_accident() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        fs::write(&path, "{ not json").unwrap();
        let error = match load(&path) {
            Err(error) => error,
            Ok(_) => panic!("corrupted settings must not load"),
        };
        assert!(error.contains("workreport.json"));
        save(
            &path,
            &StoredSettings {
                ui: None,
                database: crate::database::Settings::default(),
                appearance: crate::appearance::Settings { scale: 90, ..Default::default() },
                ai: crate::ai::Settings::default(),
                excel: crate::excel::Settings::default(),
                summary: crate::summary::Settings::default(),
            },
        )
        .unwrap();
        assert_eq!(load(&path).unwrap().unwrap().appearance.scale, 90);
    }

    #[test]
    fn old_settings_gain_summary_default_and_summary_updates_preserve_other_sections() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(FILE_NAME);
        fs::write(&path, r#"{"appearance":{"scale":120},"ai":{"providers":[],"defaultProviderId":null,"generationPreference":"保留措辞","summaryPrompt":"保留总结提示词"},"excel":{"enabled":false,"templatePath":"/weekly.xlsx","prompt":"保留 Excel 提示词","outputDirectory":"/reports"}}"#).unwrap();
        let mut stored = load(&path).unwrap().unwrap();
        assert_eq!(stored.summary.output_directory, "总结");
        stored.summary = crate::summary::normalize_settings(
            root.path(),
            &crate::summary::Settings {
                output_directory: "总结/归档".into(),
            },
        )
        .unwrap();
        save(&path, &stored).unwrap();
        let restored = load(&path).unwrap().unwrap();
        assert_eq!(restored.summary.output_directory, "总结/归档");
        assert_eq!(restored.appearance.scale, 120);
        assert_eq!(restored.ai.generation_preference, "保留措辞");
        assert_eq!(restored.ai.summary_prompt, "保留总结提示词");
        assert_eq!(restored.excel.prompt, "保留 Excel 提示词");
        assert_eq!(restored.excel.template_path, "/weekly.xlsx");
        assert!(!root.path().join("总结").exists());
    }

    fn layout() -> crate::ui::Preferences {
        crate::ui::Preferences {
            columns: 1,
            modules: vec![
                crate::ui::Placement { id: "editor".into() },
                crate::ui::Placement { id: "calendar".into() },
            ],
            skin: crate::ui::DEFAULT_SKIN.into(),
            ..crate::ui::Preferences::default()
        }
    }

    #[test]
    fn ui_preferences_round_trip_and_stay_optional() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);

        // 没有提交过布局时，ui 段保持缺失而不是被写入默认布局。
        save(&path, &StoredSettings::default()).unwrap();
        assert!(load(&path).unwrap().unwrap().ui.is_none());

        save(&path, &StoredSettings { ui: Some(layout()), ..StoredSettings::default() }).unwrap();
        assert_eq!(load(&path).unwrap().unwrap().ui, Some(layout()));

        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["ui"]["columns"], 1);
        assert_eq!(written["ui"]["modules"][0]["id"], "editor");
        assert_eq!(written["ui"]["skin"], "default");
        assert!(written["ui"]["modules"][0].get("span").is_none());
        assert!(written["ui"].get("axis").is_none());
        assert!(written["ui"].get("order").is_none());
    }

    #[test]
    fn legacy_ui_layout_is_migrated_as_soon_as_the_file_is_loaded() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        fs::write(
            &path,
            r#"{"ui":{"axis":"column","order":["editor","calendar"],"skin":"default"}}"#,
        )
        .unwrap();

        let stored = load(&path).unwrap().unwrap();
        let ui = stored.ui.as_ref().expect("旧布局应被读取并迁移");
        assert_eq!(ui.columns, 1);
        let ids: Vec<&str> = ui.modules.iter().map(|module| module.id.as_str()).collect();
        assert_eq!(ids, vec!["editor", "calendar"]);
        assert_eq!(ui.axis, None);
        assert!(ui.order.is_empty());

        // 重新保存后文件里只有新 schema。
        save(&path, &stored).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("axis") && !text.contains("order"));
    }
}
