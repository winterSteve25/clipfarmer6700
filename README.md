# ClipFarmer 6700

ClipFarmer is an always-on Twitch monitor that turns synchronized stream evidence into edited vertical clips. Local Whisper supplies the complete transcript; GPT-5.6 Terra watches overlapping windows; GPT-5.6 Sol independently directs, edits, and critiques each matured candidate; GPT Audio 1.5 is used only for candidate tone and nonverbal events. A clip is never accepted by a chat spike or audio heuristic alone.

Accepted moments render as clean 1080×1920 H.264/AAC MP4 files. YouTube and Instagram adapters publish immediately when production credentials are configured. TikTok deliberately uploads to the creator inbox as a draft for manual completion.

## Runtime flow

```text
streamlink + chat_downloader
          │
          ▼
growing local capture and timestamped chat
          │
          ├── local whisper.cpp transcript
          ├── ffmpeg chronological frames
          └── raw chat plus local signal summaries
                         │
              Terra overlapping observer
                         │ 20-second maturation delay
                         ▼
        GPT Audio annotation → Sol director → Sol editor → Sol critic
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
- `whisper-cli` from whisper.cpp and a local `large-v3-turbo` GGML model.
- `streamlink` and `chat_downloader` for live Twitch sessions.
- `curl` for OpenAI and platform HTTP APIs.
- `aws` CLI when `[staging].provider = "s3"`.
- `OPENAI_API_KEY` for non-deterministic analysis.

GPT-5.6 Sol supports image input and structured output but not audio/video input, so the program sends ordered image samples and local transcripts through the Responses API. Candidate audio uses the Chat Completions audio-input format documented for `gpt-audio-1.5`.

## Quick start

```bash
cargo build --release
./target/release/clipfarmer init
# Edit clipfarmer.toml, install local tools, and set OPENAI_API_KEY.
./target/release/clipfarmer run --channel CHANNEL_LOGIN
```

Replay a VOD through the identical editorial path:

```bash
./target/release/clipfarmer replay --channel CHANNEL_LOGIN --input /absolute/path/vod.mp4
```

For an offline plumbing test, `--deterministic-models` replaces only the hosted editorial/audio calls. Local Whisper, frame extraction, rendering, persistence, staging, and dry-run publishing still execute normally.

Other commands:

```bash
clipfarmer status
clipfarmer auth youtube
clipfarmer profile rebuild --channel CHANNEL --from profile.json
clipfarmer outcomes import metrics.jsonl
```

`auth` reports the credential environment variables required by a platform. OAuth consent and token refresh remain the responsibility of the platform developer application; secrets are never persisted by ClipFarmer.

## Configuration

See [`clipfarmer.toml.example`](clipfarmer.toml.example). Model identifiers are validated so a stale configuration cannot silently replace the intended architecture:

- Observer: `gpt-5.6-terra`
- Director/editor/critic: `gpt-5.6-sol`
- Candidate audio: `gpt-audio-1.5`
- Continuous transcription: local `whisper-cli`, never a hosted transcription endpoint

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
