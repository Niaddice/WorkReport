use std::path::{Path, PathBuf};

use chrono::Datelike;
use gpui_kit::{
    Context, Entity, MouseButton, PathPromptOptions, Subscription, Task, Window,
    base::Disableable,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        input::{Editor, EditorState, Input, InputEvent, InputState},
    },
    div, prelude::*, px,
};
use workreport_core::{
    database, file_migration, file_rules,
    git::Repository as GitRepo,
    holidays::{self, HolidayState},
    network::{self, Mode as ProxyMode, Settings as ProxySettings},
    settings::StoredSettings,
    updates::{self, Channel as UpdateChannel, LegacyMigrationSummary, UpdateCheckResult, UpdatePreference},
    workspace::Format,
    workspace_preview,
};

const APP_BASE_VERSION: &str = "0.1.8";

use crate::{
    Prototype,
    ai_panel::run_tokio,
    backup_restore::{self, RestorePlan, RestorePolicy, RestoreResult, WorkspaceBackupBundle},
    layout::SKINS,
    theme_skin::apply_skin_palette,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    Appearance,
    WorkspaceFiles,
    Database,
    BackupRestore,
    Editor,
    Holidays,
    Summary,
    Excel,
    Git,
    Updates,
}

impl SettingsSection {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Appearance => "外观与导航",
            Self::WorkspaceFiles => "文件规则与迁移",
            Self::Database => "存储与数据库",
            Self::BackupRestore => "备份与恢复",
            Self::Editor => "编辑器与模板",
            Self::Holidays => "节假日与代理",
            Self::Summary => "总结设置",
            Self::Excel => "Excel 导出",
            Self::Git => "Git 仓库",
            Self::Updates => "更新与迁移",
        }
    }

    pub(crate) fn matches(self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        let keywords = match self {
            Self::Appearance => "外观 导航 皮肤 主题 字体 缩放 appRail 侧边栏 长按 appearance skin scale rail",
            Self::WorkspaceFiles => "文件规则 迁移 日报目录 文件名 workreport-files files migration",
            Self::Database => "存储 数据库 sqlite mysql postgresql 双存储 仅数据库 database workspaceId",
            Self::BackupRestore => "备份 恢复 导入 导出 json backup restore",
            Self::Editor => "编辑器 默认模板 自动填充 引入上篇 自定义模板 editor template",
            Self::Holidays => "节假日 放假 调休 农历 网络代理 proxy http socks5 holiday",
            Self::Summary => "总结 导出 目录 prompt ai summary",
            Self::Excel => "excel xlsx 表格 模板 填充 提示词 prompt",
            Self::Git => "git 仓库 分支 提交 token gh 凭据 环境变量 repository",
            Self::Updates => "更新 版本 渠道 main dev 降级 回退 迁移 com.workreport.desktop tauri update channel",
        };
        self.label().to_lowercase().contains(&q) || keywords.contains(&q)
    }
}

const SECTIONS: [SettingsSection; 10] = [
    SettingsSection::Appearance,
    SettingsSection::WorkspaceFiles,
    SettingsSection::Database,
    SettingsSection::BackupRestore,
    SettingsSection::Editor,
    SettingsSection::Holidays,
    SettingsSection::Summary,
    SettingsSection::Excel,
    SettingsSection::Git,
    SettingsSection::Updates,
];

pub(crate) const JOURNAL_TEMPLATE_IDS: [(&str, &str); 6] = [
    ("none", "空白（不使用模板）"),
    ("standard", "标准工作日报"),
    ("agile", "敏捷站会三问"),
    ("project", "项目重点推进"),
    ("minimal", "极简打卡清单"),
    ("custom", "自定义模板"),
];

pub(crate) fn render_journal_template(
    template_id: &str,
    format: Format,
    custom_template: &str,
    date_str: &str,
) -> String {
    let raw = match template_id {
        "none" | "" => return String::new(),
        "custom" => custom_template.trim().to_string(),
        "standard" => match format {
            Format::Md => "### 📌 今日工作产出\n1. \n2. \n\n### 🎯 明日工作计划\n1. \n2. \n\n### 💡 思考与问题\n- 无".into(),
            Format::Txt => "【今日工作产出】\n1、\n2、\n\n【明日工作计划】\n1、\n2、\n\n【思考与问题】\n- 无".into(),
        },
        "agile" => match format {
            Format::Md => "### ⏪ 昨天完成了什么\n1. \n\n### ⏩ 今天计划做什么\n1. \n\n### ⚠️ 遇到的阻碍与风险\n- 无".into(),
            Format::Txt => "【昨天完成了什么】\n1、\n\n【今天计划做什么】\n1、\n\n【遇到的阻碍与风险】\n- 无".into(),
        },
        "project" => match format {
            Format::Md => "### 🚀 核心里程碑进展\n- **重点项目**：\n\n### 📋 今日任务清单\n- [ ] \n- [ ] \n\n### 🔍 技术细节与遗留事项\n- ".into(),
            Format::Txt => "【核心里程碑进展】\n- 重点项目：\n\n【今日任务清单】\n[待办] \n[待办] \n\n【技术细节与遗留事项】\n- ".into(),
        },
        "minimal" => match format {
            Format::Md => "### 今日事项\n- [ ] \n- [ ] \n\n### 备注\n- ".into(),
            Format::Txt => "【今日事项】\n1、\n2、\n\n【备注】\n- ".into(),
        },
        _ => return String::new(),
    };
    interpolate_template(&raw, date_str)
}

fn interpolate_template(template: &str, date_str: &str) -> String {
    if template.is_empty() {
        return String::new();
    }
    let parsed = chrono::NaiveDate::parse_from_str(date_str, "%Y%m%d")
        .unwrap_or_else(|_| chrono::Local::now().date_naive());
    let year = parsed.format("%Y").to_string();
    let month = parsed.format("%m").to_string();
    let day = parsed.format("%d").to_string();
    let date_dash = format!("{year}-{month}-{day}");
    let raw_date = format!("{year}{month}{day}");
    let weekday = match parsed.weekday() {
        chrono::Weekday::Sun => "星期日",
        chrono::Weekday::Mon => "星期一",
        chrono::Weekday::Tue => "星期二",
        chrono::Weekday::Wed => "星期三",
        chrono::Weekday::Thu => "星期四",
        chrono::Weekday::Fri => "星期五",
        chrono::Weekday::Sat => "星期六",
    };
    let mut out = template.to_string();
    for (needle, val) in [
        ("{date}", date_dash.as_str()),
        ("{DATE}", date_dash.as_str()),
        ("{rawdate}", raw_date.as_str()),
        ("{rawDate}", raw_date.as_str()),
        ("{year}", year.as_str()),
        ("{yyyy}", year.as_str()),
        ("{month}", month.as_str()),
        ("{mm}", month.as_str()),
        ("{day}", day.as_str()),
        ("{dd}", day.as_str()),
        ("{weekday}", weekday),
    ] {
        out = out.replace(needle, val);
    }
    out
}

pub(crate) struct SettingsViewState {
    pub(crate) open: bool,
    pub(crate) section: SettingsSection,
    pub(crate) config_dir: PathBuf,
    pub(crate) stored: StoredSettings,
    pub(crate) baseline: StoredSettings,
    pub(crate) skin_id: String,
    pub(crate) baseline_skin_id: String,
    pub(crate) files: file_rules::Settings,
    pub(crate) migration_plan: Option<file_migration::Plan>,
    pub(crate) migrate_confirmed: bool,
    pub(crate) db_test_message: Option<String>,
    pub(crate) transfer_direction: &'static str,
    pub(crate) transfer_action: &'static str,
    pub(crate) transfer_conflict: &'static str,
    pub(crate) holiday_state: HolidayState,
    pub(crate) proxy: ProxySettings,
    pub(crate) backup_include_settings: bool,
    pub(crate) backup_bundle: Option<WorkspaceBackupBundle>,
    pub(crate) backup_plan: Option<RestorePlan>,
    pub(crate) restore_policy: RestorePolicy,
    pub(crate) restore_settings: bool,
    pub(crate) restore_result: Option<RestoreResult>,
    pub(crate) git_env_status: Option<String>,
    pub(crate) update_pref: UpdatePreference,
    pub(crate) update_checking: bool,
    pub(crate) update_status: Option<String>,
    pub(crate) update_result: Option<UpdateCheckResult>,
    pub(crate) update_download_url: Option<String>,
    pub(crate) legacy_migration: Option<LegacyMigrationSummary>,
    pub(crate) dirty: bool,
    pub(crate) confirm_discard: bool,
    pub(crate) busy: bool,
    pub(crate) holiday_syncing: bool,
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<String>,
    suppress_dirty: bool,
    task: Option<Task<()>>,
    holiday_task: Option<Task<()>>,
    update_task: Option<Task<()>>,

    // File rules inputs
    pub(crate) journal_dir_input: Entity<InputState>,
    pub(crate) journal_tpl_input: Entity<InputState>,
    pub(crate) excel_dir_input: Entity<InputState>,
    pub(crate) excel_tpl_input: Entity<InputState>,
    pub(crate) summary_dir_input: Entity<InputState>,
    pub(crate) summary_tpl_input: Entity<InputState>,

    // Database inputs
    pub(crate) db_sqlite_input: Entity<InputState>,
    pub(crate) db_host_input: Entity<InputState>,
    pub(crate) db_port_input: Entity<InputState>,
    pub(crate) db_name_input: Entity<InputState>,
    pub(crate) db_user_input: Entity<InputState>,
    pub(crate) db_password_input: Entity<InputState>,

    // Editor & Summary inputs
    pub(crate) editor_custom_tpl_editor: Entity<EditorState>,
    pub(crate) summary_output_dir_input: Entity<InputState>,

    // Holidays & Proxy inputs
    pub(crate) holiday_year_input: Entity<InputState>,
    pub(crate) proxy_url_input: Entity<InputState>,
    pub(crate) proxy_bypass_input: Entity<InputState>,

    // Excel inputs
    pub(crate) excel_template_path_input: Entity<InputState>,
    pub(crate) excel_content_prompt_editor: Entity<EditorState>,
    pub(crate) excel_fill_prompt_editor: Entity<EditorState>,

    // Git inputs
    pub(crate) git_exec_input: Entity<InputState>,
    pub(crate) git_commit_prompt_editor: Entity<EditorState>,
    pub(crate) git_new_name_input: Entity<InputState>,
    pub(crate) git_new_path_input: Entity<InputState>,
    pub(crate) git_new_author_input: Entity<InputState>,
    pub(crate) git_new_branch_input: Entity<InputState>,
    pub(crate) search_input: Entity<InputState>,

    _subs: Vec<Subscription>,
}

impl SettingsViewState {
    pub(crate) fn new(
        config_dir: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Prototype>,
    ) -> Box<Self> {
        let config_dir = config_dir.unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join(updates::GPUI_CONFIG_DIR_NAME)
        });
        let journal_dir_input = cx.new(|cx| InputState::new(window, cx).placeholder("日报目录（留空为根目录）"));
        let journal_tpl_input = cx.new(|cx| InputState::new(window, cx).placeholder("{date}"));
        let excel_dir_input = cx.new(|cx| InputState::new(window, cx).placeholder("excel"));
        let excel_tpl_input = cx.new(|cx| InputState::new(window, cx));
        let summary_dir_input = cx.new(|cx| InputState::new(window, cx).placeholder("summary"));
        let summary_tpl_input = cx.new(|cx| InputState::new(window, cx));

        let db_sqlite_input = cx.new(|cx| InputState::new(window, cx).placeholder("workreport.sqlite"));
        let db_host_input = cx.new(|cx| InputState::new(window, cx).placeholder("127.0.0.1"));
        let db_port_input = cx.new(|cx| InputState::new(window, cx).placeholder("默认端口"));
        let db_name_input = cx.new(|cx| InputState::new(window, cx).placeholder("workreport"));
        let db_user_input = cx.new(|cx| InputState::new(window, cx).placeholder("用户名"));
        let db_password_input = cx.new(|cx| InputState::new(window, cx).placeholder("密码"));

        let editor_custom_tpl_editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
        });
        let summary_output_dir_input = cx.new(|cx| InputState::new(window, cx).placeholder("summary"));

        let holiday_year_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("2026")
                .default_value(chrono::Local::now().year().to_string())
        });
        let proxy_url_input = cx.new(|cx| InputState::new(window, cx).placeholder("http://127.0.0.1:7890"));
        let proxy_bypass_input = cx.new(|cx| InputState::new(window, cx).placeholder("localhost,127.0.0.1,::1"));

        let excel_template_path_input = cx.new(|cx| InputState::new(window, cx).placeholder("/path/to/template.xlsx"));
        let excel_content_prompt_editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
        });
        let excel_fill_prompt_editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
        });

        let git_exec_input = cx.new(|cx| InputState::new(window, cx).placeholder("git 或绝对路径"));
        let git_commit_prompt_editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
        });
        let git_new_name_input = cx.new(|cx| InputState::new(window, cx).placeholder("仓库名称"));
        let git_new_path_input = cx.new(|cx| InputState::new(window, cx).placeholder("/path/to/repo"));
        let git_new_author_input = cx.new(|cx| InputState::new(window, cx).placeholder("可选：作者名/邮箱"));
        let git_new_branch_input = cx.new(|cx| InputState::new(window, cx).placeholder("可选：分支过滤"));
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索设置项…"));

        let mut subs = Vec::new();
        for input in [
            &journal_dir_input,
            &journal_tpl_input,
            &excel_dir_input,
            &excel_tpl_input,
            &summary_dir_input,
            &summary_tpl_input,
            &db_sqlite_input,
            &db_host_input,
            &db_port_input,
            &db_name_input,
            &db_user_input,
            &db_password_input,
            &summary_output_dir_input,
            &excel_template_path_input,
            &git_exec_input,
        ] {
            subs.push(cx.subscribe_in(input, window, |this, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    if let Some(sv) = &mut this.settings_view {
                        if !sv.suppress_dirty {
                            sv.dirty = true;
                            sv.confirm_discard = false;
                            cx.notify();
                        }
                    }
                }
            }));
        }
        for editor in [
            &editor_custom_tpl_editor,
            &excel_content_prompt_editor,
            &excel_fill_prompt_editor,
            &git_commit_prompt_editor,
        ] {
            subs.push(cx.subscribe_in(editor, window, |this, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    if let Some(sv) = &mut this.settings_view {
                        if !sv.suppress_dirty {
                            sv.dirty = true;
                            sv.confirm_discard = false;
                            cx.notify();
                        }
                    }
                }
            }));
        }
        subs.push(cx.subscribe_in(&search_input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                if let Some(sv) = &mut this.settings_view {
                    let q = sv.search_input.read(cx).value().to_string();
                    if !sv.section.matches(&q) {
                        if let Some(first) = SECTIONS.iter().copied().find(|s| s.matches(&q)) {
                            sv.section = first;
                        }
                    }
                    cx.notify();
                }
            }
        }));

        let stored = StoredSettings::default();
        Box::new(Self {
            open: false,
            section: SettingsSection::Appearance,
            config_dir,
            baseline: stored.clone(),
            stored,
            skin_id: "default".into(),
            baseline_skin_id: "default".into(),
            files: file_rules::Settings::default(),
            migration_plan: None,
            migrate_confirmed: false,
            db_test_message: None,
            transfer_direction: "filesToDatabase",
            transfer_action: "copy",
            transfer_conflict: "abort",
            holiday_state: HolidayState::default(),
            proxy: ProxySettings::default(),
            backup_include_settings: true,
            backup_bundle: None,
            backup_plan: None,
            restore_policy: RestorePolicy::Skip,
            restore_settings: true,
            restore_result: None,
            git_env_status: None,
            update_pref: UpdatePreference::default(),
            update_checking: false,
            update_status: None,
            update_result: None,
            update_download_url: None,
            legacy_migration: None,
            dirty: false,
            confirm_discard: false,
            busy: false,
            holiday_syncing: false,
            error: None,
            notice: None,
            suppress_dirty: false,
            task: None,
            holiday_task: None,
            update_task: None,
            journal_dir_input,
            journal_tpl_input,
            excel_dir_input,
            excel_tpl_input,
            summary_dir_input,
            summary_tpl_input,
            db_sqlite_input,
            db_host_input,
            db_port_input,
            db_name_input,
            db_user_input,
            db_password_input,
            editor_custom_tpl_editor,
            summary_output_dir_input,
            holiday_year_input,
            proxy_url_input,
            proxy_bypass_input,
            excel_template_path_input,
            excel_content_prompt_editor,
            excel_fill_prompt_editor,
            git_exec_input,
            git_commit_prompt_editor,
            git_new_name_input,
            git_new_path_input,
            git_new_author_input,
            git_new_branch_input,
            search_input,
            _subs: subs,
        })
    }
}

fn sv_mut(app: &mut Prototype) -> Option<&mut SettingsViewState> {
    app.settings_view.as_deref_mut()
}

impl Prototype {
    pub(crate) fn open_settings_section(
        &mut self,
        section: SettingsSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            self.status = "请先打开一个工作空间后再修改工作空间设置".into();
            cx.notify();
            return;
        };
        let stored = match workspace.reader.load_stored_settings() {
            Ok(stored) => stored,
            Err(error) => {
                self.status = format!("无法读取工作空间设置：{error}").into();
                cx.notify();
                return;
            }
        };
        let effective_files = match workspace.reader.effective_file_settings() {
            Ok(files) => files,
            Err(error) => {
                self.status = format!("无法读取文件规则：{error}").into();
                cx.notify();
                return;
            }
        };
        let config_dir_override = self.settings_config_dir.clone();
        let skin_default = self.module_layout.preferences.skin.clone();
        let sv_box = self.settings_view.get_or_insert_with(|| {
            SettingsViewState::new(config_dir_override.clone(), window, cx)
        });
        if let Some(dir) = config_dir_override {
            sv_box.config_dir = dir;
        }
        let config_dir = sv_box.config_dir.clone();
        let holiday_state = run_tokio(holidays::Runtime::default().load(&config_dir))
            .ok()
            .and_then(|res| res.ok())
            .unwrap_or_default();
        let proxy = network::load(&config_dir).unwrap_or_default();
        let legacy_dir = config_dir
            .parent()
            .map(|parent| parent.join(updates::TAURI_CONFIG_DIR_NAME));
        let update_pref =
            updates::load_preference_with_fallback(&config_dir, legacy_dir.as_deref())
                .unwrap_or_default();
        let legacy_migration = legacy_dir
            .as_deref()
            .map(|dir| updates::inspect_legacy_config(dir, &config_dir));

        let skin_id = stored
            .ui
            .as_ref()
            .map(|ui| ui.skin.clone())
            .filter(|skin| !skin.trim().is_empty())
            .unwrap_or(skin_default);

        let sv = self.settings_view.as_deref_mut().expect("settings_view initialized");
        sv.suppress_dirty = true;
        sv.open = true;
        sv.section = section;
        sv.stored = stored.clone();
        sv.baseline = stored.clone();
        sv.skin_id = skin_id.clone();
        sv.baseline_skin_id = skin_id;
        sv.files = effective_files.clone();
        sv.holiday_state = holiday_state;
        sv.proxy = proxy.clone();
        sv.update_pref = update_pref;
        sv.legacy_migration = legacy_migration;
        sv.migration_plan = None;
        sv.migrate_confirmed = false;
        sv.db_test_message = None;
        sv.dirty = false;
        sv.confirm_discard = false;
        sv.error = None;
        sv.notice = None;

        sv.journal_dir_input.update(cx, |i, cx| i.set_value(effective_files.journal.directory.clone(), window, cx));
        sv.journal_tpl_input.update(cx, |i, cx| i.set_value(effective_files.journal.filename_template.clone(), window, cx));
        sv.excel_dir_input.update(cx, |i, cx| i.set_value(effective_files.excel.directory.clone(), window, cx));
        sv.excel_tpl_input.update(cx, |i, cx| i.set_value(effective_files.excel.filename_template.clone(), window, cx));
        sv.summary_dir_input.update(cx, |i, cx| i.set_value(effective_files.summary.directory.clone(), window, cx));
        sv.summary_tpl_input.update(cx, |i, cx| i.set_value(effective_files.summary.filename_template.clone(), window, cx));

        sv.db_sqlite_input.update(cx, |i, cx| i.set_value(stored.database.sqlite_path.clone(), window, cx));
        sv.db_host_input.update(cx, |i, cx| i.set_value(stored.database.host.clone(), window, cx));
        sv.db_port_input.update(cx, |i, cx| {
            let port_str = if stored.database.port == 0 {
                String::new()
            } else {
                stored.database.port.to_string()
            };
            i.set_value(port_str, window, cx);
        });
        sv.db_name_input.update(cx, |i, cx| i.set_value(stored.database.database.clone(), window, cx));
        sv.db_user_input.update(cx, |i, cx| i.set_value(stored.database.user.clone(), window, cx));
        sv.db_password_input.update(cx, |i, cx| i.set_value(stored.database.password.clone(), window, cx));

        sv.editor_custom_tpl_editor.update(cx, |e, cx| e.set_value(stored.editor.custom_template.clone(), window, cx));
        sv.summary_output_dir_input.update(cx, |i, cx| i.set_value(stored.summary.output_directory.clone(), window, cx));

        sv.proxy_url_input.update(cx, |i, cx| i.set_value(proxy.url.clone(), window, cx));
        sv.proxy_bypass_input.update(cx, |i, cx| i.set_value(proxy.no_proxy.clone(), window, cx));

        sv.excel_template_path_input.update(cx, |i, cx| i.set_value(stored.excel.template_path.clone(), window, cx));
        sv.excel_content_prompt_editor.update(cx, |e, cx| e.set_value(stored.excel.content_prompt.clone(), window, cx));
        sv.excel_fill_prompt_editor.update(cx, |e, cx| e.set_value(stored.excel.prompt.clone(), window, cx));

        sv.git_exec_input.update(cx, |i, cx| i.set_value(stored.git.executable.clone(), window, cx));
        sv.git_commit_prompt_editor.update(cx, |e, cx| e.set_value(stored.git.commit_prompt.clone(), window, cx));
        sv.search_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        sv.suppress_dirty = false;
        cx.notify();
    }

    pub(crate) fn attempt_close_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sv) = sv_mut(self) else {
            return;
        };
        if sv.busy {
            return;
        }
        if sv.dirty && !sv.confirm_discard {
            sv.confirm_discard = true;
            cx.notify();
            return;
        }
        self.discard_and_close_settings(window, cx);
    }

    pub(crate) fn discard_and_close_settings(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sv) = self.settings_view.take() else {
            return;
        };
        let baseline_appearance = sv.baseline.appearance;
        let baseline_skin = sv.baseline_skin_id;
        // Revert any live appearance preview changes on discard.
        self.module_layout.set_skin(&baseline_skin);
        self.module_layout.set_rail(&baseline_appearance.rail);
        apply_skin_palette(
            &baseline_skin,
            baseline_appearance.scale,
            cx,
        );
        cx.notify();
    }

    fn collect_settings_draft(&mut self, cx: &Context<Self>) -> Result<(StoredSettings, file_rules::Settings), String> {
        let sv = self
            .settings_view
            .as_deref()
            .ok_or_else(|| "设置窗口尚未打开".to_string())?;
        let mut stored = sv.stored.clone();

        let files = file_rules::Settings {
            journal: file_rules::Rule {
                directory: sv.journal_dir_input.read(cx).value().trim().to_string(),
                filename_template: sv.journal_tpl_input.read(cx).value().trim().to_string(),
            },
            excel: file_rules::Rule {
                directory: sv.excel_dir_input.read(cx).value().trim().to_string(),
                filename_template: sv.excel_tpl_input.read(cx).value().trim().to_string(),
            },
            summary: file_rules::Rule {
                directory: sv.summary_dir_input.read(cx).value().trim().to_string(),
                filename_template: sv.summary_tpl_input.read(cx).value().trim().to_string(),
            },
        };
        stored.files = Some(files.clone());

        let summary_dir_tab = sv.summary_output_dir_input.read(cx).value().trim().to_string();
        if sv.section == SettingsSection::Summary && summary_dir_tab != files.summary.directory {
            stored.summary.output_directory = summary_dir_tab.clone();
            if let Some(file_rules) = &mut stored.files {
                file_rules.summary.directory = summary_dir_tab;
            }
        } else {
            stored.summary.output_directory = files.summary.directory.clone();
        }

        let port_text = sv.db_port_input.read(cx).value().trim().to_string();
        let port = if port_text.is_empty() {
            0
        } else {
            port_text
                .parse::<u16>()
                .map_err(|_| "数据库端口必须是 0 到 65535 之间的整数".to_string())?
        };
        stored.database.sqlite_path = sv.db_sqlite_input.read(cx).value().trim().to_string();
        stored.database.host = sv.db_host_input.read(cx).value().trim().to_string();
        stored.database.port = port;
        stored.database.database = sv.db_name_input.read(cx).value().trim().to_string();
        stored.database.user = sv.db_user_input.read(cx).value().trim().to_string();
        stored.database.password = sv.db_password_input.read(cx).value().to_string();

        stored.editor.custom_template = sv.editor_custom_tpl_editor.read(cx).value().to_string();

        stored.excel.template_path = sv.excel_template_path_input.read(cx).value().trim().to_string();
        stored.excel.content_prompt = sv.excel_content_prompt_editor.read(cx).value().to_string();
        stored.excel.prompt = sv.excel_fill_prompt_editor.read(cx).value().to_string();

        stored.git.executable = sv.git_exec_input.read(cx).value().trim().to_string();
        stored.git.commit_prompt = sv.git_commit_prompt_editor.read(cx).value().to_string();

        let mut ui = stored.ui.clone().unwrap_or_else(|| {
            self.workspace
                .as_ref()
                .and_then(|ws| ws.loaded_ui.clone())
                .unwrap_or_default()
        });
        ui.skin = sv.skin_id.clone();
        stored.ui = Some(ui);

        Ok((stored, files))
    }

    pub(crate) fn preview_file_migration(&mut self, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let expected_path = workspace.info_path();
        let root = workspace.reader.root().to_path_buf();
        let draft = self.collect_settings_draft(cx);
        let Some(sv) = sv_mut(self) else {
            return;
        };
        let (_, files) = match draft {
            Ok(draft) => draft,
            Err(error) => {
                sv.error = Some(error);
                cx.notify();
                return;
            }
        };
        match workspace_preview::preview_file_settings_at(&root, &expected_path, &files) {
            Ok(plan) => {
                sv.migration_plan = Some(plan);
                sv.error = None;
                sv.notice = Some("已生成文件目录/命名迁移预览，请确认后保存".into());
            }
            Err(error) => {
                sv.error = Some(error);
            }
        }
        cx.notify();
    }

    pub(crate) fn test_database_connection(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        if self.settings_view.as_ref().is_none_or(|sv| sv.busy) {
            return;
        }
        let expected_path = workspace.info_path();
        let root = workspace.reader.root().to_path_buf();
        let draft = self.collect_settings_draft(cx);
        let Some(sv) = sv_mut(self) else {
            return;
        };
        let (stored, _) = match draft {
            Ok(draft) => draft,
            Err(error) => {
                sv.error = Some(error);
                cx.notify();
                return;
            }
        };
        let proposed = stored.database;
        sv.busy = true;
        sv.error = None;
        sv.db_test_message = None;
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            workspace_preview::test_database_connection_at(&root, &expected_path, proposed)
        });
        let spawned = cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(sv) = sv_mut(this) {
                    sv.busy = false;
                    sv.task = None;
                    match result {
                        Ok((status, count)) => {
                            sv.db_test_message =
                                Some(format!("{status}（当前数据库已有 {count} 条日报记录）"));
                        }
                        Err(error) => {
                            sv.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        });
        if let Some(sv) = sv_mut(self) {
            sv.task = Some(spawned);
        }
    }

    pub(crate) fn run_storage_transfer(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let expected_path = workspace.info_path();
        let root = workspace.reader.root().to_path_buf();
        let Some(sv) = sv_mut(self) else {
            return;
        };
        if sv.busy {
            return;
        }
        let direction = sv.transfer_direction.to_string();
        let action = sv.transfer_action.to_string();
        let conflict = sv.transfer_conflict.to_string();
        sv.busy = true;
        sv.error = None;
        cx.notify();

        let expected_path_bg = expected_path.clone();
        let task = cx.background_executor().spawn(async move {
            workspace_preview::transfer_workspace_storage_at(
                &root,
                &expected_path_bg,
                &direction,
                &action,
                &conflict,
            )
        });
        let spawned = cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let mut should_reopen = false;
                if let Some(sv) = sv_mut(this) {
                    sv.busy = false;
                    sv.task = None;
                    match result {
                        Ok(transfer) => {
                            sv.stored.database = transfer.settings.clone();
                            sv.baseline.database = transfer.settings.clone();
                            sv.notice = Some(format!(
                                "{}（复制 {} 条，跳过 {} 条）",
                                transfer.message, transfer.copied, transfer.skipped
                            ));
                            should_reopen = true;
                        }
                        Err(error) => {
                            sv.error = Some(error);
                        }
                    }
                }
                if should_reopen {
                    this.open_workspace(PathBuf::from(&expected_path), window, cx);
                }
                cx.notify();
            });
        });
        if let Some(sv) = sv_mut(self) {
            sv.task = Some(spawned);
        }
    }

    pub(crate) fn sync_holidays_year(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(sv) = sv_mut(self) else {
            return;
        };
        if sv.holiday_syncing {
            return;
        }
        let year_text = sv.holiday_year_input.read(cx).value().trim().to_string();
        let Ok(year) = year_text.parse::<i32>() else {
            sv.error = Some("请输入有效的 4 位年份（例如 2026）".into());
            cx.notify();
            return;
        };
        let config_dir = sv.config_dir.clone();
        sv.holiday_syncing = true;
        sv.error = None;
        sv.notice = None;
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            run_tokio(holidays::Runtime::default().sync(&config_dir, year))?
        });
        let spawned = cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(sv) = sv_mut(this) {
                    sv.holiday_syncing = false;
                    sv.holiday_task = None;
                    match result {
                        Ok(state) => {
                            sv.holiday_state = state;
                            sv.notice = Some(format!("已同步 {year} 年中国放假安排缓存"));
                        }
                        Err(error) => {
                            sv.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        });
        if let Some(sv) = sv_mut(self) {
            sv.holiday_task = Some(spawned);
        }
    }

    pub(crate) fn save_network_proxy(&mut self, cx: &mut Context<Self>) {
        let Some(sv) = sv_mut(self) else {
            return;
        };
        let mut proxy = sv.proxy.clone();
        proxy.url = sv.proxy_url_input.read(cx).value().trim().to_string();
        proxy.no_proxy = sv.proxy_bypass_input.read(cx).value().trim().to_string();
        match network::save(&sv.config_dir, &proxy) {
            Ok(saved) => {
                sv.proxy = saved;
                sv.error = None;
                sv.notice = Some("已保存本机网络代理设置".into());
            }
            Err(error) => {
                sv.error = Some(error);
            }
        }
        cx.notify();
    }

    pub(crate) fn add_git_repo_from_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sv) = sv_mut(self) else {
            return;
        };
        let name = sv.git_new_name_input.read(cx).value().trim().to_string();
        let path = sv.git_new_path_input.read(cx).value().trim().to_string();
        let author = sv.git_new_author_input.read(cx).value().trim().to_string();
        let branch = sv.git_new_branch_input.read(cx).value().trim().to_string();
        if path.is_empty() {
            sv.error = Some("请填写或选择 Git 仓库目录".into());
            cx.notify();
            return;
        }
        let repo_name = if name.is_empty() {
            Path::new(&path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("未命名仓库")
                .to_string()
        } else {
            name
        };
        let id = format!(
            "repo-{}-{}",
            chrono::Local::now().timestamp_millis(),
            sv.stored.git.repositories.len()
        );
        sv.stored.git.repositories.push(GitRepo {
            id,
            name: repo_name,
            path,
            author,
            branch,
        });
        sv.dirty = true;
        sv.error = None;
        sv.git_new_name_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        sv.git_new_path_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        sv.git_new_author_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        sv.git_new_branch_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        cx.notify();
    }

    pub(crate) fn pick_excel_template_file(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择 Excel 模板 (.xlsx)".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(selected))) = paths.await {
                if let Some(path) = selected.into_iter().next() {
                    let _ = this.update_in(cx, |this, window, cx| {
                        if let Some(sv) = sv_mut(this) {
                            sv.excel_template_path_input.update(cx, |input, cx| {
                                input.set_value(path.to_string_lossy().to_string(), window, cx);
                            });
                            sv.dirty = true;
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    pub(crate) fn pick_git_repo_directory(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择 Git 仓库根目录".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(selected))) = paths.await {
                if let Some(path) = selected.into_iter().next() {
                    let _ = this.update_in(cx, |this, window, cx| {
                        if let Some(sv) = sv_mut(this) {
                            let path_str = path.to_string_lossy().to_string();
                            sv.git_new_path_input
                                .update(cx, |input, cx| input.set_value(path_str, window, cx));
                            if sv.git_new_name_input.read(cx).value().trim().is_empty() {
                                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                    sv.git_new_name_input.update(cx, |input, cx| {
                                        input.set_value(name.to_string(), window, cx)
                                    });
                                }
                            }
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    pub(crate) fn save_all_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        if self.settings_view.as_ref().is_none_or(|sv| sv.busy) {
            return;
        }
        let expected_path = workspace.info_path();
        let root = workspace.reader.root().to_path_buf();
        let draft = self.collect_settings_draft(cx);
        let Some(sv) = sv_mut(self) else {
            return;
        };
        let (stored, _) = match draft {
            Ok(draft) => draft,
            Err(error) => {
                sv.error = Some(error);
                cx.notify();
                return;
            }
        };
        let migrate_files = sv.migrate_confirmed && sv.migration_plan.is_some();
        let confirmation = if migrate_files {
            sv.migration_plan.as_ref().map(|plan| {
                let mut rename_existing = std::collections::BTreeMap::new();
                for change in &plan.filenames {
                    rename_existing.insert(change.feature, true);
                }
                file_migration::Confirmation {
                    token: plan.token.clone(),
                    rename_existing,
                }
            })
        } else {
            None
        };

        sv.busy = true;
        sv.error = None;
        sv.notice = None;
        cx.notify();

        let expected_path_bg = expected_path.clone();
        let task = cx.background_executor().spawn(async move {
            workspace_preview::save_workspace_settings_at(
                &root,
                &expected_path_bg,
                stored,
                confirmation.as_ref(),
                migrate_files,
            )
        });
        let spawned = cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let mut saved_ok = None;
                if let Some(sv) = sv_mut(this) {
                    sv.busy = false;
                    sv.task = None;
                    match result {
                        Ok(saved) => {
                            sv.stored = saved.clone();
                            sv.baseline = saved.clone();
                            sv.dirty = false;
                            sv.confirm_discard = false;
                            sv.migration_plan = None;
                            sv.migrate_confirmed = false;
                            sv.notice = Some("已保存全部工作空间设置".into());
                            sv.open = false;
                            saved_ok = Some(saved);
                        }
                        Err(error) => {
                            sv.error = Some(error);
                        }
                    }
                }
                if let Some(saved) = saved_ok {
                    this.module_layout
                        .load_from_workspace(saved.ui.as_ref(), &saved.appearance);
                    this.apply_skin_theme(window, cx);
                    this.new_journal_format = saved.database.default_format;
                    this.excel.settings = saved.excel.clone();
                    this.excel.invalidate_exports();
                    this.git.settings = saved.git.clone();
                    this.git.invalidate();
                    this.summary.reset_for_workspace(
                        saved.summary.clone(),
                        &this.ai.settings.summary_prompt,
                        &this.ai.runtime,
                        window,
                        cx,
                    );
                    if let Some(ws) = &mut this.workspace {
                        ws.loaded_ui = saved.ui.clone();
                        ws.loaded_appearance = saved.appearance.clone();
                        ws.loaded_summary = saved.summary.clone();
                    }
                    this.overview.invalidate();
                    this.directory.invalidate_tree();
                    this.calendar_details.invalidate_previews();
                    this.status = "工作空间设置已保存并生效".into();

                    let root_path = PathBuf::from(&expected_path);
                    this.open_workspace(root_path, window, cx);
                }
                cx.notify();
            });
        });
        if let Some(sv) = sv_mut(self) {
            sv.task = Some(spawned);
        }
    }

    pub(crate) fn check_git_and_gh_environment(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let executable = self
            .settings_view
            .as_deref()
            .map(|sv| sv.git_exec_input.read(cx).value().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "git".to_string());
        let task = cx.background_executor().spawn(async move {
            let git_status = match std::process::Command::new(&executable).arg("--version").output() {
                Ok(out) if out.status.success() => {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                }
                Ok(out) => format!("Git 异常：{}", String::from_utf8_lossy(&out.stderr).trim()),
                Err(_) => format!("未找到 Git 可执行文件（{executable}）"),
            };
            let gh_status = match std::process::Command::new("gh").args(["auth", "status"]).output() {
                Ok(out) if out.status.success() => "gh CLI 已登录".to_string(),
                Ok(_) => "检测到 gh CLI，但尚未登录".to_string(),
                Err(_) => "未检测到 GitHub CLI (gh)".to_string(),
            };
            format!("{git_status} · {gh_status}")
        });
        cx.spawn_in(window, async move |this, cx| {
            let summary = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(sv) = sv_mut(this) {
                    sv.git_env_status = Some(summary);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn pick_and_export_backup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择导出备份 JSON 的文件夹".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(selected))) = paths.await {
                if let Some(dir) = selected.into_iter().next() {
                    let filename = format!(
                        "workreport-backup-{}.json",
                        chrono::Local::now().format("%Y%m%d-%H%M%S")
                    );
                    let dest = dir.join(filename);
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.export_workspace_backup_to(dest, window, cx);
                    });
                }
            }
        })
        .detach();
    }

    pub(crate) fn export_workspace_backup_to(
        &mut self,
        destination: PathBuf,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let include_settings = self
            .settings_view
            .as_deref()
            .is_some_and(|sv| sv.backup_include_settings);
        let reader = workspace.reader.clone();
        if let Some(sv) = sv_mut(self) {
            sv.busy = true;
            sv.error = None;
            sv.notice = None;
        }
        cx.notify();

        let dest_display = destination.to_string_lossy().to_string();
        let task = cx.background_executor().spawn(async move {
            backup_restore::export_backup_to_file(&reader, include_settings, &destination)
        });
        let spawned = cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(sv) = sv_mut(this) {
                    sv.busy = false;
                    sv.task = None;
                    match result {
                        Ok(count) => {
                            sv.notice = Some(format!(
                                "已导出 {count} 篇日报备份至 {dest_display}"
                            ));
                        }
                        Err(error) => {
                            sv.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        });
        if let Some(sv) = sv_mut(self) {
            sv.task = Some(spawned);
        }
    }

    pub(crate) fn pick_and_plan_backup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择工作空间备份 JSON 文件".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(selected))) = paths.await {
                if let Some(path) = selected.into_iter().next() {
                    let _ = this.update_in(cx, |this, _, cx| {
                        this.load_and_plan_backup(&path, cx);
                    });
                }
            }
        })
        .detach();
    }

    pub(crate) fn load_and_plan_backup(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let plan_result = backup_restore::load_backup_file(path).and_then(|bundle| {
            let plan = backup_restore::plan_restore(&workspace.reader, &bundle)?;
            Ok((bundle, plan))
        });
        if let Some(sv) = sv_mut(self) {
            match plan_result {
                Ok((bundle, plan)) => {
                    sv.backup_bundle = Some(bundle);
                    sv.backup_plan = Some(plan);
                    sv.restore_result = None;
                    sv.error = None;
                    sv.notice = Some("已加载备份包并生成恢复预检计划".into());
                }
                Err(error) => {
                    sv.error = Some(error);
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn execute_workspace_restore(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let expected_path = workspace.info_path();
        let root = workspace.reader.root().to_path_buf();
        let Some(sv) = sv_mut(self) else {
            return;
        };
        if sv.busy {
            return;
        }
        let Some(bundle) = sv.backup_bundle.clone() else {
            sv.error = Some("请先选择一个有效的工作空间备份 JSON 文件".into());
            cx.notify();
            return;
        };
        let policy = sv.restore_policy;
        let restore_settings = sv.restore_settings;
        sv.busy = true;
        sv.error = None;
        cx.notify();

        let expected_path_bg = expected_path.clone();
        let task = cx.background_executor().spawn(async move {
            backup_restore::execute_restore(
                &root,
                &expected_path_bg,
                &bundle,
                policy,
                restore_settings,
            )
        });
        let spawned = cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let mut ok_result = false;
                if let Some(sv) = sv_mut(this) {
                    sv.busy = false;
                    sv.task = None;
                    match result {
                        Ok(res) => {
                            sv.notice = Some(format!(
                                "恢复完成：新增 {} 篇，覆盖 {} 篇，跳过 {} 篇{}",
                                res.created_count,
                                res.overwritten_count,
                                res.skipped_count,
                                if res.settings_restored { "（已同步配置）" } else { "" }
                            ));
                            sv.restore_result = Some(res);
                            sv.backup_plan = None;
                            ok_result = true;
                        }
                        Err(error) => {
                            sv.error = Some(error);
                        }
                    }
                }
                if ok_result {
                    this.open_workspace(PathBuf::from(&expected_path), window, cx);
                }
                cx.notify();
            });
        });
        if let Some(sv) = sv_mut(self) {
            sv.task = Some(spawned);
        }
    }

    pub(crate) fn set_update_channel(&mut self, channel: UpdateChannel, cx: &mut Context<Self>) {
        let Some(sv) = sv_mut(self) else {
            return;
        };
        sv.update_pref.channel = channel;
        sv.update_result = None;
        sv.update_status = None;
        match updates::save_preference(&sv.config_dir, &sv.update_pref) {
            Ok(()) => {
                sv.error = None;
                sv.notice = Some(format!("已切换更新渠道为「{}」", channel.label()));
            }
            Err(error) => sv.error = Some(error),
        }
        cx.notify();
    }

    pub(crate) fn toggle_allow_channel_switch(&mut self, cx: &mut Context<Self>) {
        let Some(sv) = sv_mut(self) else {
            return;
        };
        sv.update_pref.allow_channel_switch = !sv.update_pref.allow_channel_switch;
        match updates::save_preference(&sv.config_dir, &sv.update_pref) {
            Ok(()) => {
                sv.error = None;
                sv.notice = Some(if sv.update_pref.allow_channel_switch {
                    "已允许跨渠道切换（含经签名确认后回退旧版）".into()
                } else {
                    "已关闭跨渠道回退确认，默认仅升级到更新版本".into()
                });
            }
            Err(error) => sv.error = Some(error),
        }
        cx.notify();
    }

    pub(crate) fn check_for_app_updates(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(sv) = sv_mut(self) else {
            return;
        };
        if sv.update_checking || sv.busy {
            return;
        }
        let config_dir = sv.config_dir.clone();
        let mut pref = sv.update_pref.clone();
        sv.update_checking = true;
        sv.update_status = Some("正在检查更新…".into());
        sv.error = None;
        sv.notice = None;
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            network::initialize(config_dir.clone());
            let result = run_tokio(updates::check_channel_update(
                APP_BASE_VERSION,
                pref.channel,
                pref.allow_channel_switch,
                None,
                None,
            ))??;
            pref.last_checked = Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string());
            let _ = updates::save_preference(&config_dir, &pref);
            Ok::<_, String>((result, pref))
        });
        let spawned = cx.spawn_in(window, async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(sv) = sv_mut(this) {
                    sv.update_checking = false;
                    sv.update_task = None;
                    match outcome {
                        Ok((res, pref)) => {
                            sv.update_pref = pref;
                            match &res {
                                UpdateCheckResult::UpToDate { latest_version, download_url, .. } => {
                                    sv.update_status = Some(format!("当前已是最新版本（{latest_version}）"));
                                    sv.update_download_url = Some(download_url.clone());
                                }
                                UpdateCheckResult::UpdateAvailable { version, is_downgrade, download_url, .. } => {
                                    sv.update_status = Some(if *is_downgrade {
                                        format!("发现目标渠道可回退版本：{version}")
                                    } else {
                                        format!("发现新版本：{version}")
                                    });
                                    sv.update_download_url = Some(download_url.clone());
                                }
                                UpdateCheckResult::DowngradeRequiresConfirmation { version, download_url, .. } => {
                                    sv.update_status = Some(format!(
                                        "目标渠道版本（{version}）低于当前版本（{APP_BASE_VERSION}），请先开启“允许跨渠道切换”后再检查安装"
                                    ));
                                    sv.update_download_url = Some(download_url.clone());
                                }
                                UpdateCheckResult::Unpublished { download_url } => {
                                    sv.update_status = Some("该渠道暂未发布包含签名清单（latest.json）的版本".into());
                                    sv.update_download_url = Some(download_url.clone());
                                }
                            }
                            sv.update_result = Some(res);
                        }
                        Err(error) => {
                            sv.update_status = Some("暂时无法检查更新，可手动打开下载页。".into());
                            sv.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        });
        if let Some(sv) = sv_mut(self) {
            sv.update_task = Some(spawned);
        }
    }

    pub(crate) fn open_update_download_page(&mut self, cx: &mut Context<Self>) {
        let Some(sv) = sv_mut(self) else {
            return;
        };
        let url = sv
            .update_download_url
            .clone()
            .unwrap_or_else(|| sv.update_pref.channel.download_url());
        let launched = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(&url).spawn()
        } else if cfg!(target_os = "windows") {
            std::process::Command::new("cmd")
                .args(["/c", "start", "", &url])
                .spawn()
        } else {
            std::process::Command::new("xdg-open").arg(&url).spawn()
        };
        match launched {
            Ok(_) => sv.notice = Some(format!("已打开更新发布页面：{url}")),
            Err(error) => sv.error = Some(format!("无法打开浏览器，请手动访问 {url}：{error}")),
        }
        cx.notify();
    }

    pub(crate) fn import_legacy_tauri_config(&mut self, overwrite: bool, cx: &mut Context<Self>) {
        let Some(sv) = sv_mut(self) else {
            return;
        };
        let Some(summary) = sv.legacy_migration.as_ref() else {
            sv.error = Some("当前配置目录结构无法定位旧版 Tauri 配置目录".into());
            cx.notify();
            return;
        };
        let legacy_dir = summary.legacy_dir.clone();
        let target_dir = summary.target_dir.clone();
        match updates::migrate_legacy_config(&legacy_dir, &target_dir, overwrite) {
            Ok(copied) => {
                sv.legacy_migration = Some(updates::inspect_legacy_config(&legacy_dir, &target_dir));
                sv.update_pref =
                    updates::load_preference_with_fallback(&target_dir, Some(&legacy_dir))
                        .unwrap_or_default();
                sv.proxy = network::load(&target_dir).unwrap_or_default();
                sv.holiday_state = run_tokio(holidays::Runtime::default().load(&target_dir))
                    .ok()
                    .and_then(|res| res.ok())
                    .unwrap_or_default();
                sv.error = None;
                sv.notice = Some(if copied == 0 {
                    "旧版 Tauri 配置均已存在于当前本机目录，无需重复导入（旧版目录保持原样）".into()
                } else {
                    format!("已从旧版 Tauri（com.workreport.desktop）导入 {copied} 项本机配置（旧版目录未改动）")
                });
            }
            Err(error) => sv.error = Some(error),
        }
        cx.notify();
    }
}

#[inline(never)]
pub(crate) fn render_settings_dialog(
    app: &Prototype,
    sv: &SettingsViewState,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    let theme = cx.theme();
    let border = theme.border;
    let muted = theme.muted_foreground;
    let surface = theme.popover;
    let danger = theme.danger_foreground;
    let success = theme.success_foreground;
    let accent = theme.accent;
    let secondary = theme.secondary;

    div()
        .id("settings-modal-backdrop")
        .occlude()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .bg(gpui_kit::hsla(0., 0., 0., 0.45))
        .flex()
        .justify_center()
        .items_center()
        .p_6()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("settings-modal-card")
                .w(px(920.))
                .max_w_full()
                .h(px(640.))
                .max_h_full()
                .bg(surface)
                .border_1()
                .border_color(border)
                .rounded(px(14.))
                .shadow_lg()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(border)
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_base().child("工作空间与桌面设置"))
                        .when(sv.dirty, |bar| {
                            bar.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(accent)
                                    .child("• 未保存修改"),
                            )
                        })
                        .child(div().flex_1())
                        .child(
                            Button::new("settings-jump-ai")
                                .label("AI 服务商配置")
                                .disabled(sv.busy)
                                .px_2()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_ai_settings_dialog(window, cx);
                                })),
                        )
                        .child(
                            Button::new("settings-close")
                                .label("关闭")
                                .disabled(sv.busy)
                                .px_2()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.attempt_close_settings(window, cx);
                                })),
                        ),
                )
                .when(sv.confirm_discard, |card| {
                    card.child(
                        div()
                            .px_4()
                            .py_2()
                            .bg(secondary)
                            .border_b_1()
                            .border_color(border)
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.))
                                    .text_color(danger)
                                    .child("当前设置尚未保存，关闭将还原实时皮肤预览并放弃未保存更改。"),
                            )
                            .child(
                                Button::new("settings-discard-confirm")
                                    .label("放弃并关闭")
                                    .danger()
                                    .px_2()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.discard_and_close_settings(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("settings-discard-cancel")
                                    .label("继续编辑")
                                    .px_2()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(sv) = sv_mut(this) {
                                            sv.confirm_discard = false;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .child({
                            let query = sv.search_input.read(cx).value().to_string();
                            let matched: Vec<SettingsSection> = SECTIONS
                                .iter()
                                .copied()
                                .filter(|section| section.matches(&query))
                                .collect();
                            div()
                                .w(px(192.))
                                .border_r_1()
                                .border_color(border)
                                .p_2()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    Input::new(&sv.search_input)
                                        .aria_label("搜索设置项")
                                        .mb_1(),
                                )
                                .when(matched.is_empty(), |nav| {
                                    nav.child(
                                        div()
                                            .px_2()
                                            .py_2()
                                            .text_size(px(11.))
                                            .text_color(muted)
                                            .child("没有匹配的设置项"),
                                    )
                                })
                                .children(matched.into_iter().map(|target| {
                                    let active = sv.section == target;
                                    Button::new(format!("settings-nav-{}", target.label()))
                                        .label(target.label())
                                        .when(active, Button::primary)
                                        .disabled(sv.busy)
                                        .w_full()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(sv) = sv_mut(this) {
                                                sv.section = target;
                                                sv.error = None;
                                            }
                                            cx.notify();
                                        }))
                                }))
                                .child(div().flex_1())
                                .child(
                                    Button::new("settings-nav-ai-models")
                                        .label("AI 模型配置…")
                                        .disabled(sv.busy)
                                        .w_full()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_ai_settings_dialog(window, cx);
                                        })),
                                )
                        })
                        .child(
                            div()
                                .id("settings-section-scroll")
                                .flex_1()
                                .min_w_0()
                                .min_h_0()
                                .overflow_y_scroll()
                                .p_4()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .when_some(sv.error.clone(), |pane, error| {
                                    pane.child(
                                        div()
                                            .p_2()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(danger)
                                            .text_size(px(12.))
                                            .text_color(danger)
                                            .child(error),
                                    )
                                })
                                .when_some(sv.notice.clone(), |pane, notice| {
                                    pane.child(
                                        div()
                                            .p_2()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(success)
                                            .text_size(px(12.))
                                            .text_color(success)
                                            .child(notice),
                                    )
                                })
                                .child(render_section_body(app, sv, muted, border, cx)),
                        ),
                )
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_t_1()
                        .border_color(border)
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child("保存后将事务写入 workreport.json 并自动刷新相关模块"),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("settings-cancel")
                                .label("取消")
                                .disabled(sv.busy)
                                .px_3()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.attempt_close_settings(window, cx);
                                })),
                        )
                        .child(
                            Button::new("settings-save")
                                .label(if sv.busy { "保存中…" } else { "保存设置" })
                                .primary()
                                .disabled(sv.busy)
                                .px_3()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.save_all_settings(window, cx);
                                })),
                        ),
                ),
        )
        .into_any_element()
}

fn render_section_body(
    app: &Prototype,
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    border: gpui_kit::Hsla,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    match sv.section {
        SettingsSection::Appearance => render_appearance_section(sv, muted, border, cx).into_any_element(),
        SettingsSection::WorkspaceFiles => render_files_section(sv, muted, border, cx).into_any_element(),
        SettingsSection::Database => render_database_section(sv, muted, border, cx).into_any_element(),
        SettingsSection::BackupRestore => render_backup_restore_section(sv, muted, border, cx).into_any_element(),
        SettingsSection::Editor => render_editor_section(sv, muted, border, cx).into_any_element(),
        SettingsSection::Holidays => render_holidays_section(sv, muted, border, cx).into_any_element(),
        SettingsSection::Summary => render_summary_section(sv, muted, cx).into_any_element(),
        SettingsSection::Excel => render_excel_section(sv, muted, cx).into_any_element(),
        SettingsSection::Git => render_git_section(app, sv, muted, border, cx).into_any_element(),
        SettingsSection::Updates => render_updates_section(sv, muted, border, cx).into_any_element(),
    }
}

fn render_appearance_section(
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    border: gpui_kit::Hsla,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let scale = sv.stored.appearance.scale;
    let rail = sv.stored.appearance.rail.as_str();
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(div().text_sm().child("主题皮肤（实时预览）"))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .children(SKINS.iter().map(|skin| {
                    let id = skin.id.to_string();
                    let active = sv.skin_id == skin.id;
                    Button::new(format!("settings-skin-{}", skin.id))
                        .label(skin.name)
                        .when(active, Button::primary)
                        .px_2()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.skin_id = id.clone();
                                sv.dirty = true;
                            }
                            this.module_layout.set_skin(&id);
                            this.apply_skin_theme(window, cx);
                            cx.notify();
                        }))
                })),
        )
        .child(div().border_t_1().border_color(border))
        .child(div().text_sm().child("功能导航栏位置 (AppRail)"))
        .child(
            div()
                .flex()
                .gap_2()
                .children([("left", "左侧停靠"), ("right", "右侧停靠")].into_iter().map(
                    |(val, label)| {
                        let active = rail == val;
                        Button::new(format!("settings-rail-{val}"))
                            .label(label)
                            .when(active, Button::primary)
                            .px_3()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(sv) = sv_mut(this) {
                                    sv.stored.appearance.rail = val.to_string();
                                    sv.dirty = true;
                                }
                                this.module_layout.set_rail(val);
                                cx.notify();
                            }))
                    },
                )),
        )
        .child(div().border_t_1().border_color(border))
        .child(div().text_sm().child(format!("界面文字缩放：{scale}%")))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("settings-scale-down")
                        .label("缩小 (-5%)")
                        .disabled(scale <= 90)
                        .px_2()
                        .on_click(cx.listener(|this, _, _window, cx| {
                            if let Some(sv) = sv_mut(this) {
                                let next = (sv.stored.appearance.scale.saturating_sub(5)).max(90);
                                sv.stored.appearance.scale = next;
                                sv.dirty = true;
                                let skin = sv.skin_id.clone();
                                apply_skin_palette(&skin, next, cx);
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-scale-reset")
                        .label("100% 默认")
                        .px_2()
                        .on_click(cx.listener(|this, _, _window, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.stored.appearance.scale = 100;
                                sv.dirty = true;
                                let skin = sv.skin_id.clone();
                                apply_skin_palette(&skin, 100, cx);
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-scale-up")
                        .label("放大 (+5%)")
                        .disabled(scale >= 130)
                        .px_2()
                        .on_click(cx.listener(|this, _, _window, cx| {
                            if let Some(sv) = sv_mut(this) {
                                let next = (sv.stored.appearance.scale + 5).min(130);
                                sv.stored.appearance.scale = next;
                                sv.dirty = true;
                                let skin = sv.skin_id.clone();
                                apply_skin_palette(&skin, next, cx);
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(muted)
                        .child("范围 90%–130%"),
                ),
        )
}

fn render_files_section(
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    border: gpui_kit::Hsla,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .text_size(px(12.))
                .text_color(muted)
                .child("目录规则支持相对路径（为空表示工作空间根目录）；文件名模板支持 {date}、{startDate}、{endDate}、{time}。"),
        )
        .child(div().text_sm().child("日报规则"))
        .child(
            div()
                .flex()
                .gap_2()
                .child(Input::new(&sv.journal_dir_input).aria_label("日报目录").flex_1())
                .child(Input::new(&sv.journal_tpl_input).aria_label("日报文件名模板").flex_1()),
        )
        .child(div().text_sm().child("Excel 报告规则"))
        .child(
            div()
                .flex()
                .gap_2()
                .child(Input::new(&sv.excel_dir_input).aria_label("Excel 目录").flex_1())
                .child(Input::new(&sv.excel_tpl_input).aria_label("Excel 文件名模板").flex_1()),
        )
        .child(div().text_sm().child("总结输出规则"))
        .child(
            div()
                .flex()
                .gap_2()
                .child(Input::new(&sv.summary_dir_input).aria_label("总结目录").flex_1())
                .child(Input::new(&sv.summary_tpl_input).aria_label("总结文件名模板").flex_1()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("settings-files-preview")
                        .label("预览迁移计划")
                        .disabled(sv.busy)
                        .px_3()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.preview_file_migration(cx);
                        })),
                )
                .when(sv.migration_plan.is_some(), |row| {
                    row.child(
                        Button::new("settings-files-confirm-migrate")
                            .label(if sv.migrate_confirmed {
                                "✓ 保存时同时迁移现有文件"
                            } else {
                                "仅保存规则（不移动旧文件）"
                            })
                            .when(sv.migrate_confirmed, Button::primary)
                            .px_2()
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(sv) = sv_mut(this) {
                                    sv.migrate_confirmed = !sv.migrate_confirmed;
                                }
                                cx.notify();
                            })),
                    )
                }),
        )
        .when_some(sv.migration_plan.as_ref(), |col, plan| {
            col.child(
                div()
                    .border_1()
                    .border_color(border)
                    .rounded_sm()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_size(px(12.)).child(format!("迁移计划摘要（目录变更 {} 项，文件名变更 {} 项）", plan.directories.len(), plan.filenames.len())))
                    .children(plan.directories.iter().map(|dir| {
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(format!("目录 [{}]: `{}` -> `{}`（{} 个文件）", dir.label, dir.from, dir.to, dir.file_count))
                    }))
                    .children(plan.filenames.iter().map(|name| {
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(format!("命名 [{}]: `{}` -> `{}`（匹配 {} 个）", name.label, name.from, name.to, name.file_count))
                    }))
                    .children(plan.warnings.iter().map(|w| {
                        div().text_size(px(11.)).text_color(muted).child(format!("⚠ {w}"))
                    })),
            )
        })
}

fn render_database_section(
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    border: gpui_kit::Hsla,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let db = &sv.stored.database;
    let is_sqlite = db.engine == database::Engine::Sqlite;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("settings-db-toggle")
                        .label(if db.enabled { "✓ 已启用数据库存储" } else { "仅使用本地文件存储" })
                        .when(db.enabled, Button::primary)
                        .px_3()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.stored.database.enabled = !sv.stored.database.enabled;
                                sv.dirty = true;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-db-mode-dual")
                        .label("双写模式 (文件+DB)")
                        .when(db.mode == database::Mode::Dual, Button::primary)
                        .disabled(!db.enabled)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.stored.database.mode = database::Mode::Dual;
                                sv.dirty = true;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-db-mode-only")
                        .label("纯数据库模式")
                        .when(db.mode == database::Mode::Database, Button::primary)
                        .disabled(!db.enabled)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.stored.database.mode = database::Mode::Database;
                                sv.dirty = true;
                            }
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .children([
                    (database::Engine::Sqlite, "SQLite"),
                    (database::Engine::Mysql, "MySQL"),
                    (database::Engine::Postgres, "PostgreSQL"),
                ].into_iter().map(|(engine, label)| {
                    Button::new(format!("settings-db-engine-{label}"))
                        .label(label)
                        .when(db.engine == engine, Button::primary)
                        .px_2()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.stored.database.engine = engine;
                                sv.dirty = true;
                            }
                            cx.notify();
                        }))
                }))
                .child(div().flex_1())
                .child(
                    Button::new("settings-db-fmt-md")
                        .label("新建默认 MD")
                        .when(db.default_format == Format::Md, Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.stored.database.default_format = Format::Md;
                                sv.dirty = true;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-db-fmt-txt")
                        .label("新建默认 TXT")
                        .when(db.default_format == Format::Txt, Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.stored.database.default_format = Format::Txt;
                                sv.dirty = true;
                            }
                            cx.notify();
                        })),
                ),
        )
        .when(is_sqlite, |pane| {
            pane.child(Input::new(&sv.db_sqlite_input).aria_label("SQLite 文件路径").w_full())
        })
        .when(!is_sqlite, |pane| {
            pane.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(Input::new(&sv.db_host_input).aria_label("数据库主机").flex_1())
                            .child(Input::new(&sv.db_port_input).aria_label("数据库端口").w(px(120.)))
                            .child(Input::new(&sv.db_name_input).aria_label("数据库名").flex_1()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(Input::new(&sv.db_user_input).aria_label("数据库用户").flex_1())
                            .child(Input::new(&sv.db_password_input).aria_label("数据库密码").flex_1()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .child(div().text_size(px(11.)).text_color(muted).child("TLS 模式："))
                            .children([
                                (database::TlsMode::Disabled, "Disabled"),
                                (database::TlsMode::Preferred, "Preferred"),
                                (database::TlsMode::Required, "Required"),
                                (database::TlsMode::VerifyFull, "VerifyFull"),
                            ].into_iter().map(|(tls, label)| {
                                Button::new(format!("settings-db-tls-{label}"))
                                    .label(label)
                                    .when(db.tls_mode == tls, Button::primary)
                                    .px_2()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(sv) = sv_mut(this) {
                                            sv.stored.database.tls_mode = tls;
                                            sv.dirty = true;
                                        }
                                        cx.notify();
                                    }))
                            })),
                    ),
            )
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("settings-db-test")
                        .label("测试连接")
                        .disabled(sv.busy)
                        .px_3()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.test_database_connection(window, cx);
                        })),
                )
                .when_some(sv.db_test_message.clone(), |row, msg| {
                    row.child(div().text_size(px(12.)).text_color(muted).child(msg))
                }),
        )
        .child(div().border_t_1().border_color(border))
        .child(div().text_sm().child("存储复制与迁移工具"))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_1()
                .child(
                    Button::new("settings-xfer-f2d")
                        .label("文件 -> 数据库")
                        .when(sv.transfer_direction == "filesToDatabase", Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.transfer_direction = "filesToDatabase";
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-xfer-d2f")
                        .label("数据库 -> 文件")
                        .when(sv.transfer_direction == "databaseToFiles", Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.transfer_direction = "databaseToFiles";
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-xfer-copy")
                        .label("仅复制")
                        .when(sv.transfer_action == "copy", Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.transfer_action = "copy";
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-xfer-migrate")
                        .label("迁移并切换")
                        .when(sv.transfer_action == "migrate", Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.transfer_action = "migrate";
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-xfer-run")
                        .label("执行复制/迁移")
                        .disabled(sv.busy)
                        .px_2()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run_storage_transfer(window, cx);
                        })),
                ),
        )
}

fn render_editor_section(
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    _border: gpui_kit::Hsla,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let editor = &sv.stored.editor;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            Button::new("settings-editor-auto-apply")
                .label(if editor.auto_apply_template {
                    "✓ 新建空白日报时自动填入默认模板"
                } else {
                    "新建空白日报时不自动填入模板"
                })
                .when(editor.auto_apply_template, Button::primary)
                .px_3()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(sv) = sv_mut(this) {
                        sv.stored.editor.auto_apply_template = !sv.stored.editor.auto_apply_template;
                        sv.dirty = true;
                    }
                    cx.notify();
                })),
        )
        .child(div().text_sm().child("默认日报模板"))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .children(JOURNAL_TEMPLATE_IDS.iter().map(|(id, label)| {
                    let id_owned = (*id).to_string();
                    Button::new(format!("settings-tpl-{id}"))
                        .label(*label)
                        .when(editor.default_template_id == *id, Button::primary)
                        .px_2()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.stored.editor.default_template_id = id_owned.clone();
                                sv.dirty = true;
                            }
                            cx.notify();
                        }))
                })),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(muted)
                .child("自定义模板内容（支持变量 {date}、{year}、{month}、{day}、{weekday}）"),
        )
        .child(
            Editor::new(&sv.editor_custom_tpl_editor)
                .aria_label("自定义日报模板")
                .h(px(180.))
                .w_full(),
        )
}

fn render_holidays_section(
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    border: gpui_kit::Hsla,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(div().text_sm().child("中国法定节假日缓存与在线同步"))
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(Input::new(&sv.holiday_year_input).aria_label("同步年份").w(px(110.)))
                .child(
                    Button::new("settings-holiday-sync")
                        .label(if sv.holiday_syncing { "同步中…" } else { "手动同步该年放假数据" })
                        .primary()
                        .disabled(sv.holiday_syncing)
                        .px_3()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.sync_holidays_year(window, cx);
                        })),
                ),
        )
        .child(
            div()
                .border_1()
                .border_color(border)
                .rounded_sm()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .when(sv.holiday_state.years.is_empty(), |box_el| {
                    box_el.child(div().text_size(px(12.)).text_color(muted).child("暂无本地节假日缓存（应用启动不自动联网）"))
                })
                .children(sv.holiday_state.years.iter().map(|year| {
                    div()
                        .text_size(px(12.))
                        .child(format!("{} 年：{} 条调休/节假日记录（同步于 {}）", year.year, year.days.len(), year.synced_at))
                }))
                .children(sv.holiday_state.failures.iter().map(|fail| {
                    div()
                        .text_size(px(11.))
                        .text_color(muted)
                        .child(format!("{} 年同步记录：{}（{}）", fail.year, fail.message, fail.attempted_at))
                })),
        )
        .child(div().border_t_1().border_color(border))
        .child(div().text_sm().child("本机网络代理设置（仅影响本机节假日同步与 AI 网络请求）"))
        .child(
            div()
                .flex()
                .gap_2()
                .children([
                    (ProxyMode::System, "系统代理"),
                    (ProxyMode::Direct, "直连网络"),
                    (ProxyMode::Manual, "手动代理"),
                ].into_iter().map(|(mode, label)| {
                    Button::new(format!("settings-proxy-mode-{label}"))
                        .label(label)
                        .when(sv.proxy.mode == mode, Button::primary)
                        .px_2()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.proxy.mode = mode;
                            }
                            cx.notify();
                        }))
                })),
        )
        .when(sv.proxy.mode == ProxyMode::Manual, |pane| {
            pane.child(
                div()
                    .flex()
                    .gap_2()
                    .child(Input::new(&sv.proxy_url_input).aria_label("代理 URL").flex_1())
                    .child(Input::new(&sv.proxy_bypass_input).aria_label("绕过地址").flex_1()),
            )
        })
        .child(
            Button::new("settings-proxy-save")
                .label("保存本机代理")
                .px_3()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.save_network_proxy(cx);
                })),
        )
}

fn render_summary_section(
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    _cx: &Context<Prototype>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_sm().child("总结导出默认目录"))
        .child(
            div()
                .text_size(px(12.))
                .text_color(muted)
                .child("保存后将同步更新文件规则中的总结输出目录，AI 提示词请在 AI 服务商配置中调整。"),
        )
        .child(
            Input::new(&sv.summary_output_dir_input)
                .aria_label("总结输出目录")
                .w_full(),
        )
}

fn render_excel_section(
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            Button::new("settings-excel-enabled")
                .label(if sv.stored.excel.enabled {
                    "✓ 已启用 Excel 报告导出"
                } else {
                    "未启用 Excel 报告导出"
                })
                .when(sv.stored.excel.enabled, Button::primary)
                .px_3()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(sv) = sv_mut(this) {
                        sv.stored.excel.enabled = !sv.stored.excel.enabled;
                        sv.dirty = true;
                    }
                    cx.notify();
                })),
        )
        .child(div().text_sm().child("Excel .xlsx 模板文件路径（需绝对路径）"))
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(
                    Input::new(&sv.excel_template_path_input)
                        .aria_label("Excel 模板路径")
                        .flex_1(),
                )
                .child(
                    Button::new("settings-excel-browse")
                        .label("选择文件")
                        .px_2()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.pick_excel_template_file(window, cx);
                        })),
                ),
        )
        .child(div().text_size(px(12.)).text_color(muted).child("阶段 1：报告正文归纳提示词"))
        .child(Editor::new(&sv.excel_content_prompt_editor).aria_label("Excel 正文提示词").h(px(110.)).w_full())
        .child(div().text_size(px(12.)).text_color(muted).child("阶段 2：工作簿填表指令提示词"))
        .child(Editor::new(&sv.excel_fill_prompt_editor).aria_label("Excel 填表提示词").h(px(110.)).w_full())
}

fn render_git_section(
    app: &Prototype,
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    border: gpui_kit::Hsla,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let repos = &sv.stored.git.repositories;
    let saved_creds = &app.git.saved_credentials;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(
                    Input::new(&sv.git_exec_input)
                        .aria_label("Git 可执行文件")
                        .flex_1(),
                )
                .child(
                    Button::new("settings-git-merges")
                        .label(if sv.stored.git.include_merges {
                            "✓ 提交转日报包含 Merge"
                        } else {
                            "忽略 Merge 提交"
                        })
                        .when(sv.stored.git.include_merges, Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.stored.git.include_merges = !sv.stored.git.include_merges;
                                sv.dirty = true;
                            }
                            cx.notify();
                        })),
                ),
        )
        .child(div().text_size(px(12.)).text_color(muted).child("已配置的 Git 仓库（最多 20 个）"))
        .child(
            div()
                .border_1()
                .border_color(border)
                .rounded_sm()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .when(repos.is_empty(), |box_el| {
                    box_el.child(div().text_size(px(12.)).text_color(muted).child("尚未添加仓库"))
                })
                .children(repos.iter().map(|repo| {
                    let remove_id = repo.id.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.))
                                .child(format!("{} · {}", repo.name, repo.path)),
                        )
                        .child(
                            Button::new(format!("settings-git-remove-{}", repo.id))
                                .label("移除")
                                .px_2()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(sv) = sv_mut(this) {
                                        sv.stored
                                            .git
                                            .repositories
                                            .retain(|item| item.id != remove_id);
                                        sv.dirty = true;
                                    }
                                    cx.notify();
                                })),
                        )
                })),
        )
        .child(div().text_sm().child("添加新仓库"))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Input::new(&sv.git_new_name_input).aria_label("仓库别名").w(px(150.)))
                        .child(Input::new(&sv.git_new_path_input).aria_label("仓库本地路径").flex_1())
                        .child(
                            Button::new("settings-git-pick-dir")
                                .label("选择目录")
                                .px_2()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.pick_git_repo_directory(window, cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Input::new(&sv.git_new_author_input).aria_label("筛选作者").flex_1())
                        .child(Input::new(&sv.git_new_branch_input).aria_label("目标分支").flex_1())
                        .child(
                            Button::new("settings-git-add-repo")
                                .label("添加到列表")
                                .primary()
                                .px_3()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.add_git_repo_from_draft(window, cx);
                                })),
                        ),
                ),
        )
        .child(div().text_size(px(12.)).text_color(muted).child("AI 生成提交说明提示词"))
        .child(Editor::new(&sv.git_commit_prompt_editor).aria_label("Git 提交提示词").h(px(100.)).w_full())
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("settings-git-check-env")
                        .label("检测 Git / gh 环境")
                        .px_2()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.check_git_and_gh_environment(window, cx);
                        })),
                )
                .when_some(sv.git_env_status.clone(), |row, msg| {
                    row.child(div().text_size(px(11.)).text_color(muted).child(msg))
                }),
        )
        .child(div().border_t_1().border_color(border))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().child("当前会话保存的 HTTP Git 凭据"))
                .when(!saved_creds.is_empty(), |row| {
                    row.child(
                        Button::new("settings-git-cred-clear-all")
                            .label("清空全部")
                            .px_2()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.git.saved_credentials.clear();
                                if let Some(sv) = sv_mut(this) {
                                    sv.notice = Some("已清除全部缓存的 Git HTTP 凭据".into());
                                }
                                cx.notify();
                            })),
                    )
                }),
        )
        .when(saved_creds.is_empty(), |pane| {
            pane.child(
                div()
                    .text_size(px(11.))
                    .text_color(muted)
                    .child("尚未在本会话保存 HTTP Git 凭据；认证失败时填写并确认重试会自动记忆。"),
            )
        })
        .when(!saved_creds.is_empty(), |pane| {
            pane.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(saved_creds.iter().enumerate().map(|(idx, entry)| {
                        div()
                            .border_1()
                            .border_color(border)
                            .rounded_sm()
                            .px_2()
                            .py_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(11.))
                                    .child(format!("{} · {}", entry.url, entry.username)),
                            )
                            .child(
                                Button::new(format!("settings-git-cred-clear-{idx}"))
                                    .label("清除")
                                    .px_2()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if idx < this.git.saved_credentials.len() {
                                            this.git.saved_credentials.remove(idx);
                                        }
                                        if let Some(sv) = sv_mut(this) {
                                            sv.notice = Some("已清除该来源的 Git 凭据".into());
                                        }
                                        cx.notify();
                                    })),
                            )
                    })),
            )
        })
}

fn render_backup_restore_section(
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    border: gpui_kit::Hsla,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(div().text_sm().child("导出工作空间备份包 (JSON)"))
        .child(
            div()
                .text_size(px(12.))
                .text_color(muted)
                .child("兼容本地文件、双存储与仅数据库模式，导出格式与原版 WorkReport 备份 JSON 互通。"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("settings-backup-include-settings")
                        .label(if sv.backup_include_settings {
                            "✓ 导出时包含工作空间配置"
                        } else {
                            "仅导出日报正文"
                        })
                        .when(sv.backup_include_settings, Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.backup_include_settings = !sv.backup_include_settings;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-backup-export-btn")
                        .label("导出备份 JSON…")
                        .primary()
                        .disabled(sv.busy)
                        .px_3()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.pick_and_export_backup(window, cx);
                        })),
                ),
        )
        .child(div().border_t_1().border_color(border))
        .child(div().text_sm().child("从备份文件恢复"))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    Button::new("settings-backup-pick-btn")
                        .label("选择备份 JSON…")
                        .disabled(sv.busy)
                        .px_3()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.pick_and_plan_backup(window, cx);
                        })),
                )
                .child(
                    Button::new("settings-restore-policy-skip")
                        .label("跳过已有日期")
                        .when(sv.restore_policy == RestorePolicy::Skip, Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.restore_policy = RestorePolicy::Skip;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-restore-policy-overwrite")
                        .label("覆盖已有日期")
                        .when(sv.restore_policy == RestorePolicy::Overwrite, Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.restore_policy = RestorePolicy::Overwrite;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-restore-with-settings")
                        .label(if sv.restore_settings {
                            "✓ 同步恢复设置"
                        } else {
                            "仅恢复日报"
                        })
                        .when(sv.restore_settings, Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(sv) = sv_mut(this) {
                                sv.restore_settings = !sv.restore_settings;
                            }
                            cx.notify();
                        })),
                ),
        )
        .when_some(sv.backup_plan.as_ref(), |pane, plan| {
            pane.child(
                div()
                    .border_1()
                    .border_color(border)
                    .rounded_sm()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .when_some(sv.backup_bundle.as_ref(), |box_el, bundle| {
                        box_el.child(
                            div()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(format!(
                                    "来源：{}（{} · 格式 v{} · 路径 {}）",
                                    bundle.workspace_name,
                                    bundle.exported_at,
                                    bundle.version,
                                    bundle.workspace_path
                                )),
                        )
                    })
                    .child(div().text_size(px(12.)).child(format!(
                        "备份摘要：共 {} 篇日报（新增 {} 篇，冲突 {} 篇，含设置：{}）",
                        plan.total_in_backup,
                        plan.new_entries_count,
                        plan.conflicting_entries_count,
                        if plan.has_settings { "是" } else { "否" }
                    )))
                    .child(
                        Button::new("settings-backup-execute-restore")
                            .label(if sv.busy { "恢复中…" } else { "确认执行恢复" })
                            .primary()
                            .disabled(sv.busy)
                            .px_3()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.execute_workspace_restore(window, cx);
                            })),
                    ),
            )
        })
}

fn render_updates_section(
    sv: &SettingsViewState,
    muted: gpui_kit::Hsla,
    border: gpui_kit::Hsla,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let channel = sv.update_pref.channel;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(div().text_sm().child(format!(
            "WorkReport {APP_BASE_VERSION}（GPUI 原型 v{} · 平台 {}）",
            env!("CARGO_PKG_VERSION"),
            updates::current_platform_key()
        )))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    Button::new("settings-update-channel-main")
                        .label("正式版（main）")
                        .when(channel == UpdateChannel::Main, Button::primary)
                        .disabled(sv.update_checking)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_update_channel(UpdateChannel::Main, cx);
                        })),
                )
                .child(
                    Button::new("settings-update-channel-dev")
                        .label("开发版（dev）")
                        .when(channel == UpdateChannel::Dev, Button::primary)
                        .disabled(sv.update_checking)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_update_channel(UpdateChannel::Dev, cx);
                        })),
                )
                .child(
                    Button::new("settings-update-allow-switch")
                        .label(if sv.update_pref.allow_channel_switch {
                            "✓ 允许跨渠道切换（含回退旧版）"
                        } else {
                            "允许跨渠道切换"
                        })
                        .when(sv.update_pref.allow_channel_switch, Button::primary)
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_allow_channel_switch(cx);
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    Button::new("settings-update-check")
                        .label(if sv.update_checking { "正在检查…" } else { "检查更新" })
                        .primary()
                        .disabled(sv.update_checking || sv.busy)
                        .px_3()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.check_for_app_updates(window, cx);
                        })),
                )
                .child(
                    Button::new("settings-update-open-download")
                        .label("打开发布页面")
                        .px_2()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_update_download_page(cx);
                        })),
                )
                .when_some(sv.update_pref.last_checked.clone(), |row, ts| {
                    row.child(div().text_size(px(11.)).text_color(muted).child(format!("最近检查：{ts}")))
                }),
        )
        .when_some(sv.update_status.clone(), |pane, msg| {
            pane.child(div().text_size(px(12.)).text_color(muted).child(msg))
        })
        .when_some(sv.update_result.as_ref(), |pane, res| match res {
            UpdateCheckResult::UpdateAvailable {
                version,
                notes,
                pub_date,
                format,
                artifact_url,
                ..
            } => pane.child(
                div()
                    .border_1()
                    .border_color(border)
                    .rounded_sm()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_size(px(12.)).child(format!(
                        "目标版本：{version}{}{}",
                        format
                            .as_deref()
                            .map(|f| format!("（格式：{f}）"))
                            .unwrap_or_default(),
                        pub_date
                            .as_deref()
                            .map(|d| format!(" · {d}"))
                            .unwrap_or_default()
                    )))
                    .when(!notes.trim().is_empty(), |box_el| {
                        box_el.child(div().text_size(px(11.)).text_color(muted).child(notes.clone()))
                    })
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(format!("签名清单附件：{artifact_url}")),
                    ),
            ),
            _ => pane,
        })
        .child(div().border_t_1().border_color(border))
        .child(div().text_sm().child("旧版 Tauri（com.workreport.desktop）本机状态迁移"))
        .child(
            div()
                .text_size(px(12.))
                .text_color(muted)
                .child("不改动 Tauri 现有配置与安装；可将最近工作空间、全局 AI、网络代理、节假日缓存与更新渠道原子复制到 GPUI 本机配置目录。"),
        )
        .when_some(sv.legacy_migration.as_ref(), |pane, summary| {
            let legacy_exists = summary.legacy_dir.exists();
            pane.child(
                div()
                    .border_1()
                    .border_color(border)
                    .rounded_sm()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_size(px(11.)).text_color(muted).child(format!(
                        "旧版目录：{}{}",
                        summary.legacy_dir.display(),
                        if legacy_exists { "" } else { "（当前未检测到）" }
                    )))
                    .when_some(summary.workspace_path.clone(), |col, ws| {
                        col.child(
                            div()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(format!("旧版最近工作空间：{ws}")),
                        )
                    })
                    .children(summary.items.iter().map(|item| {
                        let status = match (item.exists_in_legacy, item.exists_in_target) {
                            (false, _) => "旧版无此项",
                            (true, false) => "可导入",
                            (true, true) => "GPUI 已有（可覆盖）",
                        };
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(format!("· {} ({}) — {status}", item.label, item.key))
                    }))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("settings-legacy-import-missing")
                                    .label("仅导入缺失项")
                                    .primary()
                                    .disabled(!legacy_exists)
                                    .px_3()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.import_legacy_tauri_config(false, cx);
                                    })),
                            )
                            .child(
                                Button::new("settings-legacy-import-overwrite")
                                    .label("覆盖导入全部")
                                    .disabled(!legacy_exists)
                                    .px_2()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.import_legacy_tauri_config(true, cx);
                                    })),
                            ),
                    ),
            )
        })
}
