//! Cancellable host runtime for live and VOD processing jobs.
//! This module translates job requests into configured pipeline services and progress events.
//! It is the integration boundary for GUI or other hosts that need progress and cancellation.

use crate::{
    Service, ServiceDependencies,
    adapters::{
        DryRunPublisher, FfmpegRenderer, FfmpegSignalExtractor, FfmpegVisualSampler,
        LocalObjectStore, ObjectStore, Publisher, S3CommandStore, ScribbleTranscriber,
        TwitchCapture, TwitchChatCapture, TwitchVodSource,
    },
    config::{Config, ModelProvider, ScribbleModel},
    editorial::{
        CandidateAudioAnalyzer, DeterministicAudioAnnotation, DeterministicEditorial,
        EditorialModel,
    },
    models::{
        gemini::{GeminiAudioAnalyzer, GeminiEditorial},
        openai::{OpenAiAudioAnalyzer, OpenAiEditorial},
    },
    pipeline::RunSummary,
    progress,
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
    Channel {
        channel: String,
    },
    Vod {
        url: String,
    },
    VodSlice {
        url: String,
        start_ms: i64,
        end_ms: i64,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub phase: String,
    pub message: String,
    pub elapsed_ms: u64,
    pub captured_ms: Option<i64>,
    pub completed_units: Option<usize>,
    pub total_units: Option<usize>,
    pub transferred_bytes: Option<u64>,
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
    pub estimated_api_cost_usd: Option<f64>,
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
            estimated_api_cost_usd: value.estimated_api_cost_usd,
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
    pub large_turbo_transcription: PathBuf,
    pub tiny_transcription: PathBuf,
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
        config.scribble.model_path = match config.scribble.model_variant {
            ScribbleModel::LargeTurbo => model_paths.large_turbo_transcription,
            ScribbleModel::Tiny => model_paths.tiny_transcription,
        };
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
        resume_from_ms: i64,
    ) -> Result<RunResult> {
        match source {
            JobSource::Channel { channel } => self.run_live(&channel, &mut cancellation).await,
<<<<<<< HEAD
            JobSource::Vod { url } => self.run_vod(&url, &mut cancellation).await,
            JobSource::VodSlice {
                url,
                start_ms,
                end_ms,
            } => {
                self.run_vod_slice(&url, start_ms, end_ms, &mut cancellation)
                    .await
            }
        }
    }

    async fn run_vod(&self, url: &str, cancellation: &mut Cancellation) -> Result<RunResult> {
        self.run_vod_window(url, None, None, cancellation).await
    }

    async fn run_vod_slice(
        &self,
        url: &str,
        start_ms: i64,
        end_ms: i64,
        cancellation: &mut Cancellation,
    ) -> Result<RunResult> {
        self.run_vod_window(url, Some(start_ms), Some(end_ms), cancellation)
            .await
    }

    async fn run_vod_window(
        &self,
        url: &str,
        requested_start_ms: Option<i64>,
        requested_end_ms: Option<i64>,
        cancellation: &mut Cancellation,
=======
            JobSource::Vod { url } => self.run_vod(&url, &mut cancellation, resume_from_ms).await,
        }
    }

    async fn run_vod(
        &self,
        url: &str,
        cancellation: &mut Cancellation,
        resume_from_ms: i64,
>>>>>>> realui
    ) -> Result<RunResult> {
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
<<<<<<< HEAD
        let prepared = match (requested_start_ms, requested_end_ms) {
            (Some(start_ms), Some(end_ms)) => tokio::select! {
                result = source.prepare_slice(url, None, start_ms, end_ms) => result?,
                _ = cancellation.cancelled() => anyhow::bail!(Cancelled),
            },
            (None, None) => tokio::select! {
                result = source.prepare(url, None) => result?,
                _ = cancellation.cancelled() => anyhow::bail!(Cancelled),
            },
            _ => anyhow::bail!("VOD slice start and end must be provided together"),
=======
        let prepared = tokio::select! {
            result = source.prepare_with_progress(url, None, |transferred_bytes| {
                self.report_download_progress(transferred_bytes);
            }) => result?,
            _ = cancellation.cancelled() => anyhow::bail!(Cancelled),
>>>>>>> realui
        };
        self.report(
            "preparing_vod",
            format!("VOD {} downloaded", prepared.id),
            None,
            None,
        );
        let input = prepared.media_path.to_string_lossy().into_owned();
        let channel = prepared.channel_login;
        let (media_start_ms, media_end_ms) = tokio::select! {
            result = probe_media_bounds(&self.config, &input) => result?,
            _ = cancellation.cancelled() => anyhow::bail!(Cancelled),
        };
        let requested_start = requested_start_ms.unwrap_or(media_start_ms);
        let requested_end = requested_end_ms.unwrap_or(media_end_ms);
        let start_ms = requested_start.max(media_start_ms);
        let end_ms = requested_end.min(media_end_ms);
        if start_ms != requested_start || end_ms != requested_end {
            progress::warning(format!(
                "downloaded media bounds are {} → {}; analyzing the available overlap",
                progress::timestamp(media_start_ms),
                progress::timestamp(media_end_ms)
            ));
        }
        validate_vod_window(media_end_ms, start_ms, end_ms)?;
        let service = self.build_service(media_start_ms)?;
        self.report(
            "analyzing",
<<<<<<< HEAD
            if requested_start_ms.is_some() {
                format!(
                    "Analyzing VOD slice {} → {}",
                    progress::timestamp(start_ms),
                    progress::timestamp(end_ms)
=======
            if resume_from_ms > 0 {
                format!(
                    "Resuming VOD analysis near {} seconds",
                    resume_from_ms / 1_000
>>>>>>> realui
                )
            } else {
                "Analyzing the downloaded VOD".to_owned()
            },
<<<<<<< HEAD
            Some(end_ms - start_ms),
            None,
        );
        let mut work = Box::pin(service.scan_file(&channel, &input, end_ms, start_ms));
        let mut pulse = tokio::time::interval(Duration::from_secs(2));
        let summary = loop {
            tokio::select! {
                result = &mut work => break result?,
                _ = cancellation.cancelled() => anyhow::bail!(Cancelled),
                _ = pulse.tick() => self.report(
                    "analyzing",
                    if requested_start_ms.is_some() {
                        format!(
                            "Analyzing VOD slice {} → {}",
                            progress::timestamp(start_ms),
                            progress::timestamp(end_ms)
                        )
                    } else {
                        "Analyzing the downloaded VOD".to_owned()
                    },
                    Some(end_ms - start_ms),
                    None,
                ),
            }
=======
            Some(duration),
            None,
        );
        let mut work = Box::pin(service.scan_file_with_progress(
            &channel,
            &input,
            duration,
            resume_from_ms.min(duration).max(0),
            |completed, total, summary| {
                self.report_progress(
                    "analyzing",
                    format!("Analyzed window {completed} of {total}"),
                    Some(duration),
                    Some(summary),
                    completed,
                    total,
                );
            },
        ));
        let summary = tokio::select! {
            result = &mut work => result?,
            _ = cancellation.cancelled() => anyhow::bail!(Cancelled),
>>>>>>> realui
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
            "starting_capture",
            format!("Starting Twitch video capture for {channel}"),
            None,
            None,
        );
        let mut video = TwitchCapture {
            streamlink: self.config.media.streamlink_path.clone(),
        }
        .start(channel, &capture_path)
        .await?;
        self.report(
            "starting_chat",
            format!("Starting Twitch chat capture for {channel}"),
            None,
            None,
        );
        let mut chat = TwitchChatCapture {
            executable: self.config.media.chat_downloader_path.clone(),
        }
        .start(channel, &capture_path.with_extension("chat.jsonl"))?;
        let service = self.build_service(0)?;
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
                        let scan = service.scan_file_with_progress(
                            channel,
                            &input,
                            duration,
                            scan_start,
                            |completed, total, summary| {
                                self.report_progress(
                                    "analyzing",
                                    format!("Analyzed window {completed} of {total}"),
                                    Some(duration),
                                    Some(summary),
                                    completed,
                                    total,
                                );
                            },
                        );
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
                    let scan = service.scan_file_with_progress(
                        channel,
                        &input,
                        duration,
                        analyzed_through.saturating_sub(120_000),
                        |completed, total, summary| {
                            self.report_progress(
                                "analyzing",
                                format!("Analyzed window {completed} of {total}"),
                                Some(duration),
                                Some(summary),
                                completed,
                                total,
                            );
                        },
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
            completed_units: None,
            total_units: None,
            transferred_bytes: None,
            summary: summary.map(RunSummaryDto::from),
        });
    }

    fn report_download_progress(&self, transferred_bytes: u64) {
        (self.progress)(JobProgress {
            phase: "preparing_vod".to_owned(),
            message: "Downloading the Twitch VOD".to_owned(),
            elapsed_ms: self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            captured_ms: None,
            completed_units: None,
            total_units: None,
            transferred_bytes: Some(transferred_bytes),
            summary: None,
        });
    }

    fn report_progress(
        &self,
        phase: impl Into<String>,
        message: impl Into<String>,
        captured_ms: Option<i64>,
        summary: Option<&RunSummary>,
        completed_units: usize,
        total_units: usize,
    ) {
        (self.progress)(JobProgress {
            phase: phase.into(),
            message: message.into(),
            elapsed_ms: self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            captured_ms,
            completed_units: Some(completed_units),
            total_units: Some(total_units),
            transferred_bytes: None,
            summary: summary.map(RunSummaryDto::from),
        });
    }

    fn build_service(&self, media_start_ms: i64) -> Result<Service> {
        let cfg = &self.config;
        self.report(
            "loading_transcription",
            format!(
                "Loading {:?} transcription model",
                cfg.scribble.model_variant
            ),
            None,
            None,
        );
        let transcriber = Arc::new(ScribbleTranscriber::new(
            cfg.media.ffmpeg_path.clone(),
            &cfg.scribble.model_path,
            &cfg.scribble.vad_model_path,
            cfg.data_dir.join("scribble"),
            &cfg.scribble.language,
            cfg.scribble.enable_vad,
            cfg.scribble.incremental_min_window_seconds,
            media_start_ms,
        )?);
        self.report(
            "configuring_models",
            if self.deterministic_models {
                "Selecting deterministic editorial models".to_owned()
            } else {
                format!("Configuring {:?} editorial models", cfg.models.provider)
            },
            None,
            None,
        );
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
                            db_path: cfg.db_path(),
                        }),
                        Arc::new(OpenAiAudioAnalyzer {
                            api_key,
                            model: cfg.openai.audio_model.clone(),
                            ffmpeg: cfg.media.ffmpeg_path.clone(),
                            work_dir: cfg.data_dir.join("audio-analysis/openai"),
<<<<<<< HEAD
                            media_start_ms,
=======
                            db_path: cfg.db_path(),
>>>>>>> realui
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
                            media_start_ms,
                        }),
                    )
                }
            }
        };
        self.report(
            "configuring_staging",
            format!("Configuring {} object staging", cfg.staging.provider),
            None,
            None,
        );
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
        self.report(
            "configuring_publishers",
            if cfg.publishers.dry_run {
                "Configuring publishers in dry-run mode"
            } else {
                "Configuring live publishers"
            },
            None,
            None,
        );
        let publishers = build_publishers(cfg)?;
        self.report(
            "opening_database",
            "Opening the pipeline database",
            None,
            None,
        );
        Service::new(
            cfg.clone(),
            ServiceDependencies {
                transcriber,
                visual_sampler: Arc::new(FfmpegVisualSampler {
                    executable: cfg.media.ffmpeg_path.clone(),
                    media_start_ms,
                }),
                signal_extractor: Arc::new(FfmpegSignalExtractor {
                    executable: cfg.media.ffmpeg_path.clone(),
                    media_start_ms,
                }),
                editorial,
                audio_analyzer,
                renderer: Arc::new(FfmpegRenderer {
                    executable: cfg.media.ffmpeg_path.clone(),
                    media_start_ms,
                }),
                object_store,
                publishers,
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
    if let Some(cost) = pass.estimated_api_cost_usd {
        total.estimated_api_cost_usd =
            Some(total.estimated_api_cost_usd.unwrap_or_default() + cost);
    }
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

async fn probe_media_bounds(config: &Config, input: &str) -> Result<(i64, i64)> {
    let output = tokio::process::Command::new(&config.media.ffprobe_path)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=start_time,duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            "--",
            input,
        ])
        .kill_on_drop(true)
        .output()
        .await
        .context("run ffprobe for media bounds")?;
    ensure!(
        output.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout)?;
    let values = stdout
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    ensure!(values.len() >= 2, "ffprobe did not return media bounds");
    let start_seconds: f64 = values[0].parse().context("parse media start time")?;
    let duration_seconds: f64 = values[1].parse().context("parse media duration")?;
    ensure!(
        start_seconds.is_finite() && start_seconds >= 0.0,
        "invalid media start time"
    );
    ensure!(
        duration_seconds.is_finite() && duration_seconds > 0.0,
        "invalid media duration"
    );
    let start_ms = (start_seconds * 1_000.0).round() as i64;
    let duration_ms = (duration_seconds * 1_000.0).round() as i64;
    Ok((start_ms, start_ms.saturating_add(duration_ms)))
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

fn validate_vod_window(duration_ms: i64, start_ms: i64, end_ms: i64) -> Result<()> {
    ensure!(start_ms >= 0, "VOD slice start must not be negative");
    ensure!(end_ms > start_ms, "VOD slice end must be after its start");
    ensure!(
        end_ms - start_ms >= 5_000,
        "VOD slice must be at least five seconds"
    );
    ensure!(end_ms <= duration_ms, "VOD slice exceeds the VOD duration");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_handle_marks_receiver_cancelled() {
        let (handle, cancellation) = CancellationHandle::new();
        assert!(!cancellation.is_cancelled());
        handle.cancel();
        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn summary_conversion_preserves_all_counts() {
        let summary = RunSummary {
            windows_observed: 1,
            candidates_reviewed: 2,
            candidates_accepted: 3,
            candidates_rejected: 4,
            posts_completed: 5,
            publish_failures: 6,
        };

        let dto = RunSummaryDto::from(&summary);
        assert_eq!(dto.windows_observed, 1);
        assert_eq!(dto.candidates_reviewed, 2);
        assert_eq!(dto.candidates_accepted, 3);
        assert_eq!(dto.candidates_rejected, 4);
        assert_eq!(dto.posts_completed, 5);
        assert_eq!(dto.publish_failures, 6);
    }

    #[test]
    fn channel_validation_rejects_values_that_could_escape_source_paths() {
        assert!(validate_channel("streamer_42").is_ok());
        assert!(validate_channel("").is_err());
        assert!(validate_channel("streamer/name").is_err());
        assert!(validate_channel("streamer name").is_err());
    }

    #[test]
    fn vod_slice_validation_requires_a_bounded_five_second_window() {
        assert!(validate_vod_window(120_000, 30_000, 45_000).is_ok());
        assert!(validate_vod_window(120_000, -1, 45_000).is_err());
        assert!(validate_vod_window(120_000, 45_000, 45_000).is_err());
        assert!(validate_vod_window(120_000, 30_000, 34_999).is_err());
        assert!(validate_vod_window(120_000, 30_000, 120_001).is_err());
    }

    #[test]
    fn vod_slice_job_source_uses_snake_case_payload_fields() {
        let source = JobSource::VodSlice {
            url: "https://www.twitch.tv/videos/123456789".to_owned(),
            start_ms: 30_000,
            end_ms: 45_000,
        };
        let payload = serde_json::to_value(source).unwrap();
        assert_eq!(payload["type"], "vod_slice");
        assert_eq!(payload["start_ms"], 30_000);
        assert_eq!(payload["end_ms"], 45_000);
    }

    #[test]
    fn deterministic_mode_builds_fake_publishers_without_provider_credentials() {
        let mut config = Config::default();
        config.publishers.dry_run = true;
        config.publishers.youtube = true;
        config.publishers.instagram = false;
        config.publishers.tiktok_drafts = false;
        config.publishers.twitch_clips = false;

        let publishers = build_publishers(&config).unwrap();
        assert_eq!(publishers.len(), 1);
        assert_eq!(publishers[0].platform(), "youtube");
    }
}
