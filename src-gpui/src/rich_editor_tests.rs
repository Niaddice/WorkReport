use std::fs;

use gpui_kit::{AppContext, TestAppContext};
use workreport_core::workspace::{Format, Workspace};

use crate::{
    layout::ModuleType,
    rich_editor::{JournalEditorMode, RichBlockKind, RichDocument},
    workspace_edit_tests::{click, drain_ready_tasks, editor_text, open_window, open_workspace},
};

#[test]
fn rich_markdown_preserves_frontmatter_html_refs_and_linked_images_verbatim() {
    let source = concat!(
        "---\n",
        "title: 周报归档\n",
        "tags: [gpui, rust]\n",
        "---\n\n",
        "# 本周总结\n\n",
        "- [ ] 完成 GPUI 日历\n",
        "- [x] 完成历史对比\n\n",
        "<details><summary>展开</summary>详情</details>\n\n",
        "[![架构图](arch.png)](https://example.com)\n\n",
        "[ref1]: https://example.com \"参考\"\n\n",
        "| 模块 | 状态 |\n",
        "| --- | --- |\n",
        "| Git | 进行中 |"
    );

    let mut doc = RichDocument::parse(source);
    assert!(matches!(doc.blocks[0].kind, RichBlockKind::Preserved { .. }));
    assert!(doc.toggle_task(2));
    doc.add_table_row(7);

    let roundtrip = doc.to_markdown();
    assert!(roundtrip.starts_with("---\ntitle: 周报归档\ntags: [gpui, rust]\n---"));
    assert!(roundtrip.contains("- [x] 完成 GPUI 日历"));
    assert!(roundtrip.contains("<details><summary>展开</summary>详情</details>"));
    assert!(roundtrip.contains("[![架构图](arch.png)](https://example.com)"));
    assert!(roundtrip.contains("[ref1]: https://example.com \"参考\""));
    assert!(roundtrip.contains("| 内容 | 内容 |"));
}

#[test]
fn rich_document_block_transforms_and_moves_work_reliably() {
    let mut doc = RichDocument::parse("第一段\n\n第二段");
    assert_eq!(doc.blocks.len(), 2);

    doc.set_heading(0, 2);
    doc.wrap_inline(0, "**", "**");
    doc.convert_block_to(1, "task");
    doc.move_block(1, -1);

    let out = doc.to_markdown();
    assert_eq!(out, "- [ ] 第二段\n\n## **第一段**");
}

#[gpui_kit::test]
fn gpui_workspace_editor_toggles_wysiwyg_blocks_and_saves_markdown(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("2026")).unwrap();
    fs::write(
        dir.path().join("2026/20261119.md"),
        "上篇重点工作：完成底层验证",
    )
    .unwrap();
    fs::write(
        dir.path().join("2026/20261120.md"),
        "---\nauthor: tester\n---\n\n### 今日事项\n\n- [ ] 待完成项",
    )
    .unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, dir.path());

    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.module_layout.activate(ModuleType::Editor);
            app.open_workspace_journal_editor(
                "20261120".into(),
                Format::Md,
                Some(
                    Workspace::open(dir.path())
                        .unwrap()
                        .read("20261120", Format::Md)
                        .unwrap()
                        .revision,
                ),
                window,
                cx,
            );
        });
    })
    .unwrap();
    drain_ready_tasks(cx);

    click(cx, window, "journal-mode-rich");
    cx.update(|cx| {
        let edit = prototype.read(cx).workspace_edit.as_ref().unwrap();
        assert_eq!(edit.rich.mode, JournalEditorMode::RichBlocks);
        assert!(matches!(edit.rich.doc.blocks[0].kind, RichBlockKind::Preserved { .. }));
    });

    // Toggle task block index 2 ("- [ ] 待完成项") -> checked
    click(cx, window, "rich-task-toggle-2");
    // Select heading block index 1 and convert bold inline
    click(cx, window, "rich-block-1");
    click(cx, window, "rich-tb-bold");

    let synced = editor_text(cx, &prototype);
    assert!(synced.contains("---\nauthor: tester\n---"));
    assert!(synced.contains("### **今日事项**"));
    assert!(synced.contains("- [x] 待完成项"));

    click(cx, window, "save-journal");
    let saved = fs::read_to_string(dir.path().join("2026/20261120.md")).unwrap();
    assert!(saved.contains("- [x] 待完成项"));

    // Import previous day's journal and insert template via quick editor actions
    click(cx, window, "editor-import-previous");
    click(cx, window, "editor-insert-template");
    let with_helpers = editor_text(cx, &prototype);
    assert!(with_helpers.contains("上篇重点工作：完成底层验证"));
    assert!(with_helpers.contains("今日工作产出"));
}
