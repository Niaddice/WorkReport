use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

pub const DEFAULT_RELEASES_API: &str = "https://api.github.com/repos/Niaddice/WorkReport/releases";
pub const DEFAULT_RELEASES_URL: &str = "https://github.com/Niaddice/WorkReport/releases";
pub const TAURI_CONFIG_DIR_NAME: &str = "com.workreport.desktop";
pub const GPUI_CONFIG_DIR_NAME: &str = "com.workreport.gpui-prototype";
const PREFERENCE_FILE_NAME: &str = "update-channel.json";

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Main,
    Dev,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Dev => "dev",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "正式版（main）",
            Self::Dev => "开发版（dev）",
        }
    }

    pub fn download_url(self) -> String {
        match self {
            Self::Main => format!("{DEFAULT_RELEASES_URL}/latest"),
            Self::Dev => DEFAULT_RELEASES_URL.into(),
        }
    }

    pub fn matches(self, release: &PublishedRelease) -> bool {
        if release.draft {
            return false;
        }
        let commit = matches!(release.target_commitish.len(), 40 | 64)
            && release
                .target_commitish
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit());
        match self {
            Self::Main => !release.prerelease && (release.target_commitish == "main" || commit),
            Self::Dev => release.target_commitish == "dev" || (release.prerelease && commit),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    #[serde(default)]
    pub browser_download_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PublishedRelease {
    pub draft: bool,
    pub prerelease: bool,
    pub target_commitish: String,
    pub tag_name: String,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

impl PublishedRelease {
    pub fn has_manifest(&self) -> bool {
        self.assets.iter().any(|asset| asset.name == "latest.json")
    }

    pub fn manifest_url(&self, releases_base_url: &str) -> Result<String, String> {
        if let Some(asset) = self
            .assets
            .iter()
            .find(|asset| asset.name == "latest.json")
            .and_then(|asset| asset.browser_download_url.as_ref())
        {
            return Ok(asset.clone());
        }
        self.release_url(releases_base_url, true)
    }

    pub fn release_url(&self, releases_base_url: &str, download_manifest: bool) -> Result<String, String> {
        let mut url = reqwest::Url::parse(releases_base_url).map_err(|_| "更新地址无效")?;
        {
            let mut segments = url.path_segments_mut().map_err(|_| "更新地址无效")?;
            segments
                .push(if download_manifest { "download" } else { "tag" })
                .push(&self.tag_name);
            if download_manifest {
                segments.push("latest.json");
            }
        }
        Ok(url.into())
    }
}

pub fn select_release<'a>(
    releases: &'a [PublishedRelease],
    channel: Channel,
) -> Option<&'a PublishedRelease> {
    releases.iter().find(|release| channel.matches(release))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PlatformArtifact {
    pub signature: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct LatestManifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub pub_date: Option<String>,
    #[serde(default)]
    pub platforms: BTreeMap<String, PlatformArtifact>,
}

pub fn current_platform_key() -> &'static str {
    if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "darwin-aarch64"
        } else {
            "darwin-x86_64"
        }
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "windows-aarch64"
        } else {
            "windows-x86_64"
        }
    } else if cfg!(target_arch = "aarch64") {
        "linux-aarch64"
    } else {
        "linux-x86_64"
    }
}

pub fn resolve_platform_artifact<'a>(
    manifest: &'a LatestManifest,
    platform_key: &str,
) -> Result<&'a PlatformArtifact, String> {
    let artifact = manifest
        .platforms
        .get(platform_key)
        .ok_or_else(|| format!("更新清单中不包含当前平台（{platform_key}）的安装包"))?;
    if artifact.signature.trim().is_empty() {
        return Err("更新包签名为空，已拒绝使用未签名的更新清单".into());
    }
    let parsed = reqwest::Url::parse(artifact.url.trim())
        .map_err(|_| "更新清单中的下载地址无效".to_string())?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err("更新清单中的下载地址协议不受支持".into());
    }
    Ok(artifact)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedSemver {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Option<String>,
}

fn parse_semver(raw: &str) -> Result<ParsedSemver, String> {
    let trimmed = raw.trim().strip_prefix('v').unwrap_or(raw.trim());
    let without_build = trimmed.split('+').next().unwrap_or(trimmed);
    let (core, pre) = match without_build.split_once('-') {
        Some((core, pre)) => (core, Some(pre.to_string())),
        None => (without_build, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("版本号格式无效：{raw}"));
    }
    let major = parts[0]
        .parse::<u64>()
        .map_err(|_| format!("版本号格式无效：{raw}"))?;
    let minor = parts[1]
        .parse::<u64>()
        .map_err(|_| format!("版本号格式无效：{raw}"))?;
    let patch = parts[2]
        .parse::<u64>()
        .map_err(|_| format!("版本号格式无效：{raw}"))?;
    Ok(ParsedSemver {
        major,
        minor,
        patch,
        pre,
    })
}

pub fn compare_semver(left: &str, right: &str) -> Result<Ordering, String> {
    let a = parse_semver(left)?;
    let b = parse_semver(right)?;
    match (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch)) {
        Ordering::Equal => match (&a.pre, &b.pre) {
            (None, None) => Ok(Ordering::Equal),
            (Some(_), None) => Ok(Ordering::Less),
            (None, Some(_)) => Ok(Ordering::Greater),
            (Some(pa), Some(pb)) => Ok(pa.cmp(pb)),
        },
        other => Ok(other),
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePreference {
    #[serde(default)]
    pub channel: Channel,
    #[serde(default)]
    pub allow_channel_switch: bool,
    #[serde(default)]
    pub last_checked: Option<String>,
}

pub fn load_preference(config_dir: &Path) -> Result<UpdatePreference, String> {
    match fs::read(config_dir.join(PREFERENCE_FILE_NAME)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "更新渠道配置损坏，请重新选择更新渠道".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(UpdatePreference::default()),
        Err(error) => Err(format!("无法读取更新渠道配置：{error}")),
    }
}

pub fn load_preference_with_fallback(
    primary_dir: &Path,
    legacy_dir: Option<&Path>,
) -> Result<UpdatePreference, String> {
    let primary = primary_dir.join(PREFERENCE_FILE_NAME);
    if primary.exists() {
        return load_preference(primary_dir);
    }
    if let Some(legacy) = legacy_dir {
        if legacy.join(PREFERENCE_FILE_NAME).exists() {
            return load_preference(legacy);
        }
    }
    Ok(UpdatePreference::default())
}

pub fn save_preference(config_dir: &Path, pref: &UpdatePreference) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(pref)
        .map_err(|error| format!("无法序列化更新渠道配置：{error}"))?;
    write_bytes_atomic(&config_dir.join(PREFERENCE_FILE_NAME), &bytes, "更新渠道配置")
}

fn write_bytes_atomic(destination: &Path, bytes: &[u8], label: &str) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("{label}路径无效"))?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建{label}目录：{error}"))?;
    let mut temp = NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建{label}临时文件：{error}"))?;
    temp.write_all(bytes)
        .map_err(|error| format!("无法写入{label}：{error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("无法同步{label}：{error}"))?;
    temp.persist(destination)
        .map_err(|error| format!("无法保存{label}：{}", error.error))?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCheckResult {
    UpToDate {
        current_version: String,
        latest_version: String,
        download_url: String,
    },
    UpdateAvailable {
        current_version: String,
        version: String,
        notes: String,
        pub_date: Option<String>,
        artifact_url: String,
        signature: String,
        format: Option<String>,
        is_downgrade: bool,
        download_url: String,
    },
    DowngradeRequiresConfirmation {
        current_version: String,
        version: String,
        notes: String,
        pub_date: Option<String>,
        download_url: String,
    },
    Unpublished {
        download_url: String,
    },
}

pub fn evaluate_manifest(
    current_version: &str,
    allow_channel_switch: bool,
    manifest: &LatestManifest,
    platform_key: &str,
    download_url: String,
) -> Result<UpdateCheckResult, String> {
    let cmp = compare_semver(&manifest.version, current_version)?;
    match cmp {
        Ordering::Equal => Ok(UpdateCheckResult::UpToDate {
            current_version: current_version.to_string(),
            latest_version: manifest.version.clone(),
            download_url,
        }),
        Ordering::Less if !allow_channel_switch => Ok(UpdateCheckResult::DowngradeRequiresConfirmation {
            current_version: current_version.to_string(),
            version: manifest.version.clone(),
            notes: manifest.notes.clone(),
            pub_date: manifest.pub_date.clone(),
            download_url,
        }),
        Ordering::Greater | Ordering::Less => {
            let artifact = resolve_platform_artifact(manifest, platform_key)?;
            Ok(UpdateCheckResult::UpdateAvailable {
                current_version: current_version.to_string(),
                version: manifest.version.clone(),
                notes: manifest.notes.clone(),
                pub_date: manifest.pub_date.clone(),
                artifact_url: artifact.url.clone(),
                signature: artifact.signature.clone(),
                format: artifact.format.clone(),
                is_downgrade: cmp == Ordering::Less,
                download_url,
            })
        }
    }
}

pub async fn check_channel_update(
    current_version: &str,
    channel: Channel,
    allow_channel_switch: bool,
    releases_api_url: Option<&str>,
    releases_web_url: Option<&str>,
) -> Result<UpdateCheckResult, String> {
    let client = crate::network::client()?
        .timeout(Duration::from_secs(12))
        .connect_timeout(Duration::from_secs(5))
        .user_agent(concat!("WorkReport/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "无法初始化更新网络客户端".to_string())?;

    let api = releases_api_url.unwrap_or(DEFAULT_RELEASES_API);
    let web_base = releases_web_url.unwrap_or(DEFAULT_RELEASES_URL);
    let url = if api.contains('?') {
        api.to_string()
    } else {
        format!("{api}?per_page=100&page=1")
    };
    let releases = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "无法连接更新服务".to_string())?
        .error_for_status()
        .map_err(|_| "无法读取已发布版本".to_string())?
        .json::<Vec<PublishedRelease>>()
        .await
        .map_err(|_| "无法解析已发布版本列表".to_string())?;

    let Some(release) = select_release(&releases, channel) else {
        return Ok(UpdateCheckResult::Unpublished {
            download_url: channel.download_url(),
        });
    };
    let download_url = release.release_url(web_base, false)?;
    if !release.has_manifest() {
        return Ok(UpdateCheckResult::Unpublished { download_url });
    }
    let manifest_url = release.manifest_url(web_base)?;
    let manifest = client
        .get(&manifest_url)
        .send()
        .await
        .map_err(|_| "无法获取更新清单".to_string())?
        .error_for_status()
        .map_err(|_| "无法读取更新清单".to_string())?
        .json::<LatestManifest>()
        .await
        .map_err(|_| "更新清单格式无效".to_string())?;

    evaluate_manifest(
        current_version,
        allow_channel_switch,
        &manifest,
        current_platform_key(),
        download_url,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyConfigItem {
    pub key: String,
    pub label: String,
    pub exists_in_legacy: bool,
    pub exists_in_target: bool,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyMigrationSummary {
    pub legacy_dir: PathBuf,
    pub target_dir: PathBuf,
    pub items: Vec<LegacyConfigItem>,
    pub workspace_path: Option<String>,
}

const LEGACY_CONFIG_ITEMS: [(&str, &str, bool); 5] = [
    ("workspace.json", "最近打开的工作空间", false),
    ("ai-config.json", "本机 AI 模型全局配置", false),
    ("network.json", "本机网络代理配置", false),
    ("update-channel.json", "更新渠道偏好", false),
    ("holidays", "离线节假日年份缓存", true),
];

pub fn inspect_legacy_config(legacy_dir: &Path, target_dir: &Path) -> LegacyMigrationSummary {
    let mut items = Vec::with_capacity(LEGACY_CONFIG_ITEMS.len());
    for &(key, label, is_dir) in &LEGACY_CONFIG_ITEMS {
        let src = legacy_dir.join(key);
        let dst = target_dir.join(key);
        let exists_in_legacy = if is_dir { src.is_dir() } else { src.is_file() };
        let exists_in_target = if is_dir { dst.is_dir() } else { dst.is_file() };
        items.push(LegacyConfigItem {
            key: key.to_string(),
            label: label.to_string(),
            exists_in_legacy,
            exists_in_target,
            is_dir,
        });
    }
    let workspace_path = fs::read(legacy_dir.join("workspace.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<String>(&bytes).ok())
        .filter(|path| !path.trim().is_empty());
    LegacyMigrationSummary {
        legacy_dir: legacy_dir.to_path_buf(),
        target_dir: target_dir.to_path_buf(),
        items,
        workspace_path,
    }
}

pub fn migrate_legacy_config(
    legacy_dir: &Path,
    target_dir: &Path,
    overwrite: bool,
) -> Result<usize, String> {
    if !legacy_dir.exists() {
        return Err(format!("未找到旧版 Tauri 配置目录：{}", legacy_dir.display()));
    }
    fs::create_dir_all(target_dir)
        .map_err(|error| format!("无法创建目标配置目录：{error}"))?;
    let mut copied = 0usize;
    for &(key, label, is_dir) in &LEGACY_CONFIG_ITEMS {
        let src = legacy_dir.join(key);
        let dst = target_dir.join(key);
        if is_dir {
            if !src.is_dir() {
                continue;
            }
            fs::create_dir_all(&dst)
                .map_err(|error| format!("无法创建{label}目录：{error}"))?;
            let entries = fs::read_dir(&src)
                .map_err(|error| format!("无法读取{label}目录：{error}"))?;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") || !path.is_file() {
                    continue;
                }
                let Some(name) = path.file_name() else {
                    continue;
                };
                let target_file = dst.join(name);
                if target_file.exists() && !overwrite {
                    continue;
                }
                let bytes =
                    fs::read(&path).map_err(|error| format!("无法读取{label}文件：{error}"))?;
                write_bytes_atomic(&target_file, &bytes, label)?;
                copied += 1;
            }
        } else {
            if !src.is_file() || (dst.exists() && !overwrite) {
                continue;
            }
            let bytes = fs::read(&src).map_err(|error| format!("无法读取{label}：{error}"))?;
            write_bytes_atomic(&dst, &bytes, label)?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn release(
        target: &str,
        prerelease: bool,
        draft: bool,
        manifest_url: Option<&str>,
    ) -> PublishedRelease {
        PublishedRelease {
            draft,
            prerelease,
            target_commitish: target.into(),
            tag_name: "v0.9.0".into(),
            assets: manifest_url
                .map(|url| {
                    vec![ReleaseAsset {
                        name: "latest.json".into(),
                        browser_download_url: Some(url.into()),
                    }]
                })
                .unwrap_or_default(),
        }
    }

    #[test]
    fn channels_select_their_own_releases_and_skip_drafts_and_other_branches() {
        let releases = [
            release("dev", true, true, Some("https://example.com/1")),
            release("feature", false, false, Some("https://example.com/2")),
            release("dev", true, false, Some("https://example.com/3")),
            release("main", false, false, Some("https://example.com/4")),
        ];
        assert_eq!(
            select_release(&releases, Channel::Main)
                .unwrap()
                .target_commitish,
            "main"
        );
        assert_eq!(
            select_release(&releases, Channel::Dev)
                .unwrap()
                .target_commitish,
            "dev"
        );
    }

    #[test]
    fn semver_comparison_and_manifest_contract_rejects_unsigned_or_unconfirmed_downgrade() {
        assert_eq!(compare_semver("0.1.9", "0.1.8").unwrap(), Ordering::Greater);
        assert_eq!(compare_semver("v0.2.0-dev.1", "0.2.0").unwrap(), Ordering::Less);
        assert_eq!(compare_semver("0.1.8", "0.1.8").unwrap(), Ordering::Equal);

        let mut platforms = BTreeMap::new();
        platforms.insert(
            "darwin-aarch64".into(),
            PlatformArtifact {
                signature: "sig-ok".into(),
                url: "https://github.com/Niaddice/WorkReport/releases/download/v0.2.0/WorkReport.app.tar.gz".into(),
                format: Some("app".into()),
            },
        );
        let upgrade = LatestManifest {
            version: "0.2.0".into(),
            notes: "New release".into(),
            pub_date: Some("2026-11-21T00:00:00Z".into()),
            platforms: platforms.clone(),
        };
        let eval = evaluate_manifest(
            "0.1.8",
            false,
            &upgrade,
            "darwin-aarch64",
            "https://github.com/Niaddice/WorkReport/releases/tag/v0.2.0".into(),
        )
        .unwrap();
        assert!(matches!(
            eval,
            UpdateCheckResult::UpdateAvailable {
                is_downgrade: false,
                format: Some(ref fmt),
                ..
            } if fmt == "app"
        ));

        let older = LatestManifest {
            version: "0.1.7".into(),
            notes: "Rollback main".into(),
            pub_date: None,
            platforms: platforms.clone(),
        };
        let blocked = evaluate_manifest(
            "0.1.8",
            false,
            &older,
            "darwin-aarch64",
            "https://example.com/tag/v0.1.7".into(),
        )
        .unwrap();
        assert!(matches!(
            blocked,
            UpdateCheckResult::DowngradeRequiresConfirmation { .. }
        ));
        let allowed = evaluate_manifest(
            "0.1.8",
            true,
            &older,
            "darwin-aarch64",
            "https://example.com/tag/v0.1.7".into(),
        )
        .unwrap();
        assert!(matches!(
            allowed,
            UpdateCheckResult::UpdateAvailable {
                is_downgrade: true,
                ..
            }
        ));

        let mut tampered = upgrade.clone();
        tampered.platforms.get_mut("darwin-aarch64").unwrap().signature = "   ".into();
        assert!(evaluate_manifest(
            "0.1.8",
            false,
            &tampered,
            "darwin-aarch64",
            "https://example.com".into()
        )
        .is_err());
    }

    #[tokio::test]
    async fn local_http_update_check_reads_manifest_and_reports_available_version() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let platform_key = current_platform_key().to_string();
        let releases_url = format!("http://{addr}/releases");
        let manifest_url = format!("http://{addr}/latest.json");

        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 2048];
                let n = socket.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);
                let body = if req.contains("GET /latest.json") {
                    serde_json::json!({
                        "version": "0.9.9",
                        "notes": "Loopback test update",
                        "pub_date": "2026-11-21T12:00:00Z",
                        "platforms": {
                            &platform_key: {
                                "signature": "minisign-test-sig",
                                "url": format!("http://{addr}/WorkReport.app.tar.gz"),
                                "format": "app"
                            }
                        }
                    })
                    .to_string()
                } else {
                    serde_json::json!([{
                        "draft": false,
                        "prerelease": false,
                        "target_commitish": "main",
                        "tag_name": "v0.9.9",
                        "assets": [{
                            "name": "latest.json",
                            "browser_download_url": manifest_url
                        }]
                    }])
                    .to_string()
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(resp.as_bytes()).await.unwrap();
            }
        });

        let result = check_channel_update(
            "0.1.8",
            Channel::Main,
            false,
            Some(&releases_url),
            Some(&releases_url),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert!(matches!(
            result,
            UpdateCheckResult::UpdateAvailable { ref version, ref format, .. }
            if version == "0.9.9" && format.as_deref() == Some("app")
        ));
    }

    #[test]
    fn legacy_tauri_config_inspection_and_migration_preserves_source_and_existing_targets() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join(TAURI_CONFIG_DIR_NAME);
        let target = dir.path().join(GPUI_CONFIG_DIR_NAME);
        fs::create_dir_all(legacy.join("holidays")).unwrap();
        fs::write(legacy.join("workspace.json"), br#""/tmp/my-work""#).unwrap();
        fs::write(legacy.join("ai-config.json"), br#"{"model":"gpt-5"}"#).unwrap();
        fs::write(legacy.join("holidays/2026.json"), br#"{"year":2026}"#).unwrap();

        let summary = inspect_legacy_config(&legacy, &target);
        assert_eq!(summary.workspace_path.as_deref(), Some("/tmp/my-work"));
        assert!(summary.items.iter().any(|i| i.key == "ai-config.json" && i.exists_in_legacy && !i.exists_in_target));

        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("ai-config.json"), br#"{"model":"gpui-custom"}"#).unwrap();

        let copied = migrate_legacy_config(&legacy, &target, false).unwrap();
        assert_eq!(copied, 2); // workspace.json + holidays/2026.json (ai-config preserved)
        assert_eq!(
            fs::read_to_string(target.join("ai-config.json")).unwrap(),
            r#"{"model":"gpui-custom"}"#
        );
        assert_eq!(
            fs::read_to_string(target.join("workspace.json")).unwrap(),
            r#""/tmp/my-work""#
        );
        assert_eq!(
            fs::read_to_string(target.join("holidays/2026.json")).unwrap(),
            r#"{"year":2026}"#
        );

        let overwritten = migrate_legacy_config(&legacy, &target, true).unwrap();
        assert_eq!(overwritten, 3);
        assert_eq!(
            fs::read_to_string(target.join("ai-config.json")).unwrap(),
            r#"{"model":"gpt-5"}"#
        );
    }
}
