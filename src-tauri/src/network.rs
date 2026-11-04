use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::{Path, PathBuf}, sync::OnceLock, time::Duration};
use tempfile::NamedTempFile;
use tokio::process::Command;

const FILE_NAME: &str = "proxy-settings.json";
const DEFAULT_BYPASS: &str = "localhost,127.0.0.1,::1";
static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode { #[default] System, Direct, Manual }

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Settings {
    pub mode: Mode,
    pub url: String,
    pub no_proxy: String,
}
impl Default for Settings {
    fn default() -> Self { Self { mode: Mode::System, url: String::new(), no_proxy: DEFAULT_BYPASS.into() } }
}

pub fn initialize(directory: PathBuf) { let _ = DIRECTORY.set(directory); }

pub fn validate(settings: &Settings) -> Result<Settings, String> {
    let mut settings = settings.clone();
    settings.url = settings.url.trim().into();
    if settings.url.len() > 2048 || settings.no_proxy.len() > 4096
        || settings.url.chars().any(char::is_control) || settings.no_proxy.chars().any(char::is_control) {
        return Err("代理地址或绕过列表过长，或包含无效字符".into());
    }
    // Retain a valid manual address across mode changes. Discard invalid or
    // credential-bearing drafts so they never prevent returning to direct mode
    // or get stored as an inactive password in this ordinary settings file.
    if settings.mode != Mode::Manual {
        if !settings.url.is_empty() {
            match validate(&Settings { mode: Mode::Manual, ..settings.clone() }) {
                Ok(manual) => { settings.url = manual.url; settings.no_proxy = manual.no_proxy; },
                Err(_) => { settings.url.clear(); settings.no_proxy = DEFAULT_BYPASS.into(); },
            }
        }
        return Ok(settings);
    }
    {
        let url = reqwest::Url::parse(&settings.url).map_err(|_| "请填写完整的代理地址，例如 http://127.0.0.1:7890")?;
        if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h") || url.host_str().is_none()
            || !url.username().is_empty() || url.password().is_some()
            || !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some()
            || url.port() == Some(0) {
            return Err("代理地址须为 HTTP、HTTPS、SOCKS5 或 SOCKS5H 服务器，不包含账号密码、路径或查询参数".into());
        }
        if matches!(url.scheme(), "socks5" | "socks5h") && url.port().is_none() {
            return Err("SOCKS5 代理地址需要填写端口".into());
        }
        reqwest::Proxy::all(url.as_str()).map_err(|_| "代理地址无效")?;
        settings.url = url.as_str().trim_end_matches('/').into();
    }
    let mut bypass = Vec::new();
    for entry in settings.no_proxy.split(',').map(str::trim).filter(|entry| !entry.is_empty()) {
        let entry = entry.strip_prefix("*.").map_or_else(|| entry.to_string(), |domain| format!(".{domain}"));
        let entry = if entry.starts_with('[') && entry.ends_with(']') { entry[1..entry.len() - 1].to_string() } else { entry };
        let (address, mask) = entry.split_once('/').map_or((entry.as_str(), None), |(address, mask)| (address, Some(mask)));
        let ip = address.trim_matches(['[', ']']).parse::<std::net::IpAddr>();
        let valid_mask = mask.is_none_or(|mask| ip.as_ref().is_ok_and(|ip| mask.parse::<u8>().is_ok_and(|size| size <= if ip.is_ipv4() { 32 } else { 128 })));
        let valid_host = address == "*" || ip.is_ok() || (!address.is_empty() && address.chars().all(|ch| ch.is_ascii_alphanumeric() || ".-_".contains(ch)));
        if !valid_host || !valid_mask { return Err("绕过代理的地址请用逗号分隔，仅填写域名、IP 或 CIDR 网段".into()); }
        if !bypass.contains(&entry) { bypass.push(entry); }
    }
    settings.no_proxy = bypass.join(",");
    Ok(settings)
}

pub fn load(directory: &Path) -> Result<Settings, String> {
    let path = directory.join(FILE_NAME);
    match fs::metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(_) => return Err("无法读取本机代理配置".into()),
        Ok(meta) if meta.len() > 16 * 1024 => return Err("本机代理配置文件过大，请重新保存代理设置".into()),
        Ok(_) => {},
    }
    let bytes = fs::read(path).map_err(|_| "无法读取本机代理配置")?;
    let settings: Settings = serde_json::from_slice(&bytes).map_err(|_| "本机代理配置无效，请重新保存代理设置")?;
    validate(&settings)
}

pub fn save(directory: &Path, settings: &Settings) -> Result<Settings, String> {
    let settings = validate(settings)?;
    fs::create_dir_all(directory).map_err(|_| "无法创建本机配置目录")?;
    let mut file = NamedTempFile::new_in(directory).map_err(|_| "无法创建代理配置临时文件")?;
    file.write_all(&serde_json::to_vec_pretty(&settings).map_err(|_| "代理配置无效")?).map_err(|_| "无法写入代理配置")?;
    file.as_file().sync_all().map_err(|_| "无法同步代理配置")?;
    file.persist(directory.join(FILE_NAME)).map_err(|_| "无法保存代理配置，原配置保持不变")?;
    Ok(settings)
}

#[derive(Clone)]
pub struct Policy { settings: Settings, proxy: Option<reqwest::Proxy> }
impl Policy {
    pub fn new(settings: &Settings) -> Result<Self, String> {
        let settings = validate(settings)?;
        let proxy = if settings.mode == Mode::Manual {
            Some(reqwest::Proxy::all(&settings.url).map_err(|_| "代理地址无效")?
                .no_proxy(reqwest::NoProxy::from_string(&settings.no_proxy)))
        } else { None };
        Ok(Self { settings, proxy })
    }
    pub fn client(&self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        match self.settings.mode {
            Mode::System => builder,
            Mode::Direct => builder.no_proxy(),
            Mode::Manual => builder.no_proxy().proxy(self.proxy.as_ref().unwrap().clone()),
        }
    }
    pub fn command(&self, command: &mut Command, git: bool) -> Result<(), String> {
        if self.settings.mode == Mode::System { return Ok(()); }
        for key in ["HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy", "NO_PROXY", "no_proxy"] {
            command.env_remove(key);
        }
        let (url, bypass) = if self.settings.mode == Mode::Manual { (self.settings.url.as_str(), self.settings.no_proxy.as_str()) } else { ("", "*") };
        if self.settings.mode == Mode::Manual {
            for key in ["HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"] { command.env(key, url); }
        }
        command.env("NO_PROXY", bypass).env("no_proxy", bypass);
        if git {
            let existing = command.as_std().get_envs().find(|(key, _)| *key == "GIT_CONFIG_COUNT")
                .map(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()).unwrap_or_else(|| "0".into()))
                .unwrap_or_else(|| std::env::var("GIT_CONFIG_COUNT").unwrap_or_else(|_| "0".into()));
            let count = existing.parse::<usize>().map_err(|_| "Git 临时配置环境无效")?;
            command.env("GIT_CONFIG_COUNT", count.checked_add(1).ok_or("Git 临时配置数量过多")?.to_string())
                .env(format!("GIT_CONFIG_KEY_{count}"), "http.proxy")
                .env(format!("GIT_CONFIG_VALUE_{count}"), url);
        }
        Ok(())
    }
}

pub fn current() -> Result<Policy, String> {
    Policy::new(&match DIRECTORY.get() { Some(directory) => load(directory)?, None => Settings::default() })
}
pub fn client() -> Result<reqwest::ClientBuilder, String> { Ok(current()?.client(reqwest::Client::builder())) }

#[tauri::command]
pub fn load_proxy_settings(app: tauri::AppHandle) -> Result<Settings, String> { load(&crate::legacy_config_dir(&app)?) }

#[tauri::command]
pub fn save_proxy_settings(app: tauri::AppHandle, settings: Settings) -> Result<Settings, String> { save(&crate::legacy_config_dir(&app)?, &settings) }

#[tauri::command]
pub async fn test_proxy_settings(settings: Settings) -> Result<String, String> {
    let client = Policy::new(&settings)?.client(reqwest::Client::builder())
        .timeout(Duration::from_secs(12)).connect_timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none()).user_agent("WorkReport/proxy-test")
        .build().map_err(|_| "无法初始化代理测试")?;
    let response = client.get("https://api.github.com").send().await
        .map_err(|_| "连接测试失败，请检查代理服务、端口或网络连接")?;
    if !response.status().is_success() { return Err(format!("测试站点返回 HTTP {}，请检查代理或稍后重试", response.status().as_u16())); }
    Ok("连接测试成功；保存设置后用于后续请求。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};

    fn manual(url: &str) -> Settings { Settings { mode: Mode::Manual, url: url.into(), no_proxy: String::new() } }

    #[test]
    fn saves_for_future_processes_and_keeps_previous_file_on_invalid_input() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(load(directory.path()).unwrap(), Settings::default());
        let saved = save(directory.path(), &Settings {
            no_proxy: " localhost, *.example.test,127.0.0.1,localhost,[::1],10.0.0.0/8 ".into(),
            ..manual(" http://127.0.0.1:7890/ ")
        }).unwrap();
        assert_eq!(saved.url, "http://127.0.0.1:7890");
        assert_eq!(saved.no_proxy, "localhost,.example.test,127.0.0.1,::1,10.0.0.0/8");
        assert_eq!(load(directory.path()).unwrap(), saved);
        assert!(save(directory.path(), &manual("http://user:secret@proxy.test:80")).is_err());
        assert_eq!(load(directory.path()).unwrap(), saved);
        let direct = Settings { mode: Mode::Direct, url: "unfinished address".into(), ..saved };
        let direct = save(directory.path(), &direct).unwrap();
        assert_eq!(direct.mode, Mode::Direct);
        assert!(direct.url.is_empty());
        assert!(save(directory.path(), &Settings { url: "http://user:secret@proxy.test:80".into(), ..direct }).unwrap().url.is_empty());
    }

    #[test]
    fn validates_proxy_protocols_ports_and_bypass_entries() {
        for url in ["http://127.0.0.1:7890", "https://proxy.test:8443", "socks5://127.0.0.1:1080", "socks5h://[::1]:1080"] {
            assert!(Policy::new(&manual(url)).is_ok(), "{url}");
        }
        for url in ["", "127.0.0.1:7890", "ftp://proxy.test", "http://proxy.test/path", "http://proxy.test?token=a", "http://proxy.test#part", "socks5://proxy.test", "http://proxy.test:0"] {
            assert!(Policy::new(&manual(url)).is_err(), "{url}");
        }
        for bypass in ["http://example.test", "example.test:443", "10.0.0.0/33", "::1/129", "domain/24", "localhost\n*"] {
            assert!(validate(&Settings { no_proxy: bypass.into(), ..manual("http://127.0.0.1:7890") }).is_err(), "{bypass}");
        }
    }

    #[test]
    fn refuses_corrupted_or_oversized_files_without_resetting_them() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        for bytes in [b"{broken".to_vec(), br#"{"mode":"unexpected"}"#.to_vec(), vec![b' '; 16 * 1024 + 1]] {
            fs::write(&path, &bytes).unwrap();
            assert!(load(directory.path()).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        save(directory.path(), &Settings::default()).unwrap();
        assert_eq!(load(directory.path()).unwrap(), Settings::default());
    }

    fn env(command: &Command, key: &str) -> Option<Option<String>> {
        command.as_std().get_envs().find(|(name, _)| *name == key)
            .map(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()))
    }

    #[test]
    fn child_processes_receive_proxy_overrides_without_changing_parent_environment() {
        let original = std::env::var_os("HTTPS_PROXY");
        let mut command = Command::new("git");
        command.env("GIT_CONFIG_COUNT", "1").env("GIT_CONFIG_KEY_0", "credential.helper").env("GIT_CONFIG_VALUE_0", "native");
        Policy::new(&manual("socks5h://127.0.0.1:1080")).unwrap().command(&mut command, true).unwrap();
        assert_eq!(env(&command, "HTTPS_PROXY"), Some(Some("socks5h://127.0.0.1:1080".into())));
        assert_eq!(env(&command, "all_proxy"), env(&command, "HTTPS_PROXY"));
        assert_eq!(env(&command, "GIT_CONFIG_COUNT"), Some(Some("2".into())));
        assert_eq!(env(&command, "GIT_CONFIG_KEY_0"), Some(Some("credential.helper".into())));
        assert_eq!(env(&command, "GIT_CONFIG_KEY_1"), Some(Some("http.proxy".into())));
        assert_eq!(env(&command, "GIT_CONFIG_VALUE_1"), env(&command, "HTTPS_PROXY"));
        assert_eq!(std::env::var_os("HTTPS_PROXY"), original);
        let mut cli = Command::new("cli");
        Policy::new(&Settings { mode: Mode::Direct, ..Settings::default() }).unwrap().command(&mut cli, false).unwrap();
        assert_eq!(env(&cli, "HTTPS_PROXY"), Some(None));
        assert_eq!(env(&cli, "no_proxy"), Some(Some("*".into())));
        assert_eq!(env(&cli, "GIT_CONFIG_COUNT"), None);
        let mut system = Command::new("cli");
        Policy::new(&Settings::default()).unwrap().command(&mut system, true).unwrap();
        assert_eq!(system.as_std().get_envs().count(), 0);
    }

    async fn server(body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept()).await.unwrap().unwrap();
            let mut data = Vec::new();
            loop {
                let mut chunk = [0; 1024];
                let size = stream.read(&mut chunk).await.unwrap();
                if size == 0 { break; }
                data.extend_from_slice(&chunk[..size]);
                if data.windows(4).any(|part| part == b"\r\n\r\n") { break; }
                assert!(data.len() < 16 * 1024);
            }
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            String::from_utf8(data).unwrap()
        });
        (url, task)
    }

    #[tokio::test]
    async fn http_requests_use_manual_proxy_and_bypass_local_destinations() {
        let (proxy, request) = server("proxied").await;
        let client = Policy::new(&manual(&proxy)).unwrap().client(reqwest::Client::builder()).timeout(Duration::from_secs(3)).build().unwrap();
        let response = client.get("http://unresolvable.invalid/models").send().await.unwrap();
        assert_eq!(response.text().await.unwrap(), "proxied");
        assert!(request.await.unwrap().starts_with("GET http://unresolvable.invalid/models HTTP/1.1"));
        let (destination, direct_request) = server("direct").await;
        let client = Policy::new(&Settings { no_proxy: "127.0.0.1".into(), ..manual("http://127.0.0.1:1") }).unwrap()
            .client(reqwest::Client::builder()).timeout(Duration::from_secs(3)).build().unwrap();
        assert_eq!(client.get(format!("{destination}/models")).send().await.unwrap().text().await.unwrap(), "direct");
        assert!(direct_request.await.unwrap().starts_with("GET /models HTTP/1.1"));
        let (destination, direct_request) = server("explicit direct").await;
        let client = Policy::new(&Settings { mode: Mode::Direct, ..Settings::default() }).unwrap()
            .client(reqwest::Client::builder().proxy(reqwest::Proxy::all("http://127.0.0.1:1").unwrap()))
            .timeout(Duration::from_secs(3)).build().unwrap();
        assert_eq!(client.get(&destination).send().await.unwrap().text().await.unwrap(), "explicit direct");
        assert!(direct_request.await.unwrap().starts_with("GET / HTTP/1.1"));
    }

    #[tokio::test]
    async fn git_http_transport_uses_configured_proxy() {
        let (proxy, request) = server("not a git repository").await;
        let mut command = Command::new("git");
        Policy::new(&manual(&proxy)).unwrap().command(&mut command, true).unwrap();
        // An invalid DNS name proves Git used the proxy instead of the origin.
        let _output = tokio::time::timeout(Duration::from_secs(5), command.args(["-c", "credential.helper=", "ls-remote", "http://unresolvable.invalid/project.git"])
            .env("GIT_TERMINAL_PROMPT", "0").kill_on_drop(true).output()).await.unwrap().unwrap();
        assert!(request.await.unwrap().starts_with("GET http://unresolvable.invalid/project.git/info/refs?service=git-upload-pack HTTP/1.1"));
    }
}
