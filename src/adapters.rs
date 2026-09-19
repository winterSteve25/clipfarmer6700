//! Concrete boundaries for local media tools and hosted model calls.
use crate::{
    domain::{Candidate, EditManifest, LocalSignals, Outcome, TranscriptSegment, VisualSample},
    manifest::{render_srt, validate_manifest, validate_object_key, validate_safe_path},
};
use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use scribble::{Opts, OutputType, Scribble, WhisperBackend};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::io::AsyncWriteExt;

#[async_trait]
pub trait Transcriber: Send + Sync {
    async fn transcribe(
        &self,
        input_path: &str,
        session_id: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<TranscriptSegment>>;
}

#[async_trait]
pub trait VisualSampler: Send + Sync {
    async fn sample(
        &self,
        input_path: &str,
        output_dir: &Path,
        start_ms: i64,
        end_ms: i64,
        interval_seconds: u64,
    ) -> Result<Vec<VisualSample>>;
}

#[async_trait]
pub trait SignalExtractor: Send + Sync {
    async fn extract(&self, input_path: &str, start_ms: i64, end_ms: i64) -> Result<LocalSignals>;
}

#[async_trait]
pub trait Renderer: Send + Sync {
    async fn render(&self, manifest: &EditManifest) -> Result<String>;
}

#[async_trait]
pub trait ObjectStore: Send + Sync {
    async fn stage(&self, local_path: &str, object_key: &str) -> Result<String>;
}

#[async_trait]
pub trait Publisher: Send + Sync {
    fn platform(&self) -> &str;
    async fn publish(
        &self,
        candidate: &Candidate,
        local_asset: &str,
        staged_asset: &str,
        title: &str,
        idempotency_key: &str,
    ) -> Result<Outcome>;
}

pub struct ScribbleTranscriber {
    pub ffmpeg: PathBuf,
    pub work_dir: PathBuf,
    engine: Arc<Mutex<Scribble<WhisperBackend>>>,
    language: Option<String>,
    enable_vad: bool,
    incremental_min_window_seconds: usize,
}

impl ScribbleTranscriber {
    pub fn new(
        ffmpeg: PathBuf,
        model_path: &Path,
        vad_model_path: &Path,
        work_dir: PathBuf,
        language: &str,
        enable_vad: bool,
        incremental_min_window_seconds: usize,
    ) -> Result<Self> {
        let model_path = model_path
            .to_str()
            .context("Scribble model path is not valid UTF-8")?;
        let vad_model_path = vad_model_path
            .to_str()
            .context("Scribble VAD model path is not valid UTF-8")?;
        let engine = Scribble::new([model_path], vad_model_path)
            .context("initialize embedded Scribble transcription model")?;
        let language = match language.trim() {
            "" | "auto" => None,
            language => Some(language.to_owned()),
        };
        Ok(Self {
            ffmpeg,
            work_dir,
            engine: Arc::new(Mutex::new(engine)),
            language,
            enable_vad,
            incremental_min_window_seconds: incremental_min_window_seconds.max(1),
        })
    }
}

#[async_trait]
impl Transcriber for ScribbleTranscriber {
    async fn transcribe(
        &self,
        input_path: &str,
        session_id: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<TranscriptSegment>> {
        validate_safe_path(input_path)?;
        ensure!(end_ms > start_ms, "invalid transcription window");
        fs::create_dir_all(&self.work_dir)?;
        let id = uuid::Uuid::new_v4();
        let wav_path = self.work_dir.join(format!("{id}.wav"));
        let ffmpeg = tokio::process::Command::new(&self.ffmpeg)
            .args(["-y", "-v", "error", "-ss"])
            .arg(seconds(start_ms))
            .args(["-t"])
            .arg(seconds(end_ms - start_ms))
            .args(["-i", input_path, "-vn", "-ac", "1", "-ar", "16000"])
            .arg(&wav_path)
            .output()
            .await
            .context("extract audio for Scribble")?;
        ensure!(
            ffmpeg.status.success(),
            "ffmpeg audio extraction failed: {}",
            String::from_utf8_lossy(&ffmpeg.stderr)
        );

        let engine = self.engine.clone();
        let transcribe_path = wav_path.clone();
        let opts = Opts {
            model_key: None,
            enable_translate_to_english: false,
            enable_voice_activity_detection: self.enable_vad,
            language: self.language.clone(),
            output_type: OutputType::Json,
            incremental_min_window_seconds: self.incremental_min_window_seconds,
        };
        let transcription = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let input = fs::File::open(&transcribe_path).with_context(|| {
                format!("open Scribble audio input {}", transcribe_path.display())
            })?;
            let mut output = Vec::new();
            let engine = engine
                .lock()
                .map_err(|_| anyhow::anyhow!("Scribble transcription lock was poisoned"))?;
            engine
                .transcribe(input, &mut output, &opts)
                .context("transcribe audio with embedded Scribble")?;
            Ok(output)
        })
        .await
        .context("Scribble transcription worker panicked");
        let _ = fs::remove_file(&wav_path);
        parse_scribble_json(&transcription??, session_id, start_ms, end_ms)
    }
}

#[derive(Debug, Deserialize)]
struct ScribbleJsonSegment {
    start_seconds: f64,
    end_seconds: f64,
    text: String,
    #[serde(default)]
    tokens: Vec<ScribbleJsonToken>,
}

#[derive(Debug, Deserialize)]
struct ScribbleJsonToken {
    probability: f64,
}

fn parse_scribble_json(
    raw: &[u8],
    session_id: &str,
    window_start_ms: i64,
    window_end_ms: i64,
) -> Result<Vec<TranscriptSegment>> {
    let items: Vec<ScribbleJsonSegment> =
        serde_json::from_slice(raw).context("parse Scribble JSON")?;
    let mut segments = Vec::new();
    for item in items {
        let text = item.text.trim();
        if text.is_empty() {
            continue;
        }
        let relative_start = seconds_to_millis(item.start_seconds);
        let relative_end = seconds_to_millis(item.end_seconds);
        let confidence = token_confidence(&item.tokens);
        let start_ms = (window_start_ms + relative_start).clamp(window_start_ms, window_end_ms);
        let end_ms = (window_start_ms + relative_end).clamp(window_start_ms, window_end_ms);
        if end_ms <= start_ms {
            continue;
        }
        segments.push(TranscriptSegment {
            session_id: session_id.to_owned(),
            start_ms,
            end_ms,
            text: text.to_owned(),
            confidence,
            no_speech_probability: None,
            is_final: true,
        });
    }
    Ok(segments)
}

fn seconds_to_millis(seconds: f64) -> i64 {
    if seconds.is_finite() && seconds > 0.0 {
        (seconds * 1_000.0).round() as i64
    } else {
        0
    }
}

fn token_confidence(tokens: &[ScribbleJsonToken]) -> Option<f64> {
    let probabilities = tokens
        .iter()
        .map(|token| token.probability)
        .filter(|probability| probability.is_finite() && (0.0..=1.0).contains(probability))
        .collect::<Vec<_>>();
    if probabilities.is_empty() {
        None
    } else {
        Some(probabilities.iter().sum::<f64>() / probabilities.len() as f64)
    }
}

#[derive(Debug, Clone)]
pub struct FfmpegVisualSampler {
    pub executable: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FfmpegSignalExtractor {
    pub executable: PathBuf,
}

#[async_trait]
impl SignalExtractor for FfmpegSignalExtractor {
    async fn extract(&self, input_path: &str, start_ms: i64, end_ms: i64) -> Result<LocalSignals> {
        validate_safe_path(input_path)?;
        ensure!(end_ms > start_ms, "invalid signal window");
        let result = tokio::process::Command::new(&self.executable)
            .args(["-v", "info", "-ss"])
            .arg(seconds(start_ms))
            .args(["-t"])
            .arg(seconds(end_ms - start_ms))
            .args([
                "-i",
                input_path,
                "-filter_complex",
                "[0:a]volumedetect[a];[0:v]select='gt(scene,0.25)',showinfo[v]",
                "-map",
                "[a]",
                "-map",
                "[v]",
                "-f",
                "null",
                "-",
            ])
            .output()
            .await
            .context("extract local stream signals")?;
        ensure!(
            result.status.success(),
            "ffmpeg signal extraction failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let diagnostics = String::from_utf8_lossy(&result.stderr);
        let mean_db = diagnostics
            .lines()
            .find_map(|line| line.split("mean_volume:").nth(1))
            .and_then(|value| value.trim().split(' ').next())
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(-60.0);
        let scene_changes = diagnostics
            .lines()
            .filter(|line| line.contains("showinfo") && line.contains(" n:"))
            .count();
        let seconds = (end_ms - start_ms) as f64 / 1_000.0;
        Ok(LocalSignals {
            audio_energy: ((mean_db + 60.0) / 60.0).clamp(0.0, 1.0),
            scene_change_rate: scene_changes as f64 / seconds.max(0.001),
        })
    }
}

#[async_trait]
impl VisualSampler for FfmpegVisualSampler {
    async fn sample(
        &self,
        input_path: &str,
        output_dir: &Path,
        start_ms: i64,
        end_ms: i64,
        interval_seconds: u64,
    ) -> Result<Vec<VisualSample>> {
        validate_safe_path(input_path)?;
        ensure!(
            interval_seconds > 0 && end_ms > start_ms,
            "invalid sampling window"
        );
        fs::create_dir_all(output_dir)?;
        let pattern = output_dir.join("frame-%06d.jpg");
        let result = tokio::process::Command::new(&self.executable)
            .args(["-y", "-v", "error", "-ss"])
            .arg(seconds(start_ms))
            .args(["-t"])
            .arg(seconds(end_ms - start_ms))
            .args(["-i", input_path, "-vf"])
            .arg(format!("fps=1/{interval_seconds},scale=960:-2"))
            .arg(&pattern)
            .output()
            .await?;
        ensure!(
            result.status.success(),
            "ffmpeg frame sampling failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut paths = fs::read_dir(output_dir)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|extension| extension == "jpg"))
            .collect::<Vec<_>>();
        paths.sort();
        Ok(paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| VisualSample {
                at_ms: start_ms + index as i64 * interval_seconds as i64 * 1_000,
                path: path.display().to_string(),
                region: "full_frame".to_owned(),
                reason: "baseline".to_owned(),
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct FfmpegRenderer {
    pub executable: PathBuf,
}

#[async_trait]
impl Renderer for FfmpegRenderer {
    async fn render(&self, manifest: &EditManifest) -> Result<String> {
        validate_manifest(manifest)?;
        let output = Path::new(&manifest.output_path);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let srt_path = output.with_extension("srt");
        fs::write(&srt_path, render_srt(&manifest.captions))?;
        let scale = match manifest.layout.as_str() {
            "full_frame" => {
                "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2"
            }
            "tracked_crop" => "scale=-2:1920,crop=1080:1920",
            "stacked" => {
                "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2"
            }
            _ => {
                "split=2[background][foreground];[background]scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,gblur=sigma=30[background];[foreground]scale=1080:1920:force_original_aspect_ratio=decrease[foreground];[background][foreground]overlay=(W-w)/2:(H-h)/2"
            }
        };
        let mut filters = vec![scale.to_owned()];
        if !manifest.captions.is_empty() {
            let escaped_subtitle = ffmpeg_filter_path(&srt_path);
            filters.push(format!(
                "subtitles=filename='{escaped_subtitle}':force_style='Alignment=2,FontSize=18,Outline=3,MarginV=160'"
            ));
        }
        if let Some(hook) = manifest
            .hook_text
            .as_deref()
            .filter(|hook| !hook.trim().is_empty())
        {
            let hook_path = output.with_extension("hook.txt");
            fs::write(&hook_path, hook.replace(['\0', '\r', '\n'], " "))?;
            let escaped_hook = ffmpeg_filter_path(&hook_path);
            filters.push(format!(
                "drawtext=textfile='{escaped_hook}':expansion=none:fontcolor=white:fontsize=58:borderw=5:bordercolor=black:x=(w-text_w)/2:y=h*0.12"
            ));
        }
        let filter = filters.join(",");
        let rendered = tokio::process::Command::new(&self.executable)
            .args(["-y", "-v", "error", "-ss"])
            .arg(seconds(manifest.source_start_ms))
            .args(["-t"])
            .arg(seconds(manifest.source_end_ms - manifest.source_start_ms))
            .args(["-i", &manifest.input_path, "-vf", &filter, "-r"])
            .arg(manifest.fps.to_string())
            .args([
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-preset",
                "medium",
                "-crf",
                "20",
                "-c:a",
                "aac",
                "-ar",
                "48000",
                "-b:a",
                "160k",
                "-af",
                "loudnorm=I=-14:TP=-1.5:LRA=11",
                "-movflags",
                "+faststart",
            ])
            .arg(&manifest.output_path)
            .output()
            .await
            .context("render vertical clip")?;
        ensure!(
            rendered.status.success(),
            "ffmpeg render failed: {}",
            String::from_utf8_lossy(&rendered.stderr)
        );
        ensure!(output.exists(), "ffmpeg succeeded without creating output");
        Ok(manifest.output_path.clone())
    }
}

fn seconds(ms: i64) -> String {
    format!("{:.3}", ms as f64 / 1_000.0)
}

fn ffmpeg_filter_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

#[derive(Debug, Clone)]
pub struct ManifestRenderer;

#[async_trait]
impl Renderer for ManifestRenderer {
    async fn render(&self, manifest: &EditManifest) -> Result<String> {
        validate_manifest(manifest)?;
        if let Some(parent) = Path::new(&manifest.output_path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&manifest.output_path, serde_json::to_vec_pretty(manifest)?)?;
        Ok(manifest.output_path.clone())
    }
}

#[derive(Debug, Clone)]
pub struct LocalObjectStore {
    pub root: PathBuf,
    pub bucket: String,
    pub prefix: String,
    pub public_base_url: Option<String>,
}

#[async_trait]
impl ObjectStore for LocalObjectStore {
    async fn stage(&self, local_path: &str, object_key: &str) -> Result<String> {
        validate_safe_path(local_path)?;
        validate_object_key(object_key)?;
        validate_object_key(&self.prefix)?;
        let target = self
            .root
            .join(&self.bucket)
            .join(&self.prefix)
            .join(object_key);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(local_path, &target)?;
        let relative = format!("{}/{}", self.prefix.trim_matches('/'), object_key);
        Ok(match &self.public_base_url {
            Some(base) => format!("{}/{}", base.trim_end_matches('/'), relative),
            None => format!("file://{}", target.display()),
        })
    }
}

/// S3-compatible staging through the standard AWS CLI. Credentials stay in the AWS credential
/// chain; no secret is placed in command arguments by ClipFarmer.
#[derive(Debug, Clone)]
pub struct S3CommandStore {
    pub aws_executable: PathBuf,
    pub endpoint: Option<String>,
    pub bucket: String,
    pub prefix: String,
    pub public_base_url: Option<String>,
    pub signed_url_seconds: u32,
}

#[async_trait]
impl ObjectStore for S3CommandStore {
    async fn stage(&self, local_path: &str, object_key: &str) -> Result<String> {
        validate_safe_path(local_path)?;
        validate_object_key(object_key)?;
        validate_object_key(&self.prefix)?;
        if let Some(base) = &self.public_base_url {
            ensure!(
                base.starts_with("https://"),
                "S3 public base URL must use HTTPS"
            );
        }
        let key = format!("{}/{}", self.prefix.trim_matches('/'), object_key);
        let destination = format!("s3://{}/{key}", self.bucket);
        let mut command = tokio::process::Command::new(&self.aws_executable);
        if let Some(endpoint) = &self.endpoint {
            ensure!(
                endpoint.starts_with("https://") && !endpoint.contains(['\r', '\n', '\0']),
                "invalid S3 endpoint"
            );
            command.args(["--endpoint-url", endpoint]);
        }
        let uploaded = command
            .args([
                "s3",
                "cp",
                "--only-show-errors",
                "--content-type",
                "video/mp4",
            ])
            .arg(local_path)
            .arg(&destination)
            .output()
            .await
            .context("stage media in S3-compatible object storage")?;
        ensure!(
            uploaded.status.success(),
            "S3 staging failed: {}",
            String::from_utf8_lossy(&uploaded.stderr)
        );
        let mut presign = tokio::process::Command::new(&self.aws_executable);
        if let Some(endpoint) = &self.endpoint {
            presign.args(["--endpoint-url", endpoint]);
        }
        let signed = presign
            .args([
                "s3",
                "presign",
                &destination,
                "--expires-in",
                &self.signed_url_seconds.to_string(),
            ])
            .output()
            .await
            .context("create signed staging URL")?;
        if signed.status.success() {
            let url = String::from_utf8(signed.stdout)?.trim().to_owned();
            if url.starts_with("https://") {
                return Ok(url);
            }
        }
        let base = self
            .public_base_url
            .as_ref()
            .context("S3 presign failed and no public_base_url fallback is configured")?;
        Ok(format!("{}/{}", base.trim_end_matches('/'), key))
    }
}

#[derive(Debug, Clone)]
pub struct TwitchCapture {
    pub streamlink: PathBuf,
}

impl TwitchCapture {
    pub async fn start(&self, channel: &str, output_path: &Path) -> Result<tokio::process::Child> {
        ensure!(
            !channel.is_empty()
                && channel
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_'),
            "invalid Twitch channel login"
        );
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let child = tokio::process::Command::new(&self.streamlink)
            .arg(format!("https://www.twitch.tv/{channel}"))
            .args([
                "best",
                "--retry-streams",
                "10",
                "--retry-max",
                "0",
                "--output",
            ])
            .arg(output_path)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("start streamlink Twitch capture")?;
        Ok(child)
    }
}

#[derive(Debug, Clone)]
pub struct PreparedTwitchVod {
    pub id: String,
    pub channel_login: String,
    pub media_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct TwitchVodSource {
    pub streamlink: PathBuf,
    pub chat_downloader: PathBuf,
    pub cache_root: PathBuf,
}

impl TwitchVodSource {
    pub async fn prepare(
        &self,
        url: &str,
        channel_override: Option<&str>,
    ) -> Result<PreparedTwitchVod> {
        let id = twitch_vod_id(url)?.to_owned();
        let canonical_url = format!("https://www.twitch.tv/videos/{id}");
        let cache_dir = self.cache_root.join(&id);
        fs::create_dir_all(&cache_dir)?;
        let channel_path = cache_dir.join("channel.txt");
        let cached_channel = fs::read_to_string(&channel_path)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| valid_twitch_login(value));
        let resolved_channel = match cached_channel {
            Some(channel) => channel,
            None => self.resolve_channel(&canonical_url, &id).await?,
        };
        let channel_login = match channel_override {
            Some(channel) => {
                ensure!(valid_twitch_login(channel), "invalid Twitch channel login");
                ensure!(
                    resolved_channel.eq_ignore_ascii_case(channel),
                    "provided channel does not match the Twitch VOD channel"
                );
                channel.to_ascii_lowercase()
            }
            None => resolved_channel,
        };
        fs::write(&channel_path, format!("{channel_login}\n"))?;

        let media_path = cache_dir.join("source.ts");
        if !fs::metadata(&media_path).is_ok_and(|metadata| metadata.len() > 0) {
            self.download_media(&canonical_url, &media_path).await?;
        }
        let chat_path = media_path.with_extension("chat.jsonl");
        if !chat_path.exists() {
            self.download_chat(&canonical_url, &chat_path).await?;
        }
        Ok(PreparedTwitchVod {
            id,
            channel_login,
            media_path,
        })
    }

    async fn resolve_channel(&self, url: &str, expected_id: &str) -> Result<String> {
        let output = tokio::process::Command::new(&self.streamlink)
            .arg("--json")
            .arg(url)
            .output()
            .await
            .context("resolve Twitch VOD metadata with streamlink")?;
        ensure!(
            output.status.success(),
            "streamlink could not resolve Twitch VOD metadata: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata: Value =
            serde_json::from_slice(&output.stdout).context("streamlink metadata is not JSON")?;
        channel_from_streamlink_metadata(&metadata, expected_id)
    }

    async fn download_media(&self, url: &str, destination: &Path) -> Result<()> {
        let temporary =
            destination.with_file_name(format!("source-{}.part.ts", uuid::Uuid::new_v4()));
        let output = tokio::process::Command::new(&self.streamlink)
            .args(["--force", "--progress", "no", "--output"])
            .arg(&temporary)
            .arg(url)
            .arg("best")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("download Twitch VOD with streamlink")?;
        if !output.status.success() {
            let _ = fs::remove_file(&temporary);
            anyhow::bail!(
                "streamlink could not download Twitch VOD: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        ensure!(
            fs::metadata(&temporary).is_ok_and(|metadata| metadata.len() > 0),
            "streamlink produced an empty Twitch VOD"
        );
        fs::rename(&temporary, destination)?;
        Ok(())
    }

    async fn download_chat(&self, url: &str, destination: &Path) -> Result<()> {
        let temporary =
            destination.with_file_name(format!("chat-{}.part.jsonl", uuid::Uuid::new_v4()));
        let output = tokio::process::Command::new(&self.chat_downloader)
            .arg(url)
            .args(["--output"])
            .arg(&temporary)
            .args(["--message_groups", "messages"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("download Twitch VOD chat")?;
        if !output.status.success() {
            let _ = fs::remove_file(&temporary);
            anyhow::bail!(
                "chat_downloader could not download Twitch VOD chat: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if temporary.exists() {
            fs::rename(&temporary, destination)?;
        } else {
            fs::write(destination, [])?;
        }
        Ok(())
    }
}

fn twitch_vod_id(url: &str) -> Result<&str> {
    ensure!(!url.contains(['\0', '\r', '\n']), "unsafe Twitch VOD URL");
    let rest = url
        .strip_prefix("https://")
        .context("Twitch VOD URL must use HTTPS")?;
    let (host, path) = rest.split_once('/').context("Twitch VOD URL has no path")?;
    ensure!(
        matches!(
            host.to_ascii_lowercase().as_str(),
            "twitch.tv" | "www.twitch.tv"
        ),
        "VOD URL must use twitch.tv"
    );
    let clean_path = path.split(['?', '#']).next().unwrap_or_default();
    let mut segments = clean_path.split('/').filter(|segment| !segment.is_empty());
    ensure!(segments.next() == Some("videos"), "invalid Twitch VOD URL");
    let id = segments.next().context("Twitch VOD URL omitted its ID")?;
    ensure!(
        segments.next().is_none() && !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()),
        "invalid Twitch VOD ID"
    );
    Ok(id)
}

fn valid_twitch_login(login: &str) -> bool {
    !login.is_empty()
        && login
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn channel_from_streamlink_metadata(value: &Value, expected_id: &str) -> Result<String> {
    let metadata = value
        .get("metadata")
        .context("streamlink response omitted Twitch VOD metadata")?;
    let id = metadata
        .get("id")
        .and_then(Value::as_str)
        .context("streamlink metadata omitted the Twitch VOD ID")?;
    ensure!(
        id == expected_id,
        "streamlink resolved a different Twitch VOD"
    );
    let author = metadata
        .get("author")
        .and_then(Value::as_str)
        .context("streamlink metadata omitted the Twitch channel login")?
        .to_ascii_lowercase();
    ensure!(
        valid_twitch_login(&author),
        "streamlink returned an invalid Twitch channel login"
    );
    Ok(author)
}

#[derive(Debug, Clone)]
pub struct TwitchChatCapture {
    pub executable: PathBuf,
}

impl TwitchChatCapture {
    pub fn start(&self, channel: &str, output_path: &Path) -> Result<tokio::process::Child> {
        ensure!(
            !channel.is_empty()
                && channel
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_'),
            "invalid Twitch channel login"
        );
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        tokio::process::Command::new(&self.executable)
            .arg(format!("https://www.twitch.tv/{channel}"))
            .args(["--output"])
            .arg(output_path)
            .args(["--message_groups", "messages"])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("start Twitch chat capture")
    }
}

#[derive(Debug, Clone)]
pub struct DryRunPublisher {
    pub name: String,
    pub draft_only: bool,
}

#[async_trait]
impl Publisher for DryRunPublisher {
    fn platform(&self) -> &str {
        &self.name
    }

    async fn publish(
        &self,
        candidate: &Candidate,
        _local_asset: &str,
        _staged_asset: &str,
        _title: &str,
        idempotency_key: &str,
    ) -> Result<Outcome> {
        let mut hash = Sha256::new();
        hash.update(idempotency_key);
        Ok(Outcome {
            candidate_id: candidate.id.clone(),
            platform: self.name.clone(),
            remote_id: format!("dry-{:x}", hash.finalize())[..24].to_owned(),
            status: if self.draft_only {
                "awaiting_creator"
            } else {
                "published"
            }
            .to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            url: None,
        })
    }
}

pub(crate) async fn curl_json(
    method: &str,
    url: &str,
    bearer_token: &str,
    headers: &[(&str, &str)],
    payload: Option<&Value>,
) -> Result<Value> {
    let authorization = (!bearer_token.is_empty()).then(|| format!("Bearer {bearer_token}"));
    curl_json_authenticated(
        method,
        url,
        authorization
            .as_deref()
            .map(|value| ("Authorization", value)),
        headers,
        payload,
    )
    .await
}

pub(crate) async fn curl_json_with_secret_header(
    method: &str,
    url: &str,
    secret_header_name: &str,
    secret_header_value: &str,
    headers: &[(&str, &str)],
    payload: Option<&Value>,
) -> Result<Value> {
    ensure!(
        !secret_header_value.is_empty(),
        "secret HTTP header value is empty"
    );
    curl_json_authenticated(
        method,
        url,
        Some((secret_header_name, secret_header_value)),
        headers,
        payload,
    )
    .await
}

async fn curl_json_authenticated(
    method: &str,
    url: &str,
    secret_header: Option<(&str, &str)>,
    headers: &[(&str, &str)],
    payload: Option<&Value>,
) -> Result<Value> {
    ensure!(
        !url.contains(['\0', '\n', '\r']),
        "unsafe HTTP configuration"
    );
    let mut command = tokio::process::Command::new("curl");
    command.args([
        "--fail-with-body",
        "--silent",
        "--show-error",
        "--request",
        method,
    ]);
    command.args(["--header", "Content-Type: application/json"]);
    for (name, value) in headers {
        ensure!(
            !name.contains(['\r', '\n']) && !value.contains(['\r', '\n']),
            "unsafe HTTP header"
        );
        command.arg("--header").arg(format!("{name}: {value}"));
    }
    let payload_path = if let Some(payload) = payload {
        let path =
            std::env::temp_dir().join(format!("clipfarmer-http-{}.json", uuid::Uuid::new_v4()));
        write_private(&path, serde_json::to_vec(payload)?)?;
        command
            .arg("--data-binary")
            .arg(format!("@{}", path.display()));
        Some(path)
    } else {
        None
    };
    command.arg("--").arg(url);
    let output_result = match secret_header {
        Some((name, value)) => run_curl_with_secret_header(command, name, value).await,
        None => command.output().await.context("run curl"),
    };
    if let Some(path) = payload_path {
        let _ = fs::remove_file(path);
    }
    let output = output_result?;
    ensure!(
        output.status.success(),
        "HTTP request failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("HTTP response is not JSON")
}

fn write_private(path: &Path, contents: Vec<u8>) -> Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&contents)?;
    Ok(())
}

/// Runs curl with the bearer header delivered through stdin as config, keeping the token out of
/// the process argument list shown by tools such as `ps`.
pub(crate) async fn run_curl(
    mut command: tokio::process::Command,
    bearer_token: &str,
) -> Result<std::process::Output> {
    if bearer_token.is_empty() {
        return command.output().await.context("run curl");
    }
    let value = format!("Bearer {bearer_token}");
    run_curl_with_secret_header(command, "Authorization", &value).await
}

async fn run_curl_with_secret_header(
    mut command: tokio::process::Command,
    name: &str,
    value: &str,
) -> Result<std::process::Output> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !value.contains(['\0', '\r', '\n']),
        "unsafe secret HTTP header"
    );
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    command
        .args(["--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("spawn curl")?;
    let mut stdin = child.stdin.take().context("open curl config input")?;
    stdin
        .write_all(format!("header = \"{name}: {escaped}\"\n").as_bytes())
        .await?;
    drop(stdin);
    child.wait_with_output().await.context("wait for curl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scribble_json_and_averages_token_confidence() {
        let raw = br#"[{"start_seconds":0.1,"end_seconds":0.9,"text":"hello","tokens":[{"probability":0.8},{"probability":1.0}]}]"#;
        let segments = parse_scribble_json(raw, "s", 10_000, 12_000).unwrap();
        assert_eq!(segments[0].start_ms, 10_100);
        assert_eq!(segments[0].end_ms, 10_900);
        assert_eq!(segments[0].confidence, Some(0.9));
    }

    #[tokio::test]
    async fn local_object_store_rejects_traversal() {
        let root = std::env::temp_dir().join(format!("clipfarmer-store-{}", uuid::Uuid::new_v4()));
        let input = root.join("input.mp4");
        fs::create_dir_all(&root).unwrap();
        fs::write(&input, b"video").unwrap();
        let store = LocalObjectStore {
            root,
            bucket: "bucket".to_owned(),
            prefix: "clips".to_owned(),
            public_base_url: None,
        };
        assert!(
            store
                .stage(input.to_str().unwrap(), "../escape.mp4")
                .await
                .is_err()
        );
    }

    #[test]
    fn validates_twitch_vod_urls_and_metadata() {
        assert_eq!(
            twitch_vod_id("https://www.twitch.tv/videos/123456789?t=1h2m").unwrap(),
            "123456789"
        );
        assert!(twitch_vod_id("https://example.com/videos/123456789").is_err());
        assert!(twitch_vod_id("https://www.twitch.tv/videos/not-a-number").is_err());
        let metadata = serde_json::json!({
            "metadata":{"id":"123456789","author":"TwitchDev"}
        });
        assert_eq!(
            channel_from_streamlink_metadata(&metadata, "123456789").unwrap(),
            "twitchdev"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prepares_and_reuses_a_cached_twitch_vod() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("clipfarmer-vod-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let streamlink = root.join("streamlink");
        fs::write(
            &streamlink,
            "#!/bin/sh\nif [ \"$1\" = \"--json\" ]; then\n  printf '%s\\n' '{\"metadata\":{\"id\":\"123456789\",\"author\":\"TwitchDev\"}}'\n  exit 0\nfi\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = \"--output\" ]; then shift; output=$1; fi\n  shift\ndone\nprintf 'fake-vod' > \"$output\"\n",
        )
        .unwrap();
        let chat_downloader = root.join("chat_downloader");
        fs::write(
            &chat_downloader,
            "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = \"--output\" ]; then shift; output=$1; fi\n  shift\ndone\nprintf '%s\\n' '{\"time_in_seconds\":1,\"message\":\"hello\"}' > \"$output\"\n",
        )
        .unwrap();
        for executable in [&streamlink, &chat_downloader] {
            fs::set_permissions(executable, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let source = TwitchVodSource {
            streamlink,
            chat_downloader,
            cache_root: root.join("cache"),
        };
        let prepared = source
            .prepare("https://www.twitch.tv/videos/123456789", None)
            .await
            .unwrap();
        assert_eq!(prepared.channel_login, "twitchdev");
        assert_eq!(fs::read(&prepared.media_path).unwrap(), b"fake-vod");
        assert!(prepared.media_path.with_extension("chat.jsonl").exists());

        let cached = TwitchVodSource {
            streamlink: root.join("missing-streamlink"),
            chat_downloader: root.join("missing-chat-downloader"),
            cache_root: root.join("cache"),
        }
        .prepare("https://www.twitch.tv/videos/123456789", None)
        .await
        .unwrap();
        assert_eq!(cached.channel_login, "twitchdev");
        assert_eq!(cached.media_path, prepared.media_path);
        let _ = fs::remove_dir_all(root);
    }
}
