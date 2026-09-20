# Twitch Capture

The Tauri backend embeds ClipFarmer as the internal Rust library at
`src-tauri/clipfarmer`; it does not launch or depend on the standalone CLI.

Backend commands exposed to the future frontend:

- `start_channel_clipping_job({ channel, config, deterministicModels })`
- `start_vod_clipping_job({ vodUrl, config, deterministicModels })`
- `cancel_clipping_job({ jobId })`
- `retry_clipping_job({ jobId })`
- `get_clipping_job({ jobId })`
- `list_clipping_jobs()`

Both start commands immediately return a job snapshot containing `id` and
`outputDir`. Background updates are emitted as `clipfarmer-job-progress` with
the complete current job snapshot, including status, phase, elapsed time,
captured duration, summary counts, and any terminal error. Jobs may also be
polled with `get_clipping_job` so UI state can recover after a reload.

Failed or cancelled VOD jobs can be resumed from Job History. Resume reuses the
job's cached media and SQLite state, rewinds only enough to rebuild the observer
window and candidate maturation context, and continues from the last completed
window. Previously stored candidates are protected by their idempotency keys.

Configuration is supplied by the frontend for each start command. Passing an
empty `config` object uses the defaults formerly documented in the example
TOML. The frontend can select the bundled `large_turbo` or `tiny` Scribble
transcription model with `scribble.modelVariant`. Job data directories and
model paths remain backend-controlled: every job receives an isolated directory
under app data, while both Whisper models and the Silero VAD model are loaded
from bundled resources.

Set `deterministicModels` in the UI (or command arguments) to replace hosted
editorial and audio calls with deterministic local implementations. Scribble
transcription and the rest of the capture, rendering, staging, and publishing
pipeline continue to run normally; no hosted-model API key is required.

The hosted-model defaults use a cost-optimized review profile: frames are
sampled every two seconds and deduplicated, observer images use low detail,
Terra handles observation/direction/editing at low-to-medium reasoning, and Sol
is reserved for the final high-reasoning critic. A confident Director rejection
skips audio analysis and the remaining review stages. OpenAI token usage is
stored per call and shown in job history as an estimated API cost.

## Start the app
`npm run tauri dev`
