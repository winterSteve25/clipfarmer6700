# ClipFarmer 6700

## Runtime flow

```text
streamlink + chat_downloader
          │
          ▼
growing local capture and timestamped chat
          │
          ├── embedded Scribble transcript
          ├── ffmpeg chronological frames
          └── raw chat plus local signal summaries
                         │
          selected-provider observer
                         │ 20-second maturation delay
                         ▼
       audio annotation → director → editor → independent critic
                         │ accepted only by critic
                         ▼
             ffmpeg vertical edit + captions
                         │
             per-platform idempotent jobs
         YouTube public │ Instagram Reel │ TikTok draft
```

The same path handles a live capture and an offline replay. Live scans include 120 seconds of overlap so setup and delayed reactions survive pass boundaries; source/time idempotency prevents a repeated scan from producing the same clip twice.

## Prerequisites

- Rust stable with edition 2024 support.
- `ffmpeg` and `ffprobe`.
- Local `large-v3-turbo` Whisper and `silero-v6.2.0` VAD GGML model files for embedded [Scribble](https://github.com/itsmontoya/scribble).
- `streamlink` and `chat_downloader` for live Twitch sessions.
- `curl` for hosted model and platform HTTP APIs.
- `aws` CLI when `[staging].provider = "s3"`.
- `OPENAI_API_KEY` or `GEMINI_API_KEY`, depending on `[models].provider`.

The OpenAI provider sends ordered image samples and local transcripts through the Responses API, then uses `gpt-audio-1.5` only for matured-candidate audio. The Gemini provider uses Generate Content structured outputs for both editorial image analysis and candidate audio analysis. Both implement the same provider-neutral traits, so capture, candidate maturation, rendering, persistence, and publishing are unchanged.

Download Scribble's configured local models once:

```bash
mkdir -p models
curl --fail --location --output models/ggml-large-v3-turbo.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin
curl --fail --location --output models/ggml-silero-v6.2.0.bin \
  https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin
```

## Quick start

```bash
cargo build --release
./target/release/clipfarmer init
cp .env.example .env
# Edit clipfarmer.toml, place both local models under ./models, install local tools,
# and add the API key selected by [models].provider to .env.
./target/release/clipfarmer run --channel CHANNEL_LOGIN
```

For every command except `init`, ClipFarmer loads `.env` from the directory containing the selected `clipfarmer.toml`. Values already present in the process environment take precedence, and malformed `.env` files stop startup with an error instead of being silently ignored. The `.env` file is Git-ignored; `.env.example` lists the supported secret names.

On Apple Silicon, build with `cargo build --release --features metal`. The `coreml`, `cuda`, and `vulkan` features expose Scribble's other acceleration backends. Scribble is embedded as a library: the Whisper model loads once when the service starts and is reused across transcription windows; no `whisper-cli` executable is installed or spawned.

Replay a VOD through the identical editorial path:

```bash
./target/release/clipfarmer replay --channel CHANNEL_LOGIN --input /absolute/path/vod.mp4
```

Or give ClipFarmer a public Twitch VOD URL directly. Streamlink resolves the VOD and channel, while `chat_downloader` saves its timestamped chat. Both are cached under `data/vods/<VOD_ID>/` before the normal replay begins:

```bash
./target/release/clipfarmer replay --vod https://www.twitch.tv/videos/VOD_ID
```

`--input` and `--vod` are mutually exclusive. A local input still requires `--channel`; Twitch VOD replay derives it automatically, and an optional `--channel` value is treated as a consistency check. `--duration-ms` limits analysis but does not currently shorten the initial VOD download.

For an offline plumbing test, `--deterministic-models` replaces only the hosted editorial/audio calls. Embedded Scribble transcription, frame extraction, rendering, persistence, staging, and dry-run publishing still execute normally.

Other commands:

```bash
clipfarmer status
clipfarmer auth youtube
clipfarmer profile rebuild --channel CHANNEL --from profile.json
clipfarmer outcomes import metrics.jsonl
```

`auth` reports the credential environment variables required by a platform. OAuth consent and token refresh remain the responsibility of the platform developer application; secrets are never persisted by ClipFarmer.

Twitch publishing uses the channel login supplied to `run --channel` or `replay --channel`. It resolves and caches Twitch's numeric broadcaster ID automatically, so no broadcaster ID belongs in `.env`.

## Configuration

See [`clipfarmer.toml.example`](clipfarmer.toml.example). Select the hosted model implementation with:

```toml
[models]
provider = "openai" # or "gemini"
```

Provider implementations are isolated in `src/models/openai.rs` and `src/models/gemini.rs`. The defaults are:

- OpenAI observer: `gpt-5.6-terra`
- OpenAI director/editor/critic: `gpt-5.6-sol`
- OpenAI candidate audio: `gpt-audio-1.5`
- Gemini observer and candidate audio: `gemini-3.8-flash`
- Gemini director/editor/critic: `gemini-3.1-pro-preview`
- Continuous transcription: embedded Scribble with a resident local Whisper model and local VAD, never a hosted transcription endpoint

`publishers.dry_run = true` is the safe default. It creates deterministic remote IDs and exercises job recovery without network publication. Set it to `false` only after configuring the individual OAuth tokens.

Instagram fetches the finished media from an HTTPS URL. Production Instagram use therefore requires S3-compatible staging; the adapter uploads privately and asks the AWS CLI for a one-hour presigned URL:

```toml
[staging]
provider = "s3"
bucket = "my-private-upload-bucket"
prefix = "clipfarmer"
# Optional CDN fallback if this S3 implementation cannot create presigned URLs.
# public_base_url = "https://media.example.com"
endpoint_env = "S3_ENDPOINT"
```

The S3 adapter uses the AWS CLI credential chain and supports custom endpoints. A configured public base URL is used only when presigning fails. Bucket lifecycle rules should remove staged objects afterward.

## Persistence and recovery

SQLite runs in WAL mode. It records sessions, discontinuities, transcript segments, chat, visual samples, candidates, all four editorial decisions, edit manifests, model-call metadata, platform jobs/posts, performance snapshots, and versioned channel profiles.

Publication is idempotent per candidate/platform. A partial failure leaves only the failed job retryable; successful platforms are skipped during recovery. Retry exhaustion moves the candidate to `failed`. TikTok success moves it to `awaiting_creator`, not `published`, until the creator finishes the post and reconciles metrics.

Chat and on-screen/transcribed text are serialized as untrusted evidence in a user content item. Model authority stays in the separate instructions field. File paths, object keys, edit durations, layouts, output dimensions, and caption ranges are validated before local execution.

## Testing

```bash
cargo fmt --check
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
```

Tests cover timeline discontinuities, transcript-overlap deduplication, candidate maturation and merging, the 60-second cap, manifest/path validation, prompt-injection containment, normalized feedback, full observer-to-publish replay, TikTok’s manual state, and partial-publisher retry without duplicate successful posts.

Live publishing still depends on platform-side prerequisites: a verified YouTube API project for public uploads, a Meta professional account with content-publish permission, a TikTok Content Posting application authorized for inbox uploads, and valid Twitch clip permissions. Those external approvals cannot be supplied or tested by this repository.
