use std::{ops::Range, path::PathBuf, sync::Arc};

use gpui_kit::{
    Context, Entity, SharedString,
    base::{Disableable, SelectableText},
    component::{
        ActiveTheme,
        button::Button,
        button::ButtonVariants,
        input::{Input, InputState},
    },
    div,
    prelude::*,
    px, uniform_list,
};
use workreport_core::{
    history::Version,
    workspace::{Document, Entry, Format, WorkspaceInfo},
    workspace_preview::WorkspacePreview,
};

use crate::{Prototype, markdown_preview};
use crate::{
    calendar_view::{CalendarPresentation, render_calendar, render_range},
    workspace_range::WorkspaceRange,
};

pub(super) struct WorkspaceView {
    pub reader: Arc<WorkspacePreview>,
    info: WorkspaceInfo,
    pub(super) entries: Vec<Entry>,
    pub journal: Option<Journal>,
}

pub(super) struct Journal {
    date: String,
    filename: String,
    format: Format,
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
        let mut reader = WorkspacePreview::open(path)?;
        let info = reader.info();
        let entries = reader.list()?;
        let (journal, issue) = match entries.last() {
            Some(entry) => match Journal::read(&reader, &entry.date, entry.format) {
                Ok(journal) => (Some(journal), None),
                Err(issue) => (None, Some(format!("无法读取最新日报：{issue}"))),
            },
            None => (None, None),
        };
        Ok((
            Self {
                reader: Arc::new(reader),
                info,
                entries,
                journal,
            },
            issue,
        ))
    }

    pub fn render(
        &self,
        busy: bool,
        date_input: &Entity<InputState>,
        new_format: Format,
        calendar: CalendarPresentation<'_>,
        range: Option<&WorkspaceRange>,
        range_source: bool,
        cx: &Context<Prototype>,
    ) -> impl IntoElement {
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let dates = uniform_list(
            "workspace-dates",
            self.entries.len(),
            cx.processor(move |this, range: Range<usize>, _, cx| {
                let Some(workspace) = &this.workspace else {
                    return Vec::new();
                };
                workspace
                    .entries
                    .iter()
                    .rev()
                    .skip(range.start)
                    .take(range.len())
                    .map(|entry| {
                        let date = entry.date.clone();
                        let format = entry.format;
                        let selected = workspace
                            .journal
                            .as_ref()
                            .is_some_and(|journal| journal.date == date);
                        let preview = this
                            .calendar_details
                            .previews
                            .values
                            .get(&date)
                            .map(|result| {
                                result
                                    .as_ref()
                                    .map(String::as_str)
                                    .unwrap_or_else(|error| error.as_str())
                            })
                            .unwrap_or("点击查看日报");
                        div().pb_1().child(
                            Button::new(format!("workspace-date-{date}"))
                                .accessibility_label(format!(
                                    "{} · {}",
                                    display_date(&date),
                                    format.extension().to_uppercase()
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .min_w_0()
                                        .w_full()
                                        .gap_1()
                                        .child(format!(
                                            "{} · {}",
                                            display_date(&date),
                                            format.extension().to_uppercase()
                                        ))
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .truncate()
                                                .child(preview.to_owned()),
                                        ),
                                )
                                .when(selected, Button::primary)
                                .disabled(busy)
                                .w_full()
                                .h(px(50.))
                                .p_2()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_journal(date.clone(), format, window, cx);
                                })),
                        )
                    })
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .truncate()
                    .text_color(muted)
                    .child(format!("{} · {}", self.info.name, self.info.path)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_sm().child("日期"))
                    .child(
                        Input::new(date_input)
                            .aria_label("新建日报日期")
                            .disabled(busy)
                            .w(px(155.)),
                    )
                    .child(
                        Button::new("new-journal-txt")
                            .label("TXT")
                            .when(new_format == Format::Txt, Button::primary)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.new_journal_format = Format::Txt;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("new-journal-md")
                            .label("Markdown")
                            .when(new_format == Format::Md, Button::primary)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.new_journal_format = Format::Md;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("new-journal")
                            .label("新建日报")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_new_workspace_journal(window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_h_0()
                            .w(px(264.))
                            .flex_shrink_0()
                            .gap_2()
                            .child(render_calendar(&calendar, &self.entries, busy, cx))
                            .child(div().text_sm().child("日报日期"))
                            .when(self.entries.is_empty(), |pane| {
                                pane.child(div().text_sm().text_color(muted).child("暂无日报"))
                            })
                            .child(dates),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
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
                                        .p_3()
                                        .border_1()
                                        .border_color(border)
                                        .rounded_md()
                                        .text_sm()
                                        .text_color(muted)
                                        .child(format!(
                                            "{} 暂无日报，可选择日期新建。",
                                            calendar.selection.selected_date().format("%Y-%m-%d")
                                        )),
                                )
                            }),
                    )
                    .when(range.is_none(), |pane| {
                        pane.child(
                            div()
                                .flex()
                                .flex_col()
                                .w(px(160.))
                                .flex_shrink_0()
                                .gap_2()
                                .child(div().text_sm().child("历史版本"))
                                .children(
                                    self.journal
                                        .as_ref()
                                        .map(|journal| journal.render_history(busy, cx)),
                                ),
                        )
                    }),
            )
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
    pub fn read(reader: &WorkspacePreview, date: &str, format: Format) -> Result<Self, String> {
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

    pub(super) fn edit_target(&self) -> (String, Format, String) {
        (self.date.clone(), self.format, self.revision.clone())
    }

    pub(super) fn replace_history(&mut self, versions: Result<Vec<Version>, String>) {
        self.versions = versions;
    }

    fn render_content(&self, busy: bool, cx: &Context<Prototype>) -> impl IntoElement {
        let title = match &self.snapshot {
            Some(snapshot) => format!("{} · 历史快照 {}", self.filename, snapshot.version.id),
            None => self.filename.clone(),
        };
        let (format, content) = match &self.snapshot {
            Some(snapshot) => (snapshot.version.format, &snapshot.content),
            None => (self.format, &self.content),
        };
        let element = match format {
            Format::Md => {
                markdown_preview("workspace-markdown", content.clone()).into_any_element()
            }
            Format::Txt => SelectableText::new("workspace-txt", content.clone()).into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_sm().flex_1().min_w_0().truncate().child(title))
                    .when(self.snapshot.is_none(), |row| {
                        row.child(
                            Button::new("edit-journal")
                                .label("编辑日报")
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.start_workspace_edit(window, cx)
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .id("workspace-document")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded_md()
                    .p_3()
                    .child(element),
            )
    }

    fn render_history(&self, busy: bool, cx: &Context<Prototype>) -> impl IntoElement {
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
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("暂无版本"),
                );
            }
            Err(error) => {
                return list.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(error.clone()),
                );
            }
        };
        list = list.child(
            Button::new("current-document")
                .label("当前日报")
                .when(self.snapshot.is_none(), Button::primary)
                .disabled(busy)
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
        );
        list.children(versions.iter().enumerate().map(|(index, version)| {
            let selected = self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.version.id == version.id);
            let name = format!(
                "版本 {} · {}",
                versions.len() - index,
                version.format.extension().to_uppercase()
            );
            let version = version.clone();
            Button::new(format!("history-{}", version.id))
                .label(name)
                .when(selected, Button::primary)
                .disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_snapshot(version.clone(), window, cx)
                }))
        }))
    }
}

impl Snapshot {
    pub fn read(reader: &WorkspacePreview, version: Version) -> Result<Self, String> {
        let content = reader.read_history(&version.date, &version.id)?.into();
        Ok(Self { version, content })
    }
}

fn display_date(date: &str) -> String {
    format!("{}-{}-{}", &date[..4], &date[4..6], &date[6..])
}
