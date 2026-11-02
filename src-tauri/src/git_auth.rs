use serde::{Deserialize, Serialize};
use tokio::process::Command;
use zeroize::Zeroize;

/// This helper is fixed code. Credentials are passed only through the child environment,
/// scoped to one HTTP origin, and never approved into another configured helper.
const HELPER: &str = r#"!f() {
    operation="$1"; protocol=; host=
    while IFS='=' read -r key value; do
        [ -n "$key" ] || break
        case "$key" in protocol) protocol="$value";; host) host="$value";; esac
    done
    if [ "$operation" = get ] && [ "$protocol" = "$WORKREPORT_GIT_PROTOCOL" ] && [ "$host" = "$WORKREPORT_GIT_HOST" ]; then
        printf 'username=%s\npassword=%s\n' "$WORKREPORT_GIT_USERNAME" "$WORKREPORT_GIT_PASSWORD"
    fi
}; f"#;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Credentials {
    pub url: String,
    pub username: String,
    pub password: String,
    #[serde(default, skip_serializing)]
    pub save: bool,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}

impl Drop for Credentials {
    fn drop(&mut self) {
        self.password.zeroize();
        self.username.zeroize();
    }
}

pub fn authentication_origin(message: &str) -> Option<String> {
    for marker in ["could not read Username for '", "could not read Password for '", "Authentication failed for '"] {
        if let Some((_, tail)) = message.split_once(marker) {
            let value = tail.split('\'').next()?;
            let url = reqwest::Url::parse(value).ok()?;
            if matches!(url.scheme(), "http" | "https") {
                return Some(url.origin().ascii_serialization());
            }
        }
    }
    None
}

impl Credentials {
    pub fn validate(&self) -> Result<(), String> {
        let url = reqwest::Url::parse(&self.url).map_err(|_| "Git 认证地址无效")?;
        let origin = url.origin().ascii_serialization();
        if !matches!(url.scheme(), "http" | "https")
            || origin != self.url.trim_end_matches('/')
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Git 认证仅支持有效的 HTTP / HTTPS 服务器地址".into());
        }
        if self.username.trim().is_empty()
            || self.password.is_empty()
            || self.username.len() > 512
            || self.password.len() > 8192
            || self.username.chars().any(char::is_control)
            || self.password.chars().any(char::is_control)
        {
            return Err("请填写有效的 Git 用户名和密码或访问令牌".into());
        }
        Ok(())
    }

    pub fn configure(&self, command: &mut Command) -> Result<(), String> {
        self.validate()?;
        let url = reqwest::Url::parse(&self.url).map_err(|_| "Git 认证地址无效")?;
        let origin = url.origin().ascii_serialization();
        let host = origin
            .strip_prefix(&format!("{}://", url.scheme()))
            .ok_or("Git 认证地址无效")?;
        command.args([
            "-c",
            "credential.helper=",
            "-c",
            &format!("credential.helper={HELPER}"),
            "-c",
            "core.askPass=",
            "-c",
            "trace2.envVars=",
            "-c",
            "trace2.configParams=",
        ]);
        command
            .env("WORKREPORT_GIT_PROTOCOL", url.scheme())
            .env("WORKREPORT_GIT_HOST", host)
            .env("WORKREPORT_GIT_USERNAME", &self.username)
            .env("WORKREPORT_GIT_PASSWORD", &self.password)
            .env_remove("GIT_ASKPASS")
            .env_remove("SSH_ASKPASS");
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_TRACE") || key == "GIT_CURL_VERBOSE" {
                command.env_remove(key);
            }
        }
        Ok(())
    }

    pub fn redact(&self, bytes: &mut Vec<u8>) {
        if !self.password.is_empty() {
            *bytes = String::from_utf8_lossy(bytes)
                .replace(&self.password, "[已隐藏]")
                .into_bytes();
        }
    }

    pub fn redact_error(&self, bytes: &mut Vec<u8>) {
        // Preserve a sanitized public origin so a rejected saved password can be replaced,
        // even when that password happens to occur in Git's error text or the remote URL.
        if let Some(origin) = authentication_origin(&String::from_utf8_lossy(bytes)) {
            *bytes = format!("fatal: Authentication failed for '{origin}'").into_bytes();
        } else {
            self.redact(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;

    fn credentials() -> Credentials {
        Credentials {
            url: "http://git.example.test:13000".into(),
            username: "test-user".into(),
            password: "token-$'\"`&;=".into(),
            save: false,
        }
    }

    async fn fill(host: &str) -> std::process::Output {
        let dir = tempfile::tempdir().unwrap();
        let mut command = Command::new("git");
        credentials().configure(&mut command).unwrap();
        assert!(command
            .as_std()
            .get_args()
            .all(|value| !value.to_string_lossy().contains(&credentials().password)));
        command
            .current_dir(dir.path())
            .args(["credential", "fill"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("protocol=http\nhost={host}\n\n").as_bytes())
            .await
            .unwrap();
        child.wait_with_output().await.unwrap()
    }

    #[tokio::test]
    async fn helper_provides_literal_credentials_only_to_the_requested_origin() {
        let output = fill("git.example.test:13000").await;
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("username=test-user\n"));
        assert!(text.contains(&format!("password={}\n", credentials().password)));
        let wrong_origin = fill("other.example.test:13000").await;
        assert!(!wrong_origin.status.success());
        assert!(!String::from_utf8_lossy(&wrong_origin.stdout).contains(&credentials().password));
    }

    #[tokio::test]
    async fn git_http_challenge_uses_the_retry_credentials_without_a_terminal() {
        use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
        use tokio::{io::AsyncReadExt, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let authenticated = Arc::new(AtomicBool::new(false));
        let seen = authenticated.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0; 1024];
                while !bytes.windows(4).any(|value| value == b"\r\n\r\n") && bytes.len() < 8192 {
                    let length = stream.read(&mut chunk).await.unwrap();
                    if length == 0 { break; }
                    bytes.extend_from_slice(&chunk[..length]);
                }
                let request = String::from_utf8_lossy(&bytes);
                let valid = request.lines().any(|line| line.split_once(':').is_some_and(|(key, value)| {
                    key.eq_ignore_ascii_case("authorization")
                        && value.trim() == "Basic dGVzdC11c2VyOnRlc3QtcGFzc3dvcmQ="
                }));
                if valid { seen.store(true, Ordering::SeqCst); }
                let response = if valid {
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"Git test\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let value = Credentials { url: origin.clone(), username: "test-user".into(), password: "test-password".into(), save: false };
        let arguments = vec!["-c".into(), "http.proxy=".into(), "ls-remote".into(), format!("{origin}/repo.git")];
        let result = crate::git::run_with_credentials(&crate::git::Settings::default(), dir.path(), &arguments, Some(&value)).await;
        server.abort();
        let output = result.unwrap();
        assert!(authenticated.load(Ordering::SeqCst));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("could not read Username"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(&value.password));
    }

    #[test]
    fn rejects_credential_protocol_injection_and_redacts_debug_and_output() {
        let mut value = credentials();
        assert!(!format!("{value:?}").contains(&value.password));
        let mut output = format!("server echoed {}", value.password).into_bytes();
        value.redact(&mut output);
        assert!(!String::from_utf8(output).unwrap().contains(&value.password));
        value.password.push('\n');
        assert!(value.configure(&mut Command::new("git")).is_err());
        value = credentials();
        value.url = "http://user:secret@git.example.test:13000".into();
        assert!(value.configure(&mut Command::new("git")).is_err());
    }

    #[test]
    fn rejected_saved_credentials_keep_a_public_origin_for_reauthentication() {
        let mut value = credentials();
        value.password = "13000".into();
        let mut error = b"fatal: Authentication failed for 'http://test-user:13000@git.example.test:13000/repo.git/'".to_vec();
        value.redact_error(&mut error);
        let error = String::from_utf8(error).unwrap();
        assert_eq!(authentication_origin(&error).as_deref(), Some("http://git.example.test:13000"));
        assert!(!error.contains("test-user:"));
    }
}
