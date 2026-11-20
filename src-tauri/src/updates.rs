use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{Manager, Webview};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

const RELEASES_API: &str = "https://api.github.com/repos/Niaddice/WorkReport/releases";
const RELEASES_URL: &str = "https://github.com/Niaddice/WorkReport/releases";

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Main,
    Dev,
}

impl Channel {
    fn download_url(self) -> String {
        match self {
            Self::Main => format!("{RELEASES_URL}/latest"),
            Self::Dev => RELEASES_URL.into(),
        }
    }

    fn matches(self, release: &PublishedRelease) -> bool {
        if release.draft {
            return false;
        }
        // Releases created from an existing tag often store a commit SHA rather
        // than a branch. The publisher marks dev builds as GitHub prereleases.
        let commit = matches!(release.target_commitish.len(), 40 | 64)
            && release.target_commitish.bytes().all(|byte| byte.is_ascii_hexdigit());
        match self {
            Self::Main => !release.prerelease && (release.target_commitish == "main" || commit),
            Self::Dev => release.target_commitish == "dev" || (release.prerelease && commit),
        }
    }
}

#[derive(Clone, Deserialize)]
struct ReleaseAsset {
    name: String,
}

#[derive(Clone, Deserialize)]
struct PublishedRelease {
    draft: bool,
    prerelease: bool,
    target_commitish: String,
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

impl PublishedRelease {
    fn has_manifest(&self) -> bool {
        self.assets.iter().any(|asset| asset.name == "latest.json")
    }

    fn url(&self, download: bool) -> Result<reqwest::Url, String> {
        let mut url = reqwest::Url::parse(RELEASES_URL).map_err(|_| "更新地址无效")?;
        {
            let mut segments = url.path_segments_mut().map_err(|_| "更新地址无效")?;
            segments.push(if download { "download" } else { "tag" }).push(&self.tag_name);
            if download {
                segments.push("latest.json");
            }
        }
        Ok(url)
    }
}

fn select_release(releases: &[PublishedRelease], channel: Channel) -> Option<&PublishedRelease> {
    releases.iter().find(|release| channel.matches(release))
}

async fn published_release(channel: Channel) -> Result<Option<PublishedRelease>, String> {
    let client = crate::network::client()?
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .user_agent(concat!("WorkReport/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "无法连接更新服务")?;
    for page in 1..=10 {
        let releases = client.get(format!("{RELEASES_API}?per_page=100&page={page}"))
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send().await.map_err(|_| "无法连接更新服务")?
            .error_for_status().map_err(|_| "无法读取已发布版本")?
            .json::<Vec<PublishedRelease>>().await.map_err(|_| "无法读取已发布版本的附件")?;
        if let Some(release) = select_release(&releases, channel) {
            return Ok(Some(release.clone()));
        }
        if releases.len() < 100 {
            return Ok(None);
        }
    }
    Err("该渠道的发布记录过多，请前往下载页面查看".into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    rid: tauri::ResourceId,
    current_version: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    raw_json: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelCheck {
    update: Option<UpdateMetadata>,
    published: bool,
    download_url: String,
}

#[tauri::command]
pub async fn check_channel_update(webview: Webview, channel: Channel, allow_channel_switch: bool) -> Result<ChannelCheck, String> {
    let network = crate::network::current()?;
    let release = published_release(channel).await?;
    let download_url = match &release {
        Some(release) => release.url(false)?.to_string(),
        None => channel.download_url(),
    };
    let Some(release) = release.filter(PublishedRelease::has_manifest) else {
        return Ok(ChannelCheck { update: None, published: false, download_url });
    };
    let mut builder = webview.updater_builder()
        .endpoints(vec![release.url(true)?]).map_err(|error| error.to_string())?
        .timeout(Duration::from_secs(15))
        .configure_client(move |builder| {
            // Download may start long after checking. Pick up proxy changes
            // made in settings before the user chooses to install this update.
            crate::network::current().unwrap_or_else(|_| network.clone()).client(builder)
        });
    // Switching back from dev may require an older main version. Installation
    // still requires the normal user action and package signature verification.
    if allow_channel_switch {
        builder = builder.version_comparator(|current, release| release.version != current);
    }
    let update = builder.build().map_err(|error| error.to_string())?
        .check().await.map_err(|error| error.to_string())?;
    let metadata = update.map(|update| UpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        date: update.raw_json.get("pub_date").and_then(|value| value.as_str()).map(str::to_owned),
        body: update.body.clone(),
        raw_json: update.raw_json.clone(),
        rid: webview.resources_table().add(update),
    });
    Ok(ChannelCheck { update: metadata, published: true, download_url })
}

#[tauri::command]
pub async fn open_update_download(app: tauri::AppHandle, channel: Option<Channel>) -> Result<(), String> {
    let channel = channel.unwrap_or_default();
    let url = published_release(channel).await.ok().flatten()
        .and_then(|release| release.url(false).ok())
        .map_or_else(|| channel.download_url(), |url| url.to_string());
    app.opener().open_url(url, None::<&str>)
        .map_err(|_| "无法打开浏览器，请复制下载地址后手动打开。".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(target: &str, prerelease: bool, draft: bool, manifest: bool) -> PublishedRelease {
        serde_json::from_value(serde_json::json!({
            "target_commitish": target, "tag_name": "v0.9.0", "prerelease": prerelease,
            "draft": draft, "assets": if manifest { vec![serde_json::json!({"name": "latest.json"})] } else { vec![] },
        })).unwrap()
    }

    #[test]
    fn channels_select_their_own_releases_and_skip_drafts_and_other_branches() {
        let releases = [release("dev", true, true, true), release("feature", false, false, true),
            release("dev", true, false, true), release("main", false, false, true)];
        assert_eq!(select_release(&releases, Channel::Main).unwrap().target_commitish, "main");
        assert_eq!(select_release(&releases, Channel::Dev).unwrap().target_commitish, "dev");
    }

    #[test]
    fn commit_targets_use_prerelease_status_without_mixing_channels() {
        let sha = "a".repeat(40);
        assert!(Channel::Main.matches(&release(&sha, false, false, true)));
        assert!(!Channel::Dev.matches(&release(&sha, false, false, true)));
        assert!(Channel::Dev.matches(&release(&sha, true, false, true)));
        assert!(!Channel::Main.matches(&release(&sha, true, false, true)));
        assert!(!Channel::Main.matches(&release("dev", false, false, true)));
    }

    #[test]
    fn missing_channel_or_manifest_cannot_claim_up_to_date() {
        let releases = [release("dev", true, false, false), release("dev", true, false, true)];
        assert!(select_release(&releases, Channel::Main).is_none());
        assert!(!select_release(&releases, Channel::Dev).unwrap().has_manifest());
        assert!(serde_json::from_value::<Vec<PublishedRelease>>(serde_json::json!({"message": "rate limited"})).is_err());
    }

    #[test]
    fn release_urls_stay_in_this_repository_and_encode_tag_segments() {
        let mut release = release("dev", true, false, true);
        release.tag_name = "dev/v0.9.0".into();
        assert_eq!(release.url(true).unwrap().as_str(), "https://github.com/Niaddice/WorkReport/releases/download/dev%2Fv0.9.0/latest.json");
        assert_eq!(release.url(false).unwrap().as_str(), "https://github.com/Niaddice/WorkReport/releases/tag/dev%2Fv0.9.0");
    }
}
