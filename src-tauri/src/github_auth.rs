//! GitHub CLI owns the browser/device flow. Only public progress crosses IPC;
//! the resulting HTTPS credential goes straight into the application's vault.
use crate::{git_auth::Credentials, git_credentials::CredentialStorage, network::Policy};
use serde::Serialize;
use std::{path::{Path, PathBuf}, process::Stdio, sync::{Arc, Mutex}, time::Duration};
use tauri_plugin_opener::OpenerExt;
use tokio::{io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader}, process::Command};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const ORIGIN: &str = "https://github.com";
const DEVICE_URL: &str = "https://github.com/login/device";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
type OpenBrowser = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase { Waiting, Authorized, Failed, Cancelled }

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub request_id: String,
    pub phase: Phase,
    pub code: Option<String>,
    pub username: Option<String>,
    pub message: String,
}
struct Session { progress: Progress, cancel: CancellationToken }
#[derive(Clone, Default)]
pub struct Runtime { session: Arc<Mutex<Option<Session>>> }

impl Runtime {
    fn begin(&self) -> Result<(Progress, CancellationToken), String> {
        let mut guard = self.session.lock().map_err(|_| "GitHub 登录状态不可用")?;
        if guard.as_ref().is_some_and(|value| value.progress.phase == Phase::Waiting) {
            return Err("已有 GitHub 登录正在进行，请先完成或取消。".into());
        }
        let progress = Progress { request_id: uuid::Uuid::new_v4().to_string(), phase: Phase::Waiting,
            code: None, username: None, message: "正在获取 GitHub 授权码…".into() };
        let cancel = CancellationToken::new();
        *guard = Some(Session { progress: progress.clone(), cancel: cancel.clone() });
        Ok((progress, cancel))
    }
    fn read(&self, id: &str) -> Result<Progress, String> {
        let guard = self.session.lock().map_err(|_| "GitHub 登录状态不可用")?;
        guard.as_ref().filter(|value| value.progress.request_id == id)
            .map(|value| value.progress.clone()).ok_or_else(|| "GitHub 登录会话已结束，请重新登录。".into())
    }
    fn cancel(&self, id: &str) {
        if let Ok(mut guard) = self.session.lock() {
            if let Some(value) = guard.as_mut().filter(|value| value.progress.request_id == id && value.progress.phase == Phase::Waiting) {
                value.cancel.cancel();
                value.progress.phase = Phase::Cancelled;
                value.progress.code = None;
                value.progress.message = "已取消等待 GitHub 授权。".into();
            }
        }
    }
    fn code(&self, id: &str, code: String) -> bool {
        let Ok(mut guard) = self.session.lock() else { return false; };
        let Some(value) = guard.as_mut().filter(|value| value.progress.request_id == id && value.progress.phase == Phase::Waiting && value.progress.code.is_none()) else { return false; };
        value.progress.code = Some(code);
        value.progress.message = "请在浏览器登录 GitHub，输入授权码并确认授权。".into();
        true
    }
    fn message(&self, id: &str, message: &str) {
        if let Ok(mut guard) = self.session.lock() {
            if let Some(value) = guard.as_mut().filter(|value| value.progress.request_id == id && value.progress.phase == Phase::Waiting) {
                value.progress.message = message.into();
            }
        }
    }
    fn fail(&self, id: &str, message: String) {
        if let Ok(mut guard) = self.session.lock() {
            if let Some(value) = guard.as_mut().filter(|value| value.progress.request_id == id && value.progress.phase == Phase::Waiting) {
                value.progress.phase = Phase::Failed;
                value.progress.code = None;
                value.progress.message = message;
            }
        }
    }
    // Serialize cancellation and the final vault commit. A cancelled or replaced
    // session cannot save a late token; no token is kept in the progress record.
    fn complete(&self, id: &str, credentials: Credentials, storage: &dyn CredentialStorage) -> Result<(), String> {
        let mut guard = self.session.lock().map_err(|_| "GitHub 登录状态不可用")?;
        let value = guard.as_mut().filter(|value| value.progress.request_id == id && value.progress.phase == Phase::Waiting && !value.cancel.is_cancelled())
            .ok_or("GitHub 登录已取消")?;
        storage.save(&credentials).map_err(|error| format!("GitHub 授权已完成，但应用凭据保存失败：{error}"))?;
        value.progress.username = Some(credentials.username.clone());
        value.progress.phase = Phase::Authorized;
        value.progress.code = None;
        value.progress.message = "GitHub 登录成功，授权凭据已加密保存。".into();
        Ok(())
    }
}

fn executable() -> Result<PathBuf, String> {
    let mut directories: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|path| path.is_absolute()).collect();
    directories.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from));
    if let Some(home) = std::env::var_os("HOME") { directories.push(PathBuf::from(home).join(".local/bin")); }
    if let Some(programs) = std::env::var_os("ProgramFiles") { directories.push(PathBuf::from(programs).join("GitHub CLI")); }
    let name = if cfg!(windows) { "gh.exe" } else { "gh" };
    directories.into_iter().map(|path| path.join(name)).find(|path| {
        let Ok(meta) = path.metadata() else { return false; };
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; meta.is_file() && meta.permissions().mode() & 0o111 != 0 }
        #[cfg(not(unix))] { meta.is_file() }
    }).ok_or_else(|| "未检测到 GitHub CLI（gh）。请安装 GitHub CLI 后重试，或使用访问令牌认证。".into())
}

fn command(executable: &Path, policy: &Policy, directory: &Path) -> Result<Command, String> {
    let mut command = Command::new(executable);
    command.current_dir(directory).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true)
        .env("GH_PROMPT_DISABLED", "1").env("NO_COLOR", "1").env("CLICOLOR", "0").env("GH_PAGER", "cat");
    for key in ["GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN", "GH_DEBUG", "GH_FORCE_TTY"] { command.env_remove(key); }
    let mut paths: Vec<_> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    paths.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    if let Ok(paths) = std::env::join_paths(paths) { command.env("PATH", paths); }
    policy.command(&mut command, false)?;
    #[cfg(windows)] command.creation_flags(0x08000000);
    Ok(command)
}

fn device_code(line: &str) -> Option<String> {
    // Parse only the documented code line; never expose arbitrary CLI output,
    // error bodies, usernames or tokens as an authorization code.
    let tail = line.split_once("one-time code:")?.1.trim();
    let code = tail.split_whitespace().next()?;
    if code.len() == 9 && code.as_bytes()[4] == b'-' && code.bytes().enumerate().all(|(i, byte)| i == 4 || byte.is_ascii_uppercase() || byte.is_ascii_digit()) {
        Some(code.into())
    } else { None }
}

async fn observe(reader: impl AsyncRead + Unpin, runtime: Runtime, id: String, open: OpenBrowser) -> Result<(), String> {
    let mut reader = BufReader::new(reader);
    let mut total = 0;
    loop {
        let mut bytes = Zeroizing::new(Vec::new());
        let count = (&mut reader).take(8193).read_until(b'\n', &mut bytes).await.map_err(|_| "无法读取 GitHub 授权进度")?;
        if count == 0 { return Ok(()); }
        total += count;
        if count > 8192 || total > 64 * 1024 { return Err("GitHub CLI 输出异常，请升级 GitHub CLI 后重试。".into()); }
        if let Some(code) = device_code(&String::from_utf8_lossy(&bytes)) {
            if runtime.code(&id, code) && open().is_err() {
                runtime.message(&id, "无法自动打开浏览器，请点击“打开授权页面”并输入下方授权码。");
            }
        }
    }
}

fn parse_credentials(bytes: &[u8]) -> Result<Credentials, String> {
    if bytes.len() > 16 * 1024 { return Err("GitHub CLI 凭据输出异常".into()); }
    let text = std::str::from_utf8(bytes).map_err(|_| "GitHub CLI 凭据无效")?;
    let mut values = std::collections::HashMap::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once('=').ok_or("GitHub CLI 凭据格式无效")?;
        if values.insert(key, value).is_some() { return Err("GitHub CLI 凭据字段重复".into()); }
    }
    if values.get("protocol") != Some(&"https") || values.get("host") != Some(&"github.com") {
        return Err("GitHub CLI 返回了其他服务器的凭据，已拒绝保存。".into());
    }
    let credentials = Credentials { url: ORIGIN.into(), username: values.get("username").ok_or("GitHub 登录未返回用户名")?.to_string(),
        password: values.get("password").ok_or("GitHub 登录未返回访问令牌")?.to_string(), save: false };
    credentials.validate().map_err(|_| "GitHub CLI 返回的凭据无效")?;
    Ok(credentials)
}

async fn credentials(executable: &Path, policy: &Policy, directory: &Path, cancel: &CancellationToken) -> Result<Credentials, String> {
    let mut command = command(executable, policy, directory)?;
    command.args(["auth", "git-credential", "get"]).stdin(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| "无法从 GitHub CLI 读取授权凭据")?;
    let mut input = child.stdin.take().ok_or("无法读取 GitHub CLI 输入")?;
    input.write_all(b"protocol=https\nhost=github.com\n\n").await.map_err(|_| "无法读取 GitHub 授权凭据")?;
    drop(input);
    let mut stdout = child.stdout.take().ok_or("无法读取 GitHub 授权结果")?;
    let mut bytes = Zeroizing::new(Vec::new());
    let result = async {
        (&mut stdout).take(16 * 1024 + 1).read_to_end(&mut bytes).await.map_err(|_| "无法读取 GitHub 授权结果")?;
        if bytes.len() > 16 * 1024 { return Err("GitHub CLI 凭据输出过长".to_string()); }
        let status = child.wait().await.map_err(|_| "GitHub CLI 未正常退出")?;
        if !status.success() { return Err("未能读取 GitHub 授权，请重新登录。".into()); }
        parse_credentials(&bytes)
    };
    let result = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(30), result) => result.map_err(|_| "读取 GitHub 授权凭据超时，请重试。".to_string())?,
        _ = cancel.cancelled() => Err("GitHub 登录已取消".into()),
    };
    if result.is_err() { let _ = child.kill().await; }
    result
}

async fn login(executable: PathBuf, policy: Policy, runtime: Runtime, id: String, cancel: CancellationToken,
    storage: Arc<dyn CredentialStorage>, open: OpenBrowser) -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|_| "无法创建 GitHub 登录临时目录")?;
    let mut command = command(&executable, &policy, directory.path())?;
    // Non-interactive gh prints the device URL instead of opening it. Observe
    // its one-time code and let Tauri open the fixed official page above.
    // No setup-git or protocol flag: existing global Git settings stay intact.
    command.args(["auth", "login", "--hostname", "github.com", "--web", "--skip-ssh-key"]);
    let mut child = command.spawn().map_err(|_| "无法启动 GitHub CLI，请检查安装后重试。")?;
    let stdout = child.stdout.take().ok_or("无法读取 GitHub 登录进度")?;
    let stderr = child.stderr.take().ok_or("无法读取 GitHub 登录进度")?;
    let work = async {
        let (status, _, _) = tokio::try_join!(
            async { child.wait().await.map_err(|_| "GitHub CLI 未正常退出".to_string()) },
            observe(stdout, runtime.clone(), id.clone(), open.clone()),
            observe(stderr, runtime.clone(), id.clone(), open),
        )?;
        if !status.success() { return Err("GitHub 浏览器授权未完成，可能已拒绝、过期或网络不可用，请重试。".to_string()); }
        Ok(())
    };
    let result = tokio::select! {
        result = tokio::time::timeout(LOGIN_TIMEOUT, work) => result.map_err(|_| "GitHub 授权等待超时，请重新登录。".to_string())?,
        _ = cancel.cancelled() => Err("GitHub 登录已取消".into()),
    };
    if result.is_err() { let _ = child.kill().await; }
    result?;
    let credentials = credentials(&executable, &policy, directory.path(), &cancel).await?;
    tokio::task::spawn_blocking(move || runtime.complete(&id, credentials, storage.as_ref())).await
        .map_err(|_| "保存 GitHub 授权凭据失败".to_string())?
}

#[tauri::command]
pub async fn start_github_login(app: tauri::AppHandle, runtime: tauri::State<'_, Runtime>) -> Result<Progress, String> {
    let executable = executable()?;
    let policy = crate::network::current()?;
    let directory = crate::legacy_config_dir(&app)?;
    let storage = Arc::new(crate::git_credentials::LocalStore(Ok(directory)));
    let (progress, cancel) = runtime.begin()?;
    let id = progress.request_id.clone();
    let runtime = runtime.inner().clone();
    tauri::async_runtime::spawn(async move {
        let opener: OpenBrowser = Arc::new(move || app.opener().open_url(DEVICE_URL, None::<&str>).map_err(|_| "无法打开授权页面".into()));
        if let Err(message) = login(executable, policy, runtime.clone(), id.clone(), cancel, storage, opener).await {
            runtime.fail(&id, message);
        }
    });
    Ok(progress)
}

#[tauri::command]
pub async fn read_github_login(runtime: tauri::State<'_, Runtime>, request_id: String) -> Result<Progress, String> {
    let runtime = runtime.inner().clone();
    tokio::task::spawn_blocking(move || runtime.read(&request_id)).await.map_err(|_| "读取 GitHub 登录状态失败".to_string())?
}

#[tauri::command]
pub async fn cancel_github_login(runtime: tauri::State<'_, Runtime>, request_id: String) -> Result<(), String> {
    let runtime = runtime.inner().clone();
    tokio::task::spawn_blocking(move || runtime.cancel(&request_id)).await.map_err(|_| "取消 GitHub 登录失败".to_string())
}

#[tauri::command]
pub fn open_github_login_browser(app: tauri::AppHandle) -> Result<(), String> {
    app.opener().open_url(DEVICE_URL, None::<&str>).map_err(|_| "无法打开浏览器，请手动打开 https://github.com/login/device".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Store { records: Mutex<Vec<Credentials>>, fail: bool }
    impl CredentialStorage for Store {
        fn load(&self, _: &str) -> Result<Option<Credentials>, String> { Ok(None) }
        fn save(&self, value: &Credentials) -> Result<(), String> {
            if self.fail { return Err("系统安全存储已锁定".into()); }
            self.records.lock().unwrap().push(value.clone());
            Ok(())
        }
    }
    fn reply() -> Vec<u8> {
        b"protocol=https\nhost=github.com\nusername=test-user\npassword=private-oauth-token\n".to_vec()
    }
    fn policy() -> Policy { Policy::new(&crate::network::Settings { mode: crate::network::Mode::Direct, ..Default::default() }).unwrap() }

    #[test]
    fn codes_are_extracted_only_from_the_public_device_prompt() {
        assert_eq!(device_code("! First copy your one-time code: ABCD-1234"), Some("ABCD-1234".into()));
        for line in ["ABCD-1234", "HTTP-0403", "password=ABCD-1234", "one-time code: token", "one-time code: abcd-1234", "one-time code: ABCD-12345"] {
            assert_eq!(device_code(line), None);
        }
    }

    #[test]
    fn credential_output_is_bound_to_github_https_and_never_echoed_in_errors() {
        let credentials = parse_credentials(&reply()).unwrap();
        assert_eq!(credentials.url, ORIGIN);
        assert_eq!(credentials.username, "test-user");
        assert_eq!(credentials.password, "private-oauth-token");
        for bytes in [
            String::from_utf8(reply()).unwrap().replace("host=github.com", "host=evil.example"),
            String::from_utf8(reply()).unwrap().replace("protocol=https", "protocol=http"),
            String::from_utf8(reply()).unwrap() + "host=github.com\n",
            "password=private-oauth-token\n".into(),
        ] {
            let error = parse_credentials(bytes.as_bytes()).unwrap_err();
            assert!(!error.contains("private-oauth-token"));
        }
    }

    #[test]
    fn cancellation_and_replacement_reject_late_tokens() {
        let runtime = Runtime::default();
        let store = Store::default();
        let (first, cancel) = runtime.begin().unwrap();
        assert!(runtime.begin().is_err());
        runtime.cancel("other-session");
        assert!(!cancel.is_cancelled());
        runtime.cancel(&first.request_id);
        assert!(cancel.is_cancelled());
        assert!(runtime.complete(&first.request_id, parse_credentials(&reply()).unwrap(), &store).is_err());
        let (second, _) = runtime.begin().unwrap();
        assert!(!runtime.code(&first.request_id, "ABCD-1234".into()));
        runtime.fail(&first.request_id, "late failure".into());
        assert!(runtime.complete(&first.request_id, parse_credentials(&reply()).unwrap(), &store).is_err());
        assert_eq!(runtime.read(&second.request_id).unwrap().phase, Phase::Waiting);
        assert!(store.records.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn progress_opens_the_fixed_browser_action_once_and_survives_open_failure() {
        let runtime = Runtime::default();
        let (progress, _) = runtime.begin().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let opener: OpenBrowser = Arc::new(move || { count.fetch_add(1, Ordering::Relaxed); Err("browser unavailable".into()) });
        observe(b"! First copy your one-time code: ABCD-1234\n! First copy your one-time code: EFGH-5678\n".as_slice(), runtime.clone(), progress.request_id.clone(), opener).await.unwrap();
        let result = runtime.read(&progress.request_id).unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(result.code.as_deref(), Some("ABCD-1234"));
        assert!(result.message.contains("无法自动打开浏览器"));
        assert_eq!(result.phase, Phase::Waiting);
    }

    #[tokio::test]
    async fn excessive_cli_output_is_bounded() {
        let runtime = Runtime::default();
        let (progress, _) = runtime.begin().unwrap();
        let data = vec![b'x'; 9000];
        let error = observe(data.as_slice(), runtime, progress.request_id, Arc::new(|| panic!("unexpected browser"))).await.unwrap_err();
        assert!(error.contains("输出异常"));
    }

    #[cfg(unix)]
    fn fake_cli(directory: &Path, login: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("gh");
        let script = format!(r#"#!/bin/sh
case "$*" in
  'auth login --hostname github.com --web --skip-ssh-key')
    {login}
    ;;
  'auth git-credential get')
    IFS= read -r protocol
    IFS= read -r host
    [ "$protocol" = 'protocol=https' ] && [ "$host" = 'host=github.com' ] || exit 5
    printf 'protocol=https\nhost=github.com\nusername=test-user\npassword=private-oauth-token\n'
    ;;
  *) exit 4 ;;
esac
"#);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn successful_cli_login_commits_a_credential_before_reporting_authorized() {
        let directory = tempfile::tempdir().unwrap();
        let cli = fake_cli(directory.path(), "printf '! First copy your one-time code: ABCD-1234\\n' >&2");
        let runtime = Runtime::default();
        let (progress, cancel) = runtime.begin().unwrap();
        let store = Arc::new(Store::default());
        login(cli, policy(), runtime.clone(), progress.request_id.clone(), cancel, store.clone(), Arc::new(|| Ok(()))).await.unwrap();
        let result = runtime.read(&progress.request_id).unwrap();
        assert_eq!(result.phase, Phase::Authorized);
        assert_eq!(result.username.as_deref(), Some("test-user"));
        assert!(result.code.is_none());
        assert_eq!(store.records.lock().unwrap().len(), 1);
        assert!(!serde_json::to_string(&result).unwrap().contains("private-oauth-token"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejected_or_unpersisted_authorization_never_reports_success() {
        let directory = tempfile::tempdir().unwrap();
        for (script, save_fails) in [("printf 'private-oauth-token' >&2; exit 7", false), ("exit 0", true)] {
            let runtime = Runtime::default();
            let (progress, cancel) = runtime.begin().unwrap();
            let store = Arc::new(Store { fail: save_fails, ..Default::default() });
            let error = login(fake_cli(directory.path(), script), policy(), runtime.clone(), progress.request_id.clone(), cancel, store.clone(), Arc::new(|| Ok(()))).await.unwrap_err();
            assert!(!error.contains("private-oauth-token"));
            runtime.fail(&progress.request_id, error);
            assert_eq!(runtime.read(&progress.request_id).unwrap().phase, Phase::Failed);
            assert!(store.records.lock().unwrap().is_empty());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_a_waiting_cli_stops_it_without_saving() {
        let directory = tempfile::tempdir().unwrap();
        let cli = fake_cli(directory.path(), "printf '! First copy your one-time code: ABCD-1234\\n' >&2; exec sleep 30");
        let runtime = Runtime::default();
        let (progress, cancel) = runtime.begin().unwrap();
        let store = Arc::new(Store::default());
        let opened = Arc::new(tokio::sync::Notify::new());
        let notify = opened.clone();
        let work = login(cli, policy(), runtime.clone(), progress.request_id.clone(), cancel, store.clone(), Arc::new(move || { notify.notify_one(); Ok(()) }));
        let work = tokio::spawn(work);
        tokio::time::timeout(Duration::from_secs(3), opened.notified()).await.unwrap();
        runtime.cancel(&progress.request_id);
        assert!(tokio::time::timeout(Duration::from_secs(3), work).await.unwrap().unwrap().is_err());
        assert_eq!(runtime.read(&progress.request_id).unwrap().phase, Phase::Cancelled);
        assert!(store.records.lock().unwrap().is_empty());
    }
}
