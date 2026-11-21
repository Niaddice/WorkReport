use chrono::{Local, TimeZone};
use gpui_kit::{
    ClipboardItem, Context, MouseButton, SharedString, Window,
    base::Disableable,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
    },
    div,
    prelude::*,
    px,
};
use workreport_core::{history::Version, journal_edit::JournalEdit};

use crate::{
    Prototype,
    line_diff::{DiffLineKind, DiffResult, SplitSideKind, compute_line_diff},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryDiffMode {
    Split,
    Unified,
}

pub(crate) fn render_diff_body(
    id_prefix: &str,
    diff: &DiffResult,
    mode: HistoryDiffMode,
    left_heading: &str,
    right_heading: &str,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    let theme = cx.theme();
    let background = theme.background;
    let border = theme.border;
    let muted = theme.muted_foreground;
    let danger = theme.red;
    let green = theme.green;

    if mode == HistoryDiffMode::Split {
        div()
            .id(format!("{id_prefix}-split-body"))
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .border_b_1()
                    .border_color(border)
                    .text_size(px(11.))
                    .text_color(muted)
                    .child(div().flex_1().px_2().py_1().child(left_heading.to_string()))
                    .child(
                        div()
                            .flex_1()
                            .px_2()
                            .py_1()
                            .border_l_1()
                            .border_color(border)
                            .child(right_heading.to_string()),
                    ),
            )
            .child(
                div()
                    .id(format!("{id_prefix}-split-scroll"))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(diff.split_rows.iter().map(|row| {
                        let left_bg = match row.left.kind {
                            SplitSideKind::Remove => danger.opacity(0.14),
                            SplitSideKind::Empty => muted.opacity(0.08),
                            _ => background,
                        };
                        let right_bg = match row.right.kind {
                            SplitSideKind::Add => green.opacity(0.14),
                            SplitSideKind::Empty => muted.opacity(0.08),
                            _ => background,
                        };
                        div()
                            .flex()
                            .w_full()
                            .text_size(px(12.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .gap_2()
                                    .px_2()
                                    .py(px(2.))
                                    .bg(left_bg)
                                    .child(
                                        div()
                                            .w(px(32.))
                                            .flex_shrink_0()
                                            .text_color(muted)
                                            .child(
                                                row.left
                                                    .line
                                                    .map(|n| n.to_string())
                                                    .unwrap_or_default(),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w(px(12.))
                                            .flex_shrink_0()
                                            .text_color(danger)
                                            .child(if row.left.kind == SplitSideKind::Remove {
                                                "−"
                                            } else {
                                                " "
                                            }),
                                    )
                                    .child(div().flex_1().min_w_0().child(row.left.text.clone())),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .gap_2()
                                    .px_2()
                                    .py(px(2.))
                                    .border_l_1()
                                    .border_color(border)
                                    .bg(right_bg)
                                    .child(
                                        div()
                                            .w(px(32.))
                                            .flex_shrink_0()
                                            .text_color(muted)
                                            .child(
                                                row.right
                                                    .line
                                                    .map(|n| n.to_string())
                                                    .unwrap_or_default(),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w(px(12.))
                                            .flex_shrink_0()
                                            .text_color(green)
                                            .child(if row.right.kind == SplitSideKind::Add {
                                                "+"
                                            } else {
                                                " "
                                            }),
                                    )
                                    .child(div().flex_1().min_w_0().child(row.right.text.clone())),
                            )
                    })),
            )
            .into_any_element()
    } else {
        div()
            .id(format!("{id_prefix}-unified-scroll"))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .children(diff.lines.iter().map(|line| {
                let bg = match line.kind {
                    DiffLineKind::Remove => danger.opacity(0.14),
                    DiffLineKind::Add => green.opacity(0.14),
                    DiffLineKind::Context => background,
                };
                let (marker, marker_color) = match line.kind {
                    DiffLineKind::Remove => ("−", danger),
                    DiffLineKind::Add => ("+", green),
                    DiffLineKind::Context => (" ", muted),
                };
                div()
                    .flex()
                    .gap_2()
                    .px_2()
                    .py(px(2.))
                    .bg(bg)
                    .text_size(px(12.))
                    .child(
                        div()
                            .w(px(32.))
                            .flex_shrink_0()
                            .text_color(muted)
                            .child(line.old_line.map(|n| n.to_string()).unwrap_or_default()),
                    )
                    .child(
                        div()
                            .w(px(32.))
                            .flex_shrink_0()
                            .text_color(muted)
                            .child(line.new_line.map(|n| n.to_string()).unwrap_or_default()),
                    )
                    .child(div().w(px(12.)).flex_shrink_0().text_color(marker_color).child(marker))
                    .child(div().flex_1().min_w_0().child(line.text.clone()))
            }))
            .into_any_element()
    }
}

pub(super) struct HistoryDiffState {
    pub(super) base_version: Version,
    pub(super) available_versions: Vec<Version>,
    pub(super) target_id: Option<String>,
    pub(super) mode: HistoryDiffMode,
    pub(super) base_content: String,
    pub(super) target_content: String,
    pub(super) diff: DiffResult,
    pub(super) loading: bool,
    pub(super) error: Option<String>,
    pub(super) confirming_restore: bool,
    pub(super) restoring: bool,
    pub(super) request_id: u64,
}

impl HistoryDiffState {
    fn new(base_version: Version, available_versions: Vec<Version>) -> Self {
        Self {
            base_version,
            available_versions,
            target_id: None,
            mode: HistoryDiffMode::Split,
            base_content: String::new(),
            target_content: String::new(),
            diff: compute_line_diff("", ""),
            loading: true,
            error: None,
            confirming_restore: false,
            restoring: false,
            request_id: 1,
        }
    }
}

pub(super) fn format_version_time(saved_at: u64) -> String {
    let millis = i64::try_from(saved_at).unwrap_or(0);
    Local
        .timestamp_millis_opt(millis)
        .single()
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".to_owned())
}

pub(super) fn format_version_date(date: &str) -> String {
    if date.len() == 8 {
        format!("{}年{}月{}日", &date[..4], &date[4..6], &date[6..8])
    } else {
        date.to_owned()
    }
}

pub(super) fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}

pub(super) fn render_history_diff_dialog(
    state: &HistoryDiffState,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let theme = cx.theme();
    let border = theme.border;
    let background = theme.background;
    let muted = theme.muted_foreground;
    let primary = theme.primary;
    let danger = theme.danger;
    let green = theme.green;

    let base_time = format_version_time(state.base_version.saved_at);
    let badge = state.base_version.format.extension().to_uppercase();
    let stats_el = if state.loading || state.error.is_some() {
        div().into_any_element()
    } else if state.diff.identical {
        div()
            .id("history-diff-identical")
            .text_size(px(12.))
            .text_color(muted)
            .child("内容完全一致")
            .into_any_element()
    } else {
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(12.))
            .child(
                div()
                    .id("history-diff-additions")
                    .text_color(green)
                    .child(format!("+{}", state.diff.additions)),
            )
            .child(
                div()
                    .id("history-diff-deletions")
                    .text_color(danger)
                    .child(format!("−{}", state.diff.deletions)),
            )
            .into_any_element()
    };

    let target_selector = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(border)
        .child(
            div()
                .text_size(px(12.))
                .child(format!("基准：历史快照 ({base_time})  ➔  目标：")),
        )
        .child(
            Button::new("history-diff-target-current")
                .label("当前最新内容")
                .toggled(state.target_id.is_none())
                .when(state.target_id.is_none(), Button::primary)
                .disabled(state.loading || state.restoring)
                .h(px(24.))
                .px_2()
                .text_size(px(11.))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.select_history_diff_target(None, window, cx);
                })),
        )
        .children(
            state
                .available_versions
                .iter()
                .filter(|ver| ver.id != state.base_version.id)
                .map(|ver| {
                    let selected = state.target_id.as_deref() == Some(ver.id.as_str());
                    let id = ver.id.clone();
                    let label = format!(
                        "快照 {} ({})",
                        format_version_time(ver.saved_at),
                        ver.format.extension().to_uppercase()
                    );
                    Button::new(format!("history-diff-target-{}", ver.id))
                        .label(label)
                        .toggled(selected)
                        .when(selected, Button::primary)
                        .disabled(state.loading || state.restoring)
                        .h(px(24.))
                        .px_2()
                        .text_size(px(11.))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_history_diff_target(Some(id.clone()), window, cx);
                        }))
                }),
        );

    let body = if state.loading {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(muted)
            .child("正在读取版本差异…")
            .into_any_element()
    } else if let Some(error) = &state.error {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(danger)
            .child(error.clone())
            .into_any_element()
    } else if state.diff.identical {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(muted)
            .child("两个版本内容完全一致，没有文本修改。")
            .into_any_element()
    } else {
        let target_heading = if state.target_id.is_none() {
            "当前最新内容 (新)"
        } else {
            "对比历史快照 (新)"
        };
        render_diff_body(
            "history-diff",
            &state.diff,
            state.mode,
            "基准历史版本 (旧)",
            target_heading,
            cx,
        )
    };

    let restore_controls = if state.confirming_restore {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().text_size(px(12.)).child("覆盖并恢复到此版本？"))
            .child(
                Button::new("history-diff-confirm-restore")
                    .label(if state.restoring {
                        "正在恢复…"
                    } else {
                        "确认恢复"
                    })
                    .primary()
                    .disabled(state.restoring)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.restore_history_version(window, cx);
                    })),
            )
            .child(
                Button::new("history-diff-cancel-restore")
                    .label("取消")
                    .disabled(state.restoring)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(diff) = &mut this.history_diff {
                            diff.confirming_restore = false;
                            cx.notify();
                        }
                    })),
            )
            .into_any_element()
    } else {
        Button::new("history-diff-restore")
            .label("恢复此历史版本")
            .primary()
            .disabled(
                state.loading
                    || state.restoring
                    || state.error.is_some()
                    || (state.target_id.is_none() && state.diff.identical),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(diff) = &mut this.history_diff {
                    diff.confirming_restore = true;
                    cx.notify();
                }
            }))
            .into_any_element()
    };

    div()
        .id("history-diff-backdrop")
        .absolute()
        .inset_0()
        .occlude()
        .bg(background.opacity(0.7))
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("history-diff-dialog")
                .size_full()
                .max_w(px(1020.))
                .max_h(px(680.))
                .flex()
                .flex_col()
                .bg(background)
                .border_1()
                .border_color(primary)
                .rounded_md()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(border)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_base().child("历史版本差异对比"))
                                .child(div().text_size(px(12.)).text_color(muted).child(format!(
                                    "{} · 快照时间 {} ({badge})",
                                    format_version_date(&state.base_version.date),
                                    base_time
                                ))),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(stats_el)
                                .child(
                                    Button::new("history-diff-mode-split")
                                        .label("并排")
                                        .toggled(state.mode == HistoryDiffMode::Split)
                                        .when(state.mode == HistoryDiffMode::Split, Button::primary)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(diff) = &mut this.history_diff {
                                                diff.mode = HistoryDiffMode::Split;
                                                cx.notify();
                                            }
                                        })),
                                )
                                .child(
                                    Button::new("history-diff-mode-unified")
                                        .label("逐行")
                                        .toggled(state.mode == HistoryDiffMode::Unified)
                                        .when(
                                            state.mode == HistoryDiffMode::Unified,
                                            Button::primary,
                                        )
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(diff) = &mut this.history_diff {
                                                diff.mode = HistoryDiffMode::Unified;
                                                cx.notify();
                                            }
                                        })),
                                )
                                .child(
                                    Button::new("history-diff-close")
                                        .label("关闭")
                                        .disabled(state.restoring)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.history_diff = None;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
                .child(target_selector)
                .child(body)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .px_4()
                        .py_2()
                        .border_t_1()
                        .border_color(border)
                        .child(div().text_size(px(11.)).text_color(muted).child("按 Esc 关闭"))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Button::new("history-diff-copy")
                                        .label("复制此版本全文")
                                        .disabled(state.loading || state.error.is_some())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(diff) = &this.history_diff {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    diff.base_content.clone(),
                                                ));
                                                this.status = "已复制所选历史版本全文".into();
                                                cx.notify();
                                            }
                                        })),
                                )
                                .child(restore_controls),
                        ),
                ),
        )
}

impl Prototype {
    pub(super) fn copy_history_version(
        &mut self,
        version: Version,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let reader = workspace.reader.clone();
        let load = cx
            .background_executor()
            .spawn(async move { reader.read_history(&version.date, &version.id) });
        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(content) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(content));
                        this.status = "历史版本已复制到剪贴板".into();
                    }
                    Err(error) => {
                        this.status = format!("复制历史版本失败：{error}").into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_history_diff(
        &mut self,
        version: Version,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let same_day_versions = workspace
            .journal
            .as_ref()
            .filter(|j| j.date() == version.date)
            .and_then(|j| j.versions().as_ref().ok())
            .cloned()
            .unwrap_or_default();
        self.history_diff = Some(HistoryDiffState::new(version, same_day_versions));
        self.load_history_diff_contents(window, cx);
        cx.notify();
    }

    pub(super) fn select_history_diff_target(
        &mut self,
        target_id: Option<String>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(diff) = &mut self.history_diff else {
            return;
        };
        if diff.loading || diff.restoring {
            return;
        }
        diff.target_id = target_id;
        diff.confirming_restore = false;
        self.load_history_diff_contents(window, cx);
        cx.notify();
    }

    fn load_history_diff_contents(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(diff) = &mut self.history_diff else {
            return;
        };
        diff.request_id += 1;
        let request_id = diff.request_id;
        diff.loading = true;
        diff.error = None;
        let base_version = diff.base_version.clone();
        let target_id = diff.target_id.clone();
        let reader = workspace.reader.clone();
        let entry_format = workspace.entry_format(&base_version.date);

        let live_current = if target_id.is_none() {
            if let Some(edit) = &self.workspace_edit {
                if edit.date() == base_version.date {
                    Some(edit.editor.read(cx).value().to_string())
                } else {
                    None
                }
            } else {
                workspace
                    .journal
                    .as_ref()
                    .filter(|j| j.date() == base_version.date)
                    .map(|j| j.content().to_string())
            }
        } else {
            None
        };

        let load = cx.background_executor().spawn(async move {
            let base_text = reader.read_history(&base_version.date, &base_version.id)?;
            let target_text = if let Some(target_id) = target_id {
                reader.read_history(&base_version.date, &target_id)?
            } else if let Some(live) = live_current {
                live
            } else if let Some(format) = entry_format {
                reader
                    .read(&base_version.date, format)
                    .map(|doc| doc.content)
                    .unwrap_or_default()
            } else {
                String::new()
            };
            Ok::<(String, String), String>((base_text, target_text))
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                let Some(diff) = &mut this.history_diff else {
                    return;
                };
                if diff.request_id != request_id {
                    return;
                }
                diff.loading = false;
                match result {
                    Ok((base_text, target_text)) => {
                        diff.diff = compute_line_diff(&base_text, &target_text);
                        diff.base_content = base_text;
                        diff.target_content = target_text;
                    }
                    Err(error) => {
                        diff.error = Some(format!("读取对比内容失败：{error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn restore_history_version(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(diff) = &mut self.history_diff else {
            return;
        };
        if diff.loading || diff.restoring || diff.error.is_some() {
            return;
        }
        let version = diff.base_version.clone();
        let content = diff.base_content.clone();

        if let Some(edit) = &self.workspace_edit {
            if edit.date() != version.date {
                self.status = "请先保存或退出当前正在编辑的日报，再恢复其他日期的历史版本。".into();
                diff.confirming_restore = false;
                cx.notify();
                return;
            }
            let editor = edit.editor.clone();
            editor.update(cx, |editor, cx| {
                editor.replace_all(content, window, cx);
                editor.focus(window, cx);
            });
            self.history_diff = None;
            self.save_workspace_journal(window, cx);
            self.status = SharedString::from(format!(
                "已恢复到 {} 的历史版本",
                format_version_time(version.saved_at)
            ));
            cx.notify();
            return;
        }

        let Some(workspace) = &self.workspace else {
            return;
        };
        let root = workspace.reader.root().to_path_buf();
        let reader = workspace.reader.clone();
        let target_format = workspace.entry_format(&version.date).unwrap_or(version.format);
        let expected_revision = workspace
            .journal
            .as_ref()
            .filter(|j| j.date() == version.date)
            .map(|j| j.edit_target().2);
        let existed = workspace.entry_format(&version.date).is_some();

        diff.restoring = true;
        cx.notify();

        let restore = cx.background_executor().spawn(async move {
            let (mut session, _) = if existed {
                let revision = match expected_revision {
                    Some(rev) => rev,
                    None => reader.read(&version.date, target_format)?.revision,
                };
                JournalEdit::begin(&root, &version.date, target_format, &revision)?
            } else {
                JournalEdit::begin_new(&root, &version.date, target_format)?
            };
            let document = session.save(&content)?;
            let versions = reader.list_history(Some(&version.date));
            Ok::<(workreport_core::workspace::Document, Result<Vec<Version>, String>), String>((
                document, versions,
            ))
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = restore.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((document, versions)) => {
                        this.history_diff = None;
                        this.calendar_details.invalidate_previews();
                        this.overview.invalidate();
                        this.directory.invalidate_tree();
                        this.summary.invalidate();
                        if let Ok(parsed) =
                            chrono::NaiveDate::parse_from_str(&document.date, "%Y%m%d")
                        {
                            this.calendar.show_date(parsed);
                            this.workspace_range = None;
                            this.range_single_date = None;
                        }
                        if let Some(workspace) = &mut this.workspace {
                            workspace.accept_saved_journal(document, versions);
                        }
                        this.status = SharedString::from(format!(
                            "已成功恢复到 {} 的历史版本",
                            format_version_time(version.saved_at)
                        ));
                    }
                    Err(error) => {
                        if let Some(diff) = &mut this.history_diff {
                            diff.restoring = false;
                            diff.confirming_restore = false;
                            diff.error = Some(format!("恢复历史版本失败：{error}"));
                        }
                        this.status = format!("恢复历史版本失败：{error}").into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
