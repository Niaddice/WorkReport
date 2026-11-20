//! One-click Git installation for the Git settings page.
//!
//! * Windows: `winget install Git.Git` (the installer shows its own UAC prompt).
//! * macOS: Homebrew when available, otherwise the system Command Line Tools
//!   installer (`xcode-select --install`), which continues in a system window.
//!
//! Only fixed program names and arguments are ever executed.
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::{process::Command, time::timeout};

const INSTALL_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// `APPINSTALLER_CLI_ERROR_UPDATE_NOT_APPLICABLE`: winget found the package already installed.
#[cfg(windows)]
const WINGET_ALREADY_INSTALLED: i32 = 0x8A15002B_u32 as i32;
static INSTALLING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub installed: bool,
    pub version: String,
    /// Absolute path of the detected Git, empty when none was found.
    pub path: String,
    /// `windows`, `macos` or `other`.
    pub platform: &'static str,
    /// Installer that would be used: `winget`, `homebrew`, `xcode` or `unavailable`.
    pub method: &'static str,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    /// `installed`, or `pending` when a system installer window continues the work.
    pub status: &'static str,
    pub executable: String,
    pub version: String,
    pub message: String,
}

struct InstallGuard;
impl InstallGuard {
    fn acquire() -> Result<Self, String> {
        if INSTALLING.swap(true, Ordering::AcqRel) {
            Err("Git 正在安装中，请等待当前安装完成。".into())
        } else {
            Ok(Self)
        }
    }
}
impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::Release);
    }
}

fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "other"
    }
}

fn program(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    }
}

/// GUI apps often inherit a minimal PATH (and Windows does not refresh PATH after
/// an install), so well-known locations are searched as well.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|path| path.is_absolute())
        .collect();
    #[cfg(windows)]
    {
        for key in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(base) = std::env::var_os(key) {
                dirs.push(PathBuf::from(base).join("Git").join("cmd"));
            }
        }
        if let Some(base) = std::env::var_os("LOCALAPPDATA") {
            let base = PathBuf::from(base);
            dirs.push(base.join("Programs").join("Git").join("cmd"));
            dirs.push(base.join("Microsoft").join("WindowsApps"));
        }
    }
    #[cfg(not(windows))]
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    dirs
}

fn is_runnable(path: &Path) -> bool {
    let Ok(meta) = path.metadata() else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// `/usr/bin/git` on a Mac without developer tools is a stub that pops up the
/// installer when run; it must not be treated as an installed Git.
fn is_inert_stub(path: &Path) -> bool {
    cfg!(target_os = "macos")
        && path == Path::new("/usr/bin/git")
        && !Path::new("/Library/Developer/CommandLineTools/usr/bin/git").is_file()
        && !Path::new("/Applications/Xcode.app/Contents/Developer/usr/bin/git").is_file()
}

fn find_tool(name: &str) -> Option<PathBuf> {
    let file = program(name);
    search_dirs()
        .into_iter()
        .map(|dir| dir.join(&file))
        .find(|path| is_runnable(path) && !is_inert_stub(path))
}

/// Resolve the configured Git program (`git` or an absolute path) without running it.
fn resolve(executable: &str) -> Option<PathBuf> {
    match executable.trim() {
        "" | "git" => find_tool("git"),
        custom => {
            let path = PathBuf::from(custom);
            (path.is_absolute() && is_runnable(&path) && !is_inert_stub(&path)).then_some(path)
        }
    }
}

fn base_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Ok(paths) = std::env::join_paths(search_dirs()) {
        command.env("PATH", paths);
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command
}

async fn version(path: &Path) -> Option<String> {
    let mut command = base_command(path);
    command.arg("--version");
    let output = timeout(Duration::from_secs(5), command.output()).await.ok()?.ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && text.starts_with("git version")).then_some(text)
}

fn install_method() -> &'static str {
    if cfg!(windows) {
        if find_tool("winget").is_some() {
            "winget"
        } else {
            "unavailable"
        }
    } else if cfg!(target_os = "macos") {
        if find_tool("brew").is_some() {
            "homebrew"
        } else {
            "xcode"
        }
    } else {
        "unavailable"
    }
}

/// Last few meaningful output lines, for error messages.
fn tail(stderr: &[u8], stdout: &[u8]) -> String {
    let text = [stderr, stdout]
        .iter()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .find(|text| text.trim().chars().any(char::is_alphanumeric))
        .unwrap_or_default();
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| line.chars().any(char::is_alphanumeric))
        .collect();
    let mut result = lines[lines.len().saturating_sub(3)..].join(" ");
    if result.chars().count() > 300 {
        result = result.chars().take(300).collect::<String>() + "…";
    }
    result
}

async fn run(mut command: Command, label: &str) -> Result<std::process::Output, String> {
    crate::network::current()?.command(&mut command, false)?;
    let child = command
        .spawn()
        .map_err(|error| format!("无法启动{label}：{error}"))?;
    timeout(INSTALL_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| format!("{label}安装超时，请检查网络后重试。"))?
        .map_err(|error| format!("{label}运行失败：{error}"))
}

fn failure(label: &str, output: &std::process::Output) -> String {
    let detail = tail(&output.stderr, &output.stdout);
    if detail.is_empty() {
        format!("{label}安装失败（退出码 {}）。", output.status.code().unwrap_or(-1))
    } else {
        format!("{label}安装失败：{detail}")
    }
}

#[tauri::command]
pub async fn detect_git(executable: String) -> Detection {
    let path = resolve(&executable);
    let version = match &path {
        Some(path) => version(path).await,
        None => None,
    };
    Detection {
        installed: version.is_some(),
        version: version.unwrap_or_default(),
        path: path.map(|path| path.to_string_lossy().into_owned()).unwrap_or_default(),
        platform: platform(),
        method: install_method(),
    }
}

#[tauri::command]
pub async fn install_git() -> Result<Installation, String> {
    let _guard = InstallGuard::acquire()?;
    if let Some(path) = find_tool("git") {
        if let Some(version) = version(&path).await {
            return Ok(Installation {
                status: "installed",
                executable: path.to_string_lossy().into_owned(),
                version,
                message: "本机已安装 Git，无需重复安装。".into(),
            });
        }
    }
    if cfg!(windows) {
        install_with_winget().await?;
    } else if cfg!(target_os = "macos") {
        if let Some(brew) = find_tool("brew") {
            install_with_homebrew(&brew).await?;
        } else {
            return install_with_xcode().await;
        }
    } else {
        return Err("当前系统暂不支持一键安装，请使用系统包管理器安装 Git。".into());
    }
    let path = find_tool("git").ok_or("安装已完成，但暂未找到 Git。请重启应用后在此重新检测。")?;
    let version = version(&path)
        .await
        .ok_or("安装已完成，但 Git 无法运行。请重启应用后在此重新检测。")?;
    Ok(Installation {
        status: "installed",
        executable: path.to_string_lossy().into_owned(),
        version,
        message: "Git 安装完成。".into(),
    })
}

async fn install_with_winget() -> Result<(), String> {
    let winget = find_tool("winget").ok_or(
        "未检测到 winget（Windows 程序包管理器）。请先在 Microsoft Store 更新“应用安装程序”，或从 https://git-scm.com/download/win 手动下载安装 Git。",
    )?;
    let mut command = base_command(&winget);
    command.args([
        "install",
        "--id",
        "Git.Git",
        "--exact",
        "--source",
        "winget",
        "--silent",
        "--accept-package-agreements",
        "--accept-source-agreements",
    ]);
    let output = run(command, "winget").await?;
    #[cfg(windows)]
    if output.status.code() == Some(WINGET_ALREADY_INSTALLED) {
        return Ok(());
    }
    if output.status.success() {
        Ok(())
    } else {
        Err(failure("Git ", &output))
    }
}

async fn install_with_homebrew(brew: &Path) -> Result<(), String> {
    let mut command = base_command(brew);
    command
        .args(["install", "git"])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("NONINTERACTIVE", "1");
    let output = run(command, "Homebrew").await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(failure("Git ", &output))
    }
}

/// Starts the macOS Command Line Tools installer; the user finishes it in a system window.
async fn install_with_xcode() -> Result<Installation, String> {
    let mut command = base_command(Path::new("/usr/bin/xcode-select"));
    command.arg("--install");
    let output = run(command, "xcode-select").await?;
    if output.status.success() {
        Ok(Installation {
            status: "pending",
            executable: String::new(),
            version: String::new(),
            message: "已打开系统的“命令行开发者工具”安装窗口，请按提示完成安装，然后点击“重新检测”。".into(),
        })
    } else {
        Err(failure("命令行开发者工具", &output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_executables_must_be_absolute_and_runnable() {
        assert!(resolve("relative/git").is_none());
        assert!(resolve("/definitely/missing/git").is_none());
    }

    #[test]
    fn concurrent_installs_are_rejected() {
        let first = InstallGuard::acquire().unwrap();
        assert!(InstallGuard::acquire().is_err());
        drop(first);
        assert!(InstallGuard::acquire().is_ok());
    }

    #[test]
    fn tail_keeps_the_last_meaningful_lines() {
        let stderr = "-\n\\\nfirst\nsecond\nthird\nfourth\n";
        assert_eq!(tail(stderr.as_bytes(), b""), "second third fourth");
    }

    #[tokio::test]
    async fn detection_reports_missing_git_without_failing() {
        let result = detect_git("/definitely/missing/git".into()).await;
        assert!(!result.installed);
        assert!(result.path.is_empty());
    }
}
