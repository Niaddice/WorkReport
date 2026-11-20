use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use gpui_kit::{
    AnyWindowHandle, App, AppContext, ElementId, ElementInputHandler, Entity, TestAppContext,
    VisualTestContext,
    test::{TestInput, TestWindowExt},
};
use workreport_core::{
    history,
    workspace::{Format, Workspace},
};

use crate::{
    Prototype, SaveJournal, load_workspace, open_prototype_window,
    recent_workspace::RecentWorkspace, resolve_startup_workspace,
};

const DATE: &str = "20261119";

pub(super) fn open_window(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Prototype>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::bind_prototype_keys(cx);
        open_prototype_window(cx, None)
    })
}

pub(super) fn drain_ready_tasks(cx: &TestAppContext) {
    // Tick ready tasks while leaving the fake clock under the test's control.
    while cx.executor().tick() {}
}

pub(super) fn open_workspace(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    prototype: &Entity<Prototype>,
    folder: &Path,
) {
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.open_workspace(folder.to_owned(), window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
}

pub(super) fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>) {
    cx.update_window(window, |_, window, cx| window.click(id, cx))
        .unwrap();
    drain_ready_tasks(cx);
}

pub(super) fn editor_text(cx: &mut TestAppContext, prototype: &Entity<Prototype>) -> String {
    cx.update(|cx| {
        let edit = prototype.read(cx).workspace_edit.as_ref().unwrap();
        edit.editor.read(cx).value().to_string()
    })
}

pub(super) fn replace_editor_text(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    prototype: &Entity<Prototype>,
    content: &str,
) {
    cx.update_window(window, |_, window, cx| {
        let editor = prototype
            .read(cx)
            .workspace_edit
            .as_ref()
            .unwrap()
            .editor
            .clone();
        let id = ("input", editor.entity_id());
        window.click(id, cx);
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        let bounds = window.find(id).bounds();
        let mut input = TestInput::new(ElementInputHandler::new(bounds, editor), window);
        input.commit_text(content, cx);
    })
    .unwrap();
    drain_ready_tasks(cx);
    assert_eq!(editor_text(cx, prototype), content);
}

fn text_journal_fixture(original: &str) -> (tempfile::TempDir, PathBuf) {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir(workspace.path().join("2026")).unwrap();
    let path = workspace.path().join(format!("2026/{DATE}.txt"));
    fs::write(&path, original).unwrap();
    (workspace, path)
}

pub(super) fn advance_time(cx: &mut TestAppContext, milliseconds: u64) {
    cx.executor()
        .advance_clock(Duration::from_millis(milliseconds));
    drain_ready_tasks(cx);
}

fn with_editor_input(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    prototype: &Entity<Prototype>,
    operation: impl FnOnce(&mut TestInput<'_>, &mut App),
) {
    cx.update_window(window, |_, window, cx| {
        let editor = prototype
            .read(cx)
            .workspace_edit
            .as_ref()
            .unwrap()
            .editor
            .clone();
        let bounds = window.find(("input", editor.entity_id())).bounds();
        let mut input = TestInput::new(ElementInputHandler::new(bounds, editor), window);
        operation(&mut input, cx);
    })
    .unwrap();
    drain_ready_tasks(cx);
}

fn begin_new_journal(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    prototype: &Entity<Prototype>,
    date: &str,
    format: Format,
) {
    cx.update_window(window, |_, window, cx| {
        let date_input = prototype.read(cx).new_journal_date.clone();
        date_input.update(cx, |input, cx| input.set_value(date, window, cx));
    })
    .unwrap();
    click(
        cx,
        window,
        if format == Format::Md {
            "new-journal-md"
        } else {
            "new-journal-txt"
        },
    );
    click(cx, window, "new-journal");
}

#[test]
fn startup_uses_the_prototype_recent_workspace_and_explicit_paths_take_priority() {
    let temporary = tempfile::tempdir().unwrap();
    let recent = RecentWorkspace::in_directory(temporary.path().join("gpui-config"));
    assert_eq!(
        resolve_startup_workspace(None, Some(&recent)).unwrap(),
        None
    );

    let saved = temporary.path().join("已保存工作空间");
    recent.save(&saved).unwrap();
    assert_eq!(
        resolve_startup_workspace(None, Some(&recent)).unwrap(),
        Some(saved.clone())
    );

    fs::write(
        temporary.path().join("gpui-config/workspace.json"),
        "invalid JSON",
    )
    .unwrap();
    let explicit = temporary.path().join("本次明确选择");
    assert_eq!(
        resolve_startup_workspace(Some(explicit.clone()), Some(&recent)).unwrap(),
        Some(explicit)
    );
    assert!(resolve_startup_workspace(None, Some(&recent)).is_err());
}

#[gpui_kit::test]
fn a_successful_open_is_restored_at_startup_and_a_failed_open_keeps_the_recent_record(
    cx: &mut TestAppContext,
) {
    let temporary = tempfile::tempdir().unwrap();
    let recent = RecentWorkspace::in_directory(temporary.path().join("gpui-config"));
    let folder = temporary.path().join("已打开的工作空间");
    fs::create_dir(&folder).unwrap();
    load_workspace(folder.clone(), Some(&recent)).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(recent.load().unwrap(), Some(folder.canonicalize().unwrap()));

    let missing = temporary.path().join("missing-workspace");
    assert!(load_workspace(missing, Some(&recent)).is_err());
    assert_eq!(recent.load().unwrap(), Some(folder.canonicalize().unwrap()));

    let restored = resolve_startup_workspace(None, Some(&recent))
        .unwrap()
        .unwrap();
    let loaded = load_workspace(restored, Some(&recent));
    let (_, prototype) = cx.update(|cx| {
        gpui_kit::init(cx);
        open_prototype_window(cx, Some(loaded))
    });
    assert!(cx.update(|cx| prototype.read(cx).viewing_workspace));
    assert_eq!(
        cx.update(|cx| prototype
            .read(cx)
            .workspace
            .as_ref()
            .unwrap()
            .reader
            .root()
            .to_path_buf()),
        folder.canonicalize().unwrap()
    );
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 0);
}

#[gpui_kit::test]
fn new_journals_are_created_after_nonblank_input_and_reopen_from_the_date_list(
    cx: &mut TestAppContext,
) {
    let (window, prototype) = open_window(cx);
    for (format, content) in [
        (Format::Md, "# 新建中文日报 📝\n\n**成果**\n"),
        (Format::Txt, "新建纯文本日报 📝\n保留 ## 标记\n"),
    ] {
        let workspace = tempfile::tempdir().unwrap();
        let folder = workspace.path();
        fs::write(
            folder.join("workreport.json"),
            r#"{"files":{"journal":{"directory":"日报","filenameTemplate":"工作_{date}"}}}"#,
        )
        .unwrap();
        let path = folder.join(format!("日报/2026/工作_20261120.{}", format.extension()));

        open_workspace(cx, window, &prototype, folder);
        begin_new_journal(cx, window, &prototype, "2026-11-20", format);
        assert_eq!(
            cx.update(|cx| prototype.read(cx).calendar.selected_date().to_string()),
            "2026-11-20"
        );
        assert_eq!(editor_text(cx, &prototype), "");
        assert!(!path.exists());
        assert_eq!(fs::read_dir(folder).unwrap().count(), 1);

        replace_editor_text(cx, window, &prototype, content);
        advance_time(cx, 649);
        assert!(!path.exists());
        advance_time(cx, 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
        assert_eq!(history::list(folder, Some("20261120")).unwrap().len(), 1);

        click(cx, window, "close-journal-editor");
        assert!(cx.update(|cx| prototype.read(cx).workspace_edit.is_none()));
        click(cx, window, "workspace-date-20261120");
        click(cx, window, "edit-journal");
        assert_eq!(editor_text(cx, &prototype), content);
        click(cx, window, "close-journal-editor");

        let mut legacy = Workspace::open(folder).unwrap();
        let entries = legacy.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].date, "20261120");
        assert_eq!(entries[0].format, format);
        assert_eq!(legacy.read("20261120", format).unwrap().content, content);
    }
}

#[gpui_kit::test]
fn closing_a_blank_new_journal_creates_no_file_index_or_history(cx: &mut TestAppContext) {
    let workspace = tempfile::tempdir().unwrap();
    let folder = workspace.path();
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    begin_new_journal(cx, window, &prototype, "20261120", Format::Txt);
    replace_editor_text(cx, window, &prototype, " \n\t\n");
    advance_time(cx, 650);
    assert_eq!(fs::read_dir(folder).unwrap().count(), 0);

    click(cx, window, "close-journal-editor");
    assert!(!cx.has_pending_prompt());
    assert!(cx.update(|cx| prototype.read(cx).workspace_edit.is_none()));
    assert_eq!(fs::read_dir(folder).unwrap().count(), 0);
}

#[gpui_kit::test]
fn an_external_journal_created_while_a_new_draft_is_open_is_preserved(cx: &mut TestAppContext) {
    let workspace = tempfile::tempdir().unwrap();
    let folder = workspace.path();
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    begin_new_journal(cx, window, &prototype, "20261120", Format::Txt);
    let draft = "本地尚未发布的新日报\n";
    replace_editor_text(cx, window, &prototype, draft);

    fs::create_dir(folder.join("2026")).unwrap();
    let external = folder.join("2026/20261120.md");
    fs::write(&external, "# 外部程序已创建\n").unwrap();
    advance_time(cx, 650);
    assert_eq!(fs::read_to_string(&external).unwrap(), "# 外部程序已创建\n");
    assert!(!folder.join("2026/20261120.txt").exists());
    assert!(history::list(folder, Some("20261120")).unwrap().is_empty());
    assert_eq!(editor_text(cx, &prototype), draft);
    assert!(cx.update(|cx| prototype.read(cx).status.contains("草稿已保留")));
}

#[gpui_kit::test]
fn invalid_dates_are_rejected_and_existing_journals_keep_their_actual_format(
    cx: &mut TestAppContext,
) {
    let workspace = tempfile::tempdir().unwrap();
    let folder = workspace.path();
    let settings = r#"{"database":{"defaultFormat":"md"}}"#;
    fs::write(folder.join("workreport.json"), settings).unwrap();
    fs::create_dir(folder.join("2026")).unwrap();
    let existing = folder.join("2026/20261118.txt");
    let content = "已有 TXT 日报\r\n保留 **原文**\r\n";
    fs::write(&existing, content).unwrap();
    fs::write(folder.join("2026/20261119.md"), "# 最新日报\n").unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    assert_eq!(
        cx.update(|cx| prototype.read(cx).new_journal_format),
        Format::Md
    );

    begin_new_journal(cx, window, &prototype, "2026-02-30", Format::Md);
    assert!(cx.update(|cx| prototype.read(cx).workspace_edit.is_none()));
    assert!(cx.update(|cx| prototype.read(cx).status.contains("有效日期")));
    assert_eq!(fs::read_dir(folder.join("2026")).unwrap().count(), 2);

    begin_new_journal(cx, window, &prototype, "2026-11-18", Format::Md);
    assert!(cx.update(|cx| prototype.read(cx).workspace_edit.is_none()));
    click(cx, window, "edit-journal");
    assert_eq!(editor_text(cx, &prototype), content);
    click(cx, window, "close-journal-editor");

    assert_eq!(fs::read_to_string(&existing).unwrap(), content);
    assert!(!folder.join("2026/20261118.md").exists());
    assert!(!folder.join(".history").exists());
    assert_eq!(
        fs::read_to_string(folder.join("workreport.json")).unwrap(),
        settings
    );
}

#[gpui_kit::test]
fn automatic_save_waits_for_650_milliseconds_since_the_last_edit(cx: &mut TestAppContext) {
    let original = "原始日报\n";
    let latest = "继续输入后的日报 📝\n";
    let (workspace, path) = text_journal_fixture(original);
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, workspace.path());
    click(cx, window, "edit-journal");

    replace_editor_text(cx, window, &prototype, "第一次修改\n");
    advance_time(cx, 649);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    replace_editor_text(cx, window, &prototype, latest);
    advance_time(cx, 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    advance_time(cx, 648);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    advance_time(cx, 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), latest);
    assert_eq!(editor_text(cx, &prototype), latest);
}

#[gpui_kit::test]
fn automatic_save_conflict_keeps_draft_and_requires_explicit_retry(cx: &mut TestAppContext) {
    let original = "原始日报\n";
    let external = "外部修改\n";
    let initial_draft = "本地草稿\n";
    let latest_draft = "继续编辑的草稿 📝\n";
    let (workspace, path) = text_journal_fixture(original);
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, workspace.path());
    click(cx, window, "edit-journal");

    replace_editor_text(cx, window, &prototype, initial_draft);
    fs::write(&path, external).unwrap();
    advance_time(cx, 650);
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
    assert_eq!(editor_text(cx, &prototype), initial_draft);
    assert!(cx.update(|cx| prototype.read(cx).status.contains("草稿已保留")));

    // Restore the expected revision to prove that later typing does not retry automatically.
    fs::write(&path, original).unwrap();
    replace_editor_text(cx, window, &prototype, latest_draft);
    advance_time(cx, 1_300);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(editor_text(cx, &prototype), latest_draft);

    click(cx, window, "save-journal");
    assert_eq!(fs::read_to_string(&path).unwrap(), latest_draft);

    let after_retry = "手动保存成功后继续编辑\n";
    replace_editor_text(cx, window, &prototype, after_retry);
    advance_time(cx, 650);
    assert_eq!(fs::read_to_string(&path).unwrap(), after_retry);
}

#[gpui_kit::test]
fn returning_to_browse_flushes_changes_and_checkpoints_automatic_saves(cx: &mut TestAppContext) {
    let original = "原始日报\n";
    let edited = "返回前保存的内容\n";
    let (workspace, path) = text_journal_fixture(original);
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, workspace.path());
    click(cx, window, "edit-journal");
    replace_editor_text(cx, window, &prototype, edited);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    click(cx, window, "close-journal-editor");
    assert!(!cx.has_pending_prompt());
    assert!(cx.update(|cx| prototype.read(cx).workspace_edit.is_none()));
    assert_eq!(fs::read_to_string(&path).unwrap(), edited);

    let (workspace, path) = text_journal_fixture(original);
    let folder = workspace.path();
    let first_automatic = "第一份自动保存版本\n";
    let final_automatic = "第二份自动保存版本\n";
    open_workspace(cx, window, &prototype, folder);
    click(cx, window, "edit-journal");
    replace_editor_text(cx, window, &prototype, first_automatic);
    advance_time(cx, 650);
    let versions = history::list(folder, Some(DATE)).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(
        history::read(folder, DATE, &versions[0].id).unwrap(),
        first_automatic
    );

    replace_editor_text(cx, window, &prototype, final_automatic);
    advance_time(cx, 650);
    assert_eq!(fs::read_to_string(&path).unwrap(), final_automatic);
    assert_eq!(history::list(folder, Some(DATE)).unwrap().len(), 1);

    click(cx, window, "close-journal-editor");
    assert!(!cx.has_pending_prompt());
    assert!(cx.update(|cx| prototype.read(cx).workspace_edit.is_none()));
    let versions = history::list(folder, Some(DATE)).unwrap();
    assert_eq!(versions.len(), 2);
    assert!(
        versions.iter().any(|version| {
            history::read(folder, DATE, &version.id).unwrap() == final_automatic
        })
    );
}

#[gpui_kit::test]
fn automatic_save_excludes_input_method_candidates(cx: &mut TestAppContext) {
    let original = "原始日报\n";
    let committed = "已确认的日报内容\n";
    let (workspace, path) = text_journal_fixture(original);
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, workspace.path());
    click(cx, window, "edit-journal");
    replace_editor_text(cx, window, &prototype, committed);
    advance_time(cx, 649);

    with_editor_input(cx, window, &prototype, |input, cx| {
        input.compose_text(Some(0..0), "zhong", Some(5..5), cx);
        assert!(input.marked_text_range(cx).is_some());
    });
    advance_time(cx, 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), committed);
    assert_eq!(editor_text(cx, &prototype), format!("zhong{committed}"));

    with_editor_input(cx, window, &prototype, |input, cx| {
        input.commit_text("中文", cx);
        assert!(input.marked_text_range(cx).is_none());
    });
    advance_time(cx, 650);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("中文{committed}")
    );
}

#[gpui_kit::test]
fn returning_while_a_write_is_pending_saves_the_newer_draft_before_leaving(
    cx: &mut TestAppContext,
) {
    let original = "原始日报\n";
    let submitted = "开始写入的版本\n";
    let appended = "写入完成前继续输入\n";
    let (workspace, path) = text_journal_fixture(original);
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, workspace.path());
    click(cx, window, "edit-journal");
    replace_editor_text(cx, window, &prototype, submitted);

    // Do not run the write task until the newer input and the return request
    // have both reached the same editor session.
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| app.save_workspace_journal(window, cx));
        let editor = prototype
            .read(cx)
            .workspace_edit
            .as_ref()
            .unwrap()
            .editor
            .clone();
        let bounds = window.find(("input", editor.entity_id())).bounds();
        TestInput::new(ElementInputHandler::new(bounds, editor), window).commit_text(appended, cx);
        assert!(!prototype.update(cx, |app, cx| app.allow_workspace_close(false, window, cx)));
    })
    .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    drain_ready_tasks(cx);
    assert!(!cx.has_pending_prompt());
    assert!(cx.update(|cx| prototype.read(cx).workspace_edit.is_none()));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("{submitted}{appended}")
    );
}

#[gpui_kit::test]
fn native_window_close_waits_for_the_pending_draft_to_be_written(cx: &mut TestAppContext) {
    let original = "原始日报\n";
    let edited = "关窗前写回的正文\n";
    let (workspace, path) = text_journal_fixture(original);
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, workspace.path());
    click(cx, window, "edit-journal");
    replace_editor_text(cx, window, &prototype, edited);

    assert!(!VisualTestContext::from_window(window, cx).simulate_close());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    drain_ready_tasks(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), edited);
    assert!(cx.update_window(window, |_, _, _| ()).is_err());
}

#[gpui_kit::test]
fn explicit_save_and_reopen_markdown_and_plain_text_journals(cx: &mut TestAppContext) {
    let (window, prototype) = open_window(cx);
    for (extension, original, edited) in [
        ("md", "# 原始日报\n", "# 更新后的日报 📝\n\n**保留标记**\n"),
        ("txt", "原始日报\n", "已更新的纯文本\n保留 ## 标记\n"),
    ] {
        let workspace = tempfile::tempdir().unwrap();
        let folder = workspace.path();
        fs::create_dir(folder.join("2026")).unwrap();
        let path = folder.join(format!("2026/{DATE}.{extension}"));
        fs::write(&path, original).unwrap();

        open_workspace(cx, window, &prototype, folder);
        click(cx, window, "edit-journal");
        assert_eq!(editor_text(cx, &prototype), original);
        replace_editor_text(cx, window, &prototype, edited);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        click(cx, window, "save-journal");
        assert_eq!(fs::read_to_string(&path).unwrap(), edited);
        let format = if extension == "md" {
            Format::Md
        } else {
            Format::Txt
        };
        assert_eq!(
            Workspace::open(folder)
                .unwrap()
                .read(DATE, format)
                .unwrap()
                .content,
            edited
        );

        let snapshots = history::list(folder, Some(DATE)).unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            history::read(folder, DATE, &snapshots[0].id).unwrap(),
            edited
        );

        click(cx, window, "close-journal-editor");
        assert!(cx.update(|cx| prototype.read(cx).workspace_edit.is_none()));
        open_workspace(cx, window, &prototype, folder);
        click(cx, window, "edit-journal");
        assert_eq!(editor_text(cx, &prototype), edited);
        click(cx, window, "close-journal-editor");
    }
}

#[gpui_kit::test]
fn external_change_rejects_save_and_preserves_the_editable_draft(cx: &mut TestAppContext) {
    let workspace = tempfile::tempdir().unwrap();
    let folder = workspace.path();
    fs::create_dir(folder.join("2026")).unwrap();
    let path = folder.join(format!("2026/{DATE}.md"));
    let draft = "# GPUI 草稿 📝\n";
    let external = "# 其他程序写入\n";
    fs::write(&path, "# 原始日报\n").unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    click(cx, window, "edit-journal");
    replace_editor_text(cx, window, &prototype, draft);
    fs::write(&path, external).unwrap();

    click(cx, window, "save-journal");
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
    assert_eq!(editor_text(cx, &prototype), draft);
    assert!(history::list(folder, Some(DATE)).unwrap().is_empty());

    click(cx, window, "copy-journal-draft");
    assert_eq!(
        cx.update(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        draft
    );

    click(cx, window, "close-journal-editor");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("继续编辑");
    drain_ready_tasks(cx);
    assert_eq!(editor_text(cx, &prototype), draft);
    assert_eq!(fs::read_to_string(&path).unwrap(), external);

    assert!(!VisualTestContext::from_window(window, cx).simulate_close());
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("放弃修改");
    drain_ready_tasks(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
}

#[gpui_kit::test]
fn save_action_waits_until_composition_is_committed(cx: &mut TestAppContext) {
    let workspace = tempfile::tempdir().unwrap();
    let folder = workspace.path();
    fs::create_dir(folder.join("2026")).unwrap();
    let path = folder.join(format!("2026/{DATE}.txt"));
    let original = "原始日报\n";
    fs::write(&path, original).unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    click(cx, window, "edit-journal");

    cx.update_window(window, |_, window, cx| {
        let editor = prototype
            .read(cx)
            .workspace_edit
            .as_ref()
            .unwrap()
            .editor
            .clone();
        let bounds = window.find(("input", editor.entity_id())).bounds();
        let mut input = TestInput::new(ElementInputHandler::new(bounds, editor), window);
        input.compose_text(Some(0..0), "zhong", Some(5..5), cx);
        assert!(input.marked_text_range(cx).is_some());
        window.dispatch_action(Box::new(SaveJournal), cx);
    })
    .unwrap();
    drain_ready_tasks(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(cx.update(|cx| prototype.read(cx).status.contains("输入法")));

    cx.update_window(window, |_, window, cx| {
        let editor = prototype
            .read(cx)
            .workspace_edit
            .as_ref()
            .unwrap()
            .editor
            .clone();
        let bounds = window.find(("input", editor.entity_id())).bounds();
        let mut input = TestInput::new(ElementInputHandler::new(bounds, editor), window);
        input.commit_text("中文", cx);
        window.dispatch_action(Box::new(SaveJournal), cx);
    })
    .unwrap();
    drain_ready_tasks(cx);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("中文{original}")
    );
}
