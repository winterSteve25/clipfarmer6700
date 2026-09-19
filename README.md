# Twitch Capture

The Tauri backend embeds ClipFarmer as the internal Rust library at
`src-tauri/clipfarmer`; it does not launch or depend on the standalone CLI.

Backend commands exposed to the future frontend:

- `start_channel_clipping_job({ channel })`
- `start_vod_clipping_job({ vodUrl })`
- `cancel_clipping_job({ jobId })`
- `get_clipping_job({ jobId })`
- `list_clipping_jobs()`

Both start commands immediately return a job snapshot containing `id` and
`outputDir`. Background updates are emitted as `clipfarmer-job-progress` with
the complete current job snapshot, including status, phase, elapsed time,
captured duration, summary counts, and any terminal error. Jobs may also be
polled with `get_clipping_job` so UI state can recover after a reload.

On first launch, the backend writes its default `clipfarmer.toml` to the app
configuration directory. Set `CLIPFARMER_CONFIG` to use another config file.
Every job receives an isolated data/output directory under the app data
directory. The large-turbo and tiny Whisper models plus the Silero VAD model are bundled from
`src-tauri/resources/scribble-models`; their resolved application-resource
paths override the model paths in the runtime configuration.

## Start the app
`npm run tauri dev`
