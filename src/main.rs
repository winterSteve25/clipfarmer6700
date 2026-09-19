use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use clipfarmer6700::{
    Service, ServiceDependencies,
    adapters::{
        DryRunPublisher, FfmpegRenderer, FfmpegSignalExtractor, FfmpegVisualSampler,
        LocalObjectStore, ObjectStore, Publisher, S3CommandStore, ScribbleTranscriber,
        TwitchCapture, TwitchChatCapture,
    },
    config::{Config, ModelProvider},
    domain::{ChannelProfile, OutcomeMetrics},
    editorial::{
        CandidateAudioAnalyzer, DeterministicAudioAnnotation, DeterministicEditorial,
        EditorialModel,
    },
    models::{
        gemini::{GeminiAudioAnalyzer, GeminiEditorial},
        openai::{OpenAiAudioAnalyzer, OpenAiEditorial},
    },
    publishers::{InstagramPublisher, TikTokDraftPublisher, TwitchClipPublisher, YouTubePublisher},
    store::Store,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Parser)]
#[command(name = "clipfarmer", about = "Always-on multimodal Twitch clip editor")]
struct Cli {
    #[arg(long, default_value = "clipfarmer.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
    Run {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        deterministic_models: bool,
    },
    Replay {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        input: String,
        #[arg(long)]
        duration_ms: Option<i64>,
        #[arg(long)]
        deterministic_models: bool,
    },
    Auth {
        platform: Platform,
    },
    Status,
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    Outcomes {
        #[command(subcommand)]
        command: OutcomesCommand,
    },
}

#[derive(Subcommand)]
enum ProfileCommand {
    Rebuild {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        from: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum OutcomesCommand {
    Import { file: PathBuf },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Platform {
    Youtube,
    Instagram,
    Tiktok,
    Twitch,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Command::Init) {
        fs::write(&cli.config, include_str!("../clipfarmer.toml.example"))?;
        println!("wrote {}", cli.config.display());
        return Ok(());
    }
    load_dotenv(&cli.config)?;
    let cfg = Config::load(&cli.config)?;
    match cli.command {
        Command::Run {
            channel,
            deterministic_models,
        } => run_live(cfg, &channel, deterministic_models).await,
        Command::Replay {
            channel,
            input,
            duration_ms,
            deterministic_models,
        } => {
            let duration = match duration_ms {
                Some(value) => value,
                None => probe_duration_ms(&cfg, &input).await?,
            };
            let service = build_service(cfg, deterministic_models)?;
            let summary = service.replay_file(&channel, &input, duration).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "windows_observed":summary.windows_observed,
                    "candidates_reviewed":summary.candidates_reviewed,
                    "candidates_accepted":summary.candidates_accepted,
                    "candidates_rejected":summary.candidates_rejected,
                    "posts_completed":summary.posts_completed,
                    "publish_failures":summary.publish_failures
                }))?
            );
            Ok(())
        }
        Command::Auth { platform } => auth_status(&cfg, platform),
        Command::Status => {
            let store = Store::open(cfg.db_path())?;
            for (state, count) in store.status_counts()? {
                println!("{state}: {count}");
            }
            Ok(())
        }
        Command::Profile {
            command: ProfileCommand::Rebuild { channel, from },
        } => {
            let store = Store::open(cfg.db_path())?;
            let mut profile = match from {
                Some(path) => serde_json::from_slice::<ChannelProfile>(&fs::read(path)?)?,
                None => store
                    .profile(&channel)?
                    .unwrap_or_else(|| ChannelProfile::empty(&channel)),
            };
            profile.channel_id = channel;
            profile.version = profile.version.saturating_add(1);
            store.upsert_profile(&profile)?;
            println!("stored channel profile version {}", profile.version);
            Ok(())
        }
        Command::Outcomes {
            command: OutcomesCommand::Import { file },
        } => {
            let store = Store::open(cfg.db_path())?;
            let raw = fs::read_to_string(file)?;
            let mut imported = 0;
            for line in raw.lines().filter(|line| !line.trim().is_empty()) {
                let metrics: OutcomeMetrics = serde_json::from_str(line)?;
                store.record_metrics(&metrics)?;
                imported += 1;
            }
            println!("imported {imported} outcome snapshots");
            Ok(())
        }
        Command::Init => Ok(()),
    }
}

fn load_dotenv(config_path: &Path) -> Result<()> {
    let config_dir = config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let path = config_dir.join(".env");
    match dotenvy::from_path(&path) {
        Ok(()) => Ok(()),
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("load {}", path.display())),
    }
}

fn build_service(cfg: Config, deterministic_models: bool) -> Result<Service> {
    let transcriber = Arc::new(ScribbleTranscriber::new(
        cfg.media.ffmpeg_path.clone(),
        &cfg.scribble.model_path,
        &cfg.scribble.vad_model_path,
        cfg.data_dir.join("scribble"),
        &cfg.scribble.language,
        cfg.scribble.enable_vad,
        cfg.scribble.incremental_min_window_seconds,
    )?);
    let visual_sampler = Arc::new(FfmpegVisualSampler {
        executable: cfg.media.ffmpeg_path.clone(),
    });
    let (editorial, audio_analyzer): (Arc<dyn EditorialModel>, Arc<dyn CandidateAudioAnalyzer>) =
        if deterministic_models {
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
    let publishers = build_publishers(&cfg)?;
    Service::new(
        cfg.clone(),
        ServiceDependencies {
            transcriber,
            visual_sampler,
            signal_extractor: Arc::new(FfmpegSignalExtractor {
                executable: cfg.media.ffmpeg_path.clone(),
            }),
            editorial,
            audio_analyzer,
            renderer: Arc::new(FfmpegRenderer {
                executable: cfg.media.ffmpeg_path.clone(),
            }),
            object_store,
            publishers,
        },
    )
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
            "Instagram requires S3 staging or a local staging server with an HTTPS base URL"
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

async fn run_live(cfg: Config, channel: &str, deterministic_models: bool) -> Result<()> {
    ensure!(
        !channel.is_empty()
            && channel
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_'),
        "invalid Twitch channel login"
    );
    let capture_path = cfg
        .data_dir
        .join("live")
        .join(channel)
        .join(format!("{}.ts", uuid::Uuid::new_v4()));
    let capture = TwitchCapture {
        streamlink: cfg.media.streamlink_path.clone(),
    };
    let mut child = capture.start(channel, &capture_path).await?;
    let chat_path = capture_path.with_extension("chat.jsonl");
    let mut chat_child = TwitchChatCapture {
        executable: cfg.media.chat_downloader_path.clone(),
    }
    .start(channel, &chat_path)?;
    let service = build_service(cfg.clone(), deterministic_models)?;
    let mut analyzed_through = 0_i64;
    loop {
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result?;
                child.kill().await.ok();
                chat_child.kill().await.ok();
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(cfg.worker.poll_seconds.max(5))) => {
                if let Some(status) = child.try_wait()? {
                    ensure!(status.success(), "streamlink exited with {status}");
                    chat_child.kill().await.ok();
                    break;
                }
                let Some(path) = capture_path.to_str() else { anyhow::bail!("capture path is not UTF-8") };
                let Ok(duration) = probe_duration_ms(&cfg, path).await else { continue };
                let minimum_advance = cfg.worker.observer_step_seconds as i64 * 1_000;
                if duration >= analyzed_through + minimum_advance {
                    let scan_start = analyzed_through.saturating_sub(120_000);
                    match service.scan_file(channel, path, duration, scan_start).await {
                        Ok(summary) => {
                            analyzed_through = duration;
                            println!("observed={} accepted={} posts={}", summary.windows_observed, summary.candidates_accepted, summary.posts_completed);
                        }
                        Err(error) => eprintln!("analysis pass failed and will retry: {error:#}"),
                    }
                }
            }
        }
    }
    if capture_path.exists() {
        let path = capture_path.to_string_lossy();
        if let Ok(duration) = probe_duration_ms(&cfg, &path).await
            && duration > analyzed_through
        {
            let _ = service
                .scan_file(
                    channel,
                    &path,
                    duration,
                    analyzed_through.saturating_sub(120_000),
                )
                .await;
        }
    }
    Ok(())
}

async fn probe_duration_ms(cfg: &Config, input: &str) -> Result<i64> {
    let output = tokio::process::Command::new(&cfg.media.ffprobe_path)
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

fn auth_status(cfg: &Config, platform: Platform) -> Result<()> {
    let variables: Vec<&str> = match platform {
        Platform::Youtube => vec![&cfg.publishers.youtube_token_env],
        Platform::Instagram => vec![
            &cfg.publishers.instagram_token_env,
            &cfg.publishers.instagram_account_env,
        ],
        Platform::Tiktok => vec![&cfg.publishers.tiktok_token_env],
        Platform::Twitch => vec![
            &cfg.publishers.twitch_token_env,
            &cfg.publishers.twitch_client_id_env,
        ],
    };
    for variable in variables {
        println!(
            "{variable}: {}",
            if std::env::var(variable).is_ok_and(|value| !value.trim().is_empty()) {
                "configured"
            } else {
                "missing"
            }
        );
    }
    println!(
        "OAuth consent must be completed with the platform's own developer application; tokens are read from the named environment variables and are never stored in SQLite."
    );
    Ok(())
}
