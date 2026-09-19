use crate::{
    adapters::{Publisher, curl_json, run_curl},
    domain::{Candidate, Outcome},
    manifest::validate_safe_path,
};
use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use serde_json::Value;
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct YouTubePublisher {
    pub access_token: String,
    pub privacy_status: String,
    pub work_dir: PathBuf,
}

#[async_trait]
impl Publisher for YouTubePublisher {
    fn platform(&self) -> &str {
        "youtube"
    }

    async fn publish(
        &self,
        candidate: &Candidate,
        local_asset: &str,
        _staged_asset: &str,
        title: &str,
        idempotency_key: &str,
    ) -> Result<Outcome> {
        ensure!(
            !self.access_token.is_empty(),
            "YouTube access token is not configured"
        );
        validate_safe_path(local_asset)?;
        fs::create_dir_all(&self.work_dir)?;
        let header_path = self
            .work_dir
            .join(format!("youtube-{}.headers", uuid::Uuid::new_v4()));
        let metadata = serde_json::json!({
            "snippet": {"title":title,"description":"#Shorts","categoryId":"20"},
            "status": {"privacyStatus":self.privacy_status,"selfDeclaredMadeForKids":false}
        });
        let mut command = tokio::process::Command::new("curl");
        command
            .args([
                "--fail-with-body",
                "--silent",
                "--show-error",
                "--request",
                "POST",
                "--dump-header",
            ])
            .arg(&header_path)
            .args([
                "--header",
                "Content-Type: application/json; charset=UTF-8",
                "--header",
            ])
            .arg(format!(
                "X-Upload-Content-Length: {}",
                fs::metadata(local_asset)?.len()
            ))
            .args([
                "--header",
                "X-Upload-Content-Type: video/mp4",
                "--data-binary",
            ])
            .arg(metadata.to_string());
        let initiate = run_curl(
            command,
            &self.access_token,
            "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status",
        )
            .await
            .context("start YouTube resumable upload")?;
        ensure!(
            initiate.status.success(),
            "YouTube upload initialization failed: {}",
            String::from_utf8_lossy(&initiate.stderr)
        );
        let headers = fs::read_to_string(&header_path)?;
        let _ = fs::remove_file(&header_path);
        let location = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("Location:")
                    .or_else(|| line.strip_prefix("location:"))
            })
            .map(str::trim)
            .context("YouTube response omitted resumable upload location")?;
        let uploaded = curl_upload(
            "PUT",
            location,
            &self.access_token,
            local_asset,
            &["Content-Type: video/mp4"],
        )
        .await?;
        let remote_id = uploaded
            .get("id")
            .and_then(Value::as_str)
            .context("YouTube upload response omitted video id")?;
        Ok(Outcome {
            candidate_id: candidate.id.clone(),
            platform: self.platform().to_owned(),
            remote_id: remote_id.to_owned(),
            status: "published".to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            url: Some(format!("https://youtube.com/shorts/{remote_id}")),
        })
    }
}

#[derive(Debug, Clone)]
pub struct InstagramPublisher {
    pub access_token: String,
    pub account_id: String,
    pub poll_attempts: u32,
}

#[async_trait]
impl Publisher for InstagramPublisher {
    fn platform(&self) -> &str {
        "instagram"
    }

    async fn publish(
        &self,
        candidate: &Candidate,
        _local_asset: &str,
        staged_asset: &str,
        title: &str,
        idempotency_key: &str,
    ) -> Result<Outcome> {
        ensure!(
            !self.access_token.is_empty() && !self.account_id.is_empty(),
            "Instagram credentials are not configured"
        );
        ensure!(
            staged_asset.starts_with("https://"),
            "Instagram requires a public HTTPS video URL"
        );
        let endpoint = format!("https://graph.facebook.com/v23.0/{}/media", self.account_id);
        let created = curl_json(
            "POST",
            &endpoint,
            &self.access_token,
            &[],
            Some(&serde_json::json!({
                "media_type":"REELS",
                "video_url":staged_asset,
                "caption":title,
                "share_to_feed":true
            })),
        )
        .await?;
        let container_id = created
            .get("id")
            .and_then(Value::as_str)
            .context("Instagram media container response omitted id")?;
        let status_url =
            format!("https://graph.facebook.com/v23.0/{container_id}?fields=status_code,status");
        let mut ready = false;
        for _ in 0..self.poll_attempts.max(1) {
            let status = curl_json("GET", &status_url, &self.access_token, &[], None).await?;
            match status.get("status_code").and_then(Value::as_str) {
                Some("FINISHED") => {
                    ready = true;
                    break;
                }
                Some("ERROR" | "EXPIRED") => {
                    anyhow::bail!("Instagram rejected media container: {status}")
                }
                _ => tokio::time::sleep(Duration::from_secs(5)).await,
            }
        }
        ensure!(
            ready,
            "Instagram media processing did not finish before timeout"
        );
        let published = curl_json(
            "POST",
            &format!(
                "https://graph.facebook.com/v23.0/{}/media_publish",
                self.account_id
            ),
            &self.access_token,
            &[],
            Some(&serde_json::json!({"creation_id":container_id})),
        )
        .await?;
        let remote_id = published
            .get("id")
            .and_then(Value::as_str)
            .context("Instagram publish response omitted media id")?;
        Ok(Outcome {
            candidate_id: candidate.id.clone(),
            platform: self.platform().to_owned(),
            remote_id: remote_id.to_owned(),
            status: "published".to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            url: None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TikTokDraftPublisher {
    pub access_token: String,
}

#[async_trait]
impl Publisher for TikTokDraftPublisher {
    fn platform(&self) -> &str {
        "tiktok"
    }

    async fn publish(
        &self,
        candidate: &Candidate,
        local_asset: &str,
        _staged_asset: &str,
        _title: &str,
        idempotency_key: &str,
    ) -> Result<Outcome> {
        ensure!(
            !self.access_token.is_empty(),
            "TikTok access token is not configured"
        );
        validate_safe_path(local_asset)?;
        let size = fs::metadata(local_asset)?.len();
        ensure!(size > 0, "cannot upload an empty TikTok draft");
        let initialized = curl_json(
            "POST",
            "https://open.tiktokapis.com/v2/post/publish/inbox/video/init/",
            &self.access_token,
            &[],
            Some(&serde_json::json!({
                "source_info": {
                    "source":"FILE_UPLOAD",
                    "video_size":size,
                    "chunk_size":size,
                    "total_chunk_count":1
                }
            })),
        )
        .await?;
        let data = initialized
            .get("data")
            .context("TikTok response omitted data")?;
        let upload_url = data
            .get("upload_url")
            .and_then(Value::as_str)
            .context("TikTok response omitted upload URL")?;
        let publish_id = data
            .get("publish_id")
            .and_then(Value::as_str)
            .context("TikTok response omitted publish id")?;
        let content_range = format!("Content-Range: bytes 0-{}/{}", size - 1, size);
        let _ = curl_upload(
            "PUT",
            upload_url,
            "",
            local_asset,
            &["Content-Type: video/mp4", &content_range],
        )
        .await?;
        Ok(Outcome {
            candidate_id: candidate.id.clone(),
            platform: self.platform().to_owned(),
            remote_id: publish_id.to_owned(),
            status: "awaiting_creator".to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            url: None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TwitchClipPublisher {
    pub access_token: String,
    pub client_id: String,
    broadcaster_ids: Arc<RwLock<HashMap<String, String>>>,
}

impl TwitchClipPublisher {
    pub fn new(access_token: String, client_id: String) -> Self {
        Self {
            access_token,
            client_id,
            broadcaster_ids: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn broadcaster_id(&self, login: &str) -> Result<String> {
        ensure!(valid_twitch_login(login), "invalid Twitch channel login");
        let normalized = login.to_ascii_lowercase();
        if let Some(id) = self.broadcaster_ids.read().await.get(&normalized) {
            return Ok(id.clone());
        }
        let response = curl_json(
            "GET",
            &format!("https://api.twitch.tv/helix/users?login={normalized}"),
            &self.access_token,
            &[("Client-Id", &self.client_id)],
            None,
        )
        .await?;
        let id = broadcaster_id_from_response(&response, &normalized)?.to_owned();
        self.broadcaster_ids
            .write()
            .await
            .insert(normalized, id.clone());
        Ok(id)
    }
}

#[async_trait]
impl Publisher for TwitchClipPublisher {
    fn platform(&self) -> &str {
        "twitch"
    }

    async fn publish(
        &self,
        candidate: &Candidate,
        _local_asset: &str,
        _staged_asset: &str,
        _title: &str,
        idempotency_key: &str,
    ) -> Result<Outcome> {
        ensure!(
            !self.access_token.is_empty() && !self.client_id.is_empty(),
            "Twitch clip credentials are not configured"
        );
        let broadcaster_id = self.broadcaster_id(&candidate.channel_id).await?;
        let response = curl_json(
            "POST",
            &format!("https://api.twitch.tv/helix/clips?broadcaster_id={broadcaster_id}"),
            &self.access_token,
            &[("Client-Id", &self.client_id)],
            None,
        )
        .await?;
        let remote_id = response
            .pointer("/data/0/id")
            .and_then(Value::as_str)
            .context("Twitch clip response omitted id")?;
        Ok(Outcome {
            candidate_id: candidate.id.clone(),
            platform: self.platform().to_owned(),
            remote_id: remote_id.to_owned(),
            status: "published".to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            url: Some(format!("https://clips.twitch.tv/{remote_id}")),
        })
    }
}

fn valid_twitch_login(login: &str) -> bool {
    !login.is_empty()
        && login
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn broadcaster_id_from_response<'a>(response: &'a Value, expected_login: &str) -> Result<&'a str> {
    let user = response
        .pointer("/data/0")
        .context("Twitch user lookup returned no matching channel")?;
    let login = user
        .get("login")
        .and_then(Value::as_str)
        .context("Twitch user lookup omitted login")?;
    ensure!(
        login.eq_ignore_ascii_case(expected_login),
        "Twitch user lookup returned a different channel"
    );
    user.get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
        .context("Twitch user lookup omitted a valid broadcaster ID")
}

async fn curl_upload(
    method: &str,
    url: &str,
    token: &str,
    local_path: &str,
    headers: &[&str],
) -> Result<Value> {
    ensure!(!url.contains(['\0', '\r', '\n']), "unsafe upload URL");
    validate_safe_path(local_path)?;
    let mut command = tokio::process::Command::new("curl");
    command.args([
        "--fail-with-body",
        "--silent",
        "--show-error",
        "--request",
        method,
    ]);
    for header in headers {
        ensure!(!header.contains(['\r', '\n']), "unsafe upload header");
        command.arg("--header").arg(header);
    }
    command.arg("--data-binary").arg(format!("@{local_path}"));
    let output = run_curl(command, token, url)
        .await
        .context("upload media")?;
    ensure!(
        output.status.success(),
        "media upload failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    if output.stdout.is_empty() {
        Ok(serde_json::json!({}))
    } else {
        serde_json::from_slice(&output.stdout).context("upload response is not JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_broadcaster_id_for_the_requested_login() {
        let response = serde_json::json!({
            "data":[{"id":"141981764","login":"twitchdev","display_name":"TwitchDev"}]
        });
        assert_eq!(
            broadcaster_id_from_response(&response, "TwitchDev").unwrap(),
            "141981764"
        );
    }

    #[test]
    fn rejects_an_unexpected_or_unsafe_twitch_login() {
        assert!(valid_twitch_login("some_streamer"));
        assert!(!valid_twitch_login("some-streamer?redirect=1"));
        let response = serde_json::json!({"data":[{"id":"123","login":"other"}]});
        assert!(broadcaster_id_from_response(&response, "expected").is_err());
    }
}
