use std::{path::PathBuf, process::ExitCode, sync::Arc, time::Duration};

use gpui_kit::{
    Animation, AnimationExt, AnyWindowHandle, App, Bounds, ClipboardItem, Context, ElementId,
    Entity, FocusHandle, FontWeight, ImageSource, KeyBinding, Menu, MenuItem, MouseButton,
    PathPromptOptions, ScrollHandle, SharedString, Subscription, Task, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
    base::{Button as FlatButton, Disableable},
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        input::{Editor, EditorState, Input, InputEvent, InputState},
        text::{MarkdownExtensions, TextView},
    },
    div, ease_out_quint,
    prelude::*,
    px, size,
};
use workreport_core::{history::Version, workspace::Format};

mod ai_panel;
mod backup_restore;
mod calendar;
mod calendar_details;
#[cfg(test)]
mod calendar_details_tests;
mod calendar_holidays;
#[cfg(test)]
mod calendar_modes_tests;
mod calendar_previews;
#[cfg(test)]
mod calendar_tests;
mod calendar_view;
mod directory;
#[cfg(test)]
mod directory_tests;
mod excel_view;
#[cfg(test)]
mod excel_tests;
mod git_view;
#[cfg(test)]
mod git_tests;
mod history_diff;
#[cfg(test)]
mod history_diff_tests;
mod icons;
mod journal_draft;
mod layout;
mod line_diff;
mod lunar;
mod module_board;
#[cfg(test)]
mod module_board_tests;
mod overview;
#[cfg(test)]
mod overview_tests;
mod recent_workspace;
mod rich_editor;
#[cfg(test)]
mod rich_editor_tests;
mod settings_view;
#[cfg(test)]
mod settings_tests;
mod summary_view;
#[cfg(test)]
mod summary_ai_tests;
mod table_cell;
mod theme_skin;
mod workspace_edit;
#[cfg(test)]
mod workspace_edit_tests;
mod workspace_range;
mod workspace_view;

use ai_panel::AiState;
use calendar::CalendarState;
use calendar_details::CalendarDetails;
use calendar_holidays::{HolidayService, HolidaySource};
use calendar_view::CalendarDrag;
use directory::DirectoryState;
use excel_view::ExcelState;
use git_view::GitState;
use history_diff::HistoryDiffState;
use layout::{ModuleLayoutState, ModuleType};
use module_board::ModuleBoardState;
use overview::OverviewState;
use recent_workspace::RecentWorkspace;
use settings_view::SettingsViewState;
use summary_view::SummaryState;
use workspace_edit::WorkspaceEdit;
use workspace_range::WorkspaceRange;
use workspace_view::{Journal, Snapshot, WorkspaceView};

const SAMPLE: &str =
    include_str!("../../fixtures/gpui-migration/workspace/日报/2026/日报_20261119.md");

type WorkspaceLoad = Result<(WorkspaceView, Option<String>), String>;

gpui_kit::actions!(
    workreport_prototype,
    [ChooseFolder, SaveJournal, ClearRange, OpenSearch, OpenSettings, ShowShortcuts, Quit]
);

struct PendingWorkspaceSwitch {
    target_path: PathBuf,
    source_path: PathBuf,
    source_name: String,
    error: Option<String>,
    busy: bool,
}

struct Prototype {
    focus_handle: FocusHandle,
    editor: Entity<EditorState>,
    cell_input: Entity<InputState>,
    source_cell: Option<SharedString>,
    status: SharedString,
    workspace: Option<WorkspaceView>,
    workspace_edit: Option<WorkspaceEdit>,
    new_journal_date: Entity<InputState>,
    new_journal_format: Format,
    recent_workspace: Option<Arc<RecentWorkspace>>,
    calendar: CalendarState,
    calendar_details: CalendarDetails,
    calendar_drag: Option<CalendarDrag>,
    calendar_year_scroll: ScrollHandle,
    calendar_suppress_click: bool,
    workspace_range: Option<WorkspaceRange>,
    range_single_date: Option<chrono::NaiveDate>,
    range_source: bool,
    module_layout: ModuleLayoutState,
    module_board: ModuleBoardState,
    history_diff: Option<HistoryDiffState>,
    overview: OverviewState,
    directory: DirectoryState,
    summary: SummaryState,
    excel: Box<ExcelState>,
    git: Box<GitState>,
    settings_view: Option<Box<SettingsViewState>>,
    settings_config_dir: Option<PathBuf>,
    ai: AiState,
    viewing_workspace: bool,
    scratch_mode: bool,
    shortcuts_dialog_open: bool,
    startup_update: Option<workreport_core::updates::UpdateCheckResult>,
    update_dialog_open: bool,
    workspace_switch: Option<PendingWorkspaceSwitch>,
    workspace_busy: bool,
    _editor_subscription: Subscription,
    _window_activation_subscription: Subscription,
    _calendar_clock: Task<()>,
    _update_check_task: Option<Task<()>>,
}

impl Prototype {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
                .default_value(SAMPLE)
        });
        let source_cell = first_cell_value(SAMPLE);
        let new_journal_date = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(chrono::Local::now().format("%Y-%m-%d").to_string())
        });
        let cell_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(source_cell.clone().unwrap_or_default())
        });
        let editor_subscription = cx.subscribe_in(&editor, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.sync_cell(window, cx);
                cx.notify();
            }
        });
        let window_activation_subscription =
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.cancel_calendar_drag(cx);
                    this.cancel_module_interactions(cx);
                } else {
                    this.calendar_details.today = chrono::Local::now().date_naive();
                    this.calendar_details.invalidate_previews();
                    cx.notify();
                }
            });
        let calendar_clock = Self::watch_calendar_date(window, cx);
        let module_board = ModuleBoardState::new(window, cx);
        let directory = DirectoryState::new(window, cx);
        let summary = SummaryState::new(window, cx);
        let excel = Box::new(ExcelState::new(window, cx));
        let git = Box::new(GitState::new(window, cx));

        Self {
            focus_handle,
            editor,
            cell_input,
            source_cell,
            status: "正在编辑内置样本。修改仅保存在内存中。".into(),
            workspace: None,
            workspace_edit: None,
            new_journal_date,
            new_journal_format: Format::Txt,
            recent_workspace: None,
            calendar: CalendarState::new(chrono::Local::now().date_naive()),
            calendar_details: CalendarDetails::default(),
            calendar_drag: None,
            calendar_year_scroll: ScrollHandle::new(),
            calendar_suppress_click: false,
            workspace_range: None,
            range_single_date: None,
            range_source: false,
            module_layout: ModuleLayoutState::default(),
            module_board,
            history_diff: None,
            overview: OverviewState::default(),
            directory,
            summary,
            excel,
            git,
            settings_view: None,
            settings_config_dir: None,
            ai: AiState::default(),
            viewing_workspace: false,
            scratch_mode: false,
            shortcuts_dialog_open: false,
            startup_update: None,
            update_dialog_open: false,
            workspace_switch: None,
            workspace_busy: false,
            _editor_subscription: editor_subscription,
            _window_activation_subscription: window_activation_subscription,
            _calendar_clock: calendar_clock,
            _update_check_task: None,
        }
    }

    fn start_background_update_check(&mut self, window: &Window, cx: &mut Context<Self>) {
        use workreport_core::updates;

        let config_dir = self.settings_config_dir.clone().unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join(updates::GPUI_CONFIG_DIR_NAME)
        });
        let legacy_dir = config_dir
            .parent()
            .map(|p| p.join(updates::TAURI_CONFIG_DIR_NAME));
        let pref =
            updates::load_preference_with_fallback(&config_dir, legacy_dir.as_deref())
                .unwrap_or_default();

        let task = cx.background_executor().spawn(async move {
            ai_panel::run_tokio(updates::check_channel_update(
                "0.1.8",
                pref.channel,
                pref.allow_channel_switch,
                None,
                None,
            ))?
        });
        let spawned = cx.spawn_in(window, async move |this, cx| {
            if let Ok(result) = task.await {
                let _ = this.update(cx, |this, cx| {
                    if matches!(
                        result,
                        updates::UpdateCheckResult::UpdateAvailable { .. }
                            | updates::UpdateCheckResult::DowngradeRequiresConfirmation { .. }
                    ) {
                        this.startup_update = Some(result);
                        cx.notify();
                    }
                });
            }
        });
        self._update_check_task = Some(spawned);
    }

    fn sync_cell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = first_cell_value(&self.editor.read(cx).value());
        if value != self.source_cell {
            self.source_cell = value.clone();
            self.cell_input.update(cx, |input, cx| {
                input.set_value(value.unwrap_or_default(), window, cx);
            });
        }
    }

    fn apply_cell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = self.editor.read(cx).value();
        let value = self.cell_input.read(cx).value();
        match table_cell::replace_first_plain_body_cell(&source, &value) {
            Ok(updated) if updated != source.as_ref() => {
                self.editor.update(cx, |editor, cx| {
                    editor.replace_all(updated, window, cx);
                    editor.focus(window, cx);
                });
                self.status = "已更新表格单元格，可在源码编辑区撤销。".into();
            }
            Ok(_) => self.status = "单元格内容没有变化。".into(),
            Err(error) => self.status = error.into(),
        }
        cx.notify();
    }

    fn choose_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_busy || self.workspace_edit.is_some() {
            return;
        }
        self.workspace_busy = true;
        cx.notify();
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("打开工作空间".into()),
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.workspace_busy = false;
                match result {
                    Ok(Ok(paths)) => {
                        if let Some(path) = paths.and_then(|paths| paths.into_iter().next()) {
                            this.request_workspace_switch(path, window, cx);
                        } else {
                            this.status = "已取消工作空间选择".into();
                        }
                    }
                    Ok(Err(error)) => this.status = format!("无法选择目录：{error}").into(),
                    Err(_) => this.status = "目录选择已结束".into(),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn request_workspace_switch(
        &mut self,
        path: PathBuf,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(current) = &self.workspace {
            let source_path = current.reader.root().to_path_buf();
            if source_path == path {
                self.status = "已经在当前工作空间中。".into();
                cx.notify();
                return;
            }
            let source_name = current.info_name().to_string();
            self.workspace_switch = Some(PendingWorkspaceSwitch {
                target_path: path,
                source_path,
                source_name,
                error: None,
                busy: false,
            });
            cx.notify();
        } else {
            self.open_workspace(path, window, cx);
        }
    }

    pub(crate) fn finish_workspace_switch(
        &mut self,
        sync: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut pending) = self.workspace_switch.take() else {
            return;
        };
        if pending.busy {
            self.workspace_switch = Some(pending);
            return;
        }
        if !sync {
            self.open_workspace(pending.target_path, window, cx);
            return;
        }
        pending.busy = true;
        pending.error = None;
        let source_root = pending.source_path.clone();
        let target_root = pending.target_path.clone();
        self.workspace_switch = Some(pending);
        self.workspace_busy = true;
        cx.notify();

        let recent = self.recent_workspace.clone();
        let task = cx.background_executor().spawn(async move {
            workreport_core::settings::sync_workspace_settings(&source_root, &target_root)?;
            load_workspace(target_root, recent.as_deref())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.workspace_busy = false;
                match result {
                    Ok((ws, issue)) => {
                        this.workspace_switch = None;
                        this.apply_workspace_load(Ok((ws, issue)), window, cx);
                        this.status = "设置已同步并切换到新工作空间。".into();
                    }
                    Err(error) => {
                        if let Some(pending) = &mut this.workspace_switch {
                            pending.busy = false;
                            pending.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_workspace(&mut self, path: PathBuf, window: &Window, cx: &mut Context<Self>) {
        self.workspace_busy = true;
        self.status = "正在读取工作空间…".into();
        cx.notify();
        let recent = self.recent_workspace.clone();
        let load = cx
            .background_executor()
            .spawn(async move { load_workspace(path, recent.as_deref()) });
        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if result.is_ok() {
                    this.focus_handle.focus(window, cx);
                }
                this.apply_workspace_load(result, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_workspace_load(
        &mut self,
        result: WorkspaceLoad,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_busy = false;
        self.status = match result {
            Ok((workspace, issue)) => {
                self.new_journal_format = workspace.reader.default_format();
                self.calendar = CalendarState::new(chrono::Local::now().date_naive());
                if let Some(journal) = &workspace.journal {
                    if let Ok(date) =
                        chrono::NaiveDate::parse_from_str(&journal.edit_target().0, "%Y%m%d")
                    {
                        self.calendar.show_date(date);
                    }
                }
                self.workspace_range = None;
                self.range_single_date = None;
                self.calendar_drag = None;
                self.calendar_details.invalidate_previews();
                self.module_board.reset_transient();
                self.history_diff = None;
                self.overview.reset();
                self.directory.reset(window, cx);
                self.excel.reset_for_workspace(
                    workspace.reader.excel_settings().unwrap_or_default(),
                    &self.ai.runtime,
                    window,
                    cx,
                );
                self.git.reset(window, cx);
                self.git.settings = workspace.reader.git_settings().unwrap_or_default();
                self.settings_view = None;
                let effective_ai = ai_panel::load_effective_ai_settings(workspace.loaded_ai.clone());
                self.summary.reset_for_workspace(
                    workspace.loaded_summary.clone(),
                    &effective_ai.summary_prompt,
                    &self.ai.runtime,
                    window,
                    cx,
                );
                self.ai.reset_for_workspace(effective_ai);
                self.module_layout.load_from_workspace(
                    workspace.loaded_ui.as_ref(),
                    &workspace.loaded_appearance,
                );
                self.apply_skin_theme(window, cx);
                self.workspace = Some(workspace);
                self.viewing_workspace = true;
                #[cfg(not(test))]
                if self.workspace.as_ref().is_some_and(|ws| ws.journal.is_some()) {
                    self.start_workspace_edit(window, cx);
                }
                match issue {
                    Some(issue) => issue.into(),
                    None => "工作空间只读浏览".into(),
                }
            }
            Err(error) => format!("无法打开工作空间：{error}").into(),
        };
    }

    fn open_journal(
        &mut self,
        date: String,
        format: Format,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let mut calendar = self.calendar.clone();
        if let Ok(date) = chrono::NaiveDate::parse_from_str(&date, "%Y%m%d") {
            calendar.show_date(date);
        }
        self.open_journal_with_calendar(date, format, calendar, window, cx);
    }

    fn open_journal_with_calendar(
        &mut self,
        date: String,
        format: Format,
        calendar: CalendarState,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_busy || self.workspace_edit.is_some() {
            return;
        }
        if self.directory.has_dirty_document() {
            if let Some(open_doc) = &mut self.directory.open_doc {
                open_doc.confirm_discard = true;
            }
            self.status = "当前工作空间文档有未保存修改，请先保存或放弃".into();
            cx.notify();
            return;
        }
        self.directory.open_doc = None;
        let Some(workspace) = &self.workspace else {
            return;
        };
        let reader = workspace.reader.clone();
        self.workspace_busy = true;
        self.status = "正在读取日报…".into();
        cx.notify();
        let load = cx
            .background_executor()
            .spawn(async move { Journal::read(&reader, &date, format) });
        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.workspace_busy = false;
                this.status = match result {
                    Ok(journal) => {
                        this.calendar = calendar;
                        this.reveal_calendar_year_month();
                        this.workspace_range = None;
                        this.range_single_date = None;
                        if let Some(workspace) = &mut this.workspace {
                            workspace.journal = Some(journal);
                        }
                        #[cfg(not(test))]
                        this.start_workspace_edit(window, cx);
                        #[cfg(test)]
                        let _ = window;
                        "工作空间只读浏览".into()
                    }
                    Err(error) => format!("无法读取日报：{error}").into(),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn workspace_default_format(&self) -> Format {
        self.workspace
            .as_ref()
            .map(|ws| ws.reader.default_format())
            .unwrap_or(self.new_journal_format)
    }

    pub(crate) fn refresh_workspace_entries(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let reader = workspace.reader.clone();
        let refresh = cx.background_executor().spawn(async move { reader.list() });
        cx.spawn_in(window, async move |this, cx| {
            let result = refresh.await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(entries) = result {
                    if let Some(workspace) = &mut this.workspace {
                        workspace.entries = entries;
                        this.calendar_details.invalidate_previews();
                        this.overview.invalidate();
                        this.summary.invalidate();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_snapshot(&mut self, version: Version, window: &Window, cx: &mut Context<Self>) {
        if self.workspace_busy || self.workspace_edit.is_some() {
            return;
        }
        if self.directory.has_dirty_document() {
            if let Some(open_doc) = &mut self.directory.open_doc {
                open_doc.confirm_discard = true;
            }
            self.status = "当前工作空间文档有未保存修改，请先保存或放弃".into();
            cx.notify();
            return;
        }
        self.directory.open_doc = None;
        let Some(workspace) = &self.workspace else {
            return;
        };
        let reader = workspace.reader.clone();
        self.workspace_busy = true;
        self.status = "正在读取历史版本…".into();
        cx.notify();
        let load = cx
            .background_executor()
            .spawn(async move { Snapshot::read(&reader, version) });
        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                this.workspace_busy = false;
                this.status = match result {
                    Ok(snapshot) => {
                        if let Some(journal) = this
                            .workspace
                            .as_mut()
                            .and_then(|workspace| workspace.journal.as_mut())
                        {
                            journal.snapshot = Some(snapshot);
                        }
                        "历史版本只读浏览".into()
                    }
                    Err(error) => format!("无法读取历史版本：{error}").into(),
                };
                cx.notify();
            });
        })
        .detach();
    }
}

fn first_cell_value(source: &str) -> Option<SharedString> {
    table_cell::first_plain_body_cell(source).map(|range| source[range].to_owned().into())
}

fn markdown_preview(id: impl Into<ElementId>, source: SharedString) -> TextView {
    TextView::markdown(id, source)
        .markdown_extensions(MarkdownExtensions::default().frontmatter().raw_html(false))
        .image_source(|_| ImageSource::Custom(Arc::new(|_, _| None)))
        .link(|_, _| None)
}

impl Prototype {
    #[inline(never)]
    fn render_editor(&self, cx: &Context<Self>) -> gpui_kit::AnyElement {
        let border = cx.theme().border;
        let source = self.editor.read(cx).value();
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(div().text_sm().child("Markdown 源码"))
                    .child(
                        Editor::new(&self.editor)
                            .aria_label("Markdown 源码")
                            .flex_1()
                            .min_h_0()
                            .w_full(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(div().text_sm().child("预览"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_sm().child("首个表格的首个正文单元格"))
                            .child(
                                Input::new(&self.cell_input)
                                    .aria_label("首个表格的首个正文单元格")
                                    .disabled(self.source_cell.is_none())
                                    .flex_1(),
                            )
                            .child(
                                Button::new("apply-cell")
                                    .label("应用")
                                    .disabled(self.source_cell.is_none())
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.apply_cell(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("preview-pane")
                            .flex_1()
                            .min_h_0()
                            .border_1()
                            .border_color(border)
                            .rounded_md()
                            .p_3()
                            .overflow_y_scroll()
                            .child(markdown_preview("journal-preview", source)),
                    ),
            )
            .into_any_element()
    }

    #[inline(never)]
    fn render_app_toolbar(
        &self,
        workspace: Option<&WorkspaceView>,
        navigation_busy: bool,
        cx: &Context<Self>,
    ) -> gpui_kit::AnyElement {
        use chrono::{Datelike, Timelike};
        use crate::icons::{PhIcon, ph_icon};
        use crate::theme_skin::scaled_px;

        let scale = self.module_layout.scale;
        let palette = theme_skin::palette_for(&self.module_layout.preferences.skin);
        let now = chrono::Local::now();
        let hour = now.hour();
        let greeting = if hour < 6 {
            "夜深了"
        } else if hour < 12 {
            "上午好"
        } else if hour < 14 {
            "中午好"
        } else if hour < 18 {
            "下午好"
        } else {
            "晚上好"
        };
        let weekdays = [
            "星期一", "星期二", "星期三", "星期四", "星期五", "星期六", "星期日",
        ];
        let wd = weekdays[now.weekday().num_days_from_monday() as usize];
        let date_label = format!("{}，今天是 {} 月 {} 日 {}", greeting, now.month(), now.day(), wd);
        let shortcut_mod = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" };
        let show_gate = self.workspace.is_none() && !self.scratch_mode;
        #[cfg(all(target_os = "macos", not(test)))]
        let left_pad = px(80.);
        #[cfg(not(all(target_os = "macos", not(test))))]
        let left_pad = px(6.);

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .h(px(34.))
            .pl(left_pad)
            .pr_1p5()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(22.))
                                    .h(px(22.))
                                    .rounded(px(5.))
                                    .bg(palette.accent_fill)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(scaled_px(12.5, scale))
                                    .font_weight(gpui_kit::FontWeight::BOLD)
                                    .text_color(palette.on_accent)
                                    .child("W"),
                            )
                            .child(
                                div()
                                    .text_size(scaled_px(13.5, scale))
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .text_color(palette.text)
                                    .child("WorkReport"),
                            )
                            .when(self.workspace.is_some(), |left| {
                                let ws_name = self
                                    .workspace
                                    .as_ref()
                                    .map(|ws| ws.info_name().to_string())
                                    .unwrap_or_else(|| "工作空间".to_string());
                                left.child(div().w(px(1.)).h(px(16.)).bg(palette.border))
                                    .child(
                                        FlatButton::new("choose-folder")
                                            .accessibility_label("切换工作空间")
                                            .disabled(navigation_busy)
                                            .h(px(28.))
                                            .px_2()
                                            .rounded(px(5.))
                                            .bg(palette.canvas)
                                            .hover(move |s| s.bg(palette.subtle_strong))
                                            .text_color(palette.text_body)
                                            .text_size(scaled_px(13.5, scale))
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(ws_name)
                                            .child(ph_icon(
                                                PhIcon::CaretDown,
                                                px(12.),
                                                palette.text_muted,
                                            ))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.choose_folder(window, cx);
                                            })),
                                    )
                                    .when(
                                        self.workspace
                                            .as_ref()
                                            .is_some_and(|ws| ws.reader.is_database_only()),
                                        |bar| {
                                            bar.child(
                                                div()
                                                    .px_1p5()
                                                    .py_0p5()
                                                    .rounded(px(4.))
                                                    .bg(palette.accent_soft)
                                                    .border_1()
                                                    .border_color(palette.accent_border)
                                                    .text_size(scaled_px(11., scale))
                                                    .text_color(palette.accent)
                                                    .child("仅数据库模式"),
                                            )
                                        },
                                    )
                            })
                            .when(self.workspace.is_none() && !show_gate, |left| {
                                left.child(div().w(px(1.)).h(px(14.)).bg(palette.border))
                                    .child(
                                        Button::new("choose-folder")
                                            .label("选择工作空间")
                                            .primary()
                                            .disabled(navigation_busy)
                                            .h(px(28.))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.choose_folder(window, cx);
                                            })),
                                    )
                            }),
                    )
                    .child(
                        // .home-heading-actions (order: 1 in is-native-titlebar)
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .when(self.workspace.is_some(), |right| {
                                right
                                    .child(
                                        FlatButton::new("toolbar-global-search")
                                            .accessibility_label("全局检索")
                                            .disabled(navigation_busy)
                                            .h(px(28.))
                                            .px(px(9.))
                                            .rounded(px(8.))
                                            .border_1()
                                            .border_color(palette.border)
                                            .bg(palette.surface)
                                            .hover(move |s| {
                                                s.border_color(palette.accent)
                                                    .bg(palette.accent_soft)
                                                    .text_color(palette.text)
                                            })
                                            .text_color(palette.text_body)
                                            .text_size(scaled_px(12.5, scale))
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .child(ph_icon(
                                                PhIcon::MagnifyingGlass,
                                                px(15.),
                                                palette.text_muted,
                                            ))
                                            .child("搜索…")
                                            .child(
                                                div()
                                                    .px_1p5()
                                                    .py(px(1.))
                                                    .rounded(px(4.))
                                                    .border_1()
                                                    .border_color(palette.border)
                                                    .bg(palette.canvas)
                                                    .text_size(scaled_px(10.5, scale))
                                                    .text_color(palette.text_muted)
                                                    .child(format!("{shortcut_mod}K")),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.directory.search_dialog_open = true;
                                                cx.notify();
                                            })),
                                    )
                                    .when(workspace.is_none(), |bar| {
                                        bar.child(
                                            Button::new("workspace-view")
                                                .label("返回工作区")
                                                .primary()
                                                .disabled(navigation_busy)
                                                .h(px(28.))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.viewing_workspace = true;
                                                    this.focus_handle.focus(window, cx);
                                                    this.status = "工作空间只读浏览".into();
                                                    cx.notify();
                                                })),
                                        )
                                    })
                            })
                            .when(self.workspace.is_none() && self.scratch_mode, |right| {
                                right
                                    .child(
                                        Button::new("copy-markdown")
                                            .label("复制源码")
                                            .h(px(28.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let source =
                                                    this.editor.read(cx).value().to_string();
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    source,
                                                ));
                                                this.status = "已复制 Markdown 源码".into();
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Button::new("leave-scratch")
                                            .label("返回引导页")
                                            .h(px(28.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.scratch_mode = false;
                                                cx.notify();
                                            })),
                                    )
                            })
                            .when_some(
                                self.startup_update.as_ref().and_then(|res| match res {
                                    workreport_core::updates::UpdateCheckResult::UpdateAvailable {
                                        version,
                                        ..
                                    } => Some(version.clone()),
                                    _ => None,
                                }),
                                |right, version| {
                                    right.child(
                                        Button::new("toolbar-update-btn")
                                            .label(format!("更新 v{version}"))
                                            .primary()
                                            .h(px(28.))
                                            .px_2p5()
                                            .rounded(px(6.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.update_dialog_open = true;
                                                cx.notify();
                                            })),
                                    )
                                },
                            )
                            .child(
                                FlatButton::new("toolbar-shortcuts")
                                    .accessibility_label("快捷键速查")
                                    .h(px(28.))
                                    .w(px(28.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(palette.border)
                                    .bg(palette.surface)
                                    .hover(move |s| {
                                        s.border_color(palette.accent).bg(palette.accent_soft)
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(PhIcon::Keyboard, px(16.), palette.text_muted))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.shortcuts_dialog_open = !this.shortcuts_dialog_open;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                // .home-greeting (order: 2, margin-left: auto in is-native-titlebar)
                div()
                    .text_size(scaled_px(12.5, scale))
                    .text_color(palette.text_muted)
                    .child(date_label),
            )
            .into_any_element()
    }

    #[inline(never)]
    fn render_workspace_gate(&self, cx: &Context<Self>) -> gpui_kit::AnyElement {
        use crate::icons::{PhIcon, ph_icon};
        use crate::theme_skin::scaled_px;

        let scale = self.module_layout.scale;
        let palette = theme_skin::palette_for(&self.module_layout.preferences.skin);
        let recent_path = self
            .recent_workspace
            .as_ref()
            .and_then(|recent| recent.load().ok().flatten());
        let recent_name = recent_path.as_ref().and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        });

        div()
            .id("workspace-gate")
            .flex_1()
            .min_h_0()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .w_full()
                    .max_w(px(460.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .relative()
                            .w(px(92.))
                            .h(px(80.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .w(px(68.))
                                    .h(px(68.))
                                    .rounded(px(20.))
                                    .bg(palette.accent_soft)
                                    .border_1()
                                    .border_color(palette.accent_border)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(PhIcon::FolderOpen, px(38.), palette.accent)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .right(px(4.))
                                    .bottom(px(0.))
                                    .w(px(34.))
                                    .h(px(34.))
                                    .rounded(px(10.))
                                    .bg(palette.surface)
                                    .border_1()
                                    .border_color(palette.border)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(PhIcon::FileText, px(18.), palette.accent)),
                            ),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded(px(99.))
                            .bg(palette.accent_soft)
                            .text_size(scaled_px(11.5, scale))
                            .text_color(palette.accent)
                            .child("你的工作，从这里开始"),
                    )
                    .child(
                        div()
                            .text_size(scaled_px(28., scale))
                            .text_color(palette.text)
                            .child("为日报，选一个家。"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_0p5()
                            .text_size(scaled_px(13.5, scale))
                            .text_color(palette.text_muted)
                            .child("选择一个文件夹作为工作空间。")
                            .child("每天的记录，保存为 Markdown 或 TXT 文件。"),
                    )
                    .child(
                        Button::new("choose-folder")
                            .primary()
                            .disabled(self.workspace_busy)
                            .h(px(36.))
                            .px_5()
                            .rounded(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(ph_icon(PhIcon::FolderOpen, px(17.), palette.on_accent))
                                    .child(if self.workspace_busy {
                                        "正在打开…"
                                    } else {
                                        "选择工作空间"
                                    }),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_folder(window, cx);
                            })),
                    )
                    .when_some(recent_path, |card, path| {
                        let title =
                            recent_name.unwrap_or_else(|| path.to_string_lossy().into_owned());
                        card.child(
                            FlatButton::new("gate-open-recent")
                                .w_full()
                                .min_h(px(54.))
                                .px_3p5()
                                .py_2()
                                .rounded(px(10.))
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.surface)
                                .hover(move |s| s.border_color(palette.accent_border))
                                .disabled(self.workspace_busy)
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2p5()
                                        .min_w_0()
                                        .child(ph_icon(
                                            PhIcon::Folder,
                                            px(18.),
                                            palette.accent,
                                        ))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .items_start()
                                                .min_w_0()
                                                .child(
                                                    div()
                                                        .text_size(scaled_px(11., scale))
                                                        .text_color(palette.text_muted)
                                                        .child(if self.workspace_busy {
                                                            "正在打开工作空间…"
                                                        } else {
                                                            "继续使用上次的工作空间"
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(scaled_px(13.5, scale))
                                                        .text_color(palette.text)
                                                        .truncate()
                                                        .child(title),
                                                ),
                                        ),
                                )
                                .child(ph_icon(
                                    PhIcon::ArrowRight,
                                    px(16.),
                                    palette.text_muted,
                                ))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_workspace(path.clone(), window, cx);
                                })),
                        )
                    })
                    .child(
                        div()
                            .pt_2()
                            .text_size(scaled_px(11.5, scale))
                            .text_color(palette.text_muted)
                            .child("文件保存在本地，随时可以用其他编辑器打开。"),
                    ),
            )
            .into_any_element()
    }
}

#[inline(never)]
fn render_shortcuts_dialog(cx: &Context<Prototype>) -> gpui_kit::AnyElement {
    let theme = cx.theme();
    let mod_key = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" };
    let items = [
        (format!("{mod_key}O"), "打开 / 切换工作空间"),
        (format!("{mod_key}S"), "立即保存当前日报或文档"),
        (format!("{mod_key}K"), "打开全局全文搜索"),
        (format!("{mod_key},"), "打开应用与工作空间设置"),
        (format!("{mod_key}/"), "打开或关闭快捷键速查"),
        ("Esc".to_string(), "关闭弹窗、取消拖动、退出最大化或清除日期范围"),
    ];

    div()
        .id("shortcuts-dialog-backdrop")
        .occlude()
        .absolute()
        .inset_0()
        .bg(theme.background.opacity(0.7))
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("shortcuts-dialog")
                .w(px(460.))
                .bg(theme.popover)
                .border_1()
                .border_color(theme.border)
                .rounded(px(14.))
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_base().child("快捷键速查"))
                        .child(
                            Button::new("shortcuts-dialog-close")
                                .label("关闭")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.shortcuts_dialog_open = false;
                                    cx.notify();
                                })),
                        ),
                )
                .children(items.into_iter().map(|(keys, desc)| {
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .py_1()
                        .border_b_1()
                        .border_color(theme.border)
                        .child(div().text_size(px(12.)).text_color(theme.muted_foreground).child(desc))
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded(px(6.))
                                .bg(theme.secondary)
                                .text_size(px(11.))
                                .child(keys),
                        )
                })),
        )
        .into_any_element()
}

#[inline(never)]
fn render_workspace_switch_dialog(
    pending: &PendingWorkspaceSwitch,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    let theme = cx.theme();
    div()
        .id("workspace-switch-dialog-backdrop")
        .occlude()
        .absolute()
        .inset_0()
        .bg(theme.background.opacity(0.7))
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("workspace-switch-dialog")
                .w(px(500.))
                .max_w_full()
                .bg(theme.popover)
                .border_1()
                .border_color(theme.border)
                .rounded(px(14.))
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_base().child("将设置同步到新工作空间？"))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme.muted_foreground)
                        .child(format!(
                            "将「{}」已保存的外观皮肤、AI 配置与提示词、摘要/Excel/Git 规则、编辑器模板和模块布局复制到新工作空间（目标保留各自独立的工作空间标识与日报，不会复制日报正文）。选择「仅切换」则沿用目标工作空间现有配置。",
                            pending.source_name
                        )),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(format!("目标路径：{}", pending.target_path.display())),
                )
                .when_some(pending.error.clone(), |card, err| {
                    card.child(
                        div()
                            .p_2()
                            .rounded_sm()
                            .border_1()
                            .border_color(theme.danger_foreground)
                            .text_size(px(12.))
                            .text_color(theme.danger_foreground)
                            .child(err),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("workspace-switch-cancel")
                                .label("取消")
                                .disabled(pending.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.workspace_switch = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("workspace-switch-keep")
                                .label("仅切换")
                                .disabled(pending.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.finish_workspace_switch(false, window, cx);
                                })),
                        )
                        .child(
                            Button::new("workspace-switch-sync")
                                .label(if pending.busy { "正在切换…" } else { "同步并切换" })
                                .primary()
                                .disabled(pending.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.finish_workspace_switch(true, window, cx);
                                })),
                        ),
                ),
        )
        .into_any_element()
}

#[inline(never)]
fn render_update_dialog(app: &Prototype, cx: &Context<Prototype>) -> gpui_kit::AnyElement {
    use workreport_core::updates::UpdateCheckResult;

    let theme = cx.theme();
    let (title, version_text, notes, pub_date, download_url) = match app.startup_update.as_ref() {
        Some(UpdateCheckResult::UpdateAvailable {
            version,
            notes,
            pub_date,
            download_url,
            is_downgrade,
            ..
        }) => (
            if *is_downgrade {
                format!("可切换到目标渠道版本 v{version}")
            } else {
                format!("发现新版本 WorkReport v{version}")
            },
            version.clone(),
            notes.clone(),
            pub_date.clone(),
            download_url.clone(),
        ),
        Some(UpdateCheckResult::DowngradeRequiresConfirmation {
            version,
            notes,
            pub_date,
            download_url,
            ..
        }) => (
            format!("检测到目标渠道旧版 v{version}"),
            version.clone(),
            notes.clone(),
            pub_date.clone(),
            download_url.clone(),
        ),
        _ => (
            "暂无可用更新".into(),
            String::new(),
            String::new(),
            None,
            workreport_core::updates::Channel::Main.download_url(),
        ),
    };
    let busy_guard = app.workspace_edit.is_some() || app.workspace_busy;

    div()
        .id("update-dialog-backdrop")
        .occlude()
        .absolute()
        .inset_0()
        .bg(theme.background.opacity(0.7))
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("update-dialog")
                .w(px(500.))
                .max_w_full()
                .bg(theme.popover)
                .border_1()
                .border_color(theme.border)
                .rounded(px(14.))
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_base().child(title))
                .when_some(pub_date, |card, dt| {
                    card.child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme.muted_foreground)
                            .child(format!("版本 {version_text} · 发布时间 {dt}")),
                    )
                })
                .when(!notes.trim().is_empty(), |card| {
                    card.child(
                        div()
                            .id("update-dialog-notes")
                            .border_1()
                            .border_color(theme.border)
                            .rounded_sm()
                            .p_2()
                            .max_h(px(180.))
                            .overflow_y_scroll()
                            .text_size(px(12.))
                            .text_color(theme.muted_foreground)
                            .child(notes),
                    )
                })
                .when(busy_guard, |card| {
                    card.child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme.warning_foreground)
                            .child("提示：当前有正在编辑的日报或后台任务，请先保存并返回浏览后再安装更新。"),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("update-dialog-later")
                                .label("稍后")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.update_dialog_open = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("update-dialog-settings")
                                .label("更新设置")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.update_dialog_open = false;
                                    this.open_settings_section(
                                        settings_view::SettingsSection::Updates,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("update-dialog-download")
                                .label("打开发布页面")
                                .primary()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let launched = if cfg!(target_os = "macos") {
                                        std::process::Command::new("open").arg(&download_url).spawn()
                                    } else if cfg!(target_os = "windows") {
                                        std::process::Command::new("cmd")
                                            .args(["/c", "start", "", &download_url])
                                            .spawn()
                                    } else {
                                        std::process::Command::new("xdg-open").arg(&download_url).spawn()
                                    };
                                    if let Err(err) = launched {
                                        this.status = format!("无法打开浏览器：{err}").into();
                                    }
                                    this.update_dialog_open = false;
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .into_any_element()
}

#[inline(never)]
fn render_toast_message(status: &str, cx: &Context<Prototype>) -> Option<gpui_kit::AnyElement> {
    use crate::icons::{PhIcon, ph_icon};

    let msg = status.trim();
    if msg.is_empty()
        || matches!(
            msg,
            "工作空间只读浏览"
                | "请先选择或创建工作空间"
                | "正在编辑内置样本。修改仅保存在内存中。"
                | "停笔后自动保存，也可手动保存。"
                | "日报已自动保存"
                | "正在自动保存…"
                | "正在打开日报编辑器…"
        )
    {
        return None;
    }

    let palette = crate::theme_skin::active_palette(cx);
    let theme = cx.theme();
    let (icon, icon_color) = if msg.contains("失败")
        || msg.contains("错误")
        || msg.contains("无法")
        || msg.contains("无效")
    {
        (PhIcon::XCircle, theme.red)
    } else if msg.contains("取消") || msg.contains("未找到") || msg.contains("请先") {
        (PhIcon::Warning, theme.yellow)
    } else if msg.starts_with("已") || msg.contains("成功") || msg.contains("完成") {
        (PhIcon::CheckCircle, theme.green)
    } else {
        (PhIcon::Info, palette.accent)
    };

    Some(
        div()
            .id("app-toast-host")
            .occlude()
            .absolute()
            .top(px(48.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .max_w(px(520.))
                    .px_3()
                    .py_2()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(ph_icon(icon, px(15.), icon_color))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text)
                            .child(msg.to_string()),
                    )
                    .child(
                        FlatButton::new("toast-close")
                            .w(px(20.))
                            .h(px(20.))
                            .rounded(px(4.))
                            .hover(move |s| s.bg(palette.tab_active()))
                            .child(ph_icon(PhIcon::X, px(12.), palette.text_muted))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status = "工作空间只读浏览".into();
                                cx.notify();
                            })),
                    )
                    .with_animation(
                        ElementId::Name(format!("toast-{}", msg).into()),
                        Animation::new(Duration::from_millis(160)).with_easing(ease_out_quint()),
                        |el, t| el.opacity(0.2 + 0.8 * t).mt(px((1.0 - t) * -8.0)),
                    ),
            )
            .into_any_element(),
    )
}

impl Render for Prototype {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_calendar_previews(window, cx);
        if self.viewing_workspace && self.workspace.is_some() {
            if self.module_layout.has_module(ModuleType::Overview) {
                self.ensure_overview_activity(window, cx);
            }
            if self.module_layout.has_module(ModuleType::Directory) {
                self.ensure_directory_tree(window, cx);
            }
            if self.module_layout.has_module(ModuleType::Summary) {
                self.ensure_summary_prepared(window, cx);
            }
            if self.module_layout.has_module(ModuleType::Excel) {
                self.ensure_excel_exports(window, cx);
            }
            if self.module_layout.has_module(ModuleType::Git) {
                self.ensure_git_loaded(window, cx);
            }
        }
        let theme = cx.theme();
        let background = theme.background;
        let foreground = theme.foreground;
        let muted = theme.muted_foreground;
        let workspace = self.workspace.as_ref().filter(|_| self.viewing_workspace);
        let content = match workspace {
            Some(workspace) => {
                module_board::render_workspace_shell(self, workspace, self.workspace_busy, cx)
                    .into_any_element()
            }
            None => match &self.workspace_edit {
                Some(edit) => edit
                    .render(self.workspace_busy, self.ai.polishing, cx)
                    .into_any_element(),
                None if self.workspace.is_none() && !self.scratch_mode => {
                    self.render_workspace_gate(cx).into_any_element()
                }
                None => self.render_editor(cx).into_any_element(),
            },
        };
        let navigation_busy =
            self.workspace_busy || (!self.viewing_workspace && self.workspace_edit.is_some());

        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .px(px(6.))
            .py(px(6.))
            .gap(px(6.))
            .bg(background)
            .text_color(foreground)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ChooseFolder, window, cx| {
                this.choose_folder(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SaveJournal, window, cx| {
                this.save_workspace_journal(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSearch, _, cx| {
                if this.workspace.is_some() {
                    this.directory.search_dialog_open = true;
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ShowShortcuts, _, cx| {
                this.shortcuts_dialog_open = !this.shortcuts_dialog_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                if this.workspace.is_some() {
                    this.open_settings_section(
                        settings_view::SettingsSection::Appearance,
                        window,
                        cx,
                    );
                }
            }))
            .on_action(cx.listener(|this, _: &ClearRange, window, cx| {
                if let Some(pending) = &this.workspace_switch {
                    if !pending.busy {
                        this.workspace_switch = None;
                        cx.notify();
                    }
                } else if this.update_dialog_open {
                    this.update_dialog_open = false;
                    cx.notify();
                } else if this.shortcuts_dialog_open {
                    this.shortcuts_dialog_open = false;
                    cx.notify();
                } else if this.settings_view.as_ref().is_some_and(|sv| sv.open) {
                    if this.ai.dialog.is_some() {
                        if this.ai.dialog.as_ref().is_some_and(|d| !d.saving) {
                            this.ai.dialog = None;
                            cx.notify();
                        }
                    } else {
                        this.attempt_close_settings(window, cx);
                    }
                } else if this.ai.polish_diff.take().is_some() {
                    cx.notify();
                } else if this.ai.dialog.is_some() {
                    if this.ai.dialog.as_ref().is_some_and(|d| !d.saving) {
                        this.ai.dialog = None;
                        cx.notify();
                    }
                } else if this.directory.search_dialog_open {
                    this.directory.search_dialog_open = false;
                    cx.notify();
                } else if this.directory.delete_confirm.is_some() {
                    if this
                        .directory
                        .delete_confirm
                        .as_ref()
                        .is_some_and(|c| !c.busy)
                    {
                        this.directory.delete_confirm = None;
                        cx.notify();
                    }
                } else if this.directory.form.is_some() {
                    if this.directory.form.as_ref().is_some_and(|f| !f.busy) {
                        this.directory.form = None;
                        cx.notify();
                    }
                } else if let Some(diff) = &mut this.history_diff {
                    if diff.restoring {
                        return;
                    }
                    if diff.confirming_restore {
                        diff.confirming_restore = false;
                    } else {
                        this.history_diff = None;
                    }
                    cx.notify();
                } else if this.module_board.layout_dialog_open {
                    this.module_board.layout_dialog_open = false;
                    cx.notify();
                } else if this.module_board.skin_dialog_open {
                    this.module_board.skin_dialog_open = false;
                    cx.notify();
                } else if this.module_board.module_drag.is_some()
                    || this.module_board.gutter_drag.is_some()
                    || this.module_board.pending_drag.is_some()
                {
                    this.cancel_module_interactions(cx);
                } else if this.module_board.help_module.take().is_some() {
                    cx.notify();
                } else if this.module_layout.restore_maximized() {
                    cx.notify();
                } else if this.viewing_workspace
                    && (this.workspace_range.is_some() || this.calendar_drag.is_some())
                    && !this.workspace_busy
                    && this.workspace_edit.is_none()
                {
                    this.cancel_calendar_drag(cx);
                    this.clear_calendar_range();
                    this.status = "工作空间只读浏览".into();
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .child(self.render_app_toolbar(workspace, navigation_busy, cx))
            .child(content)
            .when_some(self.workspace.as_ref(), |shell, ws| {
                let path_text = ws.info_path();
                let entries_count = ws.entries.len();
                let (doc_label, format_label) = if let Some(edit) = &self.workspace_edit {
                    (
                        edit.filename().to_string(),
                        match edit.format() {
                            workreport_core::workspace::Format::Md => "Markdown",
                            workreport_core::workspace::Format::Txt => "纯文本",
                        },
                    )
                } else if let Some(journal) = &ws.journal {
                    (
                        journal.filename.clone(),
                        match journal.format {
                            workreport_core::workspace::Format::Md => "Markdown",
                            workreport_core::workspace::Format::Txt => "纯文本",
                        },
                    )
                } else {
                    (
                        self.calendar.selected_date().format("%Y-%m-%d").to_string(),
                        match self.new_journal_format {
                            workreport_core::workspace::Format::Md => "Markdown",
                            workreport_core::workspace::Format::Txt => "纯文本",
                        },
                    )
                };
                shell.child(
                    div()
                        .min_h(px(24.))
                        .px(px(10.))
                        .py(px(2.))
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .text_size(px(12.))
                        .text_color(muted)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(path_text),
                        )
                        .child(div().flex_shrink_0().child(format!("{entries_count} 篇日报")))
                        .child(
                            div()
                                .max_w(px(320.))
                                .truncate()
                                .child(doc_label),
                        )
                        .child(div().flex_shrink_0().child(format_label)),
                )
            })
            .when(self.workspace.is_none() && self.scratch_mode, |shell| {
                shell.child(
                    div()
                        .text_size(px(11.))
                        .text_color(muted)
                        .child(self.status.clone()),
                )
            })
            .when_some(
                (!cfg!(test))
                    .then(|| render_toast_message(&self.status, cx))
                    .flatten(),
                |root, toast| root.child(toast),
            )
            .when_some(self.history_diff.as_ref(), |root, diff| {
                root.child(
                    div()
                        .size_full()
                        .absolute()
                        .inset_0()
                        .child(history_diff::render_history_diff_dialog(diff, cx))
                        .with_animation(
                            "dialog-anim-history-diff",
                            Animation::new(Duration::from_millis(160))
                                .with_easing(ease_out_quint()),
                            |el, t| el.opacity(0.25 + 0.75 * t),
                        ),
                )
            })
            .when(self.directory.search_dialog_open, |root| {
                root.child(
                    div()
                        .size_full()
                        .absolute()
                        .inset_0()
                        .child(directory::render_global_search_dialog(&self.directory, cx))
                        .with_animation(
                            "dialog-anim-global-search",
                            Animation::new(Duration::from_millis(160))
                                .with_easing(ease_out_quint()),
                            |el, t| el.opacity(0.25 + 0.75 * t),
                        ),
                )
            })
            .when_some(self.settings_view.as_deref().filter(|sv| sv.open), |root, sv| {
                root.child(
                    div()
                        .size_full()
                        .absolute()
                        .inset_0()
                        .child(settings_view::render_settings_dialog(self, sv, cx))
                        .with_animation(
                            "dialog-anim-settings",
                            Animation::new(Duration::from_millis(160))
                                .with_easing(ease_out_quint()),
                            |el, t| el.opacity(0.25 + 0.75 * t),
                        ),
                )
            })
            .when(self.ai.dialog.is_some(), |root| {
                root.child(
                    div()
                        .size_full()
                        .absolute()
                        .inset_0()
                        .child(ai_panel::render_ai_settings_dialog(&self.ai, cx))
                        .with_animation(
                            "dialog-anim-ai-settings",
                            Animation::new(Duration::from_millis(160))
                                .with_easing(ease_out_quint()),
                            |el, t| el.opacity(0.25 + 0.75 * t),
                        ),
                )
            })
            .when(self.ai.polish_diff.is_some(), |root| {
                root.child(
                    div()
                        .size_full()
                        .absolute()
                        .inset_0()
                        .child(ai_panel::render_polish_diff_dialog(&self.ai, cx))
                        .with_animation(
                            "dialog-anim-polish-diff",
                            Animation::new(Duration::from_millis(160))
                                .with_easing(ease_out_quint()),
                            |el, t| el.opacity(0.25 + 0.75 * t),
                        ),
                )
            })
            .when(self.shortcuts_dialog_open, |root| {
                root.child(
                    div()
                        .size_full()
                        .absolute()
                        .inset_0()
                        .child(render_shortcuts_dialog(cx))
                        .with_animation(
                            "dialog-anim-shortcuts",
                            Animation::new(Duration::from_millis(160))
                                .with_easing(ease_out_quint()),
                            |el, t| el.opacity(0.25 + 0.75 * t),
                        ),
                )
            })
            .when(self.update_dialog_open, |root| {
                root.child(
                    div()
                        .size_full()
                        .absolute()
                        .inset_0()
                        .child(render_update_dialog(self, cx))
                        .with_animation(
                            "dialog-anim-update",
                            Animation::new(Duration::from_millis(160))
                                .with_easing(ease_out_quint()),
                            |el, t| el.opacity(0.25 + 0.75 * t),
                        ),
                )
            })
            .when_some(self.workspace_switch.as_ref(), |root, pending| {
                root.child(
                    div()
                        .size_full()
                        .absolute()
                        .inset_0()
                        .child(render_workspace_switch_dialog(pending, cx))
                        .with_animation(
                            "dialog-anim-ws-switch",
                            Animation::new(Duration::from_millis(160))
                                .with_easing(ease_out_quint()),
                            |el, t| el.opacity(0.25 + 0.75 * t),
                        ),
                )
            })
    }
}

fn startup_workspace() -> Result<Option<PathBuf>, &'static str> {
    let mut args = std::env::args_os().skip(1);
    match (args.next(), args.next(), args.next()) {
        (None, None, None) => Ok(None),
        (Some(option), Some(path), None) if option == "--workspace" && !path.is_empty() => {
            Ok(Some(path.into()))
        }
        _ => Err("用法：workreport-gpui-prototype [--workspace <目录>]"),
    }
}

fn resolve_startup_workspace(
    explicit: Option<PathBuf>,
    recent: Option<&RecentWorkspace>,
) -> Result<Option<PathBuf>, String> {
    match explicit {
        Some(path) => Ok(Some(path)),
        None => match recent {
            Some(recent) => recent.load(),
            None => Ok(None),
        },
    }
}

fn load_workspace(path: PathBuf, recent: Option<&RecentWorkspace>) -> WorkspaceLoad {
    let (workspace, mut issue) = WorkspaceView::open(path)?;
    if let Some(recent) = recent {
        if let Err(error) = recent.save(workspace.reader.root()) {
            let error = format!("无法记住最近工作空间。{error}");
            issue = Some(match issue {
                Some(issue) => format!("{issue}\n{error}"),
                None => error,
            });
        }
    }
    Ok((workspace, issue))
}

fn bind_prototype_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", ClearRange, None),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-o"
            } else {
                "ctrl-o"
            },
            ChooseFolder,
            None,
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-s"
            } else {
                "ctrl-s"
            },
            SaveJournal,
            None,
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-k"
            } else {
                "ctrl-k"
            },
            OpenSearch,
            None,
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-,"
            } else {
                "ctrl-,"
            },
            OpenSettings,
            None,
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-/"
            } else {
                "ctrl-/"
            },
            ShowShortcuts,
            None,
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-q"
            } else {
                "ctrl-q"
            },
            Quit,
            None,
        ),
    ]);
}

fn open_prototype_window(
    cx: &mut App,
    startup: Option<WorkspaceLoad>,
) -> (AnyWindowHandle, Entity<Prototype>) {
    theme_skin::apply_skin_palette("default", 100, cx);
    let bounds = Bounds::centered(None, size(px(1320.), px(880.)), cx);
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui_kit::point(px(14.), px(12.))),
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(960.), px(680.))),
        ..WindowOptions::default()
    };
    let (window, app) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| {
            let mut view = Prototype::new(window, cx);
            if let Some(result) = startup {
                view.apply_workspace_load(result, window, cx);
            }
            view
        })
    })
    .expect("无法打开 WorkReport GPUI 原型窗口");
    let weak_app = app.downgrade();
    let _ = window.update(cx, move |_, window, cx| {
        window.on_window_should_close(cx, move |window, cx| {
            weak_app
                .update(cx, |app, cx| app.allow_workspace_close(true, window, cx))
                .unwrap_or(true)
        });
    });
    (window, app)
}

fn main() -> ExitCode {
    let explicit = match startup_workspace() {
        Ok(explicit) => explicit,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let recent = RecentWorkspace::in_config_dir().map(Arc::new);
    if let Some(config) = dirs::config_dir() {
        workreport_core::network::initialize(config.join("com.workreport.desktop"));
    }
    let holidays =
        HolidayService::in_config_dir().map(|source| Arc::new(source) as Arc<dyn HolidaySource>);
    // Resolve startup before the first native frame, including launches where
    // macOS keeps the app in the background.
    let selected = resolve_startup_workspace(explicit, recent.as_ref().ok().map(Arc::as_ref));
    let startup = match selected {
        Ok(Some(path)) => Some(load_workspace(path, recent.as_ref().ok().map(Arc::as_ref))),
        Ok(None) => recent.as_ref().err().cloned().map(Err),
        Err(error) => Some(Err(error)),
    };
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            bind_prototype_keys(cx);
            cx.set_menus([Menu {
                name: "WorkReport GPUI Prototype".into(),
                disabled: false,
                items: vec![
                    MenuItem::action("打开工作空间", ChooseFolder),
                    MenuItem::action("保存日报", SaveJournal),
                    MenuItem::action("全局全文搜索", OpenSearch),
                    MenuItem::action("应用与工作空间设置", OpenSettings),
                    MenuItem::action("快捷键速查", ShowShortcuts),
                    MenuItem::action("退出", Quit),
                ],
            }]);

            let (window, app) = open_prototype_window(cx, startup);
            app.update(cx, |app, _| app.recent_workspace = recent.ok());
            let _ = window.update(cx, |_, window, cx| {
                app.update(cx, |app, cx| {
                    match holidays {
                        Ok(source) => {
                            app.calendar_details.holiday_source = Some(source);
                            app.load_calendar_holidays(window, cx);
                        }
                        Err(error) => app.calendar_details.holiday_error = Some(error),
                    }
                    app.start_background_update_check(window, cx);
                });
            });
            let weak_app = app.downgrade();
            cx.on_action(move |_: &Quit, cx| {
                let _ = window.update(cx, |_, window, cx| {
                    let allowed = weak_app
                        .update(cx, |app, cx| app.allow_workspace_close(true, window, cx))
                        .unwrap_or(true);
                    if allowed {
                        window.remove_window();
                    }
                });
            });
            cx.activate(true);
        });
    ExitCode::SUCCESS
}
