use std::{
    fs,
    path::Path,
    process::Command,
    sync::Arc,
};

use chrono::Local;
use gpui_kit::{AppContext, TestAppContext};
use workreport_core::{
    ai::{
        DetectedCli, ModelInfo, Provider, ProviderKind, Runtime as AiRuntime,
        Settings as AiSettings, TestProviderResult,
    },
    git::{Repository as GitRepoConfig, Settings as GitSettings},
    git_client::Action as GitAction,
    workspace::{Format, Workspace},
};

use crate::{
    ai_panel::AiBackend,
    git_view::GitTab,
    workspace_edit_tests::{click, drain_ready_tasks, open_window, open_workspace},
};

struct GitMockAi;

impl AiBackend for GitMockAi {
    fn detect(&self) -> Vec<DetectedCli> {
        vec![]
    }

    fn list_models(&self, _: &str, _: &str, _: ProviderKind) -> Result<Vec<ModelInfo>, String> {
        Ok(vec![])
    }

    fn list_cli_models(&self, _: ProviderKind, _: &str) -> Result<Vec<ModelInfo>, String> {
        Ok(vec![])
    }

    fn test_provider(&self, _: &Provider) -> Result<TestProviderResult, String> {
        unreachable!()
    }

    fn polish(&self, _: &AiSettings, _: &str, _: &str, _: &str, _: &AiRuntime) -> Result<String, String> {
        unreachable!()
    }

    fn summarize(&self, _: &AiSettings, _: &str, _: Option<String>, _: &str, _: &AiRuntime) -> Result<String, String> {
        unreachable!()
    }

    fn generate_excel_content(&self, _: &Provider, _: &str, _: &str, _: &str, _: &AiRuntime) -> Result<String, String> {
        unreachable!()
    }

    fn fill_excel(&self, _: &Provider, _: &str, _: &str, _: &str, _: &str, _: &AiRuntime) -> Result<String, String> {
        unreachable!()
    }

    fn git_commit_message(
        &self,
        _: &AiSettings,
        content: &str,
        _: &str,
        _: &str,
        _: &AiRuntime,
    ) -> Result<String, String> {
        assert!(content.contains("feature.txt"));
        Ok("feat(gpui): add feature worklog item".into())
    }
}

fn run_git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("git command should run");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[gpui_kit::test]
fn git_workbench_stages_commits_diffs_branches_stashes_pushes_and_appends_journal(
    cx: &mut TestAppContext,
) {
    let remote_dir = tempfile::tempdir().unwrap();
    run_git(remote_dir.path(), &["init", "--bare", "-b", "main"]);

    let repo_dir = tempfile::tempdir().unwrap();
    run_git(repo_dir.path(), &["init", "-b", "main"]);
    run_git(repo_dir.path(), &["config", "user.name", "WorkReport Tester"]);
    run_git(repo_dir.path(), &["config", "user.email", "tester@example.com"]);
    fs::write(repo_dir.path().join("README.md"), "# Demo Repo\n").unwrap();
    run_git(repo_dir.path(), &["add", "README.md"]);
    run_git(repo_dir.path(), &["commit", "-m", "chore: initial commit"]);
    run_git(
        repo_dir.path(),
        &[
            "remote",
            "add",
            "origin",
            &remote_dir.path().to_string_lossy(),
        ],
    );
    run_git(repo_dir.path(), &["push", "-u", "origin", "main"]);

    let ws_dir = tempfile::tempdir().unwrap();
    let today = Local::now().format("%Y%m%d").to_string();
    fs::create_dir_all(ws_dir.path().join("2026")).unwrap();
    let mut stored = workreport_core::settings::StoredSettings::default();
    stored.git = GitSettings {
        executable: "git".into(),
        repositories: vec![GitRepoConfig {
            id: "repo-1".into(),
            name: "DemoRepo".into(),
            path: repo_dir.path().to_string_lossy().into(),
            author: String::new(),
            branch: String::new(),
        }],
        include_merges: false,
        commit_prompt: "生成简洁中文提交说明".into(),
    };
    workreport_core::settings::save(
        &ws_dir.path().join(workreport_core::settings::FILE_NAME),
        &stored,
    )
    .unwrap();

    // Dirty change ready to stage & commit
    fs::write(repo_dir.path().join("feature.txt"), "GPUI Git UI\n").unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, ws_dir.path());

    click(cx, window, "rail-module-git");
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.ai.backend = Arc::new(GitMockAi);
            app.calendar = crate::calendar::CalendarState::new(Local::now().date_naive());
            app.reload_git_workbench(window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);

    cx.update(|cx| {
        let app = prototype.read(cx);
        let repo = app.git.repo.as_ref().expect("git repo should load");
        assert_eq!(repo.branch, "main");
        assert!(repo.files.iter().any(|f| f.path == "feature.txt"));
        assert_eq!(app.git.commits.len(), 1);
    });

    // Stage all + generate AI commit message + commit + push
    click(cx, window, "git-stage-all");
    click(cx, window, "git-commit-ai");
    cx.update(|cx| {
        let msg = prototype.read(cx).git.commit_editor.read(cx).value().to_string();
        assert_eq!(msg, "feat(gpui): add feature worklog item");
    });
    click(cx, window, "git-commit-submit");
    click(cx, window, "git-push");

    cx.update(|cx| {
        let app = prototype.read(cx);
        assert_eq!(app.git.commits.len(), 2);
        assert_eq!(
            app.git.commits[0].message,
            "feat(gpui): add feature worklog item"
        );
        assert_eq!(app.git.repo.as_ref().unwrap().ahead, 0);
    });

    // Open commit Diff view from History tab
    click(cx, window, "git-tab-提交历史");
    let latest_hash = cx.update(|cx| prototype.read(cx).git.commits[0].hash.clone());
    click(cx, window, format!("git-commit-diff-{latest_hash}"));
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert_eq!(app.git.tab, GitTab::Diff);
        let diff = app.git.diff.as_ref().expect("diff should load");
        assert!(diff.text.contains("GPUI Git UI"));
    });

    // Create branch + stash dirty edit + pop stash
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.git.tab = GitTab::Branches;
            app.git
                .branch_name_input
                .update(cx, |i, cx| i.set_value("feat/ui".to_string(), window, cx));
        });
    })
    .unwrap();
    click(cx, window, "git-create-branch");
    cx.update(|cx| {
        assert_eq!(prototype.read(cx).git.repo.as_ref().unwrap().branch, "feat/ui");
    });

    fs::write(repo_dir.path().join("README.md"), "# Stash Me\n").unwrap();
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.git
                .stash_message_input
                .update(cx, |i, cx| i.set_value("wip".to_string(), window, cx));
        });
    })
    .unwrap();
    click(cx, window, "git-stash-save");
    cx.update(|cx| {
        assert_eq!(prototype.read(cx).git.repo.as_ref().unwrap().stashes.len(), 1);
    });
    click(cx, window, "git-stash-pop-stash@{0}");
    cx.update(|cx| {
        assert!(prototype.read(cx).git.repo.as_ref().unwrap().stashes.is_empty());
    });

    // Switch back to main and append commits to journal
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.run_git_action(
                GitAction {
                    operation: "switchBranch".into(),
                    paths: vec![],
                    message: String::new(),
                    reference: "main".into(),
                    patch: String::new(),
                    credentials: None,
                },
                None,
                window,
                cx,
            );
        });
    })
    .unwrap();
    drain_ready_tasks(cx);

    click(cx, window, "git-daily-to-journal");
    cx.update(|cx| {
        assert_eq!(prototype.read(cx).git.error, None);
    });
    let ws = Workspace::open(ws_dir.path()).unwrap();
    let doc = ws.read(&today, Format::Txt).unwrap();
    assert!(doc.content.contains("feat(gpui): add feature worklog item"));
}

#[test]
fn parses_only_valid_github_cli_one_time_device_codes() {
    use crate::git_view::parse_gh_device_code;

    assert_eq!(
        parse_gh_device_code("! First copy your one-time code: A1B2-C3D4"),
        Some("A1B2-C3D4".into())
    );
    assert_eq!(
        parse_gh_device_code("Press Enter to open github.com in your browser..."),
        None
    );
    assert_eq!(parse_gh_device_code("one-time code: lower-case"), None);
    assert_eq!(parse_gh_device_code("one-time code: SHORT"), None);
}
