# Twitch Capture

The Tauri backend embeds ClipFarmer as the internal Rust library at
`src-tauri/clipfarmer`; it does not launch or depend on the standalone CLI.

Backend commands exposed to the future frontend:

- `start_channel_clipping_job({ channel, config })`
- `start_vod_clipping_job({ vodUrl, config })`
- `cancel_clipping_job({ jobId })`
- `get_clipping_job({ jobId })`
- `list_clipping_jobs()`

Both start commands immediately return a job snapshot containing `id` and
`outputDir`. Background updates are emitted as `clipfarmer-job-progress` with
the complete current job snapshot, including status, phase, elapsed time,
captured duration, summary counts, and any terminal error. Jobs may also be
polled with `get_clipping_job` so UI state can recover after a reload.

Configuration is supplied by the frontend for each start command. Passing an
empty `config` object uses the defaults formerly documented in the example
TOML. Job data directories and model paths are backend-controlled: every job
receives an isolated directory under app data, while the large-turbo and tiny
Whisper models plus the Silero VAD model are loaded from bundled resources.

## Start the app
`npm run tauri dev`
