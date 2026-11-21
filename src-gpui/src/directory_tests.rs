use std::fs;

use gpui_kit::{AppContext, EntityInputHandler, TestAppContext, test::TestWindowExt};
use workreport_core::workspace::FileNode;

use crate::{
    directory::{build_tree, validate_node_name, visible_rows},
    workspace_edit_tests::{click, drain_ready_tasks, open_window, open_workspace},
};

fn file_node(path: &str, is_dir: bool) -> FileNode {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    FileNode {
        name,
        path: path.to_string(),
        is_dir,
        size: if is_dir { 0 } else { 12 },
        journal_date: None,
        protected: false,
    }
}

#[test]
fn directory_tree_sorting_and_name_validation_enforce_rules() {
    let nodes = vec![
        file_node("readme.md", false),
        file_node("docs", true),
        file_node("docs/b.txt", false),
        file_node("docs/a.md", false),
        file_node("2026", true),
    ];
    let tree = build_tree(&nodes);
    assert_eq!(tree[0].node.name, "2026");
    assert_eq!(tree[1].node.name, "docs");
    assert_eq!(tree[2].node.name, "readme.md");
    assert_eq!(tree[1].children[0].node.name, "a.md");
    assert_eq!(tree[1].children[1].node.name, "b.txt");

    let mut collapsed = std::collections::BTreeSet::new();
    collapsed.insert("docs".to_string());
    let rows = visible_rows(&tree, &collapsed);
    assert_eq!(rows.len(), 3);

    assert!(validate_node_name("../evil", "", &nodes, None, false).is_err());
    assert!(validate_node_name(".hidden", "", &nodes, None, false).is_err());
    assert!(validate_node_name("workreport.json", "", &nodes, None, false).is_err());
    assert!(validate_node_name("readme.md", "", &nodes, None, false).is_err());
    assert!(validate_node_name("renamed", "2026", &nodes, Some("2026/20261119.md"), true).is_err());
    assert_eq!(
        validate_node_name("new-note.md", "", &nodes, None, false).unwrap(),
        "new-note.md"
    );
}

#[gpui_kit::test]
fn directory_creates_edits_conflicts_deletes_restores_previews_tables_and_searches(
    cx: &mut TestAppContext,
) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    fs::create_dir(folder.join("2026")).unwrap();
    fs::write(folder.join("2026/20261119.md"), "# 周报\n完成全局检索接口\n").unwrap();
    fs::write(folder.join("metrics.csv"), "项目,状态\nWorkReport,进行中\n").unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    click(cx, window, "rail-module-directory");
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    drain_ready_tasks(cx);

    // 1. Create a workspace file `notes.md`, open & edit it, verify save and external conflict.
    click(cx, window, "directory-new-file");
    cx.update_window(window, |_, window, cx| {
        let input = prototype
            .read(cx)
            .directory
            .form
            .as_ref()
            .unwrap()
            .input
            .clone();
        input.update(cx, |state, cx| {
            state.replace_text_in_range(None, "notes.md", window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    click(cx, window, "directory-form-submit");
    assert!(folder.join("notes.md").is_file());

    // Open `notes.md` via directory action.
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.open_workspace_document("notes.md".into(), window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);

    cx.update_window(window, |_, window, cx| {
        let editor = prototype
            .read(cx)
            .directory
            .open_doc
            .as_ref()
            .unwrap()
            .editor
            .clone()
            .unwrap();
        editor.update(cx, |state, cx| {
            state.replace_text_in_range(None, "# 备忘录\n测试关键词\n", window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    click(cx, window, "workspace-doc-save");
    assert_eq!(
        fs::read_to_string(folder.join("notes.md")).unwrap(),
        "# 备忘录\n测试关键词\n"
    );

    // External modification triggers conflict on save and keeps draft.
    fs::write(folder.join("notes.md"), "外部重写内容\n").unwrap();
    cx.update_window(window, |_, window, cx| {
        let editor = prototype
            .read(cx)
            .directory
            .open_doc
            .as_ref()
            .unwrap()
            .editor
            .clone()
            .unwrap();
        editor.update(cx, |state, cx| {
            state.replace_text_in_range(None, "本地未保存草稿\n", window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    click(cx, window, "workspace-doc-save");
    assert_eq!(
        fs::read_to_string(folder.join("notes.md")).unwrap(),
        "外部重写内容\n"
    );
    cx.update(|cx| {
        let open_doc = prototype.read(cx).directory.open_doc.as_ref().unwrap();
        assert!(open_doc.error.is_some());
        assert!(open_doc.dirty);
    });
    // Abandon dirty doc explicitly and return to journal view.
    click(cx, window, "workspace-doc-back");
    click(cx, window, "workspace-doc-discard");
    cx.update(|cx| assert!(prototype.read(cx).directory.open_doc.is_none()));

    // 2. Preview CSV table read-only.
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.open_workspace_document("metrics.csv".into(), window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update(|cx| {
        let open_doc = prototype.read(cx).directory.open_doc.as_ref().unwrap();
        assert_eq!(open_doc.doc.kind, "table");
        assert_eq!(open_doc.doc.sheets.len(), 1);
    });
    click(cx, window, "workspace-doc-back");

    // 3. Delete `notes.md` to trash, then restore it.
    cx.update(|cx| {
        prototype.update(cx, |app, cx| {
            app.directory.selected_path = Some("notes.md".into());
            cx.notify();
        });
    });
    click(cx, window, "directory-delete");
    click(cx, window, "directory-delete-confirm");
    assert!(!folder.join("notes.md").exists());

    click(cx, window, "directory-trash");
    let trash_id = cx.update(|cx| prototype.read(cx).directory.trash_entries[0].id.clone());
    click(cx, window, format!("directory-trash-restore-{trash_id}"));
    assert!(folder.join("notes.md").is_file());

    // 4. Global search finds both journal & file.
    click(cx, window, "global-search-open");
    cx.update_window(window, |_, window, cx| {
        let input = prototype.read(cx).directory.search_input.clone();
        input.update(cx, |state, cx| {
            state.replace_text_in_range(None, "全局检索", window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update(|cx| {
        let results = &prototype.read(cx).directory.search_results;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, "journal");
    });
    click(cx, window, "global-search-close");
}
