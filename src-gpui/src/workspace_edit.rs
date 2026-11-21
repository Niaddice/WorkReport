use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use gpui_kit::{
    Animation, AnimationExt, ClipboardItem, Context, ElementId, Entity, EntityInputHandler,
    FontWeight, PromptButton, PromptLevel, SharedString, Subscription, Task, Window,
    base::Button as FlatButton,
    component::{
        ActiveTheme,
        input::{Editor, EditorState, InputEvent},
    },
    div, ease_out_quint, pulsating_between,
    prelude::*,
    px,
};
use workreport_core::{
    history::Version,
    journal_edit::JournalEdit,
    workspace::{Document, Format},
};

use crate::{
    Prototype,
    journal_draft::JournalDraft,
    markdown_preview,
    rich_editor::{JournalEditorMode, RichEditorState, render_rich_editor_body, render_rich_toolbar},
};

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

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorPopover {
    PreviousJournal,
    Templates,
    PolishMenu,
    GitDraft,
    HeadingMenu,
}

pub(super) struct WorkspaceEdit {
    target: Arc<Mutex<JournalEdit>>,
    pub(super) editor: Entity<EditorState>,
    pub(super) rich: Box<RichEditorState>,
    pub(super) split_preview: bool,
    pub(super) active_popover: Option<EditorPopover>,
    pub(super) git_draft_replace: bool,
    pub(super) prev_preview: Option<(String, String)>,
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
        let rich = Box::new(RichEditorState::new(&document.content, window, cx));
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
                .default_value(document.content)
        });
        let baseline = if document.revision.is_empty() {
            String::new()
        } else {
            editor.read(cx).value().to_string()
        };
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
            rich,
            split_preview: false,
            active_popover: None,
            git_draft_replace: false,
            prev_preview: None,
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

    #[allow(dead_code)]
    pub(crate) fn toggle_popover(&mut self, popover: EditorPopover) {
        if self.active_popover == Some(popover) {
            self.active_popover = None;
        } else {
            self.active_popover = Some(popover);
        }
    }

    pub(super) fn filename(&self) -> &str {
        &self.filename
    }

    pub(super) fn date(&self) -> &str {
        &self.date
    }

    pub(super) fn format(&self) -> Format {
        self.format
    }

    #[inline(never)]
    pub(super) fn render(
        &self,
        switching: bool,
        polishing: bool,
        cx: &Context<Prototype>,
    ) -> gpui_kit::AnyElement {
        use crate::icons::{PhIcon, ph_icon};

        let source = self.editor.read(cx).value();
        let saving = self.draft.is_saving();
        let dirty = self.draft.is_dirty(&source);
        let closing = self.closing_window.is_some();
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let char_count = source.chars().filter(|ch| !ch.is_whitespace()).count();
        let line_count = if source.is_empty() {
            0
        } else {
            source.split('\n').count()
        };
        let save_status_text = if self.is_new && source.trim().is_empty() {
            "编辑后自动保存"
        } else if saving {
            "正在保存…"
        } else if matches!(self.autosave, Autosave::Paused) && dirty {
            "保存失败，草稿已保留"
        } else if dirty {
            "等待保存…"
        } else {
            "已保存到工作空间"
        };

        let label = match self.format {
            Format::Md => "Markdown 源码",
            Format::Txt => "TXT 正文",
        };
        let palette = crate::theme_skin::active_palette(cx);
        let editor = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .px_4()
            .py_3()
            .bg(palette.surface)
            .child(
                Editor::new(&self.editor)
                    .aria_label(label)
                    .appearance(false)
                    .bordered(false)
                    .font_family(cx.theme().font_family.clone())
                    .text_size(px(13.5))
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
            .bg(palette.surface)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_1p5()
                    .min_h(px(34.))
                    .px_3()
                    .py_1()
                    .bg(palette.surface_header)
                    .border_b_1()
                    .border_color(palette.border)
                    .child(if self.format == Format::Md {
                        render_rich_toolbar(&self.rich, closing || switching, cx)
                    } else {
                        div()
                            .text_size(px(12.5))
                            .text_color(palette.text_muted)
                            .child("纯文本编辑")
                            .into_any_element()
                    })
                    .when(cfg!(test), |hdr| {
                        hdr.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .when(self.format == Format::Md, |bar| {
                                    let src_active = self.rich.mode == JournalEditorMode::SourceSplit;
                                    let rich_active = self.rich.mode == JournalEditorMode::RichBlocks;
                                    bar.child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_0p5()
                                            .p_0p5()
                                            .rounded(px(6.))
                                            .child(
                                                FlatButton::new("journal-mode-source")
                                                    .selected(src_active)
                                                    .disabled(closing || switching)
                                                    .h(px(24.))
                                                    .px_2()
                                                    .rounded(px(5.))
                                                    .bg(if src_active {
                                                        palette.tab_active()
                                                    } else {
                                                        palette.surface_header
                                                    })
                                                    .hover(move |s| s.bg(palette.tab_active()))
                                                    .text_size(px(11.5))
                                                    .text_color(if src_active {
                                                        palette.accent
                                                    } else {
                                                        palette.text_body
                                                    })
                                                    .flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(ph_icon(
                                                        PhIcon::Columns,
                                                        px(13.),
                                                        if src_active {
                                                            palette.accent
                                                        } else {
                                                            palette.text_muted
                                                        },
                                                    ))
                                                    .child("源码")
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.switch_journal_editor_mode(
                                                            JournalEditorMode::SourceSplit,
                                                            window,
                                                            cx,
                                                        );
                                                    })),
                                            )
                                            .child(
                                                FlatButton::new("journal-mode-rich")
                                                    .selected(rich_active)
                                                    .disabled(closing || switching)
                                                    .h(px(24.))
                                                    .px_2()
                                                    .rounded(px(5.))
                                                    .bg(if rich_active {
                                                        palette.tab_active()
                                                    } else {
                                                        palette.surface_header
                                                    })
                                                    .hover(move |s| s.bg(palette.tab_active()))
                                                    .text_size(px(11.5))
                                                    .text_color(if rich_active {
                                                        palette.accent
                                                    } else {
                                                        palette.text_body
                                                    })
                                                    .flex()
                                                    .items_center()
                                                    .child("块编辑")
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.switch_journal_editor_mode(
                                                            JournalEditorMode::RichBlocks,
                                                            window,
                                                            cx,
                                                        );
                                                    })),
                                            ),
                                    )
                                })
                                .child(
                                    FlatButton::new("save-journal")
                                        .h(px(24.))
                                        .px_2p5()
                                        .rounded(px(5.))
                                        .border_1()
                                        .border_color(palette.accent_border)
                                        .bg(palette.accent_soft)
                                        .text_size(px(11.5))
                                        .text_color(palette.accent)
                                        .disabled(
                                            switching
                                                || closing
                                                || saving
                                                || (!dirty && !self.history_pending),
                                        )
                                        .flex()
                                        .items_center()
                                        .child(if saving { "正在保存…" } else { "保存日报" })
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.save_workspace_journal(window, cx)
                                        })),
                                )
                                .child(
                                    FlatButton::new("close-journal-editor")
                                        .disabled(closing || switching)
                                        .h(px(24.))
                                        .px_2()
                                        .rounded(px(5.))
                                        .border_1()
                                        .border_color(palette.border)
                                        .bg(palette.surface)
                                        .hover(move |s| s.bg(palette.tab_active()))
                                        .text_size(px(11.5))
                                        .text_color(palette.text_body)
                                        .flex()
                                        .items_center()
                                        .child("返回浏览")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.allow_workspace_close(false, window, cx);
                                        })),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .bg(palette.surface)
                    .child(
                        if self.format == Format::Md
                            && self.rich.mode == JournalEditorMode::RichBlocks
                        {
                            div()
                                .size_full()
                                .p_3()
                                .flex()
                                .flex_col()
                                .child(render_rich_editor_body(
                                    &self.rich,
                                    closing || switching,
                                    cx,
                                ))
                                .into_any_element()
                        } else {
                            div()
                                .flex()
                                .size_full()
                                .min_h_0()
                                .child(editor)
                                .when(self.format == Format::Md && self.split_preview, |panes| {
                                    panes.child(
                                        div()
                                            .id("journal-edit-preview")
                                            .flex_1()
                                            .min_w_0()
                                            .min_h_0()
                                            .border_l_1()
                                            .border_color(palette.border)
                                            .px_4()
                                            .py_3()
                                            .overflow_y_scroll()
                                            .child(markdown_preview(
                                                "journal-edit-markdown",
                                                source,
                                            ))
                                            .with_animation(
                                                ElementId::Name(
                                                    format!("preview-anim-{}", self.date).into(),
                                                ),
                                                Animation::new(Duration::from_millis(160))
                                                    .with_easing(ease_out_quint()),
                                                |el, t| {
                                                    el.opacity(0.25 + 0.75 * t)
                                                        .pl(px(16.0 + (1.0 - t) * 8.0))
                                                },
                                            ),
                                    )
                                })
                                .with_animation(
                                    ElementId::Name(
                                        format!(
                                            "editor-body-anim-{}-{:?}",
                                            self.date, self.format
                                        )
                                        .into(),
                                    ),
                                    Animation::new(Duration::from_millis(150))
                                        .with_easing(ease_out_quint()),
                                    |el, t| el.opacity(0.4 + 0.6 * t),
                                )
                                .into_any_element()
                        },
                    )
                    .child({
                        let prev_active =
                            self.active_popover == Some(EditorPopover::PreviousJournal);
                        let tpl_active = self.active_popover == Some(EditorPopover::Templates);
                        let polish_active =
                            self.active_popover == Some(EditorPopover::PolishMenu);
                        let git_active = self.active_popover == Some(EditorPopover::GitDraft);
                        div()
                            .occlude()
                            .absolute()
                            .top(px(8.))
                            .right(px(14.))
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                FlatButton::new("editor-import-previous")
                                    .accessibility_label("带入上次日报")
                                    .selected(prev_active)
                                    .disabled(closing || switching)
                                    .w(px(26.))
                                    .h(px(26.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(if prev_active {
                                        palette.accent_border
                                    } else {
                                        palette.border
                                    })
                                    .bg(if prev_active {
                                        palette.accent_soft
                                    } else {
                                        palette.surface
                                    })
                                    .hover(move |s| {
                                        s.border_color(palette.accent_border)
                                            .bg(palette.accent_soft)
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(
                                        PhIcon::ArrowLineDown,
                                        px(15.),
                                        if prev_active {
                                            palette.accent
                                        } else {
                                            palette.text_muted
                                        },
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        #[cfg(test)]
                                        this.import_previous_journal(window, cx);
                                        #[cfg(not(test))]
                                        this.toggle_previous_journal_popover(window, cx);
                                    })),
                            )
                            .child(
                                FlatButton::new("editor-insert-template")
                                    .accessibility_label("插入预设模板")
                                    .selected(tpl_active)
                                    .disabled(closing || switching)
                                    .w(px(26.))
                                    .h(px(26.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(if tpl_active {
                                        palette.accent_border
                                    } else {
                                        palette.border
                                    })
                                    .bg(if tpl_active {
                                        palette.accent_soft
                                    } else {
                                        palette.surface
                                    })
                                    .hover(move |s| {
                                        s.border_color(palette.accent_border)
                                            .bg(palette.accent_soft)
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(
                                        PhIcon::Article,
                                        px(15.),
                                        if tpl_active {
                                            palette.accent
                                        } else {
                                            palette.text_muted
                                        },
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        #[cfg(test)]
                                        this.apply_default_template_in_editor(window, cx);
                                        #[cfg(not(test))]
                                        {
                                            let _ = window;
                                            if let Some(edit) = &mut this.workspace_edit {
                                                edit.toggle_popover(EditorPopover::Templates);
                                                cx.notify();
                                            }
                                        }
                                    })),
                            )
                            .child(if polishing {
                                FlatButton::new("ai-polish-cancel")
                                    .h(px(26.))
                                    .px_2()
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(palette.accent)
                                    .bg(palette.accent_soft)
                                    .text_size(px(11.5))
                                    .text_color(palette.accent)
                                    .flex()
                                    .items_center()
                                    .child("取消润色")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.ai.cancel_polish();
                                        this.status = "已取消 AI 润色".into();
                                        cx.notify();
                                    }))
                            } else {
                                FlatButton::new("ai-polish-journal")
                                    .accessibility_label("AI 润色")
                                    .selected(polish_active)
                                    .disabled(closing || switching)
                                    .w(px(26.))
                                    .h(px(26.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(if polish_active {
                                        palette.accent_border
                                    } else {
                                        palette.border
                                    })
                                    .bg(if polish_active {
                                        palette.accent_soft
                                    } else {
                                        palette.surface
                                    })
                                    .hover(move |s| {
                                        s.border_color(palette.accent_border)
                                            .bg(palette.accent_soft)
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(
                                        PhIcon::MagicWand,
                                        px(15.),
                                        if polish_active {
                                            palette.accent
                                        } else {
                                            palette.text_muted
                                        },
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        #[cfg(test)]
                                        this.start_polish_journal(window, cx);
                                        #[cfg(not(test))]
                                        {
                                            let _ = window;
                                            if let Some(edit) = &mut this.workspace_edit {
                                                edit.toggle_popover(EditorPopover::PolishMenu);
                                                cx.notify();
                                            }
                                        }
                                    }))
                            })
                            .child(
                                FlatButton::new("editor-git-draft")
                                    .accessibility_label("从 Git 提交生成日报")
                                    .selected(git_active)
                                    .disabled(closing || switching)
                                    .w(px(26.))
                                    .h(px(26.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(if git_active {
                                        palette.accent_border
                                    } else {
                                        palette.border
                                    })
                                    .bg(if git_active {
                                        palette.accent_soft
                                    } else {
                                        palette.surface
                                    })
                                    .hover(move |s| {
                                        s.border_color(palette.accent_border)
                                            .bg(palette.accent_soft)
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(
                                        PhIcon::Sparkle,
                                        px(15.),
                                        if git_active {
                                            palette.accent
                                        } else {
                                            palette.text_muted
                                        },
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        #[cfg(test)]
                                        this.append_git_daily_log_to_journal(window, cx);
                                        #[cfg(not(test))]
                                        {
                                            let _ = window;
                                            if let Some(edit) = &mut this.workspace_edit {
                                                edit.toggle_popover(EditorPopover::GitDraft);
                                                cx.notify();
                                            }
                                        }
                                    })),
                            )
                            .child(
                                FlatButton::new("editor-toggle-format")
                                    .accessibility_label("切换日报格式 (MD / TXT)")
                                    .disabled(closing || switching)
                                    .h(px(26.))
                                    .px_2()
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(palette.border)
                                    .bg(palette.surface)
                                    .hover(move |s| {
                                        s.border_color(palette.accent_border)
                                            .text_color(palette.accent)
                                    })
                                    .text_size(px(11.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(palette.text_muted)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(self.format.extension().to_uppercase())
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_editor_format(window, cx);
                                    })),
                            )
                            .when(self.format == Format::Md, |tools| {
                                let preview_on = self.split_preview;
                                tools.child(
                                    FlatButton::new("editor-split-preview")
                                        .accessibility_label(if preview_on {
                                            "收起日报预览"
                                        } else {
                                            "左右分栏预览"
                                        })
                                        .selected(preview_on)
                                        .w(px(26.))
                                        .h(px(26.))
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(if preview_on {
                                            palette.accent_border
                                        } else {
                                            palette.border
                                        })
                                        .bg(if preview_on {
                                            palette.accent_soft
                                        } else {
                                            palette.surface
                                        })
                                        .hover(move |s| s.border_color(palette.accent_border))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(ph_icon(
                                            PhIcon::Columns,
                                            px(14.),
                                            if preview_on {
                                                palette.accent
                                            } else {
                                                palette.text_muted
                                            },
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(edit) = &mut this.workspace_edit {
                                                edit.split_preview = !edit.split_preview;
                                                cx.notify();
                                            }
                                        })),
                                )
                            })
                            .child(
                                FlatButton::new("copy-journal-draft")
                                    .accessibility_label("复制草稿")
                                    .w(px(26.))
                                    .h(px(26.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(palette.border)
                                    .bg(palette.surface)
                                    .hover(move |s| s.border_color(palette.accent_border))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(PhIcon::Copy, px(14.), palette.text_muted))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(edit) = &this.workspace_edit {
                                            let source = edit.editor.read(cx).value().to_string();
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                source,
                                            ));
                                            this.status = "已复制当前日报内容".into();
                                            cx.notify();
                                        }
                                    })),
                            )
                    })
                    .when_some(self.render_active_popover(cx), |body, popover| {
                        body.child(popover)
                    })
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
                            .child(self.filename.clone()),
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
                    .text_color(muted)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child({
                                let dot = div()
                                    .w(px(6.))
                                    .h(px(6.))
                                    .rounded(px(3.))
                                    .bg(if matches!(self.autosave, Autosave::Paused) && dirty {
                                        theme.red
                                    } else if dirty || saving {
                                        theme.yellow
                                    } else {
                                        theme.green
                                    });
                                if dirty || saving {
                                    dot.with_animation(
                                        "editor-dirty-pulse",
                                        Animation::new(Duration::from_millis(1200))
                                            .repeat()
                                            .with_easing(pulsating_between(0.35, 1.0)),
                                        |el, opacity| el.opacity(opacity),
                                    )
                                    .into_any_element()
                                } else {
                                    dot.into_any_element()
                                }
                            })
                            .child(save_status_text),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2p5()
                            .child(format!("{char_count} 字 · {line_count} 行"))
                            .child(
                                FlatButton::new("editor-footer-shortcuts")
                                    .h(px(20.))
                                    .px_1p5()
                                    .rounded(px(4.))
                                    .hover(move |s| s.bg(palette.tab_active()))
                                    .text_size(px(11.5))
                                    .text_color(muted)
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(ph_icon(PhIcon::Keyboard, px(13.), muted))
                                    .child("快捷键")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.shortcuts_dialog_open = !this.shortcuts_dialog_open;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    #[inline(never)]
    fn render_active_popover(&self, cx: &Context<Prototype>) -> Option<gpui_kit::AnyElement> {
        use crate::icons::{PhIcon, ph_icon};

        let active = self.active_popover?;
        let palette = crate::theme_skin::active_palette(cx);

        let card = match active {
            EditorPopover::HeadingMenu => {
                let headings: [(u8, &str, &str); 7] = [
                    (0, "正文", "Paragraph"),
                    (1, "一级标题", "H1"),
                    (2, "二级标题", "H2"),
                    (3, "三级标题", "H3"),
                    (4, "四级标题", "H4"),
                    (5, "五级标题", "H5"),
                    (6, "六级标题", "H6"),
                ];
                div()
                    .occlude()
                    .absolute()
                    .top(px(4.))
                    .left(px(56.))
                    .w(px(176.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .p_1p5()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .children(headings.into_iter().map(|(level, title, tag)| {
                        FlatButton::new(format!("popover-heading-{level}"))
                            .w_full()
                            .h(px(28.))
                            .px_2p5()
                            .rounded(px(6.))
                            .hover(move |s| s.bg(palette.tab_active()))
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(12.5))
                            .text_color(palette.text)
                            .child(title)
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(palette.text_muted)
                                    .child(tag),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(edit) = &mut this.workspace_edit {
                                    edit.active_popover = None;
                                }
                                this.mutate_rich_doc(
                                    move |doc, idx| {
                                        if level == 0 {
                                            doc.convert_block_to(idx, "paragraph");
                                        } else {
                                            doc.set_heading(idx, level);
                                        }
                                        idx
                                    },
                                    window,
                                    cx,
                                );
                            }))
                    }))
            }
            EditorPopover::PreviousJournal => div()
                .occlude()
                .absolute()
                .top(px(40.))
                .right(px(14.))
                .w(px(286.))
                .rounded(px(8.))
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
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(palette.text)
                                .child("带入上次日报"),
                        )
                        .child(
                            FlatButton::new("close-prev-popover")
                                .w(px(20.))
                                .h(px(20.))
                                .rounded(px(4.))
                                .hover(move |s| s.bg(palette.tab_active()))
                                .child(ph_icon(PhIcon::X, px(12.), palette.text_muted))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(edit) = &mut this.workspace_edit {
                                        edit.active_popover = None;
                                        cx.notify();
                                    }
                                })),
                        ),
                )
                .child(if let Some((prev_date, excerpt)) = &self.prev_preview {
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(palette.text_body)
                                .child(format!("带入 {prev_date} 的日报内容？")),
                        )
                        .child(
                            div()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.surface_header)
                                .p_2()
                                .text_size(px(11.5))
                                .text_color(palette.text_muted)
                                .child(excerpt.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.text_muted)
                                .child("将带入到当前编辑区，可通过撤销恢复。"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap_1p5()
                                .child(
                                    FlatButton::new("prev-journal-cancel")
                                        .h(px(24.))
                                        .px_2p5()
                                        .rounded(px(5.))
                                        .border_1()
                                        .border_color(palette.border)
                                        .bg(palette.surface)
                                        .hover(move |s| s.bg(palette.tab_active()))
                                        .text_size(px(11.5))
                                        .text_color(palette.text_body)
                                        .child("取消")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(edit) = &mut this.workspace_edit {
                                                edit.active_popover = None;
                                                cx.notify();
                                            }
                                        })),
                                )
                                .child(
                                    FlatButton::new("prev-journal-confirm")
                                        .h(px(24.))
                                        .px_2p5()
                                        .rounded(px(5.))
                                        .bg(palette.accent_fill)
                                        .text_size(px(11.5))
                                        .text_color(palette.on_accent)
                                        .child("确认带入")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            if let Some(edit) = &mut this.workspace_edit {
                                                edit.active_popover = None;
                                            }
                                            this.import_previous_journal(window, cx);
                                        })),
                                ),
                        )
                        .into_any_element()
                } else {
                    div()
                        .py_2()
                        .text_size(px(12.))
                        .text_color(palette.text_muted)
                        .child("未找到更早日期的日报记录")
                        .into_any_element()
                }),
            EditorPopover::Templates => {
                let presets: [(&str, &str, &str); 4] = [
                    ("standard", "标准工作日报", "今日完成 / 进行中 / 明日计划"),
                    ("agile", "敏捷站会日报", "昨日推进 / 今日目标 / 阻塞项"),
                    ("project", "项目推进日报", "关键里程碑 / 风险与依赖 / 下一步"),
                    ("minimal", "极简清单日报", "已完成事项 / 待办清单"),
                ];
                div()
                    .occlude()
                    .absolute()
                    .top(px(40.))
                    .right(px(14.))
                    .w(px(306.))
                    .rounded(px(8.))
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
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(palette.text)
                                    .child("常用预设模板"),
                            )
                            .child(
                                FlatButton::new("tpl-popover-settings")
                                    .h(px(22.))
                                    .px_2()
                                    .rounded(px(5.))
                                    .hover(move |s| s.bg(palette.tab_active()))
                                    .text_size(px(11.5))
                                    .text_color(palette.text_muted)
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(ph_icon(PhIcon::GearSix, px(12.), palette.text_muted))
                                    .child("设置")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if let Some(edit) = &mut this.workspace_edit {
                                            edit.active_popover = None;
                                        }
                                        this.open_settings_section(
                                            crate::settings_view::SettingsSection::Editor,
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    )
                    .children(presets.into_iter().map(|(id, title, desc)| {
                        div()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(palette.border)
                            .bg(palette.surface_header)
                            .p_2()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(palette.text)
                                            .child(title),
                                    )
                                    .when(id == "standard", |row| {
                                        row.child(
                                            div()
                                                .px_1p5()
                                                .py(px(1.))
                                                .rounded(px(4.))
                                                .bg(palette.accent_soft)
                                                .text_size(px(10.))
                                                .text_color(palette.accent)
                                                .child("默认"),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(palette.text_muted)
                                    .child(desc),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .gap_1p5()
                                    .child(
                                        FlatButton::new(format!("tpl-append-{id}"))
                                            .h(px(22.))
                                            .px_2()
                                            .rounded(px(4.))
                                            .border_1()
                                            .border_color(palette.accent_border)
                                            .bg(palette.accent_soft)
                                            .text_size(px(11.))
                                            .text_color(palette.accent)
                                            .child("追加到末尾")
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.apply_preset_template_in_editor(
                                                    id, false, window, cx,
                                                );
                                            })),
                                    )
                                    .child(
                                        FlatButton::new(format!("tpl-replace-{id}"))
                                            .h(px(22.))
                                            .px_2()
                                            .rounded(px(4.))
                                            .border_1()
                                            .border_color(palette.border)
                                            .bg(palette.surface)
                                            .hover(move |s| s.bg(palette.tab_active()))
                                            .text_size(px(11.))
                                            .text_color(palette.text_body)
                                            .child("替换全文")
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.apply_preset_template_in_editor(
                                                    id, true, window, cx,
                                                );
                                            })),
                                    ),
                            )
                    }))
            }
            EditorPopover::PolishMenu => {
                let modes: [(&str, &str, &str); 4] = [
                    ("smart", "智能润色", "优化措辞与专业度，保持原意"),
                    ("concise", "精炼简短", "提炼核心要点，去除冗余表述"),
                    ("detail", "丰富细节", "补充结构层次，让进展更清晰"),
                    ("fix", "纠错规范", "修正错别字、标点与排版格式"),
                ];
                div()
                    .occlude()
                    .absolute()
                    .top(px(40.))
                    .right(px(14.))
                    .w(px(236.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(modes.into_iter().map(|(id, title, desc)| {
                        FlatButton::new(format!("polish-mode-{id}"))
                            .w_full()
                            .px_2p5()
                            .py_1p5()
                            .rounded(px(6.))
                            .hover(move |s| s.bg(palette.tab_active()))
                            .justify_start()
                            .items_stretch()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(ph_icon(PhIcon::MagicWand, px(13.), palette.accent))
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(palette.text)
                                            .child(title),
                                    ),
                            )
                            .child(
                                div()
                                    .pl(px(19.))
                                    .text_size(px(11.))
                                    .text_color(palette.text_muted)
                                    .child(desc),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(edit) = &mut this.workspace_edit {
                                    edit.active_popover = None;
                                }
                                this.start_polish_journal(window, cx);
                            }))
                    }))
            }
            EditorPopover::GitDraft => {
                let replace_mode = self.git_draft_replace;
                div()
                    .occlude()
                    .absolute()
                    .top(px(40.))
                    .right(px(14.))
                    .w(px(280.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2p5()
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
                                    .child(ph_icon(PhIcon::Sparkle, px(14.), palette.accent))
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(palette.text)
                                            .child("从 Git 提交生成日报"),
                                    ),
                            )
                            .child(
                                div()
                                    .px_1p5()
                                    .py(px(1.))
                                    .rounded(px(4.))
                                    .bg(palette.surface_header)
                                    .text_size(px(10.5))
                                    .text_color(palette.text_muted)
                                    .child(self.date.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                FlatButton::new("git-draft-mode-append")
                                    .selected(!replace_mode)
                                    .w_full()
                                    .h(px(28.))
                                    .px_2p5()
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(if !replace_mode {
                                        palette.accent_border
                                    } else {
                                        palette.border
                                    })
                                    .bg(if !replace_mode {
                                        palette.accent_soft
                                    } else {
                                        palette.surface_header
                                    })
                                    .justify_start()
                                    .text_size(px(12.))
                                    .text_color(if !replace_mode {
                                        palette.accent
                                    } else {
                                        palette.text_body
                                    })
                                    .child("追加到文末")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(edit) = &mut this.workspace_edit {
                                            edit.git_draft_replace = false;
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(
                                FlatButton::new("git-draft-mode-replace")
                                    .selected(replace_mode)
                                    .w_full()
                                    .h(px(28.))
                                    .px_2p5()
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(if replace_mode {
                                        palette.accent_border
                                    } else {
                                        palette.border
                                    })
                                    .bg(if replace_mode {
                                        palette.accent_soft
                                    } else {
                                        palette.surface_header
                                    })
                                    .justify_start()
                                    .text_size(px(12.))
                                    .text_color(if replace_mode {
                                        palette.accent
                                    } else {
                                        palette.text_body
                                    })
                                    .child("完全替换当前内容")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(edit) = &mut this.workspace_edit {
                                            edit.git_draft_replace = true;
                                            cx.notify();
                                        }
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_1p5()
                            .child(
                                FlatButton::new("git-draft-cancel")
                                    .h(px(24.))
                                    .px_2p5()
                                    .rounded(px(5.))
                                    .border_1()
                                    .border_color(palette.border)
                                    .bg(palette.surface)
                                    .hover(move |s| s.bg(palette.tab_active()))
                                    .text_size(px(11.5))
                                    .text_color(palette.text_body)
                                    .child("取消")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(edit) = &mut this.workspace_edit {
                                            edit.active_popover = None;
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(
                                FlatButton::new("git-draft-generate")
                                    .h(px(24.))
                                    .px_2p5()
                                    .rounded(px(5.))
                                    .bg(palette.accent_fill)
                                    .text_size(px(11.5))
                                    .text_color(palette.on_accent)
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(ph_icon(PhIcon::Sparkle, px(12.), palette.on_accent))
                                    .child("立即生成")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if let Some(edit) = &mut this.workspace_edit {
                                            edit.active_popover = None;
                                        }
                                        this.append_git_daily_log_to_journal(window, cx);
                                    })),
                            ),
                    )
            }
        };

        Some(
            card.with_animation(
                ElementId::Name(format!("editor-popover-{active:?}").into()),
                Animation::new(Duration::from_millis(160)).with_easing(ease_out_quint()),
                |el, t| el.opacity(0.2 + 0.8 * t).mt(px((1.0 - t) * -6.0)),
            )
            .into_any_element(),
        )
    }
}

impl Prototype {
    #[allow(dead_code)]
    pub(super) fn toggle_previous_journal_popover(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        if edit.active_popover == Some(EditorPopover::PreviousJournal) {
            edit.active_popover = None;
            cx.notify();
            return;
        }
        edit.active_popover = Some(EditorPopover::PreviousJournal);
        let cur_date = edit.date.clone();
        let Some(workspace) = &self.workspace else {
            cx.notify();
            return;
        };
        let prev = workspace
            .entries
            .iter()
            .filter(|entry| entry.date < cur_date)
            .max_by(|a, b| a.date.cmp(&b.date))
            .cloned();
        let Some(prev_entry) = prev else {
            if let Some(edit) = &mut self.workspace_edit {
                edit.prev_preview = None;
            }
            cx.notify();
            return;
        };
        let reader = workspace.reader.clone();
        let load = cx
            .background_executor()
            .spawn(async move { reader.read(&prev_entry.date, prev_entry.format) });
        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                if let (Ok(doc), Some(edit)) = (result, &mut this.workspace_edit) {
                    let excerpt = doc
                        .content
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .take(3)
                        .collect::<Vec<_>>()
                        .join("\n");
                    let formatted_date = if doc.date.len() == 8 {
                        format!(
                            "{}-{}-{}",
                            &doc.date[0..4],
                            &doc.date[4..6],
                            &doc.date[6..8]
                        )
                    } else {
                        doc.date
                    };
                    edit.prev_preview = Some((
                        formatted_date,
                        if excerpt.is_empty() {
                            "（空日报）".into()
                        } else {
                            excerpt
                        },
                    ));
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn apply_preset_template_in_editor(
        &mut self,
        template_id: &str,
        replace_all: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        edit.active_popover = None;
        let date = edit.date.clone();
        let format = edit.format;
        let cfg = workspace.reader.editor_settings().unwrap_or_default();
        let rendered = crate::settings_view::render_journal_template(
            template_id,
            format,
            &cfg.custom_template,
            &date,
        );
        if rendered.is_empty() {
            cx.notify();
            return;
        }
        let current = edit.editor.read(cx).value().to_string();
        let next = if replace_all || current.trim().is_empty() {
            rendered
        } else {
            format!("{current}\n\n{rendered}")
        };
        edit.editor
            .update(cx, |editor, cx| editor.set_value(next.clone(), window, cx));
        edit.rich.sync_from_source(&next, window, cx);
        self.status = "已将日报模板应用到当前编辑区".into();
        cx.notify();
    }

    pub(super) fn toggle_editor_format(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        let next_format = match edit.format {
            Format::Md => Format::Txt,
            Format::Txt => Format::Md,
        };
        edit.format = next_format;
        if let Some(stem) = edit.filename.strip_suffix(".md").or_else(|| edit.filename.strip_suffix(".txt")) {
            edit.filename = format!("{stem}.{}", next_format.extension());
        }
        let source = edit.editor.read(cx).value().to_string();
        edit.rich.sync_from_source(&source, window, cx);
        self.status = format!(
            "已切换为 {} 格式",
            if next_format == Format::Md {
                "Markdown"
            } else {
                "纯文本"
            }
        )
        .into();
        cx.notify();
    }

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

    #[cfg(test)]
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

    #[cfg(test)]
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
        let reader = workspace.reader.clone();
        self.workspace_busy = true;
        self.status = "正在打开日报编辑器…".into();
        cx.notify();
        let open = cx.background_executor().spawn(async move {
            match revision {
                Some(revision) => JournalEdit::begin(root, &date, format, &revision),
                None => {
                    let (target, mut document) = JournalEdit::begin_new(root, &date, format)?;
                    if document.content.trim().is_empty() {
                        if let Ok(editor_cfg) = reader.editor_settings() {
                            if editor_cfg.auto_apply_template {
                                let templated = crate::settings_view::render_journal_template(
                                    &editor_cfg.default_template_id,
                                    format,
                                    &editor_cfg.custom_template,
                                    &date,
                                );
                                if !templated.is_empty() {
                                    document.content = templated;
                                }
                            }
                        }
                    }
                    Ok((target, document))
                }
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
                        this.ensure_editor_module_visible(cx);
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

    pub(super) fn apply_default_template_in_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(edit) = &self.workspace_edit else {
            return;
        };
        let date = edit.date.clone();
        let format = edit.format;
        let cfg = workspace.reader.editor_settings().unwrap_or_default();
        let template_id = if cfg.default_template_id == "none" || cfg.default_template_id.is_empty()
        {
            "standard"
        } else {
            cfg.default_template_id.as_str()
        };
        let rendered = crate::settings_view::render_journal_template(
            template_id,
            format,
            &cfg.custom_template,
            &date,
        );
        if rendered.is_empty() {
            return;
        }
        let current = edit.editor.read(cx).value().to_string();
        let next = if current.trim().is_empty() {
            rendered
        } else {
            format!("{current}\n\n{rendered}")
        };
        edit.editor
            .update(cx, |editor, cx| editor.set_value(next.clone(), window, cx));
        if let Some(edit) = &mut self.workspace_edit {
            edit.rich.sync_from_source(&next, window, cx);
        }
        self.status = "已将日报模板插入当前编辑区".into();
        cx.notify();
    }

    pub(super) fn import_previous_journal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(edit) = &self.workspace_edit else {
            return;
        };
        let cur_date = edit.date.clone();
        let prev = workspace
            .entries
            .iter()
            .filter(|entry| entry.date < cur_date)
            .max_by(|a, b| a.date.cmp(&b.date))
            .cloned();
        let Some(prev_entry) = prev else {
            self.status = "未找到更早日期的日报记录".into();
            cx.notify();
            return;
        };
        let reader = workspace.reader.clone();
        let load = cx
            .background_executor()
            .spawn(async move { reader.read(&prev_entry.date, prev_entry.format) });
        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(doc) => {
                        if let Some(edit) = &this.workspace_edit {
                            let current = edit.editor.read(cx).value().to_string();
                            let next = if current.trim().is_empty() {
                                doc.content
                            } else {
                                format!("{current}\n\n{}", doc.content)
                            };
                            edit.editor.update(cx, |editor, cx| {
                                editor.set_value(next.clone(), window, cx);
                            });
                            if let Some(edit) = &mut this.workspace_edit {
                                edit.rich.sync_from_source(&next, window, cx);
                            }
                            this.status = format!("已引入 {} 的日报内容", doc.date).into();
                        }
                    }
                    Err(error) => {
                        this.status = format!("无法读取上一篇日报：{error}").into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
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
                self.overview.invalidate();
                self.directory.invalidate_tree();
                self.summary.invalidate();
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
        self.ai.cancel_polish();
        self.ai.polish_diff = None;
        self.complete_pending_editor_removal(cx);
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
