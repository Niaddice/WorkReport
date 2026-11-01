use serde::Deserialize;
use std::time::Duration;
use tauri_plugin_opener::OpenerExt;

// Keep this repository aligned with plugins.updater.endpoints in tauri.conf.json.
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/Niaddice/WorkReport/releases/latest";
const DOWNLOAD_URL: &str = "https://github.com/Niaddice/WorkReport/releases/latest";

#[tauri::command]
pub fn open_update_download(app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(DOWNLOAD_URL, None::<&str>)
        .map_err(|_| "无法打开浏览器，请复制下载地址后手动打开。".into())
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
}

#[derive(Deserialize)]
struct PublishedRelease {
    draft: bool,
    assets: Vec<ReleaseAsset>,
}

fn has_manifest(release: PublishedRelease) -> Result<bool, String> {
    if release.draft {
        return Err("无法确认已发布版本的更新状态".into());
    }
    Ok(release
        .assets
        .iter()
        .any(|asset| asset.name == "latest.json"))
}

/// Only used when the updater cannot fetch its feed. Missing release assets are
/// different from a network error and must never be reported as "up to date".
#[tauri::command]
pub async fn has_published_update() -> Result<bool, String> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .user_agent(concat!("WorkReport/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "无法连接更新服务")?
        .get(LATEST_RELEASE_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "无法连接更新服务")?
        .error_for_status()
        .map_err(|_| "无法读取已发布版本")?;
    let release = response
        .json::<PublishedRelease>()
        .await
        .map_err(|_| "无法读取已发布版本的附件")?;
    has_manifest(release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_release_without_feed_is_not_ready_for_updates() {
        let release = serde_json::from_value(serde_json::json!({
            "draft": false,
            "assets": [{"name": "WorkReport.app.zip"}, {"name": "SHA256SUMS"}]
        }))
        .unwrap();
        assert!(!has_manifest(release).unwrap());
    }

    #[test]
    fn published_manifest_makes_the_update_service_ready() {
        let release = serde_json::from_value(serde_json::json!({
            "draft": false,
            "assets": [{"name": "WorkReport.app.tar.gz"}, {"name": "latest.json"}]
        }))
        .unwrap();
        assert!(has_manifest(release).unwrap());
    }

    #[test]
    fn draft_and_error_responses_cannot_claim_a_successful_check() {
        assert!(
            serde_json::from_value::<PublishedRelease>(serde_json::json!({
                "message": "API rate limit exceeded"
            }))
            .is_err()
        );
        let release = serde_json::from_value(serde_json::json!({
            "draft": true, "assets": []
        }))
        .unwrap();
        assert!(has_manifest(release).is_err());
    }
}
