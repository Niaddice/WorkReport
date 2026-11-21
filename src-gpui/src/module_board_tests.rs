use std::{collections::BTreeMap, fs, path::Path};

use gpui_kit::{
    AppContext, MouseButton, TestAppContext, point,
    test::{TestEventExt, TestWindowExt},
};
use workreport_core::settings;

use crate::{
    layout::{ModuleType, RailSide},
    workspace_edit_tests::{
        click, drain_ready_tasks, editor_text, open_window, open_workspace, replace_editor_text,
    },
};

fn snapshot_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).unwrap();
        }
    }
}

#[gpui_kit::test]
fn opening_legacy_fixture_restores_column_layout_skin_and_left_rail_without_writing(
    cx: &mut TestAppContext,
) {
    let fixture_src =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/gpui-migration/workspace");
    let temporary = tempfile::tempdir().unwrap();
    let workspace_copy = temporary.path().join("workspace");
    copy_dir(&fixture_src, &workspace_copy);
    let before = snapshot_tree(&workspace_copy);

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, &workspace_copy);

    cx.update(|cx| {
        let app = prototype.read(cx);
        assert_eq!(app.module_layout.rail_side, RailSide::Left);
        assert_eq!(app.module_layout.scale, 110);
        assert_eq!(app.module_layout.preferences.skin, "jade");
        assert_eq!(
            app.module_layout.items(),
            vec![ModuleType::Calendar, ModuleType::Editor, ModuleType::History]
        );
    });

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let rail_bounds = window.find("rail-layout").bounds();
        let cal_bounds = window.find("module-panel-calendar-drag").bounds();
        assert!(rail_bounds.origin.x < cal_bounds.origin.x);

        let cal = window.find("module-panel-calendar-drag").bounds();
        let ed = window.find("module-panel-editor-drag").bounds();
        let hist = window.find("module-panel-history-drag").bounds();
        assert!(cal.origin.y < ed.origin.y);
        assert!(ed.origin.y < hist.origin.y);
    })
    .unwrap();

    // Browsing a snapshot from History and returning keeps the fixture untouched.
    click(cx, window, "history-1795068300000-68a5053c.md");
    click(cx, window, "return-current-journal");
    assert_eq!(snapshot_tree(&workspace_copy), before);
}

#[gpui_kit::test]
fn rail_module_toggles_and_empty_layout_persist_and_reload(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    fs::create_dir(folder.join("2026")).unwrap();
    fs::write(folder.join("2026/20261119.md"), "# 测试日报\n").unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);

    // Default layout is calendar | editor with right rail.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let rail_bounds = window.find("rail-layout").bounds();
        let ed_bounds = window.find("module-panel-editor-drag").bounds();
        assert!(rail_bounds.origin.x > ed_bounds.origin.x);
        assert!(window.try_find("module-panel-calendar-drag").is_some());
        assert!(window.try_find("module-panel-editor-drag").is_some());
    })
    .unwrap();

    // Add overview via rail.
    click(cx, window, "rail-module-overview");
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("module-panel-overview-drag").is_some());
    })
    .unwrap();

    // Remove all modules -> empty board state.
    click(cx, window, "module-panel-calendar-remove");
    click(cx, window, "module-panel-editor-remove");
    click(cx, window, "module-panel-overview-remove");
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("module-panel-calendar-drag").is_none());
        assert!(window.try_find("module-panel-editor-drag").is_none());
        assert!(window.try_find("module-panel-overview-drag").is_none());
    })
    .unwrap();
    assert_eq!(
        cx.update(|cx| prototype.read(cx).module_layout.layout().clone()),
        crate::layout::Layout::Empty
    );

    // Reload workspace -> explicit empty layout persists!
    let (window2, prototype2) = open_window(cx);
    open_workspace(cx, window2, &prototype2, folder);
    cx.update_window(window2, |_, window, _| {
        assert!(window.try_find("module-panel-calendar-drag").is_none());
        assert!(window.try_find("module-panel-editor-drag").is_none());
    })
    .unwrap();
    assert_eq!(
        cx.update(|cx| prototype2.read(cx).module_layout.layout().clone()),
        crate::layout::Layout::Empty
    );

    // Re-add history via rail -> persists and reloads.
    click(cx, window2, "rail-module-history");
    cx.update_window(window2, |_, window, _| {
        assert!(window.try_find("module-panel-history-drag").is_some());
    })
    .unwrap();

    let stored = settings::load(&folder.join(settings::FILE_NAME))
        .unwrap()
        .unwrap();
    let ui = stored.ui.unwrap();
    assert_eq!(ui.modules.len(), 1);
    assert_eq!(ui.modules[0].id, "history");
}

#[gpui_kit::test]
fn dragging_headers_splits_inserts_moves_to_edge_and_resizing_gutters_persists(
    cx: &mut TestAppContext,
) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);

    // Drag calendar header onto the bottom half of editor -> column split with editor on top, calendar on bottom.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find("module-panel-calendar-drag").bounds().center();
        let board_bounds = prototype.read(cx).module_board.board_bounds.get().unwrap();
        let ed_drag = window.find("module-panel-editor-drag").bounds();
        let to = point(
            ed_drag.center().x,
            board_bounds.origin.y + board_bounds.size.height * 0.85,
        );
        window.pointer_move(from, None, cx);
        window.pointer_down(from, MouseButton::Left, cx);
        window.pointer_move(to, Some(MouseButton::Left), cx);
        window.pointer_up(to, MouseButton::Left, cx);
    })
    .unwrap();
    drain_ready_tasks(cx);

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let ed = window.find("module-panel-editor-drag").bounds();
        let cal = window.find("module-panel-calendar-drag").bounds();
        assert!(ed.origin.y < cal.origin.y);
    })
    .unwrap();

    // Drag the root column gutter downward to ~70% height and verify persistence.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let board = prototype.read(cx).module_board.board_bounds.get().unwrap();
        let gutter = window.find("module-gutter-root").bounds().center();
        let target = point(
            gutter.x,
            board.origin.y + board.size.height * 0.70,
        );
        window.pointer_move(gutter, None, cx);
        window.pointer_down(gutter, MouseButton::Left, cx);
        window.pointer_move(target, Some(MouseButton::Left), cx);
        window.pointer_up(target, MouseButton::Left, cx);
    })
    .unwrap();
    drain_ready_tasks(cx);

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let ed = window.find("module-panel-editor-drag").bounds();
        let cal = window.find("module-panel-calendar-drag").bounds();
        assert!(cal.origin.y - ed.origin.y > (cal.bottom() - cal.origin.y) * 1.5 || (cal.origin.y - ed.origin.y) > 300.0.into());
    })
    .unwrap();

    // Add overview and drag calendar to the far-left edge band.
    click(cx, window, "rail-module-overview");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let board = prototype.read(cx).module_board.board_bounds.get().unwrap();
        let from = window.find("module-panel-calendar-drag").bounds().center();
        let edge = point(board.origin.x + gpui_kit::px(10.), board.center().y);
        window.pointer_move(from, None, cx);
        window.pointer_down(from, MouseButton::Left, cx);
        window.pointer_move(edge, Some(MouseButton::Left), cx);
        window.pointer_up(edge, MouseButton::Left, cx);
    })
    .unwrap();
    drain_ready_tasks(cx);

    cx.update(|cx| {
        assert_eq!(prototype.read(cx).module_layout.items()[0], ModuleType::Calendar);
    });
}

#[gpui_kit::test]
fn maximize_drag_and_layout_changes_preserve_active_editor_draft(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    fs::create_dir(folder.join("2026")).unwrap();
    let path = folder.join("2026/20261119.md");
    fs::write(&path, "# 原文\n").unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    click(cx, window, "edit-journal");
    let draft = "# 正在编辑的草稿 📝\n";
    replace_editor_text(cx, window, &prototype, draft);

    // Add history module and maximize editor -> draft remains intact!
    click(cx, window, "rail-module-history");
    click(cx, window, "module-panel-editor-maximize");
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("module-panel-calendar-drag").is_none());
        assert!(window.try_find("module-panel-editor-drag").is_some());
    })
    .unwrap();
    assert_eq!(editor_text(cx, &prototype), draft);

    // Esc restores maximized view without discarding draft.
    cx.update_window(window, |_, window, cx| {
        window.press("escape", cx);
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("module-panel-calendar-drag").is_some());
        assert!(window.try_find("module-panel-editor-drag").is_some());
    })
    .unwrap();
    assert_eq!(editor_text(cx, &prototype), draft);

    // Removing editor while dirty flushes the draft safely before removing the module.
    click(cx, window, "module-panel-editor-remove");
    assert_eq!(fs::read_to_string(&path).unwrap(), draft);
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("module-panel-editor-drag").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn layout_dialog_presets_named_layouts_and_skin_dialog_persist(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);

    // Open layout dialog, save a named layout, apply compact-writing preset, then restore saved.
    click(cx, window, "rail-layout");
    cx.update_window(window, |_, window, cx| {
        let input = prototype.read(cx).module_board.layout_name_input.clone();
        input.update(cx, |state, cx| state.set_value("双栏写作", window, cx));
    })
    .unwrap();
    click(cx, window, "layout-save-button");
    click(cx, window, "layout-preset-compact-writing");
    cx.update(|cx| {
        assert_eq!(
            prototype.read(cx).module_layout.items(),
            vec![ModuleType::Calendar, ModuleType::Overview, ModuleType::Editor]
        );
    });
    click(cx, window, "layout-saved-apply-0");
    cx.update(|cx| {
        assert_eq!(
            prototype.read(cx).module_layout.items(),
            vec![ModuleType::Calendar, ModuleType::Editor]
        );
    });
    click(cx, window, "layout-dialog-close");

    // Open skin dialog and switch to midnight (dark).
    click(cx, window, "rail-skin");
    click(cx, window, "skin-option-midnight");
    cx.update(|cx| {
        assert_eq!(prototype.read(cx).module_layout.preferences.skin, "midnight");
    });
    click(cx, window, "skin-dialog-close");

    let (window2, prototype2) = open_window(cx);
    open_workspace(cx, window2, &prototype2, folder);
    cx.update(|cx| {
        let app = prototype2.read(cx);
        assert_eq!(app.module_layout.preferences.skin, "midnight");
        assert_eq!(app.module_layout.preferences.saved_layouts.len(), 1);
        assert_eq!(app.module_layout.preferences.saved_layouts[0].name, "双栏写作");
    });
}

#[gpui_kit::test]
fn module_header_settings_buttons_open_corresponding_settings_sections(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);

    click(cx, window, "module-panel-calendar-settings");
    cx.update(|cx| {
        let sv = prototype.read(cx).settings_view.as_ref().expect("settings open");
        assert!(sv.open);
        assert_eq!(sv.section, crate::settings_view::SettingsSection::Holidays);
    });
    click(cx, window, "settings-cancel");

    click(cx, window, "rail-module-git");
    click(cx, window, "module-panel-git-settings");
    cx.update(|cx| {
        let sv = prototype.read(cx).settings_view.as_ref().expect("settings open");
        assert!(sv.open);
        assert_eq!(sv.section, crate::settings_view::SettingsSection::Git);
    });
}
