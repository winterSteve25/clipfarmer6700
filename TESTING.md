# ClipFarmer Backend Testing

Run library test commands from:

```text
/home/mahd/clipfarmer6700/src-tauri/clipfarmer
```

Run the direct VOD-slice command from the repository root:

```text
/home/mahd/clipfarmer6700
```

## Prerequisites

Install the local tools used by the backend:

- Rust and Cargo
- `ffmpeg` and `ffprobe`
- `streamlink`
- `TwitchDownloaderCLI` 1.56.5 or newer for VOD chat downloads
- Whisper GGML model files
- Silero VAD GGML model file

The obsolete Python `chat_downloader` 0.2.8 does not work with Twitch's current GraphQL persisted queries. Do not use it for VOD tests. The live test uses the native Twitch IRC WebSocket collector and does not use `TwitchDownloaderCLI`.

Install TwitchDownloaderCLI on Linux x64 without root access:

```bash
mkdir -p "$HOME/.local/share/clipfarmer/TwitchDownloaderCLI-1.56.5"
curl -fL -o /tmp/TwitchDownloaderCLI.zip \
  https://github.com/lay295/TwitchDownloader/releases/download/1.56.5/TwitchDownloaderCLI-1.56.5-Linux-x64.zip
unzip -q -o /tmp/TwitchDownloaderCLI.zip \
  -d "$HOME/.local/share/clipfarmer/TwitchDownloaderCLI-1.56.5"
chmod +x "$HOME/.local/share/clipfarmer/TwitchDownloaderCLI-1.56.5/TwitchDownloaderCLI"
ln -sfn "$HOME/.local/share/clipfarmer/TwitchDownloaderCLI-1.56.5/TwitchDownloaderCLI" \
  "$HOME/.local/bin/TwitchDownloaderCLI"
TwitchDownloaderCLI --version
```

## Model Files

Create the model directory from the crate directory:

```bash
mkdir -p models

curl -L -o models/ggml-large-v3-turbo-q5_0.bin \
  'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin?download=true'

curl -L -o models/ggml-silero-v6.2.0.bin \
  'https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin?download=true'
```

The first file is the transcription model. The second is the voice-activity-detection model.

## Environment Variables

### OpenAI Pipeline Runs

The direct VOD runner reads the OpenAI key from `CLIPFARMER_OPENAI_KEY`:

```bash
read -rsp 'OpenAI API key: ' CLIPFARMER_OPENAI_KEY
printf '\n'
export CLIPFARMER_OPENAI_KEY
```

Do not put the API key in command history, JSON configuration, source files, or test logs.

### Twitch API Test

These are used only by the opt-in read-only Twitch API test:

| Variable | Meaning |
| --- | --- |
| `TWITCH_ACCESS_TOKEN` | Fresh Twitch OAuth access token. Use the raw `access_token` value, without `Bearer ` or `oauth:`. |
| `TWITCH_CLIENT_ID` | Twitch application client ID matching the access token. |
| `TWITCH_TEST_CHANNEL` | Twitch login name, such as `ludwig`; do not use `@ludwig` or a URL. |

Set them without printing secrets:

```bash
export TWITCH_CLIENT_ID='your-client-id'
read -rsp 'Twitch access token: ' TWITCH_ACCESS_TOKEN
printf '\n'
export TWITCH_ACCESS_TOKEN
export TWITCH_TEST_CHANNEL='channel_login'
```

Do not commit or paste the access token or client secret. The client secret is not needed by the test after the access token has been generated.

### VOD Ingestion Test

| Variable | Required | Meaning |
| --- | --- | --- |
| `TWITCH_TEST_VOD_URL` | Yes | Public Twitch VOD URL, for example `https://www.twitch.tv/videos/123456789`. |
| `CLIPFARMER_WHISPER_MODEL` | Yes | Path to `ggml-large-v3-turbo-q5_0.bin`. |
| `CLIPFARMER_VAD_MODEL` | Yes | Path to `ggml-silero-v6.2.0.bin`. |
| `CLIPFARMER_TEST_START_MS` | No | Start offset into the VOD in milliseconds. Defaults to `0`. |
| `CLIPFARMER_TEST_WINDOW_MS` | No | Evidence window length in milliseconds. Defaults to `30000`. |
| `CLIPFARMER_STREAMLINK` | No | Streamlink executable path. Defaults to `streamlink`. |
| `CLIPFARMER_CHAT_DOWNLOADER` | No | VOD chat downloader executable path. Defaults to `TwitchDownloaderCLI`. |
| `CLIPFARMER_FFMPEG` | No | FFmpeg executable path. Defaults to `ffmpeg`. |

### Live Ingestion Test

| Variable | Required | Meaning |
| --- | --- | --- |
| `TWITCH_TEST_CHANNEL` | Yes | A channel that is live, using its Twitch login name. |
| `CLIPFARMER_WHISPER_MODEL` | Yes | Path to the Whisper GGML model. |
| `CLIPFARMER_VAD_MODEL` | Yes | Path to the Silero VAD GGML model. |
| `CLIPFARMER_LIVE_SECONDS` | No | Capture length. Defaults to `45`; minimum is `10`. |
| `CLIPFARMER_KEEP_LIVE_ARTIFACTS` | No | Set to any value to preserve captured files and print their directory. |
| `CLIPFARMER_STREAMLINK` | No | Streamlink executable path. Defaults to `streamlink`. |
| `CLIPFARMER_FFMPEG` | No | FFmpeg executable path. Defaults to `ffmpeg`. |
| `CLIPFARMER_FFPROBE` | No | FFprobe executable path. Defaults to `ffprobe`. |

The live test captures chat through Twitch IRC over WebSocket. It does not require `TWITCH_ACCESS_TOKEN`.

### Cargo Binding Workaround

The root `.cargo/config.toml` sets:

```text
WHISPER_DONT_GENERATE_BINDINGS=1
```

This makes `whisper-rs-sys` use its bundled bindings. It avoids the incompatible bindgen output produced by the local Clang/glibc combination. Do not remove this setting unless the toolchain issue is addressed separately.

If stale generated bindings are already present in `target`, rebuild that dependency once:

```bash
cargo clean -p whisper-rs-sys
```

## Build and Test Commands

From the crate directory:

```bash
# Format check
cargo fmt --check

# Compile the default library tests without running them
cargo test --lib --no-run

# Compile a gated test without running its network-dependent body
cargo test --lib --features twitch-live-ingestion-tests --no-run

# Run the normal offline suite
cargo test --lib
```

Normal tests use deterministic fakes and local fixtures. They do not call Twitch, OpenAI, or Gemini.

## Full OpenAI VOD-Slice Pipeline

From the repository root, run the complete observer, director, editor, critic, rendering, staging, and dry-run publishing pipeline on a bounded VOD interval:

```bash
cargo run --manifest-path src-tauri/clipfarmer/Cargo.toml \
  --bin clipfarmer-vod -- \
  --url 'https://www.twitch.tv/videos/VIDEO_ID' \
  --start '02:30:50' \
  --end '02:34:50' \
  --no-visuals \
  --data-dir './data' 2>&1 | tee clipfarmer-run.log
```

The runner accepts `HH:MM:SS`, `MM:SS`, fractional seconds, or bare seconds. The selected interval must be at least five seconds.

The runner uses OpenAI, candidate-audio analysis, chat evidence, and dry-run publishing by default. Available flags:

| Flag | Effect |
| --- | --- |
| `--no-visuals` | Do not sample or send JPEG frames; rendering uses `full_frame`. |
| `--no-audio` | Do not send candidate WAV audio to the hosted audio model. |
| `--no-chat` | Skip VOD chat evidence entirely. |
| `--data-dir PATH` | Select the cache, database, staging, and output root. Defaults to `./data`. |
| `--publish` | Use configured real publishers instead of dry-run publishers. Omit this during evaluation. |

Without `--publish`, an accepted candidate still traverses the publishing boundary but reports `status dry_run`; no remote post is created.

### Expected Decision Output

The terminal distinguishes observer detection, review stages, and the exact final source interval:

```text
Tracking candidate 78e7499e (02:31:04 → 02:32:02)
Running director (...) — accepted at 88% confidence
Running editor (...) — accepted at 91% confidence
Running critic (...) — accepted at 86% confidence
Candidate accepted: Example title (02:31:08 → 02:31:54)
Publishing 02:31:08 → 02:31:54 to youtube — status dry_run
```

If an optional model-proposed alternative is outside the candidate bounds, it is discarded and reported. The chosen primary cut remains strictly validated:

```text
Discarded 1 out-of-bounds director alternative(s)
```

The final summary reports observed windows, reviewed candidates, accepted/rejected candidates, and completed posts. The critic only runs after the observer produces a candidate and the candidate matures.

### GPU Transcription

The default build uses CPU transcription. Enable one Scribble backend at compile time:

```bash
# NVIDIA CUDA
cargo run --manifest-path src-tauri/clipfarmer/Cargo.toml \
  --features cuda --bin clipfarmer-vod -- \
  --url 'https://www.twitch.tv/videos/VIDEO_ID' \
  --start '02:30:50' --end '02:34:50' --no-visuals

# Vulkan-capable GPU
cargo run --manifest-path src-tauri/clipfarmer/Cargo.toml \
  --features vulkan --bin clipfarmer-vod -- \
  --url 'https://www.twitch.tv/videos/VIDEO_ID' \
  --start '02:30:50' --end '02:34:50' --no-visuals
```

CUDA requires a compatible NVIDIA driver and CUDA toolkit/runtime. GPU features accelerate local Scribble transcription; OpenAI calls remain remote and FFmpeg work is generally CPU-bound.

### Slice Cache Behavior

VOD slice runs download only the selected HLS range plus 120 seconds of pre-roll. Pre-roll supplies decoder/keyframe context. ClipFarmer probes the MPEG-TS start timestamp and translates absolute VOD timestamps into relative FFmpeg seeks.

Artifacts are stored under:

```text
data/vods/VIDEO_ID/
  channel.txt
  source-START_MS-END_MS-preroll.ts
  source-START_MS-END_MS-preroll.chat.jsonl
```

TwitchDownloaderCLI downloads only the selected chat interval. Its rich `comments[]` JSON is normalized into timestamped JSONL for the evidence pipeline. Empty chat sidecars are treated as failed cache entries and retried.

### Twitch API Connectivity Test

This performs a read-only Twitch Helix channel lookup:

```bash
cargo test \
  --features twitch-api-tests \
  --lib twitch_api_tests \
  -- --nocapture
```

### Real VOD Ingestion Test

This downloads the selected VOD and its chat with TwitchDownloaderCLI, then runs local frame sampling, signal extraction, and Scribble transcription:

```bash
export TWITCH_TEST_VOD_URL='https://www.twitch.tv/videos/VIDEO_ID'
export CLIPFARMER_WHISPER_MODEL="$PWD/models/ggml-large-v3-turbo-q5_0.bin"
export CLIPFARMER_VAD_MODEL="$PWD/models/ggml-silero-v6.2.0.bin"
export CLIPFARMER_TEST_START_MS=600000
export CLIPFARMER_TEST_WINDOW_MS=30000

cargo test \
  --features twitch-ingestion-tests \
  --lib twitch_ingestion_tests \
  -- --nocapture
```

Use a public VOD with chat replay available. The VOD test may download the full recording before analyzing the selected window.

### Troubleshooting VOD Slice Runs

#### Repeated `Analyzing VOD slice` Lines

The runtime emits a pulse every two seconds while local transcription, FFmpeg, or a hosted model call is active. This does not by itself indicate a stall. A four-minute interval with the default six-second observer step creates 40 overlapping analysis windows. CPU transcription with the large-turbo model can take roughly one minute per window.

#### Chat Reports Zero Messages

Check the chat sidecar size and message count:

```bash
ls -lh data/vods/VIDEO_ID/*chat.jsonl
wc -l data/vods/VIDEO_ID/*chat.jsonl
```

The old Python Chat Downloader fails with `PersistedQueryNotFound` and must not be used. Verify the maintained tool:

```bash
TwitchDownloaderCLI --version
TwitchDownloaderCLI chatdownload \
  --id VIDEO_ID \
  --output /tmp/chat.json \
  --beginning 9050s \
  --ending 9290s \
  --collision Overwrite \
  --banner false
```

#### FFmpeg Produces No Frames

Inspect the partial TS timeline:

```bash
ffprobe -v error \
  -show_entries format=start_time,duration \
  -of default=noprint_wrappers=1 \
  data/vods/VIDEO_ID/source-START_MS-END_MS-preroll.ts
```

Partial Twitch MPEG-TS files commonly begin at a non-zero timestamp. ClipFarmer carries that offset into transcription, signal extraction, frame sampling, candidate-audio extraction, and rendering. Do not replace a `-preroll.ts` artifact with a manually trimmed zero-context TS file.

### Real Live Ingestion Test

This captures only a short live window. The channel must currently be live:

```bash
export TWITCH_TEST_CHANNEL='channel_login'
export CLIPFARMER_WHISPER_MODEL="$PWD/models/ggml-large-v3-turbo-q5_0.bin"
export CLIPFARMER_VAD_MODEL="$PWD/models/ggml-silero-v6.2.0.bin"
export CLIPFARMER_LIVE_SECONDS=45
export CLIPFARMER_KEEP_LIVE_ARTIFACTS=1

cargo test \
  --features twitch-live-ingestion-tests \
  --lib twitch_live_ingestion_tests \
  -- --nocapture
```

The test reports media size, duration, chat records, frame count, transcript segments, audio energy, and scene-change rate.

## Inspecting Live Artifacts

When `CLIPFARMER_KEEP_LIVE_ARTIFACTS` is set, the test prints a directory such as `/tmp/clipfarmer-live-...` containing:

- `capture.ts`: captured live media
- `capture.chat.jsonl`: captured chat records
- `frames/`: sampled JPEG frames
- `transcript.json`: local transcription output
- `signals.json`: audio and scene-change measurements

Inspect the results:

```bash
find /tmp/clipfarmer-live-* -maxdepth 3 -type f
jq . /tmp/clipfarmer-live-XXXX/transcript.json
jq . /tmp/clipfarmer-live-XXXX/signals.json
head -20 /tmp/clipfarmer-live-XXXX/capture.chat.jsonl
ffplay /tmp/clipfarmer-live-XXXX/capture.ts
```

The temporary artifact directory is deleted automatically when `CLIPFARMER_KEEP_LIVE_ARTIFACTS` is not set.

## Feature Summary

| Feature | Behavior | Runs by default? |
| --- | --- | --- |
| `twitch-api-tests` | Read-only Twitch Helix channel lookup | No |
| `twitch-ingestion-tests` | Full VOD download plus local evidence gathering | No |
| `twitch-live-ingestion-tests` | Short live capture plus local evidence gathering | No |
