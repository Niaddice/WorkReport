use std::fs;

use gpui_kit::{
    AppContext, ElementInputHandler, TestAppContext,
    test::{TestInput, TestWindowExt},
};
use workreport_core::{
    settings::{FILE_NAME, StoredSettings},
    workspace::Format,
};

use crate::{
    layout::RailSide,
    settings_view::SettingsSection,
    workspace_edit_tests::{click, drain_ready_tasks, editor_text, open_window, open_workspace},
};

#[gpui_kit::test]
fn settings_overlay_previews_confirms_discard_saves_transactionally_and_applies_templates(
    cx: &mut TestAppContext,
) {
    let ws_dir = tempfile::tempdir().unwrap();
    let cfg_dir = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(ws_dir.path().join("summary")).unwrap();
    fs::write(ws_dir.path().join("summary/2026-summary.md"), "existing summary\n").unwrap();
    fs::create_dir_all(ws_dir.path().join("2026")).unwrap();
    fs::write(ws_dir.path().join("2026/20261119.md"), "base report\n").unwrap();

    let mut initial = StoredSettings::default();
    initial.appearance.rail = "right".into();
    workreport_core::settings::save(&ws_dir.path().join(FILE_NAME), &initial).unwrap();

    let (window, prototype) = open_window(cx);
    cx.update(|cx| {
        prototype.update(cx, |app, _| {
            app.settings_config_dir = Some(cfg_dir.path().to_path_buf());
        });
    });
    open_workspace(cx, window, &prototype, ws_dir.path());

    // 1. Open via AppRail gear button -> test live preview & dirty discard guard
    click(cx, window, "rail-settings");
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.settings_view.as_ref().is_some_and(|sv| sv.open));
        assert_eq!(app.module_layout.rail_side, RailSide::Right);
    });

    click(cx, window, "settings-skin-graphite");
    click(cx, window, "settings-rail-left");
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert_eq!(app.module_layout.preferences.skin, "graphite");
        assert_eq!(app.module_layout.rail_side, RailSide::Left);
        assert!(app.settings_view.as_ref().unwrap().dirty);
    });

    // Attempting close while dirty requires explicit discard confirmation
    click(cx, window, "settings-cancel");
    cx.update(|cx| {
        let sv = prototype.read(cx).settings_view.as_ref().unwrap();
        assert!(sv.open);
        assert!(sv.confirm_discard);
    });
    click(cx, window, "settings-discard-confirm");
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.settings_view.is_none());
        assert_eq!(app.module_layout.preferences.skin, "default");
        assert_eq!(app.module_layout.rail_side, RailSide::Right);
    });

    // 2. Reopen -> configure Appearance + Files migration + Database + Editor + Git -> Save
    click(cx, window, "rail-settings");
    click(cx, window, "settings-skin-jade");
    click(cx, window, "settings-rail-left");

    // Database test connection
    click(cx, window, "settings-nav-存储与数据库");
    click(cx, window, "settings-db-toggle");
    click(cx, window, "settings-db-test");
    cx.update(|cx| {
        let sv = prototype.read(cx).settings_view.as_ref().unwrap();
        let msg = sv.db_test_message.as_deref().expect("db test message");
        assert!(msg.contains("连接成功"));
    });

    // Editor template preset + auto-apply
    click(cx, window, "settings-nav-编辑器与模板");
    click(cx, window, "settings-editor-auto-apply");
    click(cx, window, "settings-tpl-standard");

    // Git repo draft add
    click(cx, window, "settings-nav-Git 仓库");
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            let sv = app.settings_view.as_deref().unwrap();
            sv.git_new_name_input
                .update(cx, |i, cx| i.set_value("LocalRepo", window, cx));
            sv.git_new_path_input.update(cx, |i, cx| {
                i.set_value(repo_dir.path().to_string_lossy().to_string(), window, cx)
            });
        });
    })
    .unwrap();
    click(cx, window, "settings-git-add-repo");

    // File rules migration preview + confirm
    click(cx, window, "settings-nav-文件规则与迁移");
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            let sv = app.settings_view.as_deref().unwrap();
            sv.summary_dir_input
                .update(cx, |i, cx| i.set_value("reports/summary", window, cx));
        });
    })
    .unwrap();
    click(cx, window, "settings-files-preview");
    cx.update(|cx| {
        let sv = prototype.read(cx).settings_view.as_ref().unwrap();
        let plan = sv.migration_plan.as_ref().expect("migration plan");
        assert!(!plan.directories.is_empty());
    });
    click(cx, window, "settings-files-confirm-migrate");

    // Save everything transactionally
    click(cx, window, "settings-save");
    drain_ready_tasks(cx);

    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.settings_view.as_ref().is_none_or(|sv| !sv.open));
        assert_eq!(app.module_layout.preferences.skin, "jade");
        assert_eq!(app.module_layout.rail_side, RailSide::Left);
        assert_eq!(app.git.settings.repositories.len(), 1);
    });

    assert!(ws_dir.path().join("reports/summary/2026-summary.md").is_file());
    let persisted = workreport_core::settings::load(&ws_dir.path().join(FILE_NAME))
        .unwrap()
        .unwrap();
    assert!(persisted.editor.auto_apply_template);
    assert_eq!(persisted.editor.default_template_id, "standard");
    assert!(persisted.database.enabled);

    // 3. New blank journal automatically populates the configured Markdown template
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.open_workspace_journal_editor("20261125".into(), Format::Md, None, window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);

    let new_body = editor_text(cx, &prototype);
    assert!(new_body.contains("今日工作产出"));

    // Verify clean close without unsaved edits when re-opened
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.open_settings_section(SettingsSection::BackupRestore, window, cx);
        });
    })
    .unwrap();
    let backup_path = cfg_dir.path().join("workspace-backup.json");
    let backup_export = backup_path.clone();
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.export_workspace_backup_to(backup_export, window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    assert!(backup_path.is_file());

    // Simulate data loss / change, then plan + restore via Backup & Restore UI
    fs::write(ws_dir.path().join("2026/20261119.md"), "corrupted locally\n").unwrap();
    cx.update_window(window, |_, _, cx| {
        prototype.update(cx, |app, cx| {
            app.load_and_plan_backup(&backup_path, cx);
        });
    })
    .unwrap();
    click(cx, window, "settings-restore-policy-overwrite");
    click(cx, window, "settings-backup-execute-restore");
    let restored_text = workreport_core::workspace::Workspace::open(ws_dir.path())
        .unwrap()
        .read("20261119", Format::Md)
        .unwrap()
        .content;
    assert_eq!(restored_text, "base report\n");

    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.attempt_close_settings(window, cx);
        });
    })
    .unwrap();
    cx.update(|cx| {
        assert!(prototype.read(cx).settings_view.is_none());
    });
}

#[gpui_kit::test]
fn settings_updates_section_persists_channel_and_migrates_legacy_tauri_config(
    cx: &mut TestAppContext,
) {
    let ws_dir = tempfile::tempdir().unwrap();
    let cfg_root = tempfile::tempdir().unwrap();
    let legacy_dir = cfg_root.path().join(workreport_core::updates::TAURI_CONFIG_DIR_NAME);
    let gpui_dir = cfg_root.path().join(workreport_core::updates::GPUI_CONFIG_DIR_NAME);
    fs::create_dir_all(legacy_dir.join("holidays")).unwrap();
    fs::create_dir_all(&gpui_dir).unwrap();
    fs::write(legacy_dir.join("workspace.json"), br#""/legacy/work""#).unwrap();
    fs::write(legacy_dir.join("ai-config.json"), br#"{"model":"gpt-legacy"}"#).unwrap();
    fs::write(legacy_dir.join("holidays/2026.json"), br#"{"year":2026}"#).unwrap();

    let (window, prototype) = open_window(cx);
    cx.update(|cx| {
        prototype.update(cx, |app, _| {
            app.settings_config_dir = Some(gpui_dir.clone());
        });
    });
    open_workspace(cx, window, &prototype, ws_dir.path());

    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.open_settings_section(SettingsSection::Updates, window, cx);
        });
    })
    .unwrap();

    click(cx, window, "settings-update-channel-dev");
    click(cx, window, "settings-update-allow-switch");

    let saved_pref = workreport_core::updates::load_preference(&gpui_dir).unwrap();
    assert_eq!(saved_pref.channel, workreport_core::updates::Channel::Dev);
    assert!(saved_pref.allow_channel_switch);

    click(cx, window, "settings-legacy-import-missing");
    assert_eq!(
        fs::read_to_string(gpui_dir.join("ai-config.json")).unwrap(),
        r#"{"model":"gpt-legacy"}"#
    );
    assert_eq!(
        fs::read_to_string(gpui_dir.join("holidays/2026.json")).unwrap(),
        r#"{"year":2026}"#
    );
    assert!(legacy_dir.join("workspace.json").is_file());
}

#[gpui_kit::test]
fn settings_git_credentials_and_toolbar_update_dialog_work_cleanly(cx: &mut TestAppContext) {
    let ws_dir = tempfile::tempdir().unwrap();
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, ws_dir.path());

    // Populate saved HTTP Git credentials and startup update result
    cx.update(|cx| {
        prototype.update(cx, |app, cx| {
            app.git.saved_credentials.push(workreport_core::git_auth::Credentials {
                url: "https://github.com".into(),
                username: "octocat".into(),
                password: "secret".into(),
                save: true,
            });
            app.git.saved_credentials.push(workreport_core::git_auth::Credentials {
                url: "https://gitlab.example.com".into(),
                username: "dev".into(),
                password: "secret2".into(),
                save: true,
            });
            app.startup_update = Some(workreport_core::updates::UpdateCheckResult::UpdateAvailable {
                current_version: "0.1.8".into(),
                version: "0.2.0".into(),
                notes: "Native GPUI polish release".into(),
                pub_date: Some("2026-11-21T14:00:00Z".into()),
                artifact_url: "https://example.com/WorkReport.app.tar.gz".into(),
                signature: "sig".into(),
                format: Some("app".into()),
                is_downgrade: false,
                download_url: "https://github.com/Niaddice/WorkReport/releases/tag/v0.2.0".into(),
            });
            cx.notify();
        });
    });

    // Toolbar shows update pill -> opens UpdateDialog -> can switch straight to Updates settings
    click(cx, window, "toolbar-update-btn");
    cx.update(|cx| {
        assert!(prototype.read(cx).update_dialog_open);
    });
    click(cx, window, "update-dialog-settings");
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(!app.update_dialog_open);
        let sv = app.settings_view.as_ref().expect("settings opened");
        assert!(sv.open);
        assert_eq!(sv.section, SettingsSection::Updates);
    });

    // Switch to Git section and test clearing individual + remaining credentials
    click(cx, window, "settings-nav-Git 仓库");
    click(cx, window, "settings-git-cred-clear-0");
    cx.update(|cx| {
        let creds = &prototype.read(cx).git.saved_credentials;
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].url, "https://gitlab.example.com");
    });
    click(cx, window, "settings-git-cred-clear-all");
    cx.update(|cx| {
        assert!(prototype.read(cx).git.saved_credentials.is_empty());
    });

    // Settings sidebar search filters sections & switches cleanly without marking dirty
    cx.update_window(window, |_, window, cx| {
        let search = prototype
            .read(cx)
            .settings_view
            .as_ref()
            .unwrap()
            .search_input
            .clone();
        let id = ("input", search.entity_id());
        window.click(id, cx);
        let bounds = window.find(id).bounds();
        let mut input = TestInput::new(ElementInputHandler::new(bounds, search), window);
        input.commit_text("sqlite", cx);
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update(|cx| {
        let sv = prototype.read(cx).settings_view.as_ref().unwrap();
        assert_eq!(sv.section, SettingsSection::Database);
        assert!(!sv.dirty);
    });
}
