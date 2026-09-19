use crate::{
    Service, ServiceDependencies,
    adapters::{
        DryRunPublisher, FfmpegRenderer, FfmpegSignalExtractor, FfmpegVisualSampler,
        LocalObjectStore, ObjectStore, Publisher, S3CommandStore, ScribbleTranscriber,
        TwitchCapture, TwitchChatCapture, TwitchVodSource,
    },
    config::{Config, ModelProvider},
    editorial::{
        CandidateAudioAnalyzer, DeterministicAudioAnnotation, DeterministicEditorial,
        EditorialModel,
    },
    models::{
        gemini::{GeminiAudioAnalyzer, GeminiEditorial},
        openai::{OpenAiAudioAnalyzer, OpenAiEditorial},
    },
    pipeline::RunSummary,
    publishers::{InstagramPublisher, TikTokDraftPublisher, TwitchClipPublisher, YouTubePublisher},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::watch;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobSource {
    Channel { channel: String },
    Vod { url: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub phase: String,
    pub message: String,
    pub elapsed_ms: u64,
    pub captured_ms: Option<i64>,
    pub summary: Option<RunSummaryDto>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummaryDto {
    pub windows_observed: usize,
    pub candidates_reviewed: usize,
    pub candidates_accepted: usize,
    pub candidates_rejected: usize,
    pub posts_completed: usize,
    pub publish_failures: usize,
}

impl From<&RunSummary> for RunSummaryDto {
    fn from(value: &RunSummary) -> Self {
        Self {
            windows_observed: value.windows_observed,
            candidates_reviewed: value.candidates_reviewed,
            candidates_accepted: value.candidates_accepted,
            candidates_rejected: value.candidates_rejected,
            posts_completed: value.posts_completed,
            publish_failures: value.publish_failures,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub channel: String,
    pub summary: RunSummary,
}

#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub transcription: PathBuf,
    pub voice_activity_detection: PathBuf,
}

#[derive(Clone)]
pub struct CancellationHandle(watch::Sender<bool>);

pub struct Cancellation(watch::Receiver<bool>);

impl CancellationHandle {
    pub fn new() -> (Self, Cancellation) {
        let (sender, receiver) = watch::channel(false);
        (Self(sender), Cancellation(receiver))
    }

    pub fn cancel(&self) {
        let _ = self.0.send(true);
    }
}

impl Cancellation {
    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }

    async fn cancelled(&mut self) {
        if self.is_cancelled() {
            return;
        }
        while self.0.changed().await.is_ok() {
            if self.is_cancelled() {
                return;
            }
        }
    }
}

type ProgressCallback = Arc<dyn Fn(JobProgress) + Send + Sync>;

pub struct LibraryRunner {
    config: Config,
    deterministic_models: bool,
    progress: ProgressCallback,
    started: Instant,
}

impl LibraryRunner {
    pub fn load(
        mut config: Config,
        data_dir: PathBuf,
        model_paths: ModelPaths,
        deterministic_models: bool,
        progress: impl Fn(JobProgress) + Send + Sync + 'static,
    ) -> Result<Self> {
        config.data_dir = data_dir;
        config.scribble.model_path = model_paths.transcription;
        config.scribble.vad_model_path = model_paths.voice_activity_detection;
        config.validate()?;
        fs::create_dir_all(config.data_dir.join("outputs"))?;
        Ok(Self {
            config,
            deterministic_models,
            progress: Arc::new(progress),
            started: Instant::now(),
        })
    }

    pub fn output_dir(&self) -> PathBuf {
        self.config.data_dir.join("outputs")
    }

    pub async fn run(
        &self,
        source: JobSource,
        mut cancellation: Cancellation,
    ) -> Result<RunResult> {
        match source {
            JobSource::Channel { channel } => self.run_live(&channel, &mut cancellation).await,
            JobSource::Vod { url } => self.run_vod(&url, &mut cancellation).await,
        }
    }

    async fn run_vod(&self, url: &str, cancellation: &mut Cancellation) -> Result<RunResult> {
        self.report(
            "preparing_vod",
            "Resolving and downloading the Twitch VOD",
            None,
            None,
        );
        let source = TwitchVodSource {
            streamlink: self.config.media.streamlink_path.clone(),
            chat_downloader: self.config.media.chat_downloader_path.clone(),
            cache_root: self.config.data_dir.join("vods"),
        };
        let prepared = tokio::select! {
            result = source.prepare(url, None) => result?,
            _ = cancellation.cancelled() => anyhow::bail!(Cancelled),
        };
        self.report(
            "preparing_vod",
            format!("VOD {} downloaded", prepared.id),
            None,
            None,
        );
        let input = prepared.media_path.to_string_lossy().into_owned();
        let channel = prepared.channel_login;
        let duration = tokio::select! {
            result = probe_duration_ms(&self.config, &input) => result?,
            _ = cancellation.cancelled() => anyhow::bail!(Cancelled),
        };
        let service = self.build_service()?;
        self.report(
            "analyzing",
            "Analyzing the downloaded VOD",
            Some(duration),
            None,
        );
        let mut work = Box::pin(service.replay_file(&channel, &input, duration));
        let mut pulse = tokio::time::interval(Duration::from_secs(2));
        let summary = loop {
            tokio::select! {
                result = &mut work => break result?,
                _ = cancellation.cancelled() => anyhow::bail!(Cancelled),
                _ = pulse.tick() => self.report("analyzing", "Analyzing the downloaded VOD", Some(duration), None),
            }
        };
        drop(work);
        Ok(RunResult { channel, summary })
    }

    async fn run_live(&self, channel: &str, cancellation: &mut Cancellation) -> Result<RunResult> {
        validate_channel(channel)?;
        let capture_path = self
            .config
            .data_dir
            .join("live")
            .join(channel)
            .join(format!("{}.ts", uuid::Uuid::new_v4()));
        self.report(
            "starting",
            format!("Starting live capture for {channel}"),
            None,
            None,
        );
        let mut video = TwitchCapture {
            streamlink: self.config.media.streamlink_path.clone(),
        }
        .start(channel, &capture_path)
        .await?;
        let mut chat = TwitchChatCapture {
            executable: self.config.media.chat_downloader_path.clone(),
        }
        .start(channel, &capture_path.with_extension("chat.jsonl"))?;
        let service = self.build_service()?;
        let input = capture_path.to_string_lossy().into_owned();
        let mut analyzed_through = 0_i64;
        let mut total = RunSummary::default();
        let poll = Duration::from_secs(self.config.worker.poll_seconds.max(5));

        loop {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    let _ = video.kill().await;
                    let _ = chat.kill().await;
                    anyhow::bail!(Cancelled);
                }
                _ = tokio::time::sleep(poll) => {
                    if let Some(status) = video.try_wait()? {
                        ensure!(status.success(), "streamlink exited with {status}");
                        let _ = chat.kill().await;
                        break;
                    }
                    let duration = match probe_duration_ms(&self.config, &input).await {
                        Ok(value) => value,
                        Err(error) => {
                            self.report("capturing", format!("Waiting for analyzable media: {error}"), None, Some(&total));
                            continue;
                        }
                    };
                    self.report("capturing", format!("Captured {} seconds", duration / 1_000), Some(duration), Some(&total));
                    let minimum_advance = self.config.worker.observer_step_seconds as i64 * 1_000;
                    if duration >= analyzed_through + minimum_advance {
                        let scan_start = analyzed_through.saturating_sub(120_000);
                        let scan = service.scan_file(channel, &input, duration, scan_start);
                        let pass = tokio::select! {
                            result = scan => result?,
                            _ = cancellation.cancelled() => {
                                let _ = video.kill().await;
                                let _ = chat.kill().await;
                                anyhow::bail!(Cancelled);
                            }
                        };
                        add_summary(&mut total, &pass);
                        analyzed_through = duration;
                        self.report("capturing", "Live analysis pass complete", Some(duration), Some(&total));
                    }
                }
            }
        }

        if capture_path.exists() {
            if let Ok(duration) = probe_duration_ms(&self.config, &input).await {
                if duration > analyzed_through {
                    self.report(
                        "analyzing",
                        "Running final live analysis",
                        Some(duration),
                        Some(&total),
                    );
                    let scan = service.scan_file(
                        channel,
                        &input,
                        duration,
                        analyzed_through.saturating_sub(120_000),
                    );
                    let pass = tokio::select! {
                        result = scan => result?,
                        _ = cancellation.cancelled() => anyhow::bail!(Cancelled),
                    };
                    add_summary(&mut total, &pass);
                }
            }
        }
        Ok(RunResult {
            channel: channel.to_owned(),
            summary: total,
        })
    }

    fn report(
        &self,
        phase: impl Into<String>,
        message: impl Into<String>,
        captured_ms: Option<i64>,
        summary: Option<&RunSummary>,
    ) {
        (self.progress)(JobProgress {
            phase: phase.into(),
            message: message.into(),
            elapsed_ms: self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            captured_ms,
            summary: summary.map(RunSummaryDto::from),
        });
    }

    fn build_service(&self) -> Result<Service> {
        let cfg = &self.config;
        let transcriber = Arc::new(ScribbleTranscriber::new(
            cfg.media.ffmpeg_path.clone(),
            &cfg.scribble.model_path,
            &cfg.scribble.vad_model_path,
            cfg.data_dir.join("scribble"),
            &cfg.scribble.language,
            cfg.scribble.enable_vad,
            cfg.scribble.incremental_min_window_seconds,
        )?);
        let (editorial, audio_analyzer): (
            Arc<dyn EditorialModel>,
            Arc<dyn CandidateAudioAnalyzer>,
        ) = if self.deterministic_models {
            (
                Arc::new(DeterministicEditorial { accept: true }),
                Arc::new(DeterministicAudioAnnotation),
            )
        } else {
            match cfg.models.provider {
                ModelProvider::OpenAi => {
                    let api_key = read_secret(&cfg.openai.api_key_env)?;
                    (
                        Arc::new(OpenAiEditorial {
                            api_key: api_key.clone(),
                            observer_model: cfg.openai.observer_model.clone(),
                            director_model: cfg.openai.director_model.clone(),
                            editor_model: cfg.openai.editor_model.clone(),
                            critic_model: cfg.openai.critic_model.clone(),
                        }),
                        Arc::new(OpenAiAudioAnalyzer {
                            api_key,
                            model: cfg.openai.audio_model.clone(),
                            ffmpeg: cfg.media.ffmpeg_path.clone(),
                            work_dir: cfg.data_dir.join("audio-analysis/openai"),
                        }),
                    )
                }
                ModelProvider::Gemini => {
                    let api_key = read_secret(&cfg.gemini.api_key_env)?;
                    (
                        Arc::new(GeminiEditorial {
                            api_key: api_key.clone(),
                            observer_model: cfg.gemini.observer_model.clone(),
                            director_model: cfg.gemini.director_model.clone(),
                            editor_model: cfg.gemini.editor_model.clone(),
                            critic_model: cfg.gemini.critic_model.clone(),
                        }),
                        Arc::new(GeminiAudioAnalyzer {
                            api_key,
                            model: cfg.gemini.audio_model.clone(),
                            ffmpeg: cfg.media.ffmpeg_path.clone(),
                            work_dir: cfg.data_dir.join("audio-analysis/gemini"),
                        }),
                    )
                }
            }
        };
        let object_store: Arc<dyn ObjectStore> = match cfg.staging.provider.as_str() {
            "local" => Arc::new(LocalObjectStore {
                root: cfg.data_dir.join("staging"),
                bucket: cfg.staging.bucket.clone(),
                prefix: cfg.staging.prefix.clone(),
                public_base_url: cfg.staging.public_base_url.clone(),
            }),
            "s3" => Arc::new(S3CommandStore {
                aws_executable: PathBuf::from("aws"),
                endpoint: std::env::var(&cfg.staging.endpoint_env).ok(),
                bucket: cfg.staging.bucket.clone(),
                prefix: cfg.staging.prefix.clone(),
                public_base_url: cfg.staging.public_base_url.clone(),
                signed_url_seconds: 3_600,
            }),
            provider => anyhow::bail!("unsupported staging provider {provider}"),
        };
        Service::new(
            cfg.clone(),
            ServiceDependencies {
                transcriber,
                visual_sampler: Arc::new(FfmpegVisualSampler {
                    executable: cfg.media.ffmpeg_path.clone(),
                }),
                signal_extractor: Arc::new(FfmpegSignalExtractor {
                    executable: cfg.media.ffmpeg_path.clone(),
                }),
                editorial,
                audio_analyzer,
                renderer: Arc::new(FfmpegRenderer {
                    executable: cfg.media.ffmpeg_path.clone(),
                }),
                object_store,
                publishers: build_publishers(cfg)?,
            },
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[error("job cancelled")]
pub struct Cancelled;

pub fn is_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Cancelled>().is_some()
}

fn add_summary(total: &mut RunSummary, pass: &RunSummary) {
    total.windows_observed += pass.windows_observed;
    total.candidates_reviewed += pass.candidates_reviewed;
    total.candidates_accepted += pass.candidates_accepted;
    total.candidates_rejected += pass.candidates_rejected;
    total.posts_completed += pass.posts_completed;
    total.publish_failures += pass.publish_failures;
}

async fn probe_duration_ms(config: &Config, input: &str) -> Result<i64> {
    let output = tokio::process::Command::new(&config.media.ffprobe_path)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            "--",
            input,
        ])
        .kill_on_drop(true)
        .output()
        .await
        .context("run ffprobe")?;
    ensure!(
        output.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let seconds: f64 = String::from_utf8(output.stdout)?.trim().parse()?;
    ensure!(
        seconds.is_finite() && seconds > 0.0,
        "invalid media duration"
    );
    Ok((seconds * 1_000.0) as i64)
}

fn validate_channel(channel: &str) -> Result<()> {
    ensure!(
        !channel.is_empty()
            && channel
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "invalid Twitch channel login"
    );
    Ok(())
}

fn read_secret(variable: &str) -> Result<String> {
    ensure!(
        !variable.is_empty(),
        "secret environment-variable name is empty"
    );
    let value = std::env::var(variable)
        .with_context(|| format!("required secret {variable} is not set"))?;
    ensure!(
        !value.trim().is_empty(),
        "required secret {variable} is empty"
    );
    Ok(value)
}

fn build_publishers(cfg: &Config) -> Result<Vec<Arc<dyn Publisher>>> {
    let mut publishers: Vec<Arc<dyn Publisher>> = Vec::new();
    let flags = [
        (cfg.publishers.youtube, "youtube", false),
        (cfg.publishers.instagram, "instagram", false),
        (cfg.publishers.tiktok_drafts, "tiktok", true),
        (cfg.publishers.twitch_clips, "twitch", false),
    ];
    if cfg.publishers.dry_run {
        for (enabled, name, draft_only) in flags {
            if enabled {
                publishers.push(Arc::new(DryRunPublisher {
                    name: name.to_owned(),
                    draft_only,
                }));
            }
        }
        return Ok(publishers);
    }
    if cfg.publishers.youtube {
        publishers.push(Arc::new(YouTubePublisher {
            access_token: read_secret(&cfg.publishers.youtube_token_env)?,
            privacy_status: "public".to_owned(),
            work_dir: cfg.data_dir.join("http"),
        }));
    }
    if cfg.publishers.instagram {
        ensure!(
            cfg.staging.provider == "s3"
                || cfg
                    .staging
                    .public_base_url
                    .as_deref()
                    .is_some_and(|url| url.starts_with("https://")),
            "Instagram requires S3 staging or an HTTPS local staging URL"
        );
        publishers.push(Arc::new(InstagramPublisher {
            access_token: read_secret(&cfg.publishers.instagram_token_env)?,
            account_id: read_secret(&cfg.publishers.instagram_account_env)?,
            poll_attempts: 60,
        }));
    }
    if cfg.publishers.tiktok_drafts {
        publishers.push(Arc::new(TikTokDraftPublisher {
            access_token: read_secret(&cfg.publishers.tiktok_token_env)?,
        }));
    }
    if cfg.publishers.twitch_clips {
        publishers.push(Arc::new(TwitchClipPublisher::new(
            read_secret(&cfg.publishers.twitch_token_env)?,
            read_secret(&cfg.publishers.twitch_client_id_env)?,
        )));
    }
    Ok(publishers)
}
