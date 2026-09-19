use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default)]
    pub worker: WorkerConfig,
    #[serde(default)]
    pub media: MediaConfig,
    #[serde(default)]
    pub whisper: WhisperConfig,
    #[serde(default)]
    pub openai: OpenAiConfig,
    #[serde(default)]
    pub staging: StagingConfig,
    #[serde(default)]
    pub publishers: PublisherConfig,
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./data")
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkerConfig {
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
    #[serde(default = "default_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_ring_minutes")]
    pub ring_minutes: u64,
    #[serde(default = "default_observer_window")]
    pub observer_window_seconds: u64,
    #[serde(default = "default_observer_step")]
    pub observer_step_seconds: u64,
    #[serde(default = "default_maturation_delay")]
    pub maturation_delay_seconds: u64,
    #[serde(default = "default_queue_capacity")]
    pub queue_capacity: usize,
}

fn default_poll() -> u64 {
    5
}
fn default_attempts() -> u32 {
    4
}
fn default_ring_minutes() -> u64 {
    10
}
fn default_observer_window() -> u64 {
    12
}
fn default_observer_step() -> u64 {
    6
}
fn default_maturation_delay() -> u64 {
    20
}
fn default_queue_capacity() -> usize {
    256
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            poll_seconds: default_poll(),
            max_attempts: default_attempts(),
            ring_minutes: default_ring_minutes(),
            observer_window_seconds: default_observer_window(),
            observer_step_seconds: default_observer_step(),
            maturation_delay_seconds: default_maturation_delay(),
            queue_capacity: default_queue_capacity(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaConfig {
    #[serde(default = "default_ffmpeg")]
    pub ffmpeg_path: PathBuf,
    #[serde(default = "default_ffprobe")]
    pub ffprobe_path: PathBuf,
    #[serde(default = "default_streamlink")]
    pub streamlink_path: PathBuf,
    #[serde(default = "default_chat_downloader")]
    pub chat_downloader_path: PathBuf,
    #[serde(default = "default_frame_interval")]
    pub frame_interval_seconds: u64,
}

fn default_ffmpeg() -> PathBuf {
    PathBuf::from("ffmpeg")
}
fn default_ffprobe() -> PathBuf {
    PathBuf::from("ffprobe")
}
fn default_streamlink() -> PathBuf {
    PathBuf::from("streamlink")
}
fn default_chat_downloader() -> PathBuf {
    PathBuf::from("chat_downloader")
}
fn default_frame_interval() -> u64 {
    1
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: default_ffmpeg(),
            ffprobe_path: default_ffprobe(),
            streamlink_path: default_streamlink(),
            chat_downloader_path: default_chat_downloader(),
            frame_interval_seconds: default_frame_interval(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WhisperConfig {
    #[serde(default = "default_whisper_executable")]
    pub executable: PathBuf,
    #[serde(default = "default_whisper_model")]
    pub model_path: PathBuf,
    #[serde(default = "default_whisper_threads")]
    pub threads: usize,
    #[serde(default = "default_whisper_language")]
    pub language: String,
}

fn default_whisper_executable() -> PathBuf {
    PathBuf::from("whisper-cli")
}
fn default_whisper_model() -> PathBuf {
    PathBuf::from("models/ggml-large-v3-turbo.bin")
}
fn default_whisper_threads() -> usize {
    4
}
fn default_whisper_language() -> String {
    "auto".to_owned()
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            executable: default_whisper_executable(),
            model_path: default_whisper_model(),
            threads: default_whisper_threads(),
            language: default_whisper_language(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiConfig {
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_observer_model")]
    pub observer_model: String,
    #[serde(default = "default_sol_model")]
    pub director_model: String,
    #[serde(default = "default_sol_model")]
    pub editor_model: String,
    #[serde(default = "default_sol_model")]
    pub critic_model: String,
    #[serde(default = "default_audio_model")]
    pub audio_model: String,
}

fn default_api_key_env() -> String {
    "OPENAI_API_KEY".to_owned()
}
fn default_observer_model() -> String {
    "gpt-5.6-terra".to_owned()
}
fn default_sol_model() -> String {
    "gpt-5.6-sol".to_owned()
}
fn default_audio_model() -> String {
    "gpt-audio-1.5".to_owned()
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key_env: default_api_key_env(),
            observer_model: default_observer_model(),
            director_model: default_sol_model(),
            editor_model: default_sol_model(),
            critic_model: default_sol_model(),
            audio_model: default_audio_model(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StagingConfig {
    #[serde(default = "default_staging_provider")]
    pub provider: String,
    #[serde(default = "default_bucket")]
    pub bucket: String,
    #[serde(default = "default_prefix")]
    pub prefix: String,
    #[serde(default)]
    pub public_base_url: Option<String>,
    #[serde(default = "default_s3_endpoint_env")]
    pub endpoint_env: String,
}

fn default_staging_provider() -> String {
    "local".to_owned()
}
fn default_bucket() -> String {
    "clipfarmer-artifacts".to_owned()
}
fn default_prefix() -> String {
    "staged".to_owned()
}
fn default_s3_endpoint_env() -> String {
    "S3_ENDPOINT".to_owned()
}

impl Default for StagingConfig {
    fn default() -> Self {
        Self {
            provider: default_staging_provider(),
            bucket: default_bucket(),
            prefix: default_prefix(),
            public_base_url: None,
            endpoint_env: default_s3_endpoint_env(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublisherConfig {
    #[serde(default = "default_true")]
    pub dry_run: bool,
    #[serde(default = "default_true")]
    pub youtube: bool,
    #[serde(default = "default_true")]
    pub instagram: bool,
    #[serde(default = "default_true")]
    pub tiktok_drafts: bool,
    #[serde(default)]
    pub twitch_clips: bool,
    #[serde(default = "youtube_token_env")]
    pub youtube_token_env: String,
    #[serde(default = "instagram_token_env")]
    pub instagram_token_env: String,
    #[serde(default = "instagram_account_env")]
    pub instagram_account_env: String,
    #[serde(default = "tiktok_token_env")]
    pub tiktok_token_env: String,
    #[serde(default = "twitch_token_env")]
    pub twitch_token_env: String,
    #[serde(default = "twitch_client_env")]
    pub twitch_client_id_env: String,
    #[serde(default = "twitch_broadcaster_env")]
    pub twitch_broadcaster_id_env: String,
}

fn default_true() -> bool {
    true
}
fn youtube_token_env() -> String {
    "YOUTUBE_ACCESS_TOKEN".to_owned()
}
fn instagram_token_env() -> String {
    "INSTAGRAM_ACCESS_TOKEN".to_owned()
}
fn instagram_account_env() -> String {
    "INSTAGRAM_ACCOUNT_ID".to_owned()
}
fn tiktok_token_env() -> String {
    "TIKTOK_ACCESS_TOKEN".to_owned()
}
fn twitch_token_env() -> String {
    "TWITCH_ACCESS_TOKEN".to_owned()
}
fn twitch_client_env() -> String {
    "TWITCH_CLIENT_ID".to_owned()
}
fn twitch_broadcaster_env() -> String {
    "TWITCH_BROADCASTER_ID".to_owned()
}

impl Default for PublisherConfig {
    fn default() -> Self {
        Self {
            dry_run: true,
            youtube: true,
            instagram: true,
            tiktok_drafts: true,
            twitch_clips: false,
            youtube_token_env: youtube_token_env(),
            instagram_token_env: instagram_token_env(),
            instagram_account_env: instagram_account_env(),
            tiktok_token_env: tiktok_token_env(),
            twitch_token_env: twitch_token_env(),
            twitch_client_id_env: twitch_client_env(),
            twitch_broadcaster_id_env: twitch_broadcaster_env(),
        }
    }
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let raw = fs::read_to_string(path.as_ref())
            .with_context(|| format!("read config {}", path.as_ref().display()))?;
        let config: Self = toml::from_str(&raw).context("parse TOML config")?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.worker.observer_step_seconds > 0,
            "observer step must be positive"
        );
        ensure!(
            self.worker.observer_window_seconds >= self.worker.observer_step_seconds,
            "observer window must be at least one observer step"
        );
        ensure!(
            self.worker.queue_capacity > 0,
            "queue capacity must be positive"
        );
        ensure!(
            self.openai.observer_model == "gpt-5.6-terra",
            "observer model must be gpt-5.6-terra for this architecture"
        );
        ensure!(
            [
                &self.openai.director_model,
                &self.openai.editor_model,
                &self.openai.critic_model
            ]
            .iter()
            .all(|model| model.as_str() == "gpt-5.6-sol"),
            "director, editor, and critic must use gpt-5.6-sol"
        );
        ensure!(
            self.openai.audio_model == "gpt-audio-1.5",
            "audio model must be gpt-audio-1.5"
        );
        Ok(())
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("clipfarmer.sqlite3")
    }
}
