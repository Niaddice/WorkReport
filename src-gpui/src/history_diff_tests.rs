use std::fs;

use gpui_kit::{AppContext, TestAppContext, test::TestWindowExt};
use workreport_core::history;

use crate::{
    history_diff::HistoryDiffMode,
    workspace_edit_tests::{
        click, drain_ready_tasks, editor_text, open_window, open_workspace, replace_editor_text,
    },
};

const DATE: &str = "20261119";

#[gpui_kit::test]
fn history_diff_compares_targets_copies_and_restores_while_browsing(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    fs::create_dir(folder.join("2026")).unwrap();
    let path = folder.join(format!("2026/{DATE}.md"));
    let v1 = "# 第一版\n旧行\n";
    let v2 = "# 第二版 📝\n新行\n追加行\n";
    fs::write(&path, v1).unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    click(cx, window, "rail-module-history");
    click(cx, window, "edit-journal");
    replace_editor_text(cx, window, &prototype, v2);
    click(cx, window, "save-journal");
    click(cx, window, "close-journal-editor");

    // Edit again so there are 2 snapshots and current content is v3.
    let v3 = "# 第三版\n当前行\n";
    click(cx, window, "edit-journal");
    replace_editor_text(cx, window, &prototype, v3);
    click(cx, window, "save-journal");
    click(cx, window, "close-journal-editor");

    let versions = history::list(folder, Some(DATE)).unwrap();
    assert_eq!(versions.len(), 2);
    // versions are newest-first: versions[0] is v3, versions[1] is v2.
    let older = &versions[1];
    let newer = &versions[0];

    // Copy snapshot directly from History module row.
    click(cx, window, format!("history-copy-{}", older.id));
    assert_eq!(
        cx.update(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        v2
    );

    // Open diff for the older snapshot (v2) vs current (v3).
    click(cx, window, format!("history-diff-{}", older.id));
    cx.update(|cx| {
        let diff = prototype.read(cx).history_diff.as_ref().unwrap();
        assert_eq!(diff.mode, HistoryDiffMode::Split);
        assert_eq!(diff.base_content, v2);
        assert_eq!(diff.target_content, v3);
        assert!(!diff.diff.identical);
    });

    // Switch to unified mode and back to split, then compare against newer snapshot (v3).
    click(cx, window, "history-diff-mode-unified");
    cx.update(|cx| {
        assert_eq!(
            prototype.read(cx).history_diff.as_ref().unwrap().mode,
            HistoryDiffMode::Unified
        );
    });
    click(cx, window, format!("history-diff-target-{}", newer.id));
    cx.update(|cx| {
        let diff = prototype.read(cx).history_diff.as_ref().unwrap();
        assert_eq!(diff.target_id.as_deref(), Some(newer.id.as_str()));
        assert_eq!(diff.target_content, v3);
    });

    // Switch back to current and copy from dialog.
    click(cx, window, "history-diff-target-current");
    click(cx, window, "history-diff-copy");
    assert_eq!(
        cx.update(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        v2
    );

    // Trigger restore confirmation, cancel via Esc, then re-open confirmation and restore.
    click(cx, window, "history-diff-restore");
    cx.update(|cx| {
        assert!(prototype.read(cx).history_diff.as_ref().unwrap().confirming_restore);
    });
    cx.update_window(window, |_, window, cx| {
        window.press("escape", cx);
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update(|cx| {
        let diff = prototype.read(cx).history_diff.as_ref().unwrap();
        assert!(!diff.confirming_restore);
    });

    click(cx, window, "history-diff-restore");
    click(cx, window, "history-diff-confirm-restore");
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(
            app.history_diff.is_none(),
            "history_diff still open; status={:?}, error={:?}, confirming={:?}",
            app.status,
            app.history_diff.as_ref().and_then(|d| d.error.as_ref()),
            app.history_diff.as_ref().map(|d| d.confirming_restore),
        );
    });
    assert_eq!(fs::read_to_string(&path).unwrap(), v2);
    assert_eq!(
        cx.update(|cx| prototype
            .read(cx)
            .workspace
            .as_ref()
            .unwrap()
            .journal
            .as_ref()
            .unwrap()
            .content()
            .to_owned()),
        v2
    );
    assert_eq!(history::list(folder, Some(DATE)).unwrap().len(), 3);
}

#[gpui_kit::test]
fn restoring_in_active_editor_updates_draft_and_external_conflict_blocks_overwrite(
    cx: &mut TestAppContext,
) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    fs::create_dir(folder.join("2026")).unwrap();
    let path = folder.join(format!("2026/{DATE}.txt"));
    fs::write(&path, "初始内容\n").unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    click(cx, window, "rail-module-history");

    click(cx, window, "edit-journal");
    replace_editor_text(cx, window, &prototype, "第一版快照\n");
    click(cx, window, "save-journal");
    replace_editor_text(cx, window, &prototype, "未保存的较新草稿\n");

    let versions = history::list(folder, Some(DATE)).unwrap();
    assert_eq!(versions.len(), 1);
    let target = &versions[0];

    // Diff against live unsaved draft sees the difference and restores cleanly into editor.
    click(cx, window, format!("history-diff-{}", target.id));
    cx.update(|cx| {
        let diff = prototype.read(cx).history_diff.as_ref().unwrap();
        assert_eq!(diff.target_content, "未保存的较新草稿\n");
        assert!(!diff.diff.identical);
    });
    click(cx, window, "history-diff-restore");
    click(cx, window, "history-diff-confirm-restore");
    assert_eq!(editor_text(cx, &prototype), "第一版快照\n");
    assert_eq!(fs::read_to_string(&path).unwrap(), "第一版快照\n");

    // External modification conflict during browse restore preserves external file.
    click(cx, window, "close-journal-editor");
    click(cx, window, "edit-journal");
    replace_editor_text(cx, window, &prototype, "第二版\n");
    click(cx, window, "save-journal");
    click(cx, window, "close-journal-editor");

    let external = "外部程序刚刚改过\n";
    fs::write(&path, external).unwrap();
    click(cx, window, format!("history-diff-{}", target.id));
    click(cx, window, "history-diff-restore");
    click(cx, window, "history-diff-confirm-restore");
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
    cx.update(|cx| {
        let diff = prototype.read(cx).history_diff.as_ref().unwrap();
        assert!(diff.error.is_some());
    });
}
