use std::{
    io::BufRead,
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use gpui_kit::{
    Context, Entity, FontWeight, SharedString, Task, Window,
    base::{Button as FlatButton, Disableable},
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        input::{Editor, EditorState, Input, InputState},
    },
    div, prelude::*, px,
};
use workreport_core::{
    git::{self, DailyLog, Settings as GitSettings},
    git_auth,
    git_client::{
        self, Action as GitAction, Commit, CommitSource, Diff, FileStatus, History,
        Repository as GitRepoState, sanitize_diff_for_ai,
    },
    workspace::GitReportAppend,
    workspace_preview,
};

use crate::{
    Prototype,
    ai_panel::run_tokio,
    icons::PhIcon,
    theme_skin::active_palette,
    workspace_view::WorkspaceView,
};

pub(crate) trait GitBackend: Send + Sync {
    fn load_repository(&self, settings: &GitSettings, id: &str) -> Result<GitRepoState, String>;
    fn load_history(
        &self,
        settings: &GitSettings,
        id: &str,
        query: &str,
        reference: &str,
        skip: usize,
    ) -> Result<History, String>;
    fn load_diff(
        &self,
        settings: &GitSettings,
        id: &str,
        path: &str,
        staged: bool,
        commit: &str,
    ) -> Result<Diff, String>;
    fn commit_source(&self, settings: &GitSettings, id: &str) -> Result<CommitSource, String>;
    fn run_action(
        &self,
        settings: &GitSettings,
        id: &str,
        action: GitAction,
        saved_credentials: &[git_auth::Credentials],
    ) -> Result<String, String>;
    fn daily_log(&self, settings: GitSettings, date: String) -> Result<DailyLog, String>;
}

pub(crate) struct ProductionGitBackend;

impl GitBackend for ProductionGitBackend {
    fn load_repository(&self, settings: &GitSettings, id: &str) -> Result<GitRepoState, String> {
        run_tokio(git_client::repository(settings, id))?
    }

    fn load_history(
        &self,
        settings: &GitSettings,
        id: &str,
        query: &str,
        reference: &str,
        skip: usize,
    ) -> Result<History, String> {
        run_tokio(git_client::history(settings, id, query, reference, skip))?
    }

    fn load_diff(
        &self,
        settings: &GitSettings,
        id: &str,
        path: &str,
        staged: bool,
        commit: &str,
    ) -> Result<Diff, String> {
        run_tokio(git_client::diff(settings, id, path, staged, commit))?
    }

    fn commit_source(&self, settings: &GitSettings, id: &str) -> Result<CommitSource, String> {
        run_tokio(git_client::commit_source(settings, id))?
    }

    fn run_action(
        &self,
        settings: &GitSettings,
        id: &str,
        action: GitAction,
        saved_credentials: &[git_auth::Credentials],
    ) -> Result<String, String> {
        run_tokio(git_client::action_with_credentials(
            settings,
            id,
            action,
            saved_credentials,
        ))?
    }

    fn daily_log(&self, settings: GitSettings, date: String) -> Result<DailyLog, String> {
        run_tokio(git::daily_log(settings, date))?
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GitTab {
    Changes,
    Branches,
    History,
    Diff,
}

#[derive(Clone, Debug)]
pub(crate) struct DiffSelection {
    pub(crate) title: String,
    pub(crate) text: String,
    pub(crate) truncated: bool,
}

pub(crate) struct GitState {
    pub(crate) backend: Arc<dyn GitBackend>,
    pub(crate) settings: GitSettings,
    pub(crate) selected_repo_id: Option<String>,
    pub(crate) tab: GitTab,
    pub(crate) repo: Option<GitRepoState>,
    pub(crate) commits: Vec<Commit>,
    pub(crate) history_has_more: bool,
    pub(crate) history_query_input: Entity<InputState>,
    pub(crate) branch_name_input: Entity<InputState>,
    pub(crate) stash_message_input: Entity<InputState>,
    pub(crate) commit_editor: Entity<EditorState>,
    pub(crate) diff: Option<DiffSelection>,
    pub(crate) loading: bool,
    pub(crate) action_busy: bool,
    pub(crate) generating_commit: bool,
    pub(crate) appending_daily_log: bool,
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<String>,
    pub(crate) auth_prompt_origin: Option<String>,
    pub(crate) auth_pending_operation: Option<String>,
    pub(crate) auth_username_input: Entity<InputState>,
    pub(crate) auth_password_input: Entity<InputState>,
    pub(crate) saved_credentials: Vec<git_auth::Credentials>,
    pub(crate) gh_login_code: Option<String>,
    pub(crate) gh_login_status: Option<String>,
    gh_login_cancel: Option<Arc<AtomicBool>>,
    gh_login_task: Option<Task<()>>,
    load_key: Option<(usize, u64)>,
    reload_seq: u64,
    req_seq: u64,
    active_task: Option<Task<()>>,
    history_task: Option<Task<()>>,
    diff_task: Option<Task<()>>,
    commit_ai_task: Option<Task<()>>,
    commit_ai_request: Option<String>,
}

impl GitState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Prototype>) -> Self {
        let history_query_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索提交说明或哈希…"));
        let branch_name_input = cx.new(|cx| InputState::new(window, cx).placeholder("输入新分支名（例如 feat/gpui）"));
        let stash_message_input = cx.new(|cx| InputState::new(window, cx).placeholder("可选 Stash 备注信息"));
        let commit_editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
        });
        let auth_username_input = cx.new(|cx| InputState::new(window, cx).placeholder("用户名"));
        let auth_password_input = cx.new(|cx| InputState::new(window, cx).placeholder("密码 / Personal Access Token"));
        Self {
            backend: Arc::new(ProductionGitBackend),
            settings: GitSettings::default(),
            selected_repo_id: None,
            tab: GitTab::Changes,
            repo: None,
            commits: Vec::new(),
            history_has_more: false,
            history_query_input,
            branch_name_input,
            stash_message_input,
            commit_editor,
            diff: None,
            loading: false,
            action_busy: false,
            generating_commit: false,
            appending_daily_log: false,
            error: None,
            notice: None,
            auth_prompt_origin: None,
            auth_pending_operation: None,
            auth_username_input,
            auth_password_input,
            saved_credentials: Vec::new(),
            gh_login_code: None,
            gh_login_status: None,
            gh_login_cancel: None,
            gh_login_task: None,
            load_key: None,
            reload_seq: 0,
            req_seq: 0,
            active_task: None,
            history_task: None,
            diff_task: None,
            commit_ai_task: None,
            commit_ai_request: None,
        }
    }

    pub(crate) fn reset(&mut self, window: &mut Window, cx: &mut Context<Prototype>) {
        self.cancel_active_ai(ai_runtime_noop());
        self.settings = GitSettings::default();
        self.selected_repo_id = None;
        self.tab = GitTab::Changes;
        self.repo = None;
        self.commits.clear();
        self.history_has_more = false;
        self.diff = None;
        self.loading = false;
        self.action_busy = false;
        self.generating_commit = false;
        self.appending_daily_log = false;
        self.error = None;
        self.notice = None;
        self.auth_prompt_origin = None;
        self.auth_pending_operation = None;
        if let Some(cancel) = self.gh_login_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.gh_login_code = None;
        self.gh_login_status = None;
        self.gh_login_task = None;
        self.load_key = None;
        self.reload_seq += 1;
        self.active_task = None;
        self.history_task = None;
        self.diff_task = None;
        self.commit_ai_task = None;
        self.commit_editor
            .update(cx, |editor, cx| editor.set_value(String::new(), window, cx));
    }

    pub(crate) fn invalidate(&mut self) {
        self.load_key = None;
        self.reload_seq += 1;
    }

    fn cancel_active_ai(&mut self, ai_runtime: Option<&workreport_core::ai::Runtime>) {
        if let (Some(id), Some(runtime)) = (self.commit_ai_request.take(), ai_runtime) {
            runtime.cancel(&id);
        }
        self.generating_commit = false;
        self.commit_ai_task = None;
    }

    pub(crate) fn active_repo_id(&self) -> Option<String> {
        self.selected_repo_id
            .clone()
            .filter(|id| self.settings.repositories.iter().any(|repo| &repo.id == id))
            .or_else(|| {
                self.settings
                    .repositories
                    .first()
                    .map(|repo| repo.id.clone())
            })
    }
}

fn ai_runtime_noop() -> Option<&'static workreport_core::ai::Runtime> {
    None
}

pub(crate) fn format_commits_for_journal(daily_log: &DailyLog) -> (String, usize) {
    let mut parts = Vec::new();
    let mut total_commits = 0usize;
    for repo in &daily_log.repositories {
        if repo.commits.is_empty() {
            continue;
        }
        total_commits += repo.commits.len();
        let lines: Vec<String> = repo
            .commits
            .iter()
            .enumerate()
            .map(|(index, commit)| {
                let message = commit.message.trim();
                let message = if message.is_empty() {
                    "未填写提交说明"
                } else {
                    message
                };
                let short_hash: String = commit.hash.chars().take(7).collect();
                format!("  {}. [{}] {}", index + 1, short_hash, message)
            })
            .collect();
        parts.push(format!("【项目：{}】\n{}", repo.name, lines.join("\n")));
    }
    (parts.join("\n\n"), total_commits)
}

fn file_is_staged(file: &FileStatus) -> bool {
    file.index_status != " " && file.index_status != "?"
}

fn file_is_unstaged(file: &FileStatus) -> bool {
    file.worktree_status != " " || file.index_status == "?"
}

fn status_badge(file: &FileStatus) -> String {
    if file.conflicted {
        return "冲突".to_string();
    }
    if file.index_status == "?" {
        return "新增".to_string();
    }
    format!(
        "{}{}",
        if file.index_status == " " {
            "·"
        } else {
            &file.index_status
        },
        if file.worktree_status == " " {
            "·"
        } else {
            &file.worktree_status
        }
    )
}

fn action_notice(operation: &str) -> &'static str {
    match operation {
        "stage" => "已暂存变更",
        "unstage" => "已取消暂存",
        "discard" => "已丢弃工作区修改",
        "commit" => "已完成 Git 提交",
        "commitAmend" => "已修正上一次提交",
        "undoCommit" => "已撤回最近一次提交（保留改动）",
        "createBranch" => "已创建并切换分支",
        "switchBranch" => "已切换分支",
        "mergeBranch" => "已完成快进合并",
        "deleteBranch" => "已删除本地分支",
        "stash" => "已储藏工作区修改",
        "applyStash" => "已应用 Stash",
        "popStash" => "已弹出 Stash",
        "dropStash" => "已删除 Stash",
        "fetch" => "已从远端 Fetch",
        "pull" => "已完成 fast-forward Pull",
        "push" => "已推送到远端",
        _ => "Git 操作已完成",
    }
}

impl Prototype {
    pub(crate) fn ensure_git_loaded(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let key = (
            Arc::as_ptr(&workspace.reader) as usize,
            self.git.reload_seq,
        );
        if self.git.load_key == Some(key) {
            return;
        }
        self.git.load_key = Some(key);
        self.reload_git_workbench(window, cx);
    }

    pub(crate) fn reload_git_workbench(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let reader = workspace.reader.clone();
        let backend = self.git.backend.clone();
        let current_selected = self.git.selected_repo_id.clone();
        self.git.req_seq += 1;
        let seq = self.git.req_seq;
        self.git.loading = true;
        self.git.error = None;
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            let settings = reader.git_settings()?;
            let repo_id = current_selected
                .filter(|id| settings.repositories.iter().any(|item| &item.id == id))
                .or_else(|| settings.repositories.first().map(|item| item.id.clone()));
            let Some(repo_id) = repo_id else {
                return Ok((settings, None, None, None));
            };
            let repo = backend.load_repository(&settings, &repo_id)?;
            let history = backend.load_history(&settings, &repo_id, "", "", 0)?;
            Ok::<_, String>((settings, Some(repo_id), Some(repo), Some(history)))
        });
        self.git.active_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.git.req_seq != seq {
                    return;
                }
                this.git.loading = false;
                this.git.active_task = None;
                match result {
                    Ok((settings, repo_id, repo, history)) => {
                        this.git.settings = settings;
                        this.git.selected_repo_id = repo_id;
                        this.git.repo = repo;
                        if let Some(history) = history {
                            this.git.commits = history.commits;
                            this.git.history_has_more = history.has_more;
                        } else {
                            this.git.commits.clear();
                            this.git.history_has_more = false;
                        }
                    }
                    Err(error) => {
                        this.git.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn select_git_repo(
        &mut self,
        repo_id: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.git.selected_repo_id = Some(repo_id);
        self.git.diff = None;
        self.git.auth_prompt_origin = None;
        self.git.auth_pending_operation = None;
        self.reload_git_workbench(window, cx);
    }

    pub(crate) fn run_git_action(
        &mut self,
        mut action: GitAction,
        explicit_credentials: Option<git_auth::Credentials>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_id) = self.git.active_repo_id() else {
            self.git.error = Some("请先配置至少一个 Git 仓库".into());
            cx.notify();
            return;
        };
        if self.git.action_busy {
            return;
        }
        if let Some(creds) = explicit_credentials {
            if creds.save {
                self.git
                    .saved_credentials
                    .retain(|existing| existing.url != creds.url);
                self.git.saved_credentials.push(creds.clone());
            }
            action.credentials = Some(creds);
        }

        let op_name = action.operation.clone();
        let settings = self.git.settings.clone();
        let backend = self.git.backend.clone();
        let saved = self.git.saved_credentials.clone();
        self.git.req_seq += 1;
        let seq = self.git.req_seq;
        self.git.action_busy = true;
        self.git.error = None;
        self.git.notice = None;
        self.git.auth_prompt_origin = None;
        self.git.auth_pending_operation = None;
        self.status = format!("正在执行 Git {}…", op_name).into();
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            let output = backend.run_action(&settings, &repo_id, action, &saved)?;
            let repo = backend.load_repository(&settings, &repo_id)?;
            let history = backend.load_history(&settings, &repo_id, "", "", 0)?;
            Ok::<_, String>((output, repo, history))
        });
        self.git.active_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.git.req_seq != seq {
                    return;
                }
                this.git.action_busy = false;
                this.git.active_task = None;
                match result {
                    Ok((output, repo, history)) => {
                        this.git.repo = Some(repo);
                        this.git.commits = history.commits;
                        this.git.history_has_more = history.has_more;
                        if matches!(op_name.as_str(), "commit" | "commitAmend") {
                            this.git
                                .commit_editor
                                .update(cx, |editor, cx| editor.set_value(String::new(), window, cx));
                        }
                        if matches!(op_name.as_str(), "createBranch") {
                            this.git
                                .branch_name_input
                                .update(cx, |input, cx| input.set_value(String::new(), window, cx));
                        }
                        if matches!(op_name.as_str(), "stash") {
                            this.git
                                .stash_message_input
                                .update(cx, |input, cx| input.set_value(String::new(), window, cx));
                        }
                        let detail = output.trim();
                        let summary = if detail.is_empty() {
                            action_notice(&op_name).to_string()
                        } else {
                            format!("{}（{}）", action_notice(&op_name), detail)
                        };
                        this.git.notice = Some(summary.clone());
                        this.status = summary.into();
                    }
                    Err(error) => {
                        if let Some(origin) = git_auth::authentication_origin(&error) {
                            this.git.auth_prompt_origin = Some(origin);
                            this.git.auth_pending_operation = Some(op_name);
                        }
                        this.git.error = Some(error.clone());
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn submit_git_credentials(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(origin) = self.git.auth_prompt_origin.clone() else {
            return;
        };
        let Some(operation) = self.git.auth_pending_operation.clone() else {
            return;
        };
        let username = self.git.auth_username_input.read(cx).value().to_string();
        let password = self.git.auth_password_input.read(cx).value().to_string();
        self.git
            .auth_password_input
            .update(cx, |input, cx| input.set_value(String::new(), window, cx));
        let creds = git_auth::Credentials {
            url: origin,
            username,
            password,
            save: true,
        };
        self.run_git_action(
            GitAction {
                operation,
                paths: Vec::new(),
                message: String::new(),
                reference: String::new(),
                patch: String::new(),
                credentials: None,
            },
            Some(creds),
            window,
            cx,
        );
    }

    pub(crate) fn retry_with_gh_cli_credentials(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(origin) = self.git.auth_prompt_origin.clone() else {
            return;
        };
        let Some(operation) = self.git.auth_pending_operation.clone() else {
            return;
        };
        let task = cx.background_executor().spawn(async move {
            let token_out = std::process::Command::new("gh")
                .args(["auth", "token"])
                .output()
                .map_err(|_| "未检测到 GitHub CLI (gh)，请使用上方用户名与 Personal Access Token".to_string())?;
            if !token_out.status.success() {
                return Err("GitHub CLI 尚未登录，请先在终端运行 `gh auth login` 或使用 Token 登录".into());
            }
            let token = String::from_utf8_lossy(&token_out.stdout).trim().to_string();
            if token.is_empty() {
                return Err("GitHub CLI 未返回有效授权令牌".into());
            }
            Ok::<_, String>(git_auth::Credentials {
                url: origin,
                username: "x-access-token".into(),
                password: token,
                save: true,
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(creds) => {
                    this.run_git_action(
                        GitAction {
                            operation,
                            paths: Vec::new(),
                            message: String::new(),
                            reference: String::new(),
                            patch: String::new(),
                            credentials: None,
                        },
                        Some(creds),
                        window,
                        cx,
                    );
                }
                Err(error) => {
                    this.git.error = Some(error.clone());
                    this.status = error.into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn cancel_gh_device_login(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.git.gh_login_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.git.gh_login_task = None;
        self.git.gh_login_code = None;
        self.git.gh_login_status = Some("已取消 GitHub 网页授权登录。".into());
        cx.notify();
    }

    pub(crate) fn open_github_device_page(&mut self, cx: &mut Context<Self>) {
        let url = "https://github.com/login/device";
        let opened = if cfg!(target_os = "macos") {
            Command::new("open").arg(url).spawn()
        } else if cfg!(target_os = "windows") {
            Command::new("cmd").args(["/c", "start", "", url]).spawn()
        } else {
            Command::new("xdg-open").arg(url).spawn()
        };
        if let Err(error) = opened {
            self.git.error = Some(format!("无法自动打开浏览器，请手动访问 {url}：{error}"));
            cx.notify();
        }
    }

    pub(crate) fn start_gh_device_login(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.git.gh_login_cancel.is_some() {
            return;
        }
        let Some(origin) = self.git.auth_prompt_origin.clone() else {
            return;
        };
        let Some(operation) = self.git.auth_pending_operation.clone() else {
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_bg = cancel.clone();
        let observed_code: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let code_bg = observed_code.clone();
        let outcome_slot: Arc<Mutex<Option<Result<git_auth::Credentials, String>>>> =
            Arc::new(Mutex::new(None));
        let outcome_bg = outcome_slot.clone();

        self.git.gh_login_cancel = Some(cancel);
        self.git.gh_login_code = None;
        self.git.gh_login_status = Some("正在通过 GitHub CLI 获取一次性设备授权码…".into());
        self.git.error = None;
        cx.notify();

        cx.background_executor()
            .spawn(async move {
                let res = (|| -> Result<git_auth::Credentials, String> {
                    let dir = tempfile::tempdir()
                        .map_err(|_| "无法创建 GitHub CLI 临时工作目录".to_string())?;
                    let mut cmd = Command::new("gh");
                    cmd.current_dir(dir.path())
                        .args([
                            "auth",
                            "login",
                            "--hostname",
                            "github.com",
                            "--web",
                            "--skip-ssh-key",
                        ])
                        .env("GH_PROMPT_DISABLED", "1")
                        .env("NO_COLOR", "1")
                        .env("CLICOLOR", "0")
                        .env("GH_PAGER", "cat")
                        .stdin(Stdio::null())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped());
                    let mut child = cmd
                        .spawn()
                        .map_err(|_| "未检测到 GitHub CLI (gh)，请安装后重试或使用 Token".to_string())?;

                    if let Some(stdout) = child.stdout.take() {
                        let slot = code_bg.clone();
                        std::thread::spawn(move || {
                            let reader = std::io::BufReader::new(stdout);
                            for line in reader.lines().map_while(Result::ok) {
                                if let Some(code) = parse_gh_device_code(&line) {
                                    if let Ok(mut guard) = slot.lock() {
                                        if guard.is_none() {
                                            *guard = Some(code);
                                        }
                                    }
                                }
                            }
                        });
                    }
                    if let Some(stderr) = child.stderr.take() {
                        let slot = code_bg.clone();
                        std::thread::spawn(move || {
                            let reader = std::io::BufReader::new(stderr);
                            for line in reader.lines().map_while(Result::ok) {
                                if let Some(code) = parse_gh_device_code(&line) {
                                    if let Ok(mut guard) = slot.lock() {
                                        if guard.is_none() {
                                            *guard = Some(code);
                                        }
                                    }
                                }
                            }
                        });
                    }

                    let deadline =
                        std::time::Instant::now() + std::time::Duration::from_secs(600);
                    let mut opened_browser = false;
                    loop {
                        if cancel_bg.load(Ordering::Relaxed) {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err("已取消 GitHub 网页授权。".to_string());
                        }
                        if !opened_browser {
                            if let Ok(guard) = code_bg.lock() {
                                if guard.is_some() {
                                    opened_browser = true;
                                    #[cfg(target_os = "macos")]
                                    let _ = Command::new("open")
                                        .arg("https://github.com/login/device")
                                        .spawn();
                                    #[cfg(target_os = "windows")]
                                    let _ = Command::new("cmd")
                                        .args(["/c", "start", "", "https://github.com/login/device"])
                                        .spawn();
                                    #[cfg(all(unix, not(target_os = "macos")))]
                                    let _ = Command::new("xdg-open")
                                        .arg("https://github.com/login/device")
                                        .spawn();
                                }
                            }
                        }
                        if std::time::Instant::now() >= deadline {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err("GitHub 网页授权等待超时，请重试。".to_string());
                        }
                        match child.try_wait() {
                            Ok(Some(status)) if status.success() => break,
                            Ok(Some(_)) => {
                                return Err("GitHub 网页授权未完成或被拒绝，请重试。".to_string());
                            }
                            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(200)),
                            Err(_) => return Err("无法检查 GitHub CLI 运行状态。".to_string()),
                        }
                    }

                    let token_out = Command::new("gh")
                        .args(["auth", "token"])
                        .output()
                        .map_err(|_| "GitHub 授权成功但读取令牌失败。".to_string())?;
                    if !token_out.status.success() {
                        return Err("GitHub 授权完成但未能读取访问令牌，请重试。".into());
                    }
                    let token = String::from_utf8_lossy(&token_out.stdout).trim().to_string();
                    if token.is_empty() {
                        return Err("GitHub CLI 未返回有效令牌。".into());
                    }
                    Ok(git_auth::Credentials {
                        url: origin,
                        username: "x-access-token".into(),
                        password: token,
                        save: true,
                    })
                })();
                if let Ok(mut guard) = outcome_bg.lock() {
                    *guard = Some(res);
                }
            })
            .detach();

        let spawned = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(180))
                    .await;
                let code_now = observed_code.lock().ok().and_then(|g| g.clone());
                if let Some(code) = code_now {
                    let _ = this.update(cx, |this, cx| {
                        if this.git.gh_login_code.as_deref() != Some(code.as_str()) {
                            this.git.gh_login_code = Some(code);
                            this.git.gh_login_status =
                                Some("请在浏览器输入上方授权码并确认授权…".into());
                            cx.notify();
                        }
                    });
                }
                let finished = outcome_slot.lock().ok().and_then(|mut g| g.take());
                if let Some(result) = finished {
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.git.gh_login_cancel = None;
                        this.git.gh_login_task = None;
                        match result {
                            Ok(creds) => {
                                this.git.gh_login_code = None;
                                this.git.gh_login_status =
                                    Some("GitHub 授权完成，正在重试操作…".into());
                                this.run_git_action(
                                    GitAction {
                                        operation,
                                        paths: Vec::new(),
                                        message: String::new(),
                                        reference: String::new(),
                                        patch: String::new(),
                                        credentials: None,
                                    },
                                    Some(creds),
                                    window,
                                    cx,
                                );
                            }
                            Err(error) => {
                                this.git.gh_login_code = None;
                                this.git.gh_login_status = None;
                                this.git.error = Some(error);
                                cx.notify();
                            }
                        }
                    });
                    break;
                }
            }
        });
        self.git.gh_login_task = Some(spawned);
    }

    pub(crate) fn load_git_history(
        &mut self,
        append: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_id) = self.git.active_repo_id() else {
            return;
        };
        let query = self.git.history_query_input.read(cx).value().to_string();
        let skip = if append { self.git.commits.len() } else { 0 };
        let settings = self.git.settings.clone();
        let backend = self.git.backend.clone();
        self.git.loading = true;
        self.git.error = None;
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { backend.load_history(&settings, &repo_id, &query, "", skip) });
        self.git.history_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.git.loading = false;
                this.git.history_task = None;
                match result {
                    Ok(history) => {
                        if append {
                            this.git.commits.extend(history.commits);
                        } else {
                            this.git.commits = history.commits;
                        }
                        this.git.history_has_more = history.has_more;
                    }
                    Err(error) => {
                        this.git.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn open_git_diff(
        &mut self,
        title: String,
        path: String,
        staged: bool,
        commit: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_id) = self.git.active_repo_id() else {
            return;
        };
        let settings = self.git.settings.clone();
        let backend = self.git.backend.clone();
        self.git.error = None;
        self.git.tab = GitTab::Diff;
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { backend.load_diff(&settings, &repo_id, &path, staged, &commit) });
        self.git.diff_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.git.diff_task = None;
                match result {
                    Ok(diff) => {
                        this.git.diff = Some(DiffSelection {
                            title,
                            text: diff.text,
                            truncated: diff.truncated,
                        });
                    }
                    Err(error) => {
                        this.git.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn generate_git_commit_message(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_id) = self.git.active_repo_id() else {
            return;
        };
        if self.git.generating_commit {
            return;
        }
        let settings = self.git.settings.clone();
        let ai_settings = self.ai.settings.clone();
        let git_backend = self.git.backend.clone();
        let ai_backend = self.ai.backend.clone();
        let ai_runtime = self.ai.runtime.clone();
        self.git.req_seq += 1;
        let request_id = format!("git-commit-{}", self.git.req_seq);
        let req_copy = request_id.clone();
        self.git.cancel_active_ai(Some(&self.ai.runtime));
        self.git.generating_commit = true;
        self.git.commit_ai_request = Some(request_id.clone());
        self.git.error = None;
        self.status = "正在根据已暂存变更生成 Git 提交说明…".into();
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            let source = git_backend.commit_source(&settings, &repo_id)?;
            if source.content.trim().is_empty() {
                return Err("请先暂存修改，再生成提交说明".into());
            }
            let (sanitized, _) = sanitize_diff_for_ai(&source.content);
            let content = if sanitized.trim().is_empty() {
                source.content
            } else {
                sanitized
            };
            ai_backend.git_commit_message(
                &ai_settings,
                &content,
                &settings.commit_prompt,
                &req_copy,
                &ai_runtime,
            )
        });
        self.git.commit_ai_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.git.commit_ai_request.as_deref() != Some(request_id.as_str()) {
                    return;
                }
                this.git.generating_commit = false;
                this.git.commit_ai_request = None;
                this.git.commit_ai_task = None;
                match result {
                    Ok(message) => {
                        this.git
                            .commit_editor
                            .update(cx, |editor, cx| editor.set_value(message.trim().to_string(), window, cx));
                        this.git.notice = Some("已生成 AI 提交说明，请核对后提交".into());
                        this.status = "已生成 AI 提交说明".into();
                    }
                    Err(error) => {
                        this.git.error = Some(error.clone());
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn append_git_daily_log_to_journal(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        if self.git.appending_daily_log {
            return;
        }
        let date = self.calendar.selected_range().0.format("%Y%m%d").to_string();
        let expected_path = workspace.info_path();
        let root_path = workspace.reader.root().to_path_buf();
        let settings = self.git.settings.clone();
        let backend = self.git.backend.clone();
        self.git.appending_daily_log = true;
        self.git.error = None;
        self.git.notice = None;
        self.status = format!("正在收集 {date} 的 Git 提交记录…").into();
        cx.notify();

        let date_copy = date.clone();
        let task = cx.background_executor().spawn(async move {
            let log = backend.daily_log(settings, date_copy.clone())?;
            let (formatted, count) = format_commits_for_journal(&log);
            if count == 0 || formatted.trim().is_empty() {
                return Err(format!("{date_copy} 暂无匹配的 Git 提交记录"));
            }
            let append_content = format!("### Git 提交记录\n{formatted}");
            let result = workspace_preview::append_git_reports_at(
                Path::new(&root_path),
                &expected_path,
                vec![GitReportAppend {
                    date: date_copy,
                    content: append_content,
                }],
            )?;
            Ok::<_, String>((count, result.documents.len(), result.skipped_dates))
        });
        self.git.active_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.git.appending_daily_log = false;
                this.git.active_task = None;
                match result {
                    Ok((count, updated, skipped)) => {
                        let msg = if updated > 0 {
                            format!("已将 {count} 条 Git 提交追加到 {date} 日报")
                        } else if !skipped.is_empty() {
                            format!("提交记录已在 {date} 日报中（跳过重复追加）")
                        } else {
                            format!("已处理 {count} 条 Git 提交")
                        };
                        this.git.notice = Some(msg.clone());
                        this.status = msg.into();
                        this.refresh_workspace_after_external_write(&date, window, cx);
                    }
                    Err(error) => {
                        this.git.error = Some(error.clone());
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn refresh_workspace_after_external_write(
        &mut self,
        date: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &mut self.workspace else {
            return;
        };
        let format = workspace
            .entries
            .iter()
            .find(|entry| entry.date == date)
            .map(|entry| entry.format)
            .unwrap_or(self.new_journal_format);
        let _ = workspace.reload_after_external_change(Some((date, format)));
        self.overview.invalidate();
        self.directory.invalidate_tree();
        self.excel.invalidate_exports();
        self.calendar_details.invalidate_previews();
        if let Some(open_edit) = &self.workspace_edit {
            if open_edit.date() == date {
                self.workspace_edit = None;
                self.open_journal(date.to_string(), format, window, cx);
            }
        }
    }
}

#[inline(never)]
pub(crate) fn render_git_module(
    app: &Prototype,
    workspace: &WorkspaceView,
    busy: bool,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    let git = &app.git;
    let palette = active_palette(cx);
    let theme = cx.theme();
    let border = palette.border;
    let muted = palette.text_muted;
    let danger = palette.danger;
    let success = palette.success;
    let accent = palette.accent;
    let secondary = theme.secondary;

    let repos = &git.settings.repositories;
    let active_id = git.active_repo_id();
    let active_date = app.calendar.selected_range().0.format("%Y-%m-%d").to_string();
    let _ = workspace;

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .children(repos.iter().map(|repo| {
                    let repo_id = repo.id.clone();
                    let active = active_id.as_deref() == Some(repo.id.as_str());
                    let disabled = busy || git.loading || git.action_busy;
                    FlatButton::new(format!("git-repo-{}", repo.id))
                        .disabled(disabled)
                        .h(px(26.))
                        .px_2p5()
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .bg(if active {
                            palette.accent_soft
                        } else {
                            palette.subtle.opacity(0.5)
                        })
                        .border_1()
                        .border_color(if active {
                            palette.accent
                        } else {
                            palette.border
                        })
                        .text_size(px(11.))
                        .font_weight(if active {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .text_color(if active {
                            palette.accent
                        } else {
                            palette.text_body
                        })
                        .hover(move |style| style.bg(palette.subtle_strong).text_color(palette.text))
                        .child(PhIcon::GitBranch.view(
                            12.,
                            if active {
                                palette.accent
                            } else {
                                palette.text_muted
                            },
                        ))
                        .child(repo.name.clone())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_git_repo(repo_id.clone(), window, cx);
                        }))
                }))
                .child(div().flex_1())
                .child({
                    let disabled = busy || repos.is_empty() || git.appending_daily_log;
                    FlatButton::new("git-daily-to-journal")
                        .disabled(disabled)
                        .accessibility_label(format!("收集 {active_date} 的 Git 提交并追加到日报"))
                        .h(px(26.))
                        .px_2p5()
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .bg(palette.accent_soft)
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if disabled {
                            palette.text_muted
                        } else {
                            palette.accent
                        })
                        .hover(move |style| {
                            style
                                .bg(palette.accent_fill)
                                .text_color(palette.on_accent)
                        })
                        .child(PhIcon::FileText.view(12., palette.accent))
                        .child(if git.appending_daily_log {
                            "汇总中…"
                        } else {
                            "提交转日报"
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.append_git_daily_log_to_journal(window, cx);
                        }))
                })
                .child(
                    FlatButton::new("git-refresh")
                        .disabled(busy || git.loading || git.action_busy)
                        .accessibility_label("刷新 Git 状态")
                        .w(px(26.))
                        .h(px(26.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |style| style.bg(palette.subtle_strong))
                        .child(PhIcon::ArrowClockwise.view(13., palette.text_muted))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.reload_git_workbench(window, cx);
                        })),
                )
                .child(
                    FlatButton::new("git-open-settings")
                        .disabled(busy)
                        .accessibility_label("Git 仓库设置")
                        .w(px(26.))
                        .h(px(26.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |style| style.bg(palette.subtle_strong))
                        .child(PhIcon::GearSix.view(13., palette.text_muted))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_settings_section(
                                crate::settings_view::SettingsSection::Git,
                                window,
                                cx,
                            );
                        })),
                ),
        )
        .when(repos.is_empty(), |panel| {
            panel.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .items_center()
                    .gap_2()
                    .border_1()
                    .border_color(border)
                    .rounded_md()
                    .p_4()
                    .child(div().text_sm().child("尚未配置 Git 仓库"))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(muted)
                            .child("添加本地项目 Git 仓库后，可在此直接暂存、提交、查看 Diff 与拉取推送。"),
                    )
                    .child(
                        Button::new("git-empty-configure")
                            .label("添加 Git 仓库")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_settings_section(
                                    crate::settings_view::SettingsSection::Git,
                                    window,
                                    cx,
                                );
                            })),
                    ),
            )
        })
        .when(!repos.is_empty(), |panel| {
            panel
                .child(render_git_tabs(git, busy, cx))
                .when_some(git.error.clone(), |panel, error| {
                    panel.child(
                        div()
                            .p_2()
                            .rounded_sm()
                            .border_1()
                            .border_color(danger)
                            .text_size(px(12.))
                            .text_color(danger)
                            .child(error),
                    )
                })
                .when_some(git.notice.clone(), |panel, notice| {
                    panel.child(
                        div()
                            .p_2()
                            .rounded_sm()
                            .border_1()
                            .border_color(success)
                            .text_size(px(12.))
                            .text_color(success)
                            .child(notice),
                    )
                })
                .when_some(git.auth_prompt_origin.clone(), |panel, origin| {
                    panel.child(
                        div()
                            .p_2()
                            .rounded_sm()
                            .border_1()
                            .border_color(accent)
                            .bg(secondary)
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .child(format!("需要验证远程仓库凭据：{origin}")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        Input::new(&git.auth_username_input)
                                            .aria_label("Git 用户名")
                                            .flex_1(),
                                    )
                                    .child(
                                        Input::new(&git.auth_password_input)
                                            .aria_label("Git 密码或 Token")
                                            .flex_1(),
                                    )
                                    .child(
                                        Button::new("git-auth-submit")
                                            .label("确认重试")
                                            .primary()
                                            .disabled(git.action_busy)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.submit_git_credentials(window, cx);
                                            })),
                                    )
                                    .when(origin.contains("github.com"), |row| {
                                        row.child(
                                            Button::new("git-auth-gh-cli")
                                                .label("gh 现有令牌")
                                                .disabled(git.action_busy || git.gh_login_cancel.is_some())
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.retry_with_gh_cli_credentials(window, cx);
                                                })),
                                        )
                                        .child(
                                            Button::new("git-auth-gh-web")
                                                .label(if git.gh_login_cancel.is_some() {
                                                    "等待网页授权…"
                                                } else {
                                                    "网页登录 GitHub"
                                                })
                                                .disabled(git.action_busy || git.gh_login_cancel.is_some())
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.start_gh_device_login(window, cx);
                                                })),
                                        )
                                    }),
                            )
                            .when(
                                git.gh_login_code.is_some() || git.gh_login_status.is_some(),
                                |col| {
                                    col.child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap_2()
                                            .when_some(git.gh_login_code.clone(), |row, code| {
                                                row.child(
                                                    div()
                                                        .px_2()
                                                        .py_0p5()
                                                        .rounded_sm()
                                                        .bg(cx.theme().secondary)
                                                        .text_size(px(12.))
                                                        .child(format!("授权码：{code}")),
                                                )
                                                .child(
                                                    Button::new("git-auth-gh-open-page")
                                                        .label("打开授权页")
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.open_github_device_page(cx);
                                                        })),
                                                )
                                            })
                                            .when_some(git.gh_login_status.clone(), |row, msg| {
                                                row.child(
                                                    div()
                                                        .text_size(px(11.))
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(msg),
                                                )
                                            })
                                            .when(git.gh_login_cancel.is_some(), |row| {
                                                row.child(
                                                    Button::new("git-auth-gh-cancel")
                                                        .label("取消授权")
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.cancel_gh_device_login(cx);
                                                        })),
                                                )
                                            }),
                                    )
                                },
                            ),
                    )
                })
                .child(match git.tab {
                    GitTab::Changes => render_git_changes_tab(git, busy, cx).into_any_element(),
                    GitTab::Branches => render_git_branches_tab(git, busy, cx).into_any_element(),
                    GitTab::History => render_git_history_tab(git, busy, cx).into_any_element(),
                    GitTab::Diff => render_git_diff_tab(git, cx).into_any_element(),
                })
        })
        .into_any_element()
}

fn render_git_tabs(
    git: &GitState,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let palette = active_palette(cx);
    let tabs = [
        (GitTab::Changes, "变更与提交"),
        (GitTab::Branches, "分支与 Stash"),
        (GitTab::History, "提交历史"),
        (GitTab::Diff, "Diff 预览"),
    ];
    let branch_info = git.repo.as_ref().map(|repo| {
        let mut label = if repo.detached {
            format!("HEAD detached @ {}", repo.branch)
        } else {
            repo.branch.clone()
        };
        if let Some(upstream) = &repo.upstream {
            label.push_str(&format!(" -> {upstream} (↑{} ↓{})", repo.ahead, repo.behind));
        }
        label
    });

    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_1()
        .child(
            // .island-tabs pill bar
            div()
                .flex()
                .items_center()
                .gap(px(2.))
                .p(px(2.))
                .rounded(px(7.))
                .bg(palette.subtle)
                .children(tabs.into_iter().map(|(tab, label)| {
                    let active = git.tab == tab;
                    FlatButton::new(format!("git-tab-{label}"))
                        .disabled(busy)
                        .h(px(22.))
                        .px_2p5()
                        .rounded(px(5.))
                        .flex()
                        .items_center()
                        .bg(if active {
                            palette.surface
                        } else {
                            palette.subtle.opacity(0.01)
                        })
                        .text_size(px(11.))
                        .font_weight(if active {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .text_color(if active {
                            palette.text
                        } else {
                            palette.text_body
                        })
                        .hover(move |style| style.text_color(palette.text))
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.git.tab = tab;
                            cx.notify();
                        }))
                })),
        )
        .when_some(branch_info, |row, info| {
            row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .ml_2()
                    .text_size(px(11.))
                    .text_color(palette.text_muted)
                    .child(PhIcon::GitBranch.view(12., palette.text_muted))
                    .child(info),
            )
        })
}

fn render_git_changes_tab(
    git: &GitState,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    let files = git
        .repo
        .as_ref()
        .map(|repo| repo.files.as_slice())
        .unwrap_or(&[]);
    let staged_paths: Vec<String> = files
        .iter()
        .filter(|file| file_is_staged(file))
        .map(|file| file.path.clone())
        .collect();
    let unstaged_paths: Vec<String> = files
        .iter()
        .filter(|file| file_is_unstaged(file))
        .map(|file| file.path.clone())
        .collect();
    let all_staged = staged_paths.clone();
    let all_unstaged = unstaged_paths.clone();

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_1()
                .child(
                    Button::new("git-fetch")
                        .label("Fetch")
                        .disabled(busy || git.action_busy)
                        .px_2()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run_git_action(simple_action("fetch"), None, window, cx);
                        })),
                )
                .child(
                    Button::new("git-pull")
                        .label("Pull (ff-only)")
                        .disabled(busy || git.action_busy)
                        .px_2()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run_git_action(simple_action("pull"), None, window, cx);
                        })),
                )
                .child(
                    Button::new("git-push")
                        .label("Push")
                        .disabled(busy || git.action_busy)
                        .px_2()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run_git_action(simple_action("push"), None, window, cx);
                        })),
                )
                .child(div().flex_1())
                .child(
                    Button::new("git-stage-all")
                        .label("全部暂存")
                        .disabled(busy || git.action_busy || all_unstaged.is_empty())
                        .px_2()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_git_action(
                                path_action("stage", all_unstaged.clone()),
                                None,
                                window,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("git-unstage-all")
                        .label("全部取消暂存")
                        .disabled(busy || git.action_busy || all_staged.is_empty())
                        .px_2()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_git_action(
                                path_action("unstage", all_staged.clone()),
                                None,
                                window,
                                cx,
                            );
                        })),
                ),
        )
        .child(
            div()
                .id("git-files-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .border_1()
                .border_color(border)
                .rounded_sm()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .when(files.is_empty(), |list| {
                    list.child(
                        div()
                            .text_size(px(12.))
                            .text_color(muted)
                            .child("工作区干净，暂无未提交变更"),
                    )
                })
                .children(files.iter().map(|file| {
                    let path_for_stage = file.path.clone();
                    let path_for_unstage = file.path.clone();
                    let path_for_discard = file.path.clone();
                    let path_for_diff = file.path.clone();
                    let staged = file_is_staged(file);
                    let unstaged = file_is_unstaged(file);
                    let can_discard = unstaged && file.index_status != "?" && !file.conflicted;
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_1()
                        .py(px(2.))
                        .rounded_sm()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(muted)
                                .w(px(36.))
                                .child(status_badge(file)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.))
                                .child(file.path.clone()),
                        )
                        .child(
                            Button::new(format!("git-diff-file-{}", file.path))
                                .label("Diff")
                                .disabled(busy)
                                .px_1()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_git_diff(
                                        format!("工作区差异：{}", path_for_diff),
                                        path_for_diff.clone(),
                                        staged && !unstaged,
                                        String::new(),
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .when(unstaged, |row| {
                            row.child(
                                Button::new(format!("git-stage-{}", file.path))
                                    .label("+")
                                    .tooltip("暂存该文件")
                                    .disabled(busy || git.action_busy)
                                    .px_1()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.run_git_action(
                                            path_action("stage", vec![path_for_stage.clone()]),
                                            None,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                        })
                        .when(staged, |row| {
                            row.child(
                                Button::new(format!("git-unstage-{}", file.path))
                                    .label("-")
                                    .tooltip("取消暂存")
                                    .disabled(busy || git.action_busy)
                                    .px_1()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.run_git_action(
                                            path_action("unstage", vec![path_for_unstage.clone()]),
                                            None,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                        })
                        .when(can_discard, |row| {
                            row.child(
                                Button::new(format!("git-discard-{}", file.path))
                                    .label("↩")
                                    .tooltip("丢弃工作区修改")
                                    .disabled(busy || git.action_busy)
                                    .px_1()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.run_git_action(
                                            path_action("discard", vec![path_for_discard.clone()]),
                                            None,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                        })
                })),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    Editor::new(&git.commit_editor)
                        .aria_label("Git 提交说明")
                        .disabled(busy || git.action_busy)
                        .h(px(64.))
                        .w_full(),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .child(
                            Button::new("git-commit-submit")
                                .label("提交 (Commit)")
                                .primary()
                                .disabled(busy || git.action_busy || staged_paths.is_empty())
                                .px_2()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let message = this.git.commit_editor.read(cx).value().to_string();
                                    this.run_git_action(
                                        GitAction {
                                            operation: "commit".into(),
                                            paths: Vec::new(),
                                            message,
                                            reference: String::new(),
                                            patch: String::new(),
                                            credentials: None,
                                        },
                                        None,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("git-commit-amend")
                                .label("修正上次")
                                .disabled(busy || git.action_busy)
                                .px_2()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let message = this.git.commit_editor.read(cx).value().to_string();
                                    this.run_git_action(
                                        GitAction {
                                            operation: "commitAmend".into(),
                                            paths: Vec::new(),
                                            message,
                                            reference: String::new(),
                                            patch: String::new(),
                                            credentials: None,
                                        },
                                        None,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("git-commit-undo")
                                .label("撤回上次")
                                .disabled(busy || git.action_busy)
                                .px_2()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.run_git_action(simple_action("undoCommit"), None, window, cx);
                                })),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("git-commit-ai")
                                .child(crate::icons::PhIcon::Sparkle.view(12., cx.theme().primary))
                                .label(if git.generating_commit {
                                    "AI 生成中…"
                                } else {
                                    "AI 生成提交说明"
                                })
                                .disabled(busy || git.action_busy || git.generating_commit)
                                .px_2()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.generate_git_commit_message(window, cx);
                                })),
                        ),
                ),
        )
}

fn render_git_branches_tab(
    git: &GitState,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    let branches = git
        .repo
        .as_ref()
        .map(|repo| repo.branches.as_slice())
        .unwrap_or(&[]);
    let stashes = git
        .repo
        .as_ref()
        .map(|repo| repo.stashes.as_slice())
        .unwrap_or(&[]);

    div()
        .id("git-branches-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .gap_3()
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(
                    Input::new(&git.branch_name_input)
                        .aria_label("新分支名称")
                        .flex_1(),
                )
                .child(
                    Button::new("git-create-branch")
                        .label("创建并切换")
                        .primary()
                        .disabled(busy || git.action_busy)
                        .px_2()
                        .on_click(cx.listener(|this, _, window, cx| {
                            let name = this.git.branch_name_input.read(cx).value().trim().to_string();
                            this.run_git_action(
                                ref_action("createBranch", name),
                                None,
                                window,
                                cx,
                            );
                        })),
                ),
        )
        .child(
            div()
                .border_1()
                .border_color(border)
                .rounded_sm()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_size(px(12.)).text_color(muted).child("分支列表"))
                .children(branches.iter().map(|branch| {
                    let switch_ref = branch.name.clone();
                    let merge_ref = branch.name.clone();
                    let delete_ref = branch.name.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.))
                                .child(format!(
                                    "{}{}{}",
                                    if branch.current { "* " } else { "" },
                                    branch.name,
                                    if branch.remote { " (remote)" } else { "" }
                                )),
                        )
                        .when(!branch.current, |row| {
                            row.child(
                                Button::new(format!("git-switch-{}", branch.name))
                                    .label("切换")
                                    .disabled(busy || git.action_busy)
                                    .px_2()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.run_git_action(
                                            ref_action("switchBranch", switch_ref.clone()),
                                            None,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                Button::new(format!("git-merge-{}", branch.name))
                                    .label("快进合并")
                                    .disabled(busy || git.action_busy)
                                    .px_2()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.run_git_action(
                                            ref_action("mergeBranch", merge_ref.clone()),
                                            None,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                        })
                        .when(!branch.current && !branch.remote, |row| {
                            row.child(
                                Button::new(format!("git-delete-branch-{}", branch.name))
                                    .label("删除")
                                    .disabled(busy || git.action_busy)
                                    .px_2()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.run_git_action(
                                            ref_action("deleteBranch", delete_ref.clone()),
                                            None,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                        })
                })),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(
                    Input::new(&git.stash_message_input)
                        .aria_label("Stash 备注")
                        .flex_1(),
                )
                .child(
                    Button::new("git-stash-save")
                        .label("储藏修改")
                        .disabled(busy || git.action_busy)
                        .px_2()
                        .on_click(cx.listener(|this, _, window, cx| {
                            let msg = this.git.stash_message_input.read(cx).value().trim().to_string();
                            this.run_git_action(
                                GitAction {
                                    operation: "stash".into(),
                                    paths: Vec::new(),
                                    message: msg,
                                    reference: String::new(),
                                    patch: String::new(),
                                    credentials: None,
                                },
                                None,
                                window,
                                cx,
                            );
                        })),
                ),
        )
        .child(
            div()
                .border_1()
                .border_color(border)
                .rounded_sm()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_size(px(12.)).text_color(muted).child("Stash 列表"))
                .when(stashes.is_empty(), |box_el| {
                    box_el.child(div().text_size(px(12.)).text_color(muted).child("暂无 Stash 记录"))
                })
                .children(stashes.iter().map(|stash| {
                    let apply_ref = stash.reference.clone();
                    let pop_ref = stash.reference.clone();
                    let drop_ref = stash.reference.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.))
                                .child(format!("{} · {}", stash.reference, stash.message)),
                        )
                        .child(
                            Button::new(format!("git-stash-apply-{}", stash.reference))
                                .label("应用")
                                .disabled(busy || git.action_busy)
                                .px_2()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.run_git_action(
                                        ref_action("applyStash", apply_ref.clone()),
                                        None,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new(format!("git-stash-pop-{}", stash.reference))
                                .label("弹出")
                                .disabled(busy || git.action_busy)
                                .px_2()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.run_git_action(
                                        ref_action("popStash", pop_ref.clone()),
                                        None,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new(format!("git-stash-drop-{}", stash.reference))
                                .label("删除")
                                .disabled(busy || git.action_busy)
                                .px_2()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.run_git_action(
                                        ref_action("dropStash", drop_ref.clone()),
                                        None,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                })),
        )
}

fn render_git_history_tab(
    git: &GitState,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_2()
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(
                    Input::new(&git.history_query_input)
                        .aria_label("搜索提交记录")
                        .flex_1(),
                )
                .child(
                    Button::new("git-history-search")
                        .label("搜索")
                        .disabled(busy || git.loading)
                        .px_2()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.load_git_history(false, window, cx);
                        })),
                ),
        )
        .child(
            div()
                .id("git-history-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .border_1()
                .border_color(border)
                .rounded_sm()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .when(git.commits.is_empty(), |list| {
                    list.child(div().text_size(px(12.)).text_color(muted).child("暂无提交记录"))
                })
                .children(git.commits.iter().map(|commit| {
                    let short: String = commit.hash.chars().take(7).collect();
                    let hash = commit.hash.clone();
                    let title = format!("提交 {short} · {}", commit.message);
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .py(px(2.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(div().text_size(px(11.)).text_color(muted).child(short))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(12.))
                                        .child(commit.message.clone()),
                                )
                                .child(
                                    Button::new(format!("git-commit-diff-{}", commit.hash))
                                        .label("Diff")
                                        .flex_shrink_0()
                                        .px_1()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_git_diff(
                                                title.clone(),
                                                String::new(),
                                                false,
                                                hash.clone(),
                                                window,
                                                cx,
                                            );
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(muted)
                                .truncate()
                                .child(format!("{} · {}", commit.author, commit.committed_at)),
                        )
                }))
                .when(git.history_has_more, |list| {
                    list.child(
                        Button::new("git-history-more")
                            .label("加载更多提交")
                            .disabled(busy || git.loading)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.load_git_history(true, window, cx);
                            })),
                    )
                }),
        )
}

fn render_git_diff_tab(git: &GitState, cx: &Context<Prototype>) -> impl IntoElement {
    let theme = cx.theme();
    let border = theme.border;
    let muted = theme.muted_foreground;
    let success = theme.success_foreground;
    let danger = theme.danger_foreground;
    let accent = theme.accent;

    let Some(diff) = &git.diff else {
        return div()
            .flex_1()
            .min_h_0()
            .flex()
            .justify_center()
            .items_center()
            .border_1()
            .border_color(border)
            .rounded_sm()
            .text_size(px(12.))
            .text_color(muted)
            .child("点击变更文件或提交记录旁的「Diff」即可在此查看差异")
            .into_any_element();
    };

    let lines: Vec<SharedString> = diff
        .text
        .lines()
        .take(800)
        .map(|line| SharedString::from(line.to_string()))
        .collect();

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).child(diff.title.clone()))
                .when(diff.truncated, |bar| {
                    bar.child(
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child("（Diff 内容过大，已截断显示）"),
                    )
                }),
        )
        .child(
            div()
                .id("git-diff-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .border_1()
                .border_color(border)
                .rounded_sm()
                .p_2()
                .flex()
                .flex_col()
                .children(lines.into_iter().map(|line| {
                    let color = if line.starts_with('+') && !line.starts_with("+++") {
                        success
                    } else if line.starts_with('-') && !line.starts_with("---") {
                        danger
                    } else if line.starts_with("@@") {
                        accent
                    } else {
                        muted
                    };
                    div().text_size(px(11.)).text_color(color).child(line)
                })),
        )
        .into_any_element()
}

fn simple_action(operation: &str) -> GitAction {
    GitAction {
        operation: operation.into(),
        paths: Vec::new(),
        message: String::new(),
        reference: String::new(),
        patch: String::new(),
        credentials: None,
    }
}

fn path_action(operation: &str, paths: Vec<String>) -> GitAction {
    GitAction {
        operation: operation.into(),
        paths,
        message: String::new(),
        reference: String::new(),
        patch: String::new(),
        credentials: None,
    }
}

fn ref_action(operation: &str, reference: String) -> GitAction {
    GitAction {
        operation: operation.into(),
        paths: Vec::new(),
        message: String::new(),
        reference,
        patch: String::new(),
        credentials: None,
    }
}

pub(crate) fn parse_gh_device_code(line: &str) -> Option<String> {
    let tail = line.split_once("one-time code:")?.1.trim();
    let code = tail.split_whitespace().next()?;
    if code.len() == 9
        && code.as_bytes()[4] == b'-'
        && code
            .bytes()
            .enumerate()
            .all(|(i, byte)| i == 4 || byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        Some(code.to_string())
    } else {
        None
    }
}
