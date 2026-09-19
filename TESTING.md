# ClipFarmer Backend Testing

Run commands from:

```text
/home/mahd/clipfarmer6700/src-tauri/clipfarmer
```

## Prerequisites

Install the local tools used by the backend:

- Rust and Cargo
- `ffmpeg` and `ffprobe`
- `streamlink`
- `chat_downloader` for VOD chat downloads
- Whisper GGML model files
- Silero VAD GGML model file

The live test uses the native Twitch IRC WebSocket collector and does not use `chat_downloader`.

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
| `CLIPFARMER_CHAT_DOWNLOADER` | No | VOD chat downloader executable path. Defaults to `chat_downloader`. |
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

### Twitch API Connectivity Test

This performs a read-only Twitch Helix channel lookup:

```bash
cargo test \
  --features twitch-api-tests \
  --lib twitch_api_tests \
  -- --nocapture
```

### Real VOD Ingestion Test

This downloads the selected VOD and its chat, then runs local frame sampling, signal extraction, and Scribble transcription:

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
