use std::{path::PathBuf, sync::Arc};

use gpui_kit::{
    Context, SharedString,
    base::{Button as FlatButton, SelectableText},
    component::ActiveTheme,
    div,
    prelude::*,
    px,
};
use workreport_core::{
    ai::Settings as AiSettings,
    appearance::Settings as AppearanceSettings,
    history::Version,
    summary::Settings as SummarySettings,
    ui::Preferences as CoreUiPreferences,
    workspace::{Document, Entry, Format, WorkspaceInfo},
    workspace_preview::WorkspaceSession,
};

use crate::{Prototype, markdown_preview};
use crate::{
    calendar::CalendarState,
    calendar_view::{CalendarPresentation, render_calendar, render_range},
    workspace_range::WorkspaceRange,
};

pub(super) struct WorkspaceView {
    pub reader: Arc<WorkspaceSession>,
    info: WorkspaceInfo,
    pub(super) entries: Vec<Entry>,
    pub journal: Option<Journal>,
    pub(super) loaded_ui: Option<CoreUiPreferences>,
    pub(super) loaded_appearance: AppearanceSettings,
    pub(super) loaded_summary: SummarySettings,
    pub(super) loaded_ai: Option<AiSettings>,
}

pub(super) struct Journal {
    date: String,
    pub(super) filename: String,
    pub(super) format: Format,
    revision: String,
    content: SharedString,
    pub(super) backup_warning: Option<String>,
    versions: Result<Vec<Version>, String>,
    pub snapshot: Option<Snapshot>,
}

pub(super) struct Snapshot {
    version: Version,
    content: SharedString,
}

impl WorkspaceView {
    pub fn open(path: PathBuf) -> Result<(Self, Option<String>), String> {
        let reader = WorkspaceSession::open(path)?;
        let info = reader.info();
        let loaded_ui = reader.ui_preferences()?;
        let loaded_appearance = reader.appearance_settings()?;
        let loaded_summary = reader.summary_settings()?;
        let loaded_ai = reader.ai_settings()?;
        let (entries, list_issue) = match reader.list() {
            Ok(entries) => (entries, None),
            Err(issue) => (Vec::new(), Some(issue)),
        };
        let (journal, read_issue) = match entries.last() {
            Some(entry) => match Journal::read(&reader, &entry.date, entry.format) {
                Ok(journal) => (Some(journal), None),
                Err(issue) => (None, Some(format!("无法读取最新日报：{issue}"))),
            },
            None => (None, None),
        };
        let issue = list_issue.or(read_issue);
        Ok((
            Self {
                reader: Arc::new(reader),
                info,
                entries,
                journal,
                loaded_ui,
                loaded_appearance,
                loaded_summary,
                loaded_ai,
            },
            issue,
        ))
    }

    pub(crate) fn info_name(&self) -> &str {
        &self.info.name
    }

    pub(crate) fn info_path(&self) -> String {
        self.info.path.clone()
    }

    pub(crate) fn current_date(&self) -> Option<&str> {
        self.journal.as_ref().map(|journal| journal.date.as_str())
    }

    pub(crate) fn reload_after_external_change(
        &mut self,
        target: Option<(&str, Format)>,
    ) -> Result<(), String> {
        self.entries = self.reader.list()?;
        if let Some((date, format)) = target {
            if let Ok(journal) = Journal::read(&self.reader, date, format) {
                self.journal = Some(journal);
                return Ok(());
            }
        }
        if let Some(current) = &self.journal {
            let date = current.date.clone();
            let format = current.format;
            if let Ok(journal) = Journal::read(&self.reader, &date, format) {
                self.journal = Some(journal);
            }
        }
        Ok(())
    }

    #[inline(never)]
    pub(super) fn render_calendar_module(
        &self,
        busy: bool,
        calendar: CalendarPresentation<'_>,
        _side_by_side: bool,
        cx: &Context<Prototype>,
    ) -> gpui_kit::AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(render_calendar(&calendar, &self.entries, busy, cx))
            .into_any_element()
    }

    pub(super) fn render_editor_module(
        &self,
        busy: bool,
        calendar: &CalendarState,
        range: Option<&WorkspaceRange>,
        range_source: bool,
        cx: &Context<Prototype>,
    ) -> impl IntoElement {
        use crate::icons::{PhIcon, ph_icon};
        let palette = crate::theme_skin::active_palette(cx);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(palette.surface)
            .when_some(range, |pane, range| {
                pane.child(render_range(range, range_source, cx))
            })
            .children(
                self.journal
                    .as_ref()
                    .filter(|_| range.is_none())
                    .map(|journal| journal.render_content(busy, cx)),
            )
            .when(self.journal.is_none() && range.is_none(), |pane| {
                pane.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .p_6()
                        .child(ph_icon(PhIcon::FileText, px(36.), palette.text_muted.opacity(0.6)))
                        .child(
                            div()
                                .text_size(px(14.))
                                .text_color(palette.text)
                                .child(format!(
                                    "{} 暂无日报",
                                    calendar.selected_date().format("%Y-%m-%d")
                                )),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(palette.text_muted)
                                .child("在日历中点击任意日期即可新建或打开日报。"),
                        ),
                )
            })
    }

    pub(super) fn render_history_module(
        &self,
        busy: bool,
        cx: &Context<Prototype>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_2()
            .when_some(self.journal.as_ref(), |pane, journal| {
                pane.child(journal.render_history(busy, cx))
            })
            .when(self.journal.is_none(), |pane| {
                pane.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("请先在日历中选择一篇已有日报查看历史版本。"),
                )
            })
    }

    pub(super) fn entry_format(&self, date: &str) -> Option<Format> {
        self.entries
            .iter()
            .find(|entry| entry.date == date)
            .map(|entry| entry.format)
    }

    pub(super) fn accept_saved_journal(
        &mut self,
        document: Document,
        versions: Result<Vec<Version>, String>,
    ) {
        if !self.entries.iter().any(|entry| entry.date == document.date) {
            self.entries.push(Entry {
                date: document.date.clone(),
                format: document.format,
                filename: document.filename.clone(),
            });
            self.entries.sort_by(|a, b| a.date.cmp(&b.date));
        }
        self.journal = Some(Journal::from_document(document, versions));
    }
}

impl Journal {
    pub fn read(reader: &WorkspaceSession, date: &str, format: Format) -> Result<Self, String> {
        let document = reader.read(date, format)?;
        Ok(Self::from_document(
            document,
            reader.list_history(Some(date)),
        ))
    }

    pub(super) fn from_document(
        document: Document,
        versions: Result<Vec<Version>, String>,
    ) -> Self {
        Self {
            date: document.date,
            filename: document.filename,
            format: document.format,
            revision: document.revision,
            content: document.content.into(),
            backup_warning: document.backup_warning,
            versions,
            snapshot: None,
        }
    }

    pub(super) fn date(&self) -> &str {
        &self.date
    }

    pub(super) fn content(&self) -> &str {
        &self.content
    }

    pub(super) fn versions(&self) -> &Result<Vec<Version>, String> {
        &self.versions
    }

    pub(super) fn edit_target(&self) -> (String, Format, String) {
        (self.date.clone(), self.format, self.revision.clone())
    }

    pub(super) fn replace_history(&mut self, versions: Result<Vec<Version>, String>) {
        self.versions = versions;
    }

    fn render_content(&self, busy: bool, cx: &Context<Prototype>) -> impl IntoElement {
        use crate::icons::{PhIcon, ph_icon};

        let palette = crate::theme_skin::active_palette(cx);
        let title = match &self.snapshot {
            Some(snapshot) => format!("{} · 历史快照 {}", self.filename, snapshot.version.id),
            None => self.filename.clone(),
        };
        let (format, content) = match &self.snapshot {
            Some(snapshot) => (snapshot.version.format, &snapshot.content),
            None => (self.format, &self.content),
        };
        let char_count = content.chars().filter(|ch| !ch.is_whitespace()).count();
        let line_count = if content.is_empty() {
            0
        } else {
            content.split('\n').count()
        };
        let element = match format {
            Format::Md => {
                markdown_preview("workspace-markdown", content.clone()).into_any_element()
            }
            Format::Txt => SelectableText::new("workspace-txt", content.clone()).into_any_element(),
        };
        let preview_tools = [
            PhIcon::TextH,
            PhIcon::TextB,
            PhIcon::TextItalic,
            PhIcon::TextStrikethrough,
            PhIcon::ListBullets,
            PhIcon::ListNumbers,
            PhIcon::CheckSquare,
            PhIcon::Quotes,
            PhIcon::Code,
            PhIcon::Table,
            PhIcon::Minus,
        ];
        let can_edit = self.snapshot.is_none();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(palette.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .h(px(34.))
                    .px_3()
                    .bg(palette.surface_header)
                    .border_b_1()
                    .border_color(palette.border)
                    .child(if format == Format::Md {
                        div()
                            .flex()
                            .items_center()
                            .gap_0p5()
                            .children(preview_tools.into_iter().enumerate().map(|(idx, icon)| {
                                FlatButton::new(format!("browse-fmt-tool-{idx}"))
                                    .disabled(busy || !can_edit)
                                    .w(px(26.))
                                    .h(px(26.))
                                    .rounded(px(6.))
                                    .hover(move |s| s.bg(palette.tab_active()))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(icon, px(16.), palette.text_muted))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.start_workspace_edit(window, cx);
                                    }))
                            }))
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(12.5))
                            .text_color(palette.text_muted)
                            .child("纯文本编辑")
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .bg(palette.surface)
                    .child(
                        div()
                            .id("workspace-document")
                            .size_full()
                            .overflow_y_scroll()
                            .px_5()
                            .py_4()
                            .when(can_edit && !busy, |doc| {
                                doc.on_click(cx.listener(|this, _, window, cx| {
                                    this.start_workspace_edit(window, cx);
                                }))
                            })
                            .child(element),
                    )
                    .child(
                        div()
                            .occlude()
                            .absolute()
                            .top(px(8.))
                            .right(px(14.))
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .when(self.snapshot.is_none(), |row| {
                                row.child(
                                    FlatButton::new("edit-journal")
                                        .accessibility_label("编辑日报")
                                        .disabled(busy)
                                        .h(px(26.))
                                        .px_2p5()
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(palette.accent_border)
                                        .bg(palette.accent_soft)
                                        .hover(move |s| s.border_color(palette.accent))
                                        .text_size(px(12.))
                                        .text_color(palette.accent)
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(ph_icon(PhIcon::NotePencil, px(13.), palette.accent))
                                        .child("编辑日报")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.start_workspace_edit(window, cx)
                                        })),
                                )
                            })
                            .when(self.snapshot.is_some(), |row| {
                                row.child(
                                    FlatButton::new("return-current-journal")
                                        .accessibility_label("返回当前日报")
                                        .disabled(busy)
                                        .h(px(26.))
                                        .px_2p5()
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(palette.border)
                                        .bg(palette.surface)
                                        .text_size(px(12.))
                                        .text_color(palette.text_body)
                                        .flex()
                                        .items_center()
                                        .child("返回当前日报")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(journal) = this
                                                .workspace
                                                .as_mut()
                                                .and_then(|workspace| workspace.journal.as_mut())
                                            {
                                                journal.snapshot = None;
                                                this.status = "已返回当前日报".into();
                                                cx.notify();
                                            }
                                        })),
                                )
                            })
                            .child(
                                div()
                                    .h(px(26.))
                                    .px_2()
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(palette.border)
                                    .bg(palette.surface)
                                    .text_size(px(11.))
                                    .text_color(palette.text_muted)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(format.extension().to_uppercase()),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .bottom(px(8.))
                            .right(px(16.))
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_size(px(11.5))
                            .text_color(palette.text_muted.opacity(0.6))
                            .child(ph_icon(
                                PhIcon::FileText,
                                px(13.),
                                palette.text_muted.opacity(0.6),
                            ))
                            .child(title),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(25.))
                    .px_3()
                    .bg(palette.surface_header)
                    .border_t_1()
                    .border_color(palette.border)
                    .text_size(px(11.5))
                    .text_color(palette.text_muted)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(ph_icon(PhIcon::Check, px(13.), cx.theme().green))
                            .child(if self.snapshot.is_some() {
                                "历史版本只读预览"
                            } else {
                                "已保存"
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2p5()
                            .child(format!("{char_count} 字 · {line_count} 行"))
                            .child(
                                FlatButton::new("browse-footer-shortcuts")
                                    .h(px(20.))
                                    .px_1p5()
                                    .rounded(px(4.))
                                    .hover(move |s| s.bg(palette.tab_active()))
                                    .text_size(px(11.5))
                                    .text_color(palette.text_muted)
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(ph_icon(PhIcon::Keyboard, px(13.), palette.text_muted))
                                    .child("快捷键")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.shortcuts_dialog_open = !this.shortcuts_dialog_open;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    fn render_history(&self, busy: bool, cx: &Context<Prototype>) -> impl IntoElement {
        use crate::icons::{PhIcon, ph_icon};

        let palette = crate::theme_skin::active_palette(cx);
        let date_str = if self.date.len() == 8 {
            format!("{}-{}-{}", &self.date[..4], &self.date[4..6], &self.date[6..])
        } else {
            self.date.clone()
        };
        let mut list = div()
            .id("workspace-history")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        let versions = match &self.versions {
            Ok(versions) if !versions.is_empty() => versions,
            Ok(_) => {
                return list.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(palette.text_muted)
                        .child("暂无版本"),
                );
            }
            Err(error) => {
                return list.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(palette.text_muted)
                        .child(error.clone()),
                );
            }
        };
        list = list.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(28.))
                .px_1p5()
                .rounded(px(4.))
                .bg(palette.surface_header)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(palette.text)
                                .child(date_str),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.text_muted)
                                .child(format!("{} 个快照", versions.len())),
                        ),
                )
                .child(
                    FlatButton::new("current-document")
                        .selected(self.snapshot.is_none())
                        .disabled(busy)
                        .h(px(22.))
                        .px_2()
                        .rounded(px(4.))
                        .bg(if self.snapshot.is_none() {
                            palette.tab_active()
                        } else {
                            palette.surface
                        })
                        .text_size(px(11.))
                        .text_color(if self.snapshot.is_none() {
                            palette.accent
                        } else {
                            palette.text_muted
                        })
                        .flex()
                        .items_center()
                        .child("当前日报")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(journal) = this
                                .workspace
                                .as_mut()
                                .and_then(|workspace| workspace.journal.as_mut())
                            {
                                journal.snapshot = None;
                                this.status = "已返回当前日报".into();
                                cx.notify();
                            }
                        })),
                ),
        );
        list.children(versions.iter().enumerate().map(|(index, version)| {
            let selected = self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.version.id == version.id);
            let time_label = crate::history_diff::format_version_time(version.saved_at);
            let meta_label = format!(
                "{} · {}",
                crate::history_diff::format_bytes(version.bytes),
                version.format.extension().to_uppercase(),
            );
            let preview_version = version.clone();
            let diff_version = version.clone();
            let copy_version = version.clone();
            div()
                .flex()
                .items_center()
                .gap_1()
                .h(px(28.))
                .px_1()
                .rounded(px(4.))
                .hover(move |s| s.bg(palette.subtle))
                .child(
                    FlatButton::new(format!("history-{}", version.id))
                        .selected(selected)
                        .disabled(busy)
                        .flex_1()
                        .min_w_0()
                        .h(px(26.))
                        .px_1p5()
                        .rounded(px(4.))
                        .bg(if selected {
                            palette.tab_active()
                        } else {
                            palette.surface_header
                        })
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(if selected {
                                    palette.accent
                                } else {
                                    palette.text_body
                                })
                                .truncate()
                                .child(time_label),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded(px(3.))
                                .bg(palette.subtle)
                                .text_size(px(10.))
                                .text_color(palette.text_muted)
                                .child(meta_label),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_snapshot(preview_version.clone(), window, cx)
                        })),
                )
                .child(
                    FlatButton::new(format!("history-diff-{}", version.id))
                        .accessibility_label(format!("对比版本 {} 与当前差异", versions.len() - index))
                        .disabled(busy)
                        .h(px(24.))
                        .px_1p5()
                        .rounded(px(4.))
                        .hover(move |s| s.bg(palette.tab_active()))
                        .text_size(px(11.))
                        .text_color(palette.text_muted)
                        .flex()
                        .items_center()
                        .child("对比")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_history_diff(diff_version.clone(), window, cx)
                        })),
                )
                .child(
                    FlatButton::new(format!("history-copy-{}", version.id))
                        .accessibility_label(format!("复制版本 {}", versions.len() - index))
                        .disabled(busy)
                        .w(px(24.))
                        .h(px(24.))
                        .rounded(px(4.))
                        .hover(move |s| s.bg(palette.tab_active()))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ph_icon(PhIcon::Copy, px(13.), palette.text_muted))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.copy_history_version(copy_version.clone(), window, cx)
                        })),
                )
        }))
    }
}

impl Snapshot {
    pub fn read(reader: &WorkspaceSession, version: Version) -> Result<Self, String> {
        let content = reader.read_history(&version.date, &version.id)?.into();
        Ok(Self { version, content })
    }
}
