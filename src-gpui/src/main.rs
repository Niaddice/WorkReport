use std::{path::PathBuf, process::ExitCode, sync::Arc};

use gpui_kit::{
    AnyWindowHandle, App, Bounds, ClipboardItem, Context, ElementId, Entity, FocusHandle,
    ImageSource, KeyBinding, Menu, MenuItem, PathPromptOptions, ScrollHandle, SharedString,
    Subscription, Task, TitlebarOptions, Window, WindowBounds, WindowOptions,
    base::Disableable,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        input::{Editor, EditorState, Input, InputEvent, InputState},
        text::{MarkdownExtensions, TextView},
    },
    div,
    prelude::*,
    px, size,
};
use workreport_core::{history::Version, workspace::Format};

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
mod journal_draft;
mod lunar;
mod recent_workspace;
mod table_cell;
mod workspace_edit;
#[cfg(test)]
mod workspace_edit_tests;
mod workspace_range;
mod workspace_view;

use calendar::CalendarState;
use calendar_details::CalendarDetails;
use calendar_holidays::{HolidayService, HolidaySource};
use calendar_view::{CalendarDrag, CalendarPresentation};
use recent_workspace::RecentWorkspace;
use workspace_edit::WorkspaceEdit;
use workspace_range::WorkspaceRange;
use workspace_view::{Journal, Snapshot, WorkspaceView};

const SAMPLE: &str =
    include_str!("../../fixtures/gpui-migration/workspace/日报/2026/日报_20261119.md");

type WorkspaceLoad = Result<(WorkspaceView, Option<String>), String>;

gpui_kit::actions!(
    workreport_prototype,
    [ChooseFolder, SaveJournal, ClearRange, Quit]
);

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
    viewing_workspace: bool,
    workspace_busy: bool,
    _editor_subscription: Subscription,
    _window_activation_subscription: Subscription,
    _calendar_clock: Task<()>,
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
                } else {
                    this.calendar_details.today = chrono::Local::now().date_naive();
                    this.calendar_details.invalidate_previews();
                    cx.notify();
                }
            });
        let calendar_clock = Self::watch_calendar_date(window, cx);

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
            viewing_workspace: false,
            workspace_busy: false,
            _editor_subscription: editor_subscription,
            _window_activation_subscription: window_activation_subscription,
            _calendar_clock: calendar_clock,
        }
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
                            this.open_workspace(path, window, cx);
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
                this.apply_workspace_load(result);
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_workspace_load(&mut self, result: WorkspaceLoad) {
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
                self.workspace = Some(workspace);
                self.viewing_workspace = true;
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
            let _ = this.update(cx, |this, cx| {
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
                        "工作空间只读浏览".into()
                    }
                    Err(error) => format!("无法读取日报：{error}").into(),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn open_snapshot(&mut self, version: Version, window: &Window, cx: &mut Context<Self>) {
        if self.workspace_busy || self.workspace_edit.is_some() {
            return;
        }
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
    fn render_editor(&self, cx: &Context<Self>) -> impl IntoElement {
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
    }
}

impl Render for Prototype {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_calendar_previews(window, cx);
        let theme = cx.theme();
        let background = theme.background;
        let foreground = theme.foreground;
        let muted = theme.muted_foreground;
        let workspace = self.workspace.as_ref().filter(|_| self.viewing_workspace);
        let content = match (&self.workspace_edit, workspace) {
            (Some(edit), _) => edit.render(self.workspace_busy, cx).into_any_element(),
            (None, Some(workspace)) => workspace
                .render(
                    self.workspace_busy,
                    &self.new_journal_date,
                    self.new_journal_format,
                    CalendarPresentation {
                        selection: self
                            .calendar_drag
                            .as_ref()
                            .map(|drag| &drag.selection)
                            .unwrap_or(&self.calendar),
                        year_scroll: &self.calendar_year_scroll,
                        details: &self.calendar_details,
                    },
                    self.workspace_range.as_ref(),
                    self.range_source,
                    cx,
                )
                .into_any_element(),
            (None, None) => self.render_editor(cx).into_any_element(),
        };
        let navigation_busy = self.workspace_busy || self.workspace_edit.is_some();

        div()
            .flex()
            .flex_col()
            .size_full()
            .p_4()
            .gap_3()
            .bg(background)
            .text_color(foreground)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ChooseFolder, window, cx| {
                this.choose_folder(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SaveJournal, window, cx| {
                this.save_workspace_journal(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ClearRange, _, cx| {
                if this.viewing_workspace
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
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().text_lg().child("WorkReport"))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .when(self.workspace.is_some(), |toolbar| {
                                toolbar
                                    .child(
                                        Button::new("scratch-view")
                                            .label("编辑样本")
                                            .when(workspace.is_none(), Button::primary)
                                            .disabled(navigation_busy)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cancel_calendar_drag(cx);
                                                this.viewing_workspace = false;
                                                this.status =
                                                    "正在编辑内置样本。修改仅保存在内存中。".into();
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Button::new("workspace-view")
                                            .label("浏览工作空间")
                                            .when(workspace.is_some(), Button::primary)
                                            .disabled(navigation_busy)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.viewing_workspace = true;
                                                this.focus_handle.focus(window, cx);
                                                this.status = "工作空间只读浏览".into();
                                                cx.notify();
                                            })),
                                    )
                            })
                            .when(workspace.is_none(), |toolbar| {
                                toolbar.child(
                                    Button::new("copy-markdown").label("复制源码").on_click(
                                        cx.listener(|this, _, _, cx| {
                                            let source = this.editor.read(cx).value().to_string();
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                source,
                                            ));
                                            this.status = "已复制 Markdown 源码".into();
                                            cx.notify();
                                        }),
                                    ),
                                )
                            })
                            .child(
                                Button::new("choose-folder")
                                    .label("打开工作空间")
                                    .disabled(navigation_busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.choose_folder(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(content)
            .child(div().text_sm().text_color(muted).child(self.status.clone()))
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
    let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("WorkReport GPUI Prototype".into()),
            ..TitlebarOptions::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(740.), px(460.))),
        ..WindowOptions::default()
    };
    let (window, app) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| {
            let mut view = Prototype::new(window, cx);
            if let Some(result) = startup {
                view.apply_workspace_load(result);
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
                    MenuItem::action("退出", Quit),
                ],
            }]);

            let (window, app) = open_prototype_window(cx, startup);
            app.update(cx, |app, _| app.recent_workspace = recent.ok());
            let _ = window.update(cx, |_, window, cx| {
                app.update(cx, |app, cx| match holidays {
                    Ok(source) => {
                        app.calendar_details.holiday_source = Some(source);
                        app.load_calendar_holidays(window, cx);
                    }
                    Err(error) => app.calendar_details.holiday_error = Some(error),
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
