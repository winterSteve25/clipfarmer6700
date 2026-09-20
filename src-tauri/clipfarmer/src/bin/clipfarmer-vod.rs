use anyhow::{Context, Result, bail, ensure};
use clipfarmer::{
    CancellationHandle, JobProgress, JobSource, LibraryRunner, ModelPaths,
    config::{Config, ModelProvider},
};
use std::{env, path::PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    let options = Options::parse(env::args().skip(1))?;
    let mut config = Config::default();
    config.models.provider = ModelProvider::OpenAi;
    config.openai.api_key_env = "CLIPFARMER_OPENAI_KEY".to_owned();
    config.models.visual_evidence = !options.no_visuals;
    config.models.audio_analysis = !options.no_audio;
    config.models.chat_evidence = !options.no_chat;
    config.publishers.dry_run = !options.publish;

    let model_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models");
    let runner = LibraryRunner::load(
        config,
        options.data_dir,
        ModelPaths {
            transcription: model_root.join("ggml-large-v3-turbo-q5_0.bin"),
            voice_activity_detection: model_root.join("ggml-silero-v6.2.0.bin"),
        },
        false,
        print_progress,
    )?;
    let (_cancellation_handle, cancellation) = CancellationHandle::new();
    let result = runner
        .run(
            JobSource::VodSlice {
                url: options.url,
                start_ms: options.start_ms,
                end_ms: options.end_ms,
            },
            cancellation,
        )
        .await?;

    println!(
        "completed: {} windows, {} reviewed, {} accepted, {} rejected, {} posts",
        result.summary.windows_observed,
        result.summary.candidates_reviewed,
        result.summary.candidates_accepted,
        result.summary.candidates_rejected,
        result.summary.posts_completed,
    );
    Ok(())
}

fn print_progress(progress: JobProgress) {
    println!("[{}] {}", progress.phase, progress.message);
}

struct Options {
    url: String,
    start_ms: i64,
    end_ms: i64,
    data_dir: PathBuf,
    no_visuals: bool,
    no_audio: bool,
    no_chat: bool,
    publish: bool,
}

impl Options {
    fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self> {
        let mut url = None;
        let mut start = None;
        let mut end = None;
        let mut data_dir = PathBuf::from("./data");
        let mut no_visuals = false;
        let mut no_audio = false;
        let mut no_chat = false;
        let mut publish = false;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--url" => url = Some(next_value(&mut arguments, "--url")?),
                "--start" => start = Some(next_value(&mut arguments, "--start")?),
                "--end" => end = Some(next_value(&mut arguments, "--end")?),
                "--data-dir" => data_dir = PathBuf::from(next_value(&mut arguments, "--data-dir")?),
                "--no-visuals" => no_visuals = true,
                "--no-audio" => no_audio = true,
                "--no-chat" => no_chat = true,
                "--publish" => publish = true,
                "--help" | "-h" => {
                    println!("{}", usage());
                    std::process::exit(0);
                }
                unknown => bail!("unknown argument {unknown}\n\n{}", usage()),
            }
        }

        let url = url.context("missing --url")?;
        ensure!(
            url.starts_with("https://www.twitch.tv/videos/")
                || url.starts_with("https://twitch.tv/videos/"),
            "--url must be a Twitch VOD URL"
        );
        let start_ms = parse_timestamp(&start.context("missing --start")?)?;
        let end_ms = parse_timestamp(&end.context("missing --end")?)?;
        ensure!(end_ms > start_ms, "--end must be after --start");
        ensure!(
            end_ms - start_ms >= 5_000,
            "the selected range must be at least five seconds"
        );

        Ok(Self {
            url,
            start_ms,
            end_ms,
            data_dir,
            no_visuals,
            no_audio,
            no_chat,
            publish,
        })
    }
}

fn next_value(arguments: &mut impl Iterator<Item = String>, option: &str) -> Result<String> {
    arguments
        .next()
        .with_context(|| format!("missing value for {option}"))
}

fn parse_timestamp(value: &str) -> Result<i64> {
    let parts = value.split(':').collect::<Vec<_>>();
    ensure!((1..=3).contains(&parts.len()), "invalid timestamp {value}");
    let seconds = parts
        .last()
        .context("timestamp has no seconds")?
        .parse::<f64>()?;
    ensure!(
        seconds.is_finite() && seconds >= 0.0,
        "invalid timestamp {value}"
    );
    if parts.len() > 1 {
        ensure!(seconds < 60.0, "invalid timestamp {value}");
    }

    let minutes = if parts.len() >= 2 {
        parts[parts.len() - 2].parse::<i64>()?
    } else {
        0
    };
    ensure!((0..60).contains(&minutes), "invalid timestamp {value}");

    let hours = if parts.len() == 3 {
        parts[0].parse::<i64>()?
    } else {
        0
    };
    ensure!(hours >= 0, "invalid timestamp {value}");

    Ok(((hours as f64 * 3_600.0 + minutes as f64 * 60.0 + seconds) * 1_000.0).round() as i64)
}

fn usage() -> &'static str {
    "Usage: cargo run --manifest-path src-tauri/clipfarmer/Cargo.toml --bin clipfarmer-vod -- \\
  --url https://www.twitch.tv/videos/123456789 \\
  --start 01:23:45 \\
  --end 01:24:15 \\
  [--data-dir ./data] [--no-visuals] [--no-audio] [--no-chat] [--publish]"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clock_timestamps() {
        assert_eq!(parse_timestamp("01:23:45").unwrap(), 5_025_000);
        assert_eq!(parse_timestamp("1:24:15.500").unwrap(), 5_055_500);
        assert_eq!(parse_timestamp("90").unwrap(), 90_000);
    }
}
