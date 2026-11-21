use std::{path::Path, sync::Arc};

use chrono::NaiveDate;
use gpui_kit::{
    Context, Entity, FontWeight, Subscription, Task, Window,
    base::Button as FlatButton,
    component::input::{Editor, EditorState, Input, InputEvent, InputState},
    div, prelude::*, px,
};
use workreport_core::excel::{
    self, CancellationToken, DEFAULT_CONTENT_PROMPT, ExcelPreview, ExcelSheetPreview, ExportFile,
    ExportResult, Provenance, Runtime as ExcelRuntime, Settings as ExcelSettings,
};

use crate::{
    Prototype,
    directory::render_table_sheets,
    icons::PhIcon,
    theme_skin::active_palette,
    workspace_view::WorkspaceView,
};

pub(crate) struct ExcelState {
    pub(crate) settings: ExcelSettings,
    pub(crate) runtime: ExcelRuntime,
    pub(crate) exports: Vec<ExportFile>,
    pub(crate) exports_loading: bool,
    pub(crate) exports_error: Option<String>,
    pub(crate) opened_export: Option<(ExportFile, Vec<ExcelSheetPreview>, usize)>,
    pub(crate) workflow_open: bool,
    pub(crate) content_prompt_input: Entity<InputState>,
    pub(crate) fill_prompt_input: Entity<InputState>,
    pub(crate) content_editor: Entity<EditorState>,
    pub(crate) content_draft: String,
    pub(crate) workflow_dates: Vec<String>,
    pub(crate) generating_content: bool,
    pub(crate) preparing_fill: bool,
    pub(crate) exporting: bool,
    pub(crate) preview: Option<(ExcelPreview, usize)>,
    pub(crate) last_export: Option<ExportResult>,
    pub(crate) error: Option<String>,
    exports_key: Option<(usize, u64)>,
    exports_revision: u64,
    exports_request: u64,
    exports_task: Option<Task<()>>,
    req_seq: u64,
    active_request: Option<String>,
    active_cancel: Option<CancellationToken>,
    active_task: Option<Task<()>>,
    _editor_sub: Subscription,
}

impl ExcelState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Prototype>) -> Self {
        let content_prompt_input =
            cx.new(|cx| InputState::new(window, cx).default_value(DEFAULT_CONTENT_PROMPT));
        let fill_prompt_input = cx.new(|cx| InputState::new(window, cx));
        let content_editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
        });
        let editor_sub = cx.subscribe_in(&content_editor, window, |this, entity, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.excel.content_draft = entity.read(cx).value().to_string();
                cx.notify();
            }
        });
        Self {
            settings: ExcelSettings::default(),
            runtime: ExcelRuntime::default(),
            exports: Vec::new(),
            exports_loading: false,
            exports_error: None,
            opened_export: None,
            workflow_open: false,
            content_prompt_input,
            fill_prompt_input,
            content_editor,
            content_draft: String::new(),
            workflow_dates: Vec::new(),
            generating_content: false,
            preparing_fill: false,
            exporting: false,
            preview: None,
            last_export: None,
            error: None,
            exports_key: None,
            exports_revision: 0,
            exports_request: 0,
            exports_task: None,
            req_seq: 0,
            active_request: None,
            active_cancel: None,
            active_task: None,
            _editor_sub: editor_sub,
        }
    }

    pub(crate) fn reset_for_workspace(
        &mut self,
        settings: ExcelSettings,
        ai_runtime: &workreport_core::ai::Runtime,
        window: &mut Window,
        cx: &mut Context<Prototype>,
    ) {
        self.cancel_active(ai_runtime);
        self.settings = settings.clone();
        self.exports.clear();
        self.exports_loading = false;
        self.exports_error = None;
        self.opened_export = None;
        self.workflow_open = false;
        self.content_draft.clear();
        self.workflow_dates.clear();
        self.preview = None;
        self.last_export = None;
        self.error = None;
        self.exports_key = None;
        self.exports_revision += 1;
        self.exports_request += 1;
        self.exports_task = None;

        let content_prompt = if settings.content_prompt.trim().is_empty() {
            DEFAULT_CONTENT_PROMPT.to_string()
        } else {
            settings.content_prompt
        };
        self.content_prompt_input
            .update(cx, |i, cx| i.set_value(content_prompt, window, cx));
        self.fill_prompt_input
            .update(cx, |i, cx| i.set_value(settings.prompt, window, cx));
        self.content_editor
            .update(cx, |e, cx| e.set_value("", window, cx));
    }

    pub(crate) fn cancel_active(&mut self, ai_runtime: &workreport_core::ai::Runtime) {
        if let Some(id) = self.active_request.take() {
            ai_runtime.cancel(&id);
        }
        if let Some(token) = self.active_cancel.take() {
            token.cancel();
        }
        if let Some((preview, _)) = self.preview.take() {
            self.runtime.discard(&preview.id);
        }
        self.generating_content = false;
        self.preparing_fill = false;
        self.active_task = None;
    }

    pub(crate) fn invalidate_exports(&mut self) {
        self.exports_key = None;
        self.exports_revision += 1;
    }
}

fn current_scope_dates(app: &Prototype) -> Vec<String> {
    let (start, end) = app.calendar.selected_range();
    let mut dates = Vec::new();
    let mut cursor = Some(start);
    while let Some(day) = cursor {
        if day > end {
            break;
        }
        dates.push(day.format("%Y%m%d").to_string());
        cursor = day.checked_add_days(chrono::Days::new(1));
    }
    dates
}

fn format_scope_label(dates: &[String]) -> String {
    if dates.is_empty() {
        return "未选择日期".to_string();
    }
    let first = &dates[0];
    let last = &dates[dates.len() - 1];
    let parse = |s: &str| {
        NaiveDate::parse_from_str(s, "%Y%m%d")
            .map(|d| d.format("%Y年%m月%d日").to_string())
            .unwrap_or_else(|_| s.to_string())
    };
    if first == last {
        parse(first)
    } else {
        format!("{} – {}", parse(first), parse(last))
    }
}

impl Prototype {
    pub(crate) fn ensure_excel_exports(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let key = (
            Arc::as_ptr(&workspace.reader) as usize,
            self.excel.exports_revision,
        );
        if self.excel.exports_key.as_ref() == Some(&key) {
            return;
        }
        self.excel.exports_key = Some(key);
        self.excel.exports_request += 1;
        let request_id = self.excel.exports_request;
        self.excel.exports_loading = true;
        self.excel.exports_error = None;
        let root = workspace.reader.root().to_path_buf();
        let load = cx.background_executor().spawn(async move {
            let rule = excel::output_rule(&root)?;
            let dir = workreport_core::file_rules::safe_directory(&root, &rule.directory, false);
            match dir {
                Ok(directory) => excel::list_exports(&directory),
                Err(_) => Ok(Vec::new()),
            }
        });
        self.excel.exports_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                if this.excel.exports_request != request_id {
                    return;
                }
                this.excel.exports_loading = false;
                match result {
                    Ok(exports) => this.excel.exports = exports,
                    Err(error) => this.excel.exports_error = Some(error),
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn start_excel_content_generation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        if self.excel.generating_content || self.excel.preparing_fill || self.excel.exporting {
            return;
        }
        let dates = current_scope_dates(self);
        let prompt = self.excel.content_prompt_input.read(cx).value().to_string();
        let reader = workspace.reader.clone();
        let expected_path = workspace.info_path();
        let backend = self.ai.backend.clone();
        let ai_runtime = self.ai.runtime.clone();

        self.excel.cancel_active(&self.ai.runtime);
        self.excel.req_seq += 1;
        let request_id = format!("excel-content-{}", self.excel.req_seq);
        self.excel.workflow_open = true;
        self.excel.workflow_dates = dates.clone();
        self.excel.generating_content = true;
        self.excel.error = None;
        self.excel.last_export = None;
        self.excel.active_request = Some(request_id.clone());
        let req_copy = request_id.clone();
        let prompt_override = Some(prompt.clone());
        self.status = "正在生成可核对的 Excel 报告正文（阶段 1/2）…".into();
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            let (snapshot, _) =
                reader.excel_snapshot(&expected_path, &dates, prompt_override, None)?;
            backend.generate_excel_content(
                &snapshot.provider,
                &snapshot.content,
                &snapshot.settings.content_prompt,
                &req_copy,
                &ai_runtime,
            )
        });
        self.excel.active_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.excel.active_request.as_deref() != Some(request_id.as_str()) {
                    return;
                }
                this.excel.generating_content = false;
                this.excel.active_request = None;
                this.excel.active_task = None;
                match result {
                    Ok(content) => {
                        this.excel.content_draft = content.clone();
                        this.excel
                            .content_editor
                            .update(cx, |editor, cx| editor.set_value(content, window, cx));
                        this.status = "阶段 1 正文已生成，请核对或微调后点击「填表预览」".into();
                    }
                    Err(error) => {
                        this.excel.error = Some(error.clone());
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn prepare_excel_fill_preview(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        if self.excel.generating_content || self.excel.preparing_fill || self.excel.exporting {
            return;
        }
        let dates = if self.excel.workflow_dates.is_empty() {
            current_scope_dates(self)
        } else {
            self.excel.workflow_dates.clone()
        };
        let draft = self
            .excel
            .content_editor
            .read(cx)
            .value()
            .to_string();
        let confirmed_payload = match excel::confirmed_content(&dates, &draft) {
            Ok(payload) => payload,
            Err(error) => {
                self.excel.error = Some(error);
                cx.notify();
                return;
            }
        };
        let fill_prompt = self.excel.fill_prompt_input.read(cx).value().to_string();
        let fill_override = if fill_prompt.trim().is_empty() {
            None
        } else {
            Some(fill_prompt)
        };

        self.excel.cancel_active(&self.ai.runtime);
        self.excel.req_seq += 1;
        let request_id = format!("excel-fill-{}", self.excel.req_seq);
        let cancel_token = CancellationToken::new();
        self.excel.preparing_fill = true;
        self.excel.error = None;
        self.excel.active_request = Some(request_id.clone());
        self.excel.active_cancel = Some(cancel_token.clone());

        let reader = workspace.reader.clone();
        let expected_path = workspace.info_path();
        let backend = self.ai.backend.clone();
        let ai_runtime = self.ai.runtime.clone();
        let excel_runtime = self.excel.runtime.clone();
        let req_copy = request_id.clone();
        self.status = "正在读取模板并生成工作簿填充预览（阶段 2/2）…".into();
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            let (snapshot, saved_settings) =
                reader.excel_snapshot(&expected_path, &dates, None, fill_override)?;
            let template_path = snapshot.settings.template_path.trim().to_string();
            let template = excel::read_template(Path::new(&template_path))?;
            let template_fingerprint = template.fingerprint().to_string();
            let context = template.context()?;
            let response = backend.fill_excel(
                &snapshot.provider,
                &context,
                &confirmed_payload,
                &snapshot.settings.prompt,
                &req_copy,
                &ai_runtime,
            )?;
            let provenance = Provenance {
                workspace_path: expected_path,
                dates,
                settings: saved_settings,
                file_rule: snapshot.file_rule,
                prompt: Some(snapshot.settings.prompt),
                source_revision: snapshot.source_revision,
                template_fingerprint,
            };
            let pending = excel::prepare(
                template,
                &response,
                provenance,
                snapshot.start,
                snapshot.end,
                cancel_token,
            )?;
            excel_runtime.insert(pending)
        });
        self.excel.active_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.excel.active_request.as_deref() != Some(request_id.as_str()) {
                    if let Ok(preview) = result {
                        this.excel.runtime.discard(&preview.id);
                    }
                    return;
                }
                this.excel.preparing_fill = false;
                this.excel.active_request = None;
                this.excel.active_cancel = None;
                this.excel.active_task = None;
                match result {
                    Ok(preview) => {
                        this.excel.preview = Some((preview, 0));
                        this.status = "Excel 填充预览已就绪，请核对后点击「确认导出」".into();
                    }
                    Err(error) => {
                        this.excel.error = Some(error.clone());
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn confirm_excel_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some((preview, _)) = &self.excel.preview else {
            return;
        };
        if self.excel.exporting {
            return;
        }
        let preview_id = preview.id.clone();
        let reader = workspace.reader.clone();
        let expected_path = workspace.info_path();
        let runtime = self.excel.runtime.clone();
        self.excel.exporting = true;
        self.excel.error = None;
        cx.notify();

        let export = cx.background_executor().spawn(async move {
            runtime.confirm(&preview_id, |provenance| {
                reader.validate_excel_confirmation(&expected_path, provenance)
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = export.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.excel.exporting = false;
                match result {
                    Ok(exported) => {
                        this.excel.preview = None;
                        this.excel.workflow_open = false;
                        this.excel.last_export = Some(exported);
                        this.excel.invalidate_exports();
                        this.ensure_excel_exports(window, cx);
                        this.directory.invalidate_tree();
                        this.ensure_directory_tree(window, cx);
                        this.status = "Excel 文件导出完成".into();
                    }
                    Err(error) => {
                        this.excel.error = Some(error.clone());
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn open_existing_excel_export(
        &mut self,
        file: ExportFile,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let path = file.path.clone();
        let read = cx
            .background_executor()
            .spawn(async move { excel::read_export(Path::new(&path)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(sheets) => {
                        this.excel.opened_export = Some((file, sheets, 0));
                    }
                    Err(error) => {
                        this.excel.error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[inline(never)]
pub(crate) fn render_excel_module(
    app: &Prototype,
    _workspace: &WorkspaceView,
    busy: bool,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    let palette = active_palette(cx);

    let scope_dates = current_scope_dates(app);
    let scope_label = format_scope_label(&scope_dates);
    let has_ai = app.ai.default_provider().is_some();
    let settings = &app.excel.settings;
    let template_name = Path::new(settings.template_path.trim())
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("未选择模板")
        .to_string();

    let blocked_reason: Option<&str> = if !has_ai {
        Some("请先配置默认 AI 模型")
    } else if !settings.enabled {
        Some("请在设置中启用 Excel 输出")
    } else if settings.template_path.trim().is_empty() {
        Some("请在设置中选择 .xlsx 模板文件")
    } else {
        None
    };

    let workflow_busy =
        app.excel.generating_content || app.excel.preparing_fill || app.excel.exporting;

    div()
        .id("excel-module")
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_2()
        .p_1()
        .child(
            // .excel-range-summary: bold date range + N 天 pill badge
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(palette.text)
                                .child(scope_label),
                        )
                        .child(
                            div()
                                .px_2()
                                .py(px(1.))
                                .rounded(px(999.))
                                .bg(palette.subtle)
                                .text_size(px(11.))
                                .text_color(palette.text_muted)
                                .child(format!("{} 天", scope_dates.len())),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(palette.text_muted)
                        .child(format!(
                            "模板：{template_name} · {}",
                            if settings.enabled { "已启用" } else { "未启用" }
                        )),
                ),
        )
        .when_some(blocked_reason, |el, reason| {
            el.child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.text_muted)
                    .child(reason),
            )
        })
        .when_some(app.excel.error.as_ref(), |el, error| {
            el.child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.danger)
                    .child(error.clone()),
            )
        })
        .when_some(app.excel.last_export.as_ref(), |el, res| {
            el.child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.success)
                    .child(format!("已导出 {}（写入 {} 个单元格）", res.filename, res.cells)),
            )
        })
        // <hr> divider
        .child(div().h(px(1.)).w_full().bg(palette.border).my_1())
        .child(
            // .excel-exports-head: 已导出 + 3 compact action buttons
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(palette.text_body)
                        .child("已导出"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(if workflow_busy {
                            FlatButton::new("excel-cancel-btn")
                                .h(px(26.))
                                .px_2()
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap_1()
                                .bg(palette.danger.opacity(0.14))
                                .text_size(px(11.))
                                .text_color(palette.danger)
                                .child("取消生成")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.excel.cancel_active(&this.ai.runtime);
                                    this.status = "已取消 Excel 输出".into();
                                    cx.notify();
                                }))
                        } else {
                            let disabled = busy || blocked_reason.is_some();
                            FlatButton::new("excel-gen-content-btn")
                                .disabled(disabled)
                                .accessibility_label("生成并预览 Excel 工作汇报")
                                .h(px(26.))
                                .px_2()
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap_1()
                                .bg(if disabled {
                                    palette.subtle
                                } else {
                                    palette.accent_soft
                                })
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if disabled {
                                    palette.text_muted
                                } else {
                                    palette.accent
                                })
                                .hover(move |style| {
                                    style
                                        .bg(palette.accent_fill)
                                        .text_color(palette.on_accent)
                                })
                                .child(PhIcon::FileXls.view(
                                    13.,
                                    if disabled {
                                        palette.text_muted
                                    } else {
                                        palette.accent
                                    },
                                ))
                                .child("生成 Excel")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.start_excel_content_generation(window, cx);
                                }))
                        })
                        .child(
                            FlatButton::new("excel-open-settings")
                                .disabled(busy)
                                .accessibility_label("Excel 模板与提示词设置")
                                .w(px(26.))
                                .h(px(26.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .hover(move |style| style.bg(palette.subtle_strong))
                                .child(PhIcon::GearSix.view(13., palette.text_muted))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_settings_section(
                                        crate::settings_view::SettingsSection::Excel,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            FlatButton::new("excel-refresh-exports")
                                .disabled(busy || app.excel.exports_loading)
                                .accessibility_label("刷新已导出列表")
                                .w(px(26.))
                                .h(px(26.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .hover(move |style| style.bg(palette.subtle_strong))
                                .child(PhIcon::ArrowClockwise.view(13., palette.text_muted))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.excel.invalidate_exports();
                                    this.ensure_excel_exports(window, cx);
                                })),
                        ),
                ),
        )
        .child(
            // #excel-exports-list: clean exported files list
            div()
                .id("excel-exports-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1()
                .when(app.excel.exports_loading, |el| {
                    el.child(
                        div()
                            .text_size(px(11.))
                            .text_color(palette.text_muted)
                            .child("读取导出列表…"),
                    )
                })
                .when(
                    !app.excel.exports_loading && app.excel.exports.is_empty(),
                    |el| {
                        el.child(
                            div()
                                .py_4()
                                .text_size(px(11.))
                                .text_color(palette.text_muted)
                                .child("暂无已导出的 Excel 文件"),
                        )
                    },
                )
                .children(app.excel.exports.iter().enumerate().map(|(idx, file)| {
                    let f = file.clone();
                    let range_label = match (&file.start_date, &file.end_date) {
                        (Some(s), Some(e)) => format!("{s}–{e}"),
                        _ => String::new(),
                    };
                    FlatButton::new(format!("excel-export-item-{idx}"))
                        .w_full()
                        .min_h(px(30.))
                        .px_2()
                        .py_1()
                        .rounded(px(7.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .hover(move |style| style.bg(palette.subtle_strong))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .min_w_0()
                                .child(PhIcon::FileXls.view(14., palette.success))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(palette.text)
                                        .truncate()
                                        .child(file.filename.clone()),
                                ),
                        )
                        .when(!range_label.is_empty(), |btn| {
                            btn.child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(palette.text_muted)
                                    .flex_shrink_0()
                                    .child(range_label),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_existing_excel_export(f.clone(), window, cx);
                        }))
                })),
        )
        .when(
            app.excel.workflow_open || !app.excel.content_draft.is_empty(),
            |panel| {
                // Floating Modal 1: ExcelPreviewDialog (Two-phase Content + Fill Preview)
                panel.child(
                    div()
                        .id("excel-preview-dialog")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .bg(gpui_kit::hsla(0., 0., 0., 0.34))
                        .flex()
                        .items_center()
                        .justify_center()
                        .p_2()
                        .child(
                            div()
                                .size_full()
                                .rounded(px(12.))
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.surface)
                                .p_3()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_size(px(13.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(palette.text)
                                                .child("Excel 两阶段生成与预览"),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .child(
                                                    FlatButton::new("excel-prepare-fill-btn")
                                                        .disabled(
                                                            workflow_busy
                                                                || blocked_reason.is_some()
                                                                || app
                                                                    .excel
                                                                    .content_draft
                                                                    .trim()
                                                                    .is_empty(),
                                                        )
                                                        .h(px(26.))
                                                        .px_2p5()
                                                        .rounded(px(6.))
                                                        .bg(palette.accent_soft)
                                                        .text_size(px(11.))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(palette.accent)
                                                        .child(if app.excel.preparing_fill {
                                                            "填表预览中…"
                                                        } else {
                                                            "2. 填表预览"
                                                        })
                                                        .on_click(cx.listener(
                                                            |this, _, window, cx| {
                                                                this.prepare_excel_fill_preview(
                                                                    window, cx,
                                                                );
                                                            },
                                                        )),
                                                )
                                                .when(app.excel.preview.is_some(), |bar| {
                                                    bar.child(
                                                        FlatButton::new("excel-confirm-export-btn")
                                                            .disabled(app.excel.exporting)
                                                            .h(px(26.))
                                                            .px_2p5()
                                                            .rounded(px(6.))
                                                            .bg(palette.accent_fill)
                                                            .text_size(px(11.))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(palette.on_accent)
                                                            .child(if app.excel.exporting {
                                                                "正在导出…"
                                                            } else {
                                                                "确认导出 .xlsx"
                                                            })
                                                            .on_click(cx.listener(
                                                                |this, _, window, cx| {
                                                                    this.confirm_excel_export(
                                                                        window, cx,
                                                                    );
                                                                },
                                                            )),
                                                    )
                                                })
                                                .child(
                                                    FlatButton::new("excel-close-workflow")
                                                        .w(px(26.))
                                                        .h(px(26.))
                                                        .rounded(px(6.))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .hover(move |style| {
                                                            style.bg(palette.subtle_strong)
                                                        })
                                                        .child(
                                                            PhIcon::X
                                                                .view(13., palette.text_muted),
                                                        )
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.excel
                                                                .cancel_active(&this.ai.runtime);
                                                            this.excel.workflow_open = false;
                                                            this.excel.content_draft.clear();
                                                            cx.notify();
                                                        })),
                                                ),
                                        ),
                                )
                                .child(
                                    Input::new(&app.excel.content_prompt_input)
                                        .aria_label("内容生成提示词")
                                        .w_full(),
                                )
                                .child(
                                    Editor::new(&app.excel.content_editor)
                                        .aria_label("Excel 待确认报告正文")
                                        .flex_1()
                                        .min_h(px(100.))
                                        .w_full(),
                                )
                                .when_some(
                                    app.excel.preview.as_ref(),
                                    |box_el, (preview, active_sheet)| {
                                        box_el
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(palette.text_muted)
                                                    .child(format!(
                                                        "即将修改 {} 个单元格（模板 {}）",
                                                        preview.cells.len(),
                                                        preview.template_name
                                                    )),
                                            )
                                            .child(render_table_sheets(
                                                &preview.sheets,
                                                *active_sheet,
                                                "",
                                                cx,
                                            ))
                                    },
                                ),
                        ),
                )
            },
        )
        .when_some(
            app.excel.opened_export.as_ref(),
            |panel, (file, sheets, active_sheet)| {
                // Floating Modal 2: ExcelExportPreviewDialog
                panel.child(
                    div()
                        .id("excel-export-preview-dialog")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .bg(gpui_kit::hsla(0., 0., 0., 0.34))
                        .flex()
                        .items_center()
                        .justify_center()
                        .p_2()
                        .child(
                            div()
                                .size_full()
                                .rounded(px(12.))
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.surface)
                                .p_3()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1p5()
                                                .child(PhIcon::FileXls.view(14., palette.success))
                                                .child(
                                                    div()
                                                        .text_size(px(12.))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(palette.text)
                                                        .child(file.filename.clone()),
                                                ),
                                        )
                                        .child(
                                            FlatButton::new("excel-close-opened-export")
                                                .h(px(26.))
                                                .px_2p5()
                                                .rounded(px(6.))
                                                .border_1()
                                                .border_color(palette.border)
                                                .bg(palette.surface)
                                                .text_size(px(11.))
                                                .text_color(palette.text_body)
                                                .hover(move |style| {
                                                    style
                                                        .bg(palette.subtle_strong)
                                                        .text_color(palette.text)
                                                })
                                                .child("关闭预览")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.excel.opened_export = None;
                                                    cx.notify();
                                                })),
                                        ),
                                )
                                .child(render_table_sheets(
                                    sheets,
                                    *active_sheet,
                                    "只读导出预览",
                                    cx,
                                )),
                        ),
                )
            },
        )
        .into_any_element()
}
