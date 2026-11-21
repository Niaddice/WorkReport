use std::{collections::BTreeMap, sync::Arc};

use chrono::{Datelike, NaiveDate};
use gpui_kit::{
    ClipboardItem, Context, Entity, FontWeight, SharedString, Task, Window,
    base::Button as FlatButton,
    component::input::{Input, InputState},
    div, prelude::*, px,
};
use workreport_core::{
    settings, summary,
    workspace::{Entry, Format},
};

use crate::{
    Prototype,
    icons::PhIcon,
    markdown_preview,
    theme_skin::active_palette,
    workspace_view::WorkspaceView,
};

pub(crate) const DEFAULT_SUMMARY_PROMPT: &str = "根据所选日期的日报，按项目或主题归纳工作成果、进展、问题和下一步计划，突出跨日期的变化与关联。只使用日报中的事实，保持数字、状态和原意，不编造、不夸大，使用 Markdown 输出清晰、精炼的总结。";

const WEEKDAY_CN: [&str; 7] = [
    "星期一",
    "星期二",
    "星期三",
    "星期四",
    "星期五",
    "星期六",
    "星期日",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReportDocument {
    pub(crate) date: String,
    pub(crate) content: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SummaryMode {
    None,
    Direct,
    Ai,
}

fn is_list_item(line: &str) -> Option<&str> {
    if let Some(rest) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
    {
        return Some(rest.trim());
    }
    let digits_end = line.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits_end > 0 {
        let after_digits = &line[digits_end..];
        if let Some(rest) = after_digits
            .strip_prefix(". ")
            .or_else(|| after_digits.strip_prefix(") "))
        {
            return Some(rest.trim());
        }
    }
    None
}

fn is_date_heading(line: &str) -> bool {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_suffix("日报").map(str::trim).unwrap_or(trimmed);
    let Some((y, rest)) = trimmed.split_once('年') else {
        return false;
    };
    let Some((m, rest)) = rest.split_once('月') else {
        return false;
    };
    let Some(d) = rest.strip_suffix('日') else {
        return false;
    };
    let y = y.trim();
    let m = m.trim();
    let d = d.trim();
    y.len() == 4
        && y.bytes().all(|b| b.is_ascii_digit())
        && (1..=2).contains(&m.len())
        && m.bytes().all(|b| b.is_ascii_digit())
        && (1..=2).contains(&d.len())
        && d.bytes().all(|b| b.is_ascii_digit())
}

pub(crate) fn report_items(content: &str) -> Vec<Vec<String>> {
    let mut items = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || is_date_heading(line) {
            if !current.is_empty() {
                items.push(std::mem::take(&mut current));
            }
            continue;
        }
        if let Some(item_head) = is_list_item(line) {
            if !current.is_empty() {
                items.push(std::mem::take(&mut current));
            }
            current.push(item_head.to_string());
        } else {
            current.push(line.to_string());
        }
    }
    if !current.is_empty() {
        items.push(current);
    }
    items
}

pub(crate) fn plain_text_summary(documents: &[ReportDocument]) -> String {
    documents
        .iter()
        .map(|doc| {
            let title = if let Ok(date) = NaiveDate::parse_from_str(&doc.date, "%Y%m%d") {
                format!("{}年{}月{}日", date.year(), date.month(), date.day())
            } else {
                doc.date.clone()
            };
            let items = report_items(&doc.content);
            let blocks = if items.is_empty() {
                vec![vec![doc.content.trim().to_string()]]
            } else {
                items
            };
            let mut lines = vec![title, String::new()];
            for (idx, item) in blocks.iter().enumerate() {
                let Some(first) = item.first() else {
                    continue;
                };
                if first.is_empty() {
                    continue;
                }
                lines.push(format!("{}. {}", idx + 1, first));
                for extra in &item[1..] {
                    lines.push(format!("   {extra}"));
                }
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn ai_summary_source(documents: &[ReportDocument]) -> String {
    documents
        .iter()
        .map(|doc| {
            let heading = if let Ok(date) = NaiveDate::parse_from_str(&doc.date, "%Y%m%d") {
                let wd = WEEKDAY_CN[date.weekday().num_days_from_monday() as usize];
                format!("{}年{}月{}日 {}", date.year(), date.month(), date.day(), wd)
            } else {
                doc.date.clone()
            };
            format!("### {heading}\n\n{}", doc.content.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

pub(crate) fn markdown_to_plain_text(content: &str) -> String {
    let mut out = Vec::new();
    let mut in_fence = false;
    for raw in content.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            out.push(raw.to_string());
            continue;
        }
        if trimmed == "---" || trimmed == "***" {
            continue;
        }
        let cleaned = trimmed.trim_start_matches('#').trim_start();
        let cleaned = cleaned.strip_prefix("> ").unwrap_or(cleaned);
        let cleaned = cleaned.replace("**", "").replace('`', "");
        out.push(cleaned);
    }
    out.join("\n").trim().to_string()
}

pub(crate) fn summary_export_content(content: &str, format: Format, mode: SummaryMode) -> String {
    if format == Format::Txt && mode != SummaryMode::Direct {
        markdown_to_plain_text(content)
    } else {
        content.to_string()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct PrepareKey {
    workspace: usize,
    dates: Vec<String>,
    entries_len: usize,
    revision: u64,
}

pub(crate) struct SummaryState {
    pub(crate) settings: summary::Settings,
    pub(crate) dir_input: Entity<InputState>,
    pub(crate) prompt_input: Entity<InputState>,
    pub(crate) scope_dates: Vec<String>,
    pub(crate) documents: Vec<ReportDocument>,
    pub(crate) mode: SummaryMode,
    pub(crate) result: String,
    pub(crate) export_format: Format,
    pub(crate) exporting: bool,
    pub(crate) exported_path: Option<String>,
    pub(crate) export_error: Option<String>,
    pub(crate) dir_saving: bool,
    pub(crate) dir_error: Option<String>,
    pub(crate) preparing: bool,
    pub(crate) prepare_error: Option<String>,
    pub(crate) generating: bool,
    pub(crate) ai_error: Option<String>,
    pub(crate) prompt_dialog_open: bool,
    prepare_key: Option<PrepareKey>,
    prepare_revision: u64,
    prepare_request: u64,
    prepare_task: Option<Task<()>>,
    gen_seq: u64,
    gen_request_id: Option<String>,
    gen_task: Option<Task<()>>,
}

impl SummaryState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Prototype>) -> Self {
        let dir_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(summary::Settings::default().output_directory)
        });
        let prompt_input =
            cx.new(|cx| InputState::new(window, cx).default_value(DEFAULT_SUMMARY_PROMPT));
        Self {
            settings: summary::Settings::default(),
            dir_input,
            prompt_input,
            scope_dates: Vec::new(),
            documents: Vec::new(),
            mode: SummaryMode::None,
            result: String::new(),
            export_format: Format::Md,
            exporting: false,
            exported_path: None,
            export_error: None,
            dir_saving: false,
            dir_error: None,
            preparing: false,
            prepare_error: None,
            generating: false,
            ai_error: None,
            prompt_dialog_open: false,
            prepare_key: None,
            prepare_revision: 0,
            prepare_request: 0,
            prepare_task: None,
            gen_seq: 0,
            gen_request_id: None,
            gen_task: None,
        }
    }

    pub(crate) fn reset_for_workspace(
        &mut self,
        summary_settings: summary::Settings,
        default_prompt: &str,
        runtime: &workreport_core::ai::Runtime,
        window: &mut Window,
        cx: &mut Context<Prototype>,
    ) {
        self.cancel_generation(runtime);
        self.settings = summary_settings.clone();
        self.scope_dates.clear();
        self.documents.clear();
        self.mode = SummaryMode::None;
        self.result.clear();
        self.exported_path = None;
        self.export_error = None;
        self.dir_error = None;
        self.prepare_error = None;
        self.ai_error = None;
        self.prompt_dialog_open = false;
        self.prepare_key = None;
        self.prepare_revision += 1;
        self.prepare_request += 1;
        self.prepare_task = None;

        let dir = summary_settings.output_directory;
        self.dir_input.update(cx, |i, cx| i.set_value(dir, window, cx));
        let prompt = if default_prompt.trim().is_empty() {
            DEFAULT_SUMMARY_PROMPT.to_string()
        } else {
            default_prompt.to_string()
        };
        self.prompt_input
            .update(cx, |i, cx| i.set_value(prompt, window, cx));
    }

    pub(crate) fn invalidate(&mut self) {
        self.prepare_key = None;
        self.prepare_revision += 1;
    }

    pub(crate) fn cancel_generation(&mut self, runtime: &workreport_core::ai::Runtime) {
        if let Some(id) = self.gen_request_id.take() {
            runtime.cancel(&id);
        }
        self.generating = false;
        self.gen_task = None;
    }
}

impl Prototype {
    pub(crate) fn ensure_summary_prepared(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let (start, end) = self.calendar.selected_range();
        let mut scope_dates = Vec::new();
        let mut cursor = Some(start);
        while let Some(day) = cursor {
            if day > end {
                break;
            }
            scope_dates.push(day.format("%Y%m%d").to_string());
            cursor = day.checked_add_days(chrono::Days::new(1));
        }

        let key = PrepareKey {
            workspace: Arc::as_ptr(&workspace.reader) as usize,
            dates: scope_dates.clone(),
            entries_len: workspace.entries.len(),
            revision: self.summary.prepare_revision,
        };
        if self.summary.prepare_key.as_ref() == Some(&key) {
            return;
        }
        self.summary.prepare_key = Some(key.clone());
        self.summary.scope_dates = scope_dates.clone();
        self.summary.prepare_request += 1;
        let request_id = self.summary.prepare_request;
        self.summary.prepare_error = None;
        self.summary.exported_path = None;
        self.summary.export_error = None;

        let entry_map: BTreeMap<String, Entry> = workspace
            .entries
            .iter()
            .map(|e| (e.date.clone(), e.clone()))
            .collect();
        let existing: Vec<Entry> = scope_dates
            .iter()
            .filter_map(|date| entry_map.get(date).cloned())
            .collect();

        if existing.is_empty() {
            self.summary.preparing = false;
            self.summary.documents.clear();
            self.summary.result.clear();
            self.summary.mode = SummaryMode::None;
            return;
        }

        self.summary.preparing = true;
        let reader = workspace.reader.clone();
        let load = cx.background_executor().spawn(async move {
            let mut docs = Vec::with_capacity(existing.len());
            for entry in existing {
                let doc = reader.read(&entry.date, entry.format)?;
                docs.push(ReportDocument {
                    date: doc.date,
                    content: doc.content,
                });
            }
            Ok::<_, String>(docs)
        });
        self.summary.prepare_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                if this.summary.prepare_request != request_id
                    || this.summary.prepare_key.as_ref() != Some(&key)
                {
                    return;
                }
                this.summary.preparing = false;
                match result {
                    Ok(docs) => {
                        this.summary.documents = docs;
                        this.summary.prepare_error = None;
                        if !this.summary.documents.is_empty()
                            && this.summary.mode != SummaryMode::Ai
                        {
                            this.summary.mode = SummaryMode::Direct;
                            this.summary.result = plain_text_summary(&this.summary.documents);
                        }
                    }
                    Err(error) => {
                        this.summary.prepare_error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn generate_direct_summary(&mut self, cx: &mut Context<Self>) {
        if self.summary.documents.is_empty() {
            self.summary.ai_error = Some("所选日期没有可总结的日报。".into());
            cx.notify();
            return;
        }
        self.summary.ai_error = None;
        self.summary.exported_path = None;
        self.summary.export_error = None;
        self.summary.mode = SummaryMode::Direct;
        self.summary.result = plain_text_summary(&self.summary.documents);
        self.status = "已生成直接汇总结果".into();
        cx.notify();
    }

    pub(crate) fn generate_ai_summary(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.summary.generating || self.summary.documents.is_empty() {
            return;
        }
        if let Err(error) = workreport_core::ai::default_provider(&self.ai.settings) {
            self.summary.ai_error = Some(error.clone());
            self.status = error.into();
            cx.notify();
            return;
        }
        let prompt_val = self.summary.prompt_input.read(cx).value().to_string();
        if prompt_val.trim().is_empty() {
            self.summary.ai_error = Some("请填写本次总结提示词".into());
            cx.notify();
            return;
        }
        if prompt_val.chars().count() > 2000 {
            self.summary.ai_error = Some("框选总结提示词不能超过 2000 字".into());
            cx.notify();
            return;
        }

        let source = ai_summary_source(&self.summary.documents);
        self.summary.gen_seq += 1;
        let request_id = format!("summary-{}", self.summary.gen_seq);
        self.summary.generating = true;
        self.summary.ai_error = None;
        self.summary.exported_path = None;
        self.summary.export_error = None;
        self.summary.gen_request_id = Some(request_id.clone());

        let backend = self.ai.backend.clone();
        let runtime = self.ai.runtime.clone();
        let settings = self.ai.settings.clone();
        let req_copy = request_id.clone();
        self.status = "正在调用 AI 生成总结…".into();
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            backend.summarize(&settings, &source, Some(prompt_val), &req_copy, &runtime)
        });
        self.summary.gen_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.summary.gen_request_id.as_deref() != Some(request_id.as_str()) {
                    return;
                }
                this.summary.generating = false;
                this.summary.gen_request_id = None;
                this.summary.gen_task = None;
                match result {
                    Ok(text) => {
                        this.summary.mode = SummaryMode::Ai;
                        this.summary.result = text;
                        this.status = "AI 总结生成完成".into();
                    }
                    Err(error) => {
                        this.summary.ai_error = Some(error.clone());
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn save_summary_directory(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        if self.summary.dir_saving {
            return;
        }
        let next = summary::Settings {
            output_directory: self.summary.dir_input.read(cx).value().to_string(),
        };
        let root = workspace.reader.root().to_path_buf();
        let expected_path = workspace.info_path();
        self.summary.dir_saving = true;
        self.summary.dir_error = None;
        cx.notify();
        let save = cx
            .background_executor()
            .spawn(async move { settings::save_summary_settings(&root, &expected_path, next) });
        cx.spawn_in(window, async move |this, cx| {
            let result = save.await;
            let _ = this.update(cx, |this, cx| {
                this.summary.dir_saving = false;
                match result {
                    Ok(saved) => {
                        this.summary.settings = saved;
                        this.status = "总结输出目录已保存".into();
                    }
                    Err(error) => {
                        this.summary.dir_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn export_summary_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        if self.summary.exporting || self.summary.result.trim().is_empty() {
            return;
        }
        let dates: Vec<String> = self
            .summary
            .documents
            .iter()
            .map(|doc| doc.date.clone())
            .collect();
        if dates.is_empty() {
            self.summary.export_error = Some("没有可导出的总结日期".into());
            cx.notify();
            return;
        }
        let format = self.summary.export_format;
        let content = summary_export_content(&self.summary.result, format, self.summary.mode);
        let settings = self.summary.settings.clone();
        let root = workspace.reader.root().to_path_buf();
        let expected_path = workspace.info_path();
        self.summary.exporting = true;
        self.summary.export_error = None;
        self.summary.exported_path = None;
        cx.notify();

        let export = cx.background_executor().spawn(async move {
            summary::validate_workspace(&root, &expected_path)?;
            summary::export(&root, &settings, &dates, &content, format)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = export.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.summary.exporting = false;
                match result {
                    Ok(exported) => {
                        this.summary.exported_path = Some(exported.path.clone());
                        this.directory.invalidate_tree();
                        this.ensure_directory_tree(window, cx);
                        this.status = format!("总结已导出：{}", exported.filename).into();
                    }
                    Err(error) => {
                        this.summary.export_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

pub(crate) fn render_summary_module(
    app: &Prototype,
    _workspace: &WorkspaceView,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let palette = active_palette(cx);

    let doc_count = app.summary.documents.len();
    let range_len = app.summary.scope_dates.len();
    let scope_text = if app.summary.preparing {
        "正在读取所选日期日报…".to_string()
    } else if doc_count == 0 {
        format!("选定 {range_len} 天 · 暂无记录")
    } else if range_len <= 1 {
        format!("{} 篇日报", doc_count)
    } else {
        format!("{} 篇日报（共 {} 天）", doc_count, range_len)
    };

    let has_provider = app.ai.default_provider().is_some();
    let mode_label = match app.summary.mode {
        SummaryMode::Direct => "直接汇总",
        SummaryMode::Ai => "AI 总结",
        SummaryMode::None => "待汇总",
    };

    let pill_btn = |id: &'static str, label: &'static str, active: bool, disabled: bool| {
        FlatButton::new(id)
            .disabled(disabled)
            .h(px(24.))
            .px_2()
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap_1()
            .bg(if active {
                palette.accent_soft
            } else {
                palette.subtle.opacity(0.01)
            })
            .border_1()
            .border_color(if active {
                palette.accent
            } else {
                palette.border.opacity(0.01)
            })
            .text_size(px(11.))
            .font_weight(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .text_color(if disabled {
                palette.text_muted
            } else if active {
                palette.accent
            } else {
                palette.text_body
            })
            .hover(move |style| {
                style
                    .bg(if active {
                        palette.accent_soft
                    } else {
                        palette.subtle_strong
                    })
                    .text_color(palette.text)
            })
            .child(label)
    };

    div()
        .id("summary-module")
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .bg(palette.surface)
        .child(
            // .summary-header: single compact edge-to-edge header bar
            div()
                .min_h(px(34.))
                .px_3()
                .py_1()
                .bg(palette.surface_header)
                .border_b_1()
                .border_color(palette.border)
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(palette.text)
                                .child(scope_text),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py(px(1.))
                                .rounded(px(4.))
                                .bg(if app.summary.mode == SummaryMode::Ai {
                                    palette.accent_soft
                                } else {
                                    palette.subtle
                                })
                                .text_size(px(10.))
                                .text_color(if app.summary.mode == SummaryMode::Ai {
                                    palette.accent
                                } else {
                                    palette.text_muted
                                })
                                .child(mode_label),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_1()
                        .child(
                            pill_btn(
                                "summary-direct-btn",
                                "直接汇总",
                                app.summary.mode == SummaryMode::Direct,
                                busy || app.summary.preparing || doc_count == 0,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.generate_direct_summary(cx);
                            })),
                        )
                        .child(if app.summary.generating {
                            FlatButton::new("summary-cancel-btn")
                                .h(px(24.))
                                .px_2()
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap_1()
                                .bg(palette.danger.opacity(0.14))
                                .text_size(px(11.))
                                .text_color(palette.danger)
                                .child("取消 AI")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.summary.cancel_generation(&this.ai.runtime);
                                    this.status = "已取消 AI 总结".into();
                                    cx.notify();
                                }))
                        } else {
                            let disabled = busy || app.summary.preparing || doc_count == 0;
                            FlatButton::new("summary-ai-btn")
                                .disabled(disabled)
                                .h(px(24.))
                                .px_2()
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap_1()
                                .bg(if app.summary.mode == SummaryMode::Ai {
                                    palette.accent_fill
                                } else {
                                    palette.accent_soft
                                })
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if disabled {
                                    palette.text_muted
                                } else if app.summary.mode == SummaryMode::Ai {
                                    palette.on_accent
                                } else {
                                    palette.accent
                                })
                                .hover(move |style| style.bg(palette.accent_fill).text_color(palette.on_accent))
                                .child(PhIcon::Sparkle.view(
                                    12.,
                                    if app.summary.mode == SummaryMode::Ai {
                                        palette.on_accent
                                    } else {
                                        palette.accent
                                    },
                                ))
                                .child("AI 总结")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.generate_ai_summary(window, cx);
                                }))
                        })
                        .child(
                            pill_btn(
                                "summary-prompt-dialog-btn",
                                "提示词/目录",
                                app.summary.prompt_dialog_open,
                                false,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.summary.prompt_dialog_open = !this.summary.prompt_dialog_open;
                                cx.notify();
                            })),
                        )
                        .child(div().w(px(1.)).h(px(14.)).bg(palette.border).mx(px(2.)))
                        .child(
                            pill_btn(
                                "summary-format-md",
                                "MD",
                                app.summary.export_format == Format::Md,
                                false,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.summary.export_format = Format::Md;
                                cx.notify();
                            })),
                        )
                        .child(
                            pill_btn(
                                "summary-format-txt",
                                "TXT",
                                app.summary.export_format == Format::Txt,
                                false,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.summary.export_format = Format::Txt;
                                cx.notify();
                            })),
                        )
                        .child(
                            FlatButton::new("summary-copy")
                                .disabled(app.summary.result.trim().is_empty())
                                .h(px(24.))
                                .px_1p5()
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(px(11.))
                                .text_color(if app.summary.result.trim().is_empty() {
                                    palette.text_muted
                                } else {
                                    palette.text_body
                                })
                                .hover(move |style| style.bg(palette.subtle_strong).text_color(palette.text))
                                .child(PhIcon::Copy.view(12., palette.text_muted))
                                .child("复制")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let text = summary_export_content(
                                        &this.summary.result,
                                        this.summary.export_format,
                                        this.summary.mode,
                                    );
                                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                                    this.status = "已复制总结内容".into();
                                    cx.notify();
                                })),
                        )
                        .child(
                            FlatButton::new("summary-export-btn")
                                .disabled(
                                    busy
                                        || app.summary.exporting
                                        || app.summary.result.trim().is_empty(),
                                )
                                .h(px(24.))
                                .px_2()
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(px(11.))
                                .text_color(
                                    if busy
                                        || app.summary.exporting
                                        || app.summary.result.trim().is_empty()
                                    {
                                        palette.text_muted
                                    } else {
                                        palette.text_body
                                    },
                                )
                                .hover(move |style| style.bg(palette.subtle_strong).text_color(palette.text))
                                .child(PhIcon::DownloadSimple.view(12., palette.text_muted))
                                .child(if app.summary.exporting { "导出中…" } else { "导出" })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.export_summary_file(window, cx);
                                })),
                        )
                        .child(
                            FlatButton::new("summary-open-ai-settings")
                                .accessibility_label(if has_provider { "AI 模型设置" } else { "配置默认 AI 模型" })
                                .w(px(24.))
                                .h(px(24.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .hover(move |style| style.bg(palette.subtle_strong))
                                .child(PhIcon::GearSix.view(
                                    13.,
                                    if has_provider {
                                        palette.text_muted
                                    } else {
                                        palette.accent
                                    },
                                ))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_ai_settings_dialog(window, cx);
                                })),
                        ),
                ),
        )
        .when_some(app.summary.prepare_error.as_ref(), |el, err| {
            el.child(
                div()
                    .px_3()
                    .py_1()
                    .bg(palette.danger.opacity(0.14))
                    .text_size(px(11.))
                    .text_color(palette.danger)
                    .child(err.clone()),
            )
        })
        .when_some(app.summary.ai_error.as_ref(), |el, err| {
            el.child(
                div()
                    .px_3()
                    .py_1()
                    .bg(palette.danger.opacity(0.14))
                    .text_size(px(11.))
                    .text_color(palette.danger)
                    .child(err.clone()),
            )
        })
        .when_some(app.summary.dir_error.as_ref(), |el, err| {
            el.child(
                div()
                    .px_3()
                    .py_1()
                    .bg(palette.danger.opacity(0.14))
                    .text_size(px(11.))
                    .text_color(palette.danger)
                    .child(err.clone()),
            )
        })
        .when_some(app.summary.export_error.as_ref(), |el, err| {
            el.child(
                div()
                    .px_3()
                    .py_1()
                    .bg(palette.danger.opacity(0.14))
                    .text_size(px(11.))
                    .text_color(palette.danger)
                    .child(err.clone()),
            )
        })
        .when_some(app.summary.exported_path.as_ref(), |el, path| {
            el.child(
                div()
                    .px_3()
                    .py_1()
                    .bg(palette.success.opacity(0.14))
                    .text_size(px(11.))
                    .text_color(palette.success)
                    .child(format!("已导出至：{path}")),
            )
        })
        .child(
            // Main full-bleed reading canvas
            div()
                .id("summary-result-scroll")
                .flex_1()
                .min_h_0()
                .px_4()
                .py_3()
                .overflow_y_scroll()
                .child(if app.summary.result.trim().is_empty() {
                    div()
                        .text_size(px(12.))
                        .text_color(palette.text_muted)
                        .child(if doc_count == 0 {
                            "当前选定日期或范围暂无已记录日报。"
                        } else {
                            "点击「直接汇总」或「AI 总结」生成工作总结。"
                        })
                        .into_any_element()
                } else if app.summary.mode == SummaryMode::Ai
                    && app.summary.export_format == Format::Md
                {
                    markdown_preview(
                        "summary-result-md",
                        SharedString::from(app.summary.result.clone()),
                    )
                    .into_any_element()
                } else {
                    let rendered = summary_export_content(
                        &app.summary.result,
                        app.summary.export_format,
                        app.summary.mode,
                    );
                    div()
                        .text_size(px(13.))
                        .line_height(px(22.))
                        .text_color(palette.text)
                        .child(rendered)
                        .into_any_element()
                }),
        )
        .when(app.summary.prompt_dialog_open, |el| {
            el.child(
                // Floating SummaryGenerationDialog overlay
                div()
                    .id("summary-generation-dialog")
                    .occlude()
                    .absolute()
                    .inset_0()
                    .bg(gpui_kit::hsla(0., 0., 0., 0.32))
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_3()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(520.))
                            .rounded(px(12.))
                            .border_1()
                            .border_color(palette.border)
                            .bg(palette.surface)
                            .p_3()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(palette.text)
                                            .child("AI 总结提示词与导出目录"),
                                    )
                                    .child(
                                        FlatButton::new("summary-dialog-close")
                                            .w(px(24.))
                                            .h(px(24.))
                                            .rounded(px(6.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(move |style| style.bg(palette.subtle_strong))
                                            .child(PhIcon::X.view(13., palette.text_muted))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.summary.prompt_dialog_open = false;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(palette.text_muted)
                                            .child("AI 总结提示词"),
                                    )
                                    .child(
                                        Input::new(&app.summary.prompt_input)
                                            .aria_label("AI 总结提示词")
                                            .w_full(),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(palette.text_muted)
                                                    .child("输出目录"),
                                            )
                                            .child(
                                                Input::new(&app.summary.dir_input)
                                                    .aria_label("总结输出目录")
                                                    .flex_1(),
                                            )
                                            .child(
                                                FlatButton::new("summary-dir-save")
                                                    .disabled(app.summary.dir_saving)
                                                    .h(px(28.))
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
                                                    .child(if app.summary.dir_saving {
                                                        "保存中"
                                                    } else {
                                                        "保存目录"
                                                    })
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.save_summary_directory(window, cx);
                                                    })),
                                            ),
                                    )
                                    .child(
                                        FlatButton::new("summary-dialog-run-ai")
                                            .disabled(
                                                busy || app.summary.preparing || doc_count == 0,
                                            )
                                            .h(px(28.))
                                            .px_3()
                                            .rounded(px(6.))
                                            .bg(palette.accent_fill)
                                            .text_size(px(11.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(palette.on_accent)
                                            .child("生成 AI 总结")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.summary.prompt_dialog_open = false;
                                                this.generate_ai_summary(window, cx);
                                            })),
                                    ),
                            ),
                    ),
            )
        })
}
