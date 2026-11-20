use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use gpui_kit::{
    ClipboardItem, Context, Entity, EntityInputHandler, PromptButton, PromptLevel, SharedString,
    Subscription, Task, Window,
    base::Disableable,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        input::{Editor, EditorState, InputEvent},
    },
    div,
    prelude::*,
};
use workreport_core::{
    history::Version,
    journal_edit::JournalEdit,
    workspace::{Document, Format},
};

use crate::{Prototype, journal_draft::JournalDraft, markdown_preview};

const AUTOSAVE_DELAY: Duration = Duration::from_millis(650);

enum Autosave {
    Idle,
    Waiting { _timer: Task<()> },
    Ready(SharedString),
    Paused,
}

enum SavedJournal {
    Body(Document),
    Checkpoint(bool),
}

pub(super) struct WorkspaceEdit {
    target: Arc<Mutex<JournalEdit>>,
    pub(super) editor: Entity<EditorState>,
    draft: JournalDraft,
    date: String,
    filename: String,
    format: Format,
    is_new: bool,
    autosave: Autosave,
    history_pending: bool,
    closing_window: Option<bool>,
    _editor_subscription: Subscription,
}

impl WorkspaceEdit {
    fn new(
        target: JournalEdit,
        document: Document,
        window: &mut Window,
        cx: &mut Context<Prototype>,
    ) -> Self {
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
                .default_value(document.content)
        });
        let baseline = editor.read(cx).value().to_string();
        let editor_subscription = cx.subscribe_in(
            &editor,
            window,
            |this, editor, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.schedule_workspace_autosave(editor, window, cx);
                    cx.notify();
                }
            },
        );
        editor.update(cx, |editor, cx| editor.focus(window, cx));
        Self {
            target: Arc::new(Mutex::new(target)),
            editor,
            draft: JournalDraft::new(baseline),
            date: document.date,
            filename: document.filename,
            format: document.format,
            is_new: document.revision.is_empty(),
            autosave: Autosave::Idle,
            history_pending: false,
            closing_window: None,
            _editor_subscription: editor_subscription,
        }
    }

    pub(super) fn render(&self, switching: bool, cx: &Context<Prototype>) -> impl IntoElement {
        let source = self.editor.read(cx).value();
        let saving = self.draft.is_saving();
        let dirty = self.draft.is_dirty(&source);
        let closing = self.closing_window.is_some();
        let theme = cx.theme();
        let border = theme.border;
        let muted = theme.muted_foreground;
        let label = match self.format {
            Format::Md => "Markdown 源码",
            Format::Txt => "TXT 正文",
        };
        let editor = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_2()
            .child(div().text_sm().child(label))
            .child(
                Editor::new(&self.editor)
                    .aria_label(label)
                    .disabled(closing || switching)
                    .flex_1()
                    .min_h_0()
                    .w_full(),
            );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(self.filename.clone()),
                    )
                    .when(dirty, |bar| {
                        bar.child(div().text_sm().text_color(muted).child("有未保存修改"))
                    })
                    .child(
                        Button::new("copy-journal-draft")
                            .label("复制草稿")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(edit) = &this.workspace_edit {
                                    let source = edit.editor.read(cx).value().to_string();
                                    cx.write_to_clipboard(ClipboardItem::new_string(source));
                                }
                            })),
                    )
                    .child(
                        Button::new("close-journal-editor")
                            .label("返回浏览")
                            .disabled(closing || switching)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.allow_workspace_close(false, window, cx);
                            })),
                    )
                    .child(
                        Button::new("save-journal")
                            .label(if saving {
                                "正在保存…"
                            } else {
                                "保存日报"
                            })
                            .primary()
                            .disabled(
                                switching || closing || saving || (!dirty && !self.history_pending),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_workspace_journal(window, cx)
                            })),
                    ),
            )
            .child(div().flex().flex_1().min_h_0().gap_3().child(editor).when(
                self.format == Format::Md,
                |panes| {
                    panes.child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(div().text_sm().child("预览"))
                            .child(
                                div()
                                    .id("journal-edit-preview")
                                    .flex_1()
                                    .min_h_0()
                                    .border_1()
                                    .border_color(border)
                                    .rounded_md()
                                    .p_3()
                                    .overflow_y_scroll()
                                    .child(markdown_preview("journal-edit-markdown", source)),
                            ),
                    )
                },
            ))
    }
}

impl Prototype {
    pub(super) fn start_workspace_edit(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.workspace_busy || self.workspace_edit.is_some() {
            return;
        }
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(journal) = &workspace.journal else {
            return;
        };
        if journal.snapshot.is_some() {
            return;
        }
        let (date, format, revision) = journal.edit_target();
        self.open_calendar_journal_editor(
            date,
            format,
            Some(revision),
            self.calendar.clone(),
            window,
            cx,
        );
    }

    pub(super) fn start_new_workspace_journal(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.workspace_busy || self.workspace_edit.is_some() {
            return;
        }
        let value = self.new_journal_date.read(cx).value();
        let pattern = if value.len() == 8 {
            "%Y%m%d"
        } else {
            "%Y-%m-%d"
        };
        let date = match chrono::NaiveDate::parse_from_str(&value, pattern) {
            Ok(date) => date.format("%Y%m%d").to_string(),
            Err(_) => {
                self.status = "请输入有效日期，例如 2026-11-20。".into();
                cx.notify();
                return;
            }
        };
        let Some(workspace) = &self.workspace else {
            return;
        };
        if let Some(format) = workspace.entry_format(&date) {
            self.open_journal(date, format, window, cx);
            return;
        }
        self.open_workspace_journal_editor(date, self.new_journal_format, None, window, cx);
    }

    pub(super) fn open_workspace_journal_editor(
        &mut self,
        date: String,
        format: Format,
        revision: Option<String>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let mut calendar = self.calendar.clone();
        if let Ok(date) = chrono::NaiveDate::parse_from_str(&date, "%Y%m%d") {
            calendar.show_date(date);
        }
        self.open_calendar_journal_editor(date, format, revision, calendar, window, cx);
    }

    pub(super) fn open_calendar_journal_editor(
        &mut self,
        date: String,
        format: Format,
        revision: Option<String>,
        calendar: crate::calendar::CalendarState,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let root = workspace.reader.root().to_path_buf();
        self.workspace_busy = true;
        self.status = "正在打开日报编辑器…".into();
        cx.notify();
        let open = cx.background_executor().spawn(async move {
            match revision {
                Some(revision) => JournalEdit::begin(root, &date, format, &revision),
                None => JournalEdit::begin_new(root, &date, format),
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = open.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.workspace_busy = false;
                this.status = match result {
                    Ok((target, document)) => {
                        this.calendar = calendar;
                        this.reveal_calendar_year_month();
                        this.workspace_range = None;
                        this.range_single_date = None;
                        if document.revision.is_empty() {
                            if let Some(workspace) = &mut this.workspace {
                                workspace.journal = None;
                            }
                        }
                        this.workspace_edit =
                            Some(WorkspaceEdit::new(target, document, window, cx));
                        "停笔后自动保存，也可手动保存。".into()
                    }
                    Err(error) => format!("无法编辑日报：{error}").into(),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn schedule_workspace_autosave(
        &mut self,
        editor: &Entity<EditorState>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        if edit.editor != *editor
            || edit.closing_window.is_some()
            || matches!(edit.autosave, Autosave::Paused)
        {
            return;
        }
        // The editor emits Change for committed text. Capture it now so a
        // later IME preedit cannot be included when the debounce expires.
        let source = editor.read(cx).value();
        let editor = editor.downgrade();
        let timer = cx.background_executor().timer(AUTOSAVE_DELAY);
        let task = cx.spawn_in(window, async move |this, cx| {
            timer.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(edit) = &mut this.workspace_edit else {
                    return;
                };
                if editor.upgrade().as_ref() == Some(&edit.editor)
                    && matches!(edit.autosave, Autosave::Waiting { .. })
                {
                    edit.autosave = Autosave::Ready(source);
                    this.save_journal(true, window, cx);
                }
            });
        });
        edit.autosave = Autosave::Waiting { _timer: task };
    }

    pub(super) fn save_workspace_journal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_journal(false, window, cx);
    }

    fn save_journal(&mut self, automatic: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_busy {
            return;
        }
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        if edit.draft.is_saving() {
            return;
        }
        let source = if automatic {
            let Autosave::Ready(source) = &edit.autosave else {
                return;
            };
            source.clone()
        } else {
            if edit.editor.update(cx, |editor, cx| {
                editor.marked_text_range(window, cx).is_some()
            }) {
                self.status = "请先确认输入法候选内容，再保存。".into();
                cx.notify();
                return;
            }
            edit.editor.read(cx).value()
        };
        if edit.is_new && source.trim().is_empty() {
            edit.autosave = Autosave::Idle;
            return;
        }
        let source = edit.draft.begin_save(&source);
        if source.is_none() {
            if automatic {
                edit.autosave = Autosave::Idle;
                return;
            }
            if !edit.history_pending {
                return;
            }
            edit.draft.begin_checkpoint();
        }
        edit.autosave = Autosave::Idle;
        let target = edit.target.clone();
        let date = edit.date.clone();
        let reader = workspace.reader.clone();
        self.status = if automatic {
            "正在自动保存…"
        } else {
            "正在保存日报…"
        }
        .into();
        cx.notify();
        let save = cx.background_executor().spawn(async move {
            let mut target = target.lock().map_err(|_| "日报保存任务不可用")?;
            let saved = match source {
                Some(source) => SavedJournal::Body(if automatic {
                    target.save_automatic(&source)?
                } else {
                    target.save(&source)?
                }),
                None => SavedJournal::Checkpoint(target.checkpoint_history().is_ok()),
            };
            drop(target);
            Ok((saved, reader.list_history(Some(&date))))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = save.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_journal_save(result, automatic, window, cx);
            });
        })
        .detach();
    }

    fn finish_journal_save(
        &mut self,
        result: Result<(SavedJournal, Result<Vec<Version>, String>), String>,
        automatic: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        let closing_window = edit.closing_window.take();
        self.status = match result {
            Ok((SavedJournal::Body(document), versions)) => {
                self.calendar_details.invalidate_previews();
                edit.is_new = false;
                edit.draft.finish_save(Some(document.content.clone()));
                edit.history_pending = automatic;
                let warning = document.backup_warning.clone();
                if let Some(workspace) = &mut self.workspace {
                    workspace.accept_saved_journal(document, versions);
                }
                match warning {
                    Some(warning) => warning.into(),
                    None if edit.draft.is_dirty(&edit.editor.read(cx).value()) => {
                        "已保存提交的内容，后续修改仍在草稿中。".into()
                    }
                    None if automatic => "日报已自动保存".into(),
                    None => "日报已保存".into(),
                }
            }
            Ok((SavedJournal::Checkpoint(saved), versions)) => {
                edit.draft.finish_save(None);
                // A history failure must not trap an already-saved journal in the editor.
                edit.history_pending = !saved && closing_window.is_none();
                if let Some(journal) = self
                    .workspace
                    .as_mut()
                    .and_then(|workspace| workspace.journal.as_mut())
                {
                    journal.replace_history(versions);
                }
                if saved {
                    "日报已保存"
                } else {
                    "正文已保存，未能补记历史版本。"
                }
                .into()
            }
            Err(error) => {
                edit.draft.finish_save(None);
                edit.autosave = Autosave::Paused;
                format!("保存失败，草稿已保留。{error}").into()
            }
        };
        let autosave_ready = matches!(edit.autosave, Autosave::Ready(_));
        cx.notify();
        if let Some(closing_window) = closing_window {
            if self.allow_workspace_close(closing_window, window, cx) && closing_window {
                window.remove_window();
            }
        } else if autosave_ready {
            self.save_journal(true, window, cx);
        }
    }

    pub(super) fn allow_workspace_close(
        &mut self,
        closing_window: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = &mut self.workspace_edit else {
            return true;
        };
        if self.workspace_busy || edit.closing_window.is_some() {
            return false;
        }
        let source = edit.editor.read(cx).value();
        let dirty = edit.draft.is_dirty(&source) && !(edit.is_new && source.trim().is_empty());
        if !edit.draft.is_saving() && !dirty && !edit.history_pending {
            self.finish_workspace_edit(window, cx);
            return true;
        }
        if edit.editor.update(cx, |editor, cx| {
            editor.marked_text_range(window, cx).is_some()
        }) {
            self.status = "请先确认输入法候选内容，再离开编辑器。".into();
            cx.notify();
            return false;
        }
        if !dirty || !matches!(edit.autosave, Autosave::Paused) {
            edit.closing_window = Some(closing_window);
            edit.autosave = Autosave::Idle;
            self.save_journal(false, window, cx);
            cx.notify();
            return false;
        }

        self.workspace_busy = true;
        cx.notify();
        let choice = window.prompt(
            PromptLevel::Warning,
            "放弃未保存的日报修改？",
            Some("可继续编辑、保存或复制草稿。"),
            &[
                PromptButton::Cancel("继续编辑".into()),
                PromptButton::Other("放弃修改".into()),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let discard = matches!(choice.await, Ok(1));
            let _ = this.update_in(cx, |this, window, cx| {
                this.workspace_busy = false;
                if discard {
                    this.finish_workspace_edit(window, cx);
                    if closing_window {
                        window.remove_window();
                    }
                } else {
                    cx.notify();
                }
            });
        })
        .detach();
        false
    }

    fn finish_workspace_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace_edit = None;
        self.focus_handle.focus(window, cx);
        let warning = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.journal.as_ref())
            .and_then(|journal| journal.backup_warning.as_deref());
        self.status = SharedString::from(warning.unwrap_or("工作空间只读浏览"));
        cx.notify();
    }
}
