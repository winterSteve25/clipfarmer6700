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
    #[serde(default, alias = "whisper")]
    pub scribble: ScribbleConfig,
    #[serde(default)]
    pub models: ModelSelectionConfig,
    #[serde(default)]
    pub openai: OpenAiConfig,
    #[serde(default)]
    pub gemini: GeminiConfig,
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
pub struct ScribbleConfig {
    #[serde(default = "default_scribble_model")]
    pub model_path: PathBuf,
    #[serde(default = "default_scribble_vad_model")]
    pub vad_model_path: PathBuf,
    #[serde(default = "default_scribble_vad")]
    pub enable_vad: bool,
    #[serde(default = "default_scribble_language")]
    pub language: String,
    #[serde(default = "default_scribble_window")]
    pub incremental_min_window_seconds: usize,
}

fn default_scribble_model() -> PathBuf {
    PathBuf::from("models/ggml-large-v3-turbo.bin")
}
fn default_scribble_vad_model() -> PathBuf {
    PathBuf::from("models/ggml-silero-v6.2.0.bin")
}
fn default_scribble_vad() -> bool {
    true
}
fn default_scribble_language() -> String {
    "auto".to_owned()
}
fn default_scribble_window() -> usize {
    30
}

impl Default for ScribbleConfig {
    fn default() -> Self {
        Self {
            model_path: default_scribble_model(),
            vad_model_path: default_scribble_vad_model(),
            enable_vad: default_scribble_vad(),
            language: default_scribble_language(),
            incremental_min_window_seconds: default_scribble_window(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvider {
    #[default]
    #[serde(rename = "openai", alias = "open_ai")]
    OpenAi,
    Gemini,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModelSelectionConfig {
    #[serde(default)]
    pub provider: ModelProvider,
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
pub struct GeminiConfig {
    #[serde(default = "default_gemini_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_gemini_observer_model")]
    pub observer_model: String,
    #[serde(default = "default_gemini_editorial_model")]
    pub director_model: String,
    #[serde(default = "default_gemini_editorial_model")]
    pub editor_model: String,
    #[serde(default = "default_gemini_editorial_model")]
    pub critic_model: String,
    #[serde(default = "default_gemini_audio_model")]
    pub audio_model: String,
}

fn default_gemini_api_key_env() -> String {
    "GEMINI_API_KEY".to_owned()
}
fn default_gemini_observer_model() -> String {
    "gemini-3.8-flash".to_owned()
}
fn default_gemini_editorial_model() -> String {
    "gemini-3.1-pro-preview".to_owned()
}
fn default_gemini_audio_model() -> String {
    "gemini-3.8-flash".to_owned()
}

impl Default for GeminiConfig {
    fn default() -> Self {
        Self {
            api_key_env: default_gemini_api_key_env(),
            observer_model: default_gemini_observer_model(),
            director_model: default_gemini_editorial_model(),
            editor_model: default_gemini_editorial_model(),
            critic_model: default_gemini_editorial_model(),
            audio_model: default_gemini_audio_model(),
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
            !self.scribble.model_path.as_os_str().is_empty(),
            "Scribble model path must not be empty"
        );
        ensure!(
            !self.scribble.vad_model_path.as_os_str().is_empty(),
            "Scribble VAD model path must not be empty"
        );
        ensure!(
            self.scribble.incremental_min_window_seconds > 0,
            "Scribble incremental window must be positive"
        );
        match self.models.provider {
            ModelProvider::OpenAi => {
                ensure!(
                    self.openai.observer_model == "gpt-5.6-terra",
                    "OpenAI observer model must be gpt-5.6-terra for this architecture"
                );
                ensure!(
                    [
                        &self.openai.director_model,
                        &self.openai.editor_model,
                        &self.openai.critic_model
                    ]
                    .iter()
                    .all(|model| model.as_str() == "gpt-5.6-sol"),
                    "OpenAI director, editor, and critic must use gpt-5.6-sol"
                );
                ensure!(
                    self.openai.audio_model == "gpt-audio-1.5",
                    "OpenAI audio model must be gpt-audio-1.5"
                );
            }
            ModelProvider::Gemini => {
                let models = [
                    &self.gemini.observer_model,
                    &self.gemini.director_model,
                    &self.gemini.editor_model,
                    &self.gemini.critic_model,
                    &self.gemini.audio_model,
                ];
                ensure!(
                    models.iter().all(|model| valid_gemini_model(model)),
                    "Gemini model identifiers must start with gemini- and contain only letters, numbers, dots, underscores, or hyphens"
                );
            }
        }
        Ok(())
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("clipfarmer.sqlite3")
    }
}

fn valid_gemini_model(model: &str) -> bool {
    model.starts_with("gemini-")
        && model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_validates_for_both_model_providers() {
        let mut config: Config =
            toml::from_str(include_str!("../clipfarmer.toml.example")).unwrap();
        for provider in [ModelProvider::OpenAi, ModelProvider::Gemini] {
            config.models.provider = provider;
            config.validate().unwrap();
        }
    }

    #[test]
    fn gemini_model_names_cannot_modify_the_request_url() {
        let mut config: Config =
            toml::from_str(include_str!("../clipfarmer.toml.example")).unwrap();
        config.models.provider = ModelProvider::Gemini;
        config.gemini.observer_model = "gemini-3.8-flash?key=leak".to_owned();
        assert!(config.validate().is_err());
    }
}
