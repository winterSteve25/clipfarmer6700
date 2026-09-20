use clipfarmer::{
    config::Config,
    runtime::{is_cancelled, RunSummaryDto},
    CancellationHandle, JobProgress, JobSource, LibraryRunner, ModelPaths,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{path::BaseDirectory, AppHandle, Emitter, Manager, State};

pub const JOB_PROGRESS_EVENT: &str = "clipfarmer-job-progress";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub id: String,
    pub source: JobSource,
    pub channel: Option<String>,
    pub output_dir: PathBuf,
    pub status: JobStatus,
    pub progress: JobProgress,
    pub summary: Option<RunSummaryDto>,
    pub error: Option<String>,
    pub created_at_ms: u64,
    pub finished_at_ms: Option<u64>,
}

struct JobEntry {
    snapshot: JobSnapshot,
    cancellation: CancellationHandle,
}

#[derive(Clone)]
pub struct JobManager {
    jobs_root: PathBuf,
    model_paths: ModelPaths,
    jobs: Arc<Mutex<HashMap<String, JobEntry>>>,
}

impl JobManager {
    pub fn new(jobs_root: PathBuf, model_paths: ModelPaths) -> Self {
        Self {
            jobs_root,
            model_paths,
            jobs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn start(
        &self,
        app: AppHandle,
        source: JobSource,
        config: Config,
    ) -> Result<JobSnapshot, String> {
        validate_source(&source)?;
        config
            .validate()
            .map_err(|error| format!("invalid clipping configuration: {error:#}"))?;
        let id = uuid::Uuid::new_v4().to_string();
        let job_root = self.jobs_root.join(&id);
        let output_dir = job_root.join("outputs");
        fs::create_dir_all(&output_dir)
            .map_err(|error| format!("create job output directory: {error}"))?;
        let (cancellation, receiver) = CancellationHandle::new();
        let snapshot = JobSnapshot {
            id: id.clone(),
            source: source.clone(),
            channel: match &source {
                JobSource::Channel { channel } => Some(channel.clone()),
                JobSource::Vod { .. } | JobSource::VodSlice { .. } => None,
            },
            output_dir,
            status: JobStatus::Queued,
            progress: JobProgress {
                phase: "queued".to_owned(),
                message: "Job queued".to_owned(),
                elapsed_ms: 0,
                captured_ms: None,
                summary: None,
            },
            summary: None,
            error: None,
            created_at_ms: now_ms(),
            finished_at_ms: None,
        };
        self.jobs
            .lock()
            .map_err(|_| "job state is unavailable".to_owned())?
            .insert(
                id.clone(),
                JobEntry {
                    snapshot: snapshot.clone(),
                    cancellation,
                },
            );
        emit_snapshot(&app, &snapshot);

        let manager = self.clone();
        let model_paths = self.model_paths.clone();
        std::thread::Builder::new()
            .name(format!("clipfarmer-{id}"))
            .spawn(move || {
                manager.update(&app, &id, |job| {
                    if job.status == JobStatus::Queued {
                        job.status = JobStatus::Running;
                        job.progress.phase = "starting".to_owned();
                        job.progress.message = "Initializing ClipFarmer".to_owned();
                    }
                });
                let event_manager = manager.clone();
                let event_app = app.clone();
                let event_id = id.clone();
                let runner =
                    LibraryRunner::load(config, job_root, model_paths, false, move |progress| {
                        event_manager.update(&event_app, &event_id, |job| {
                            job.progress = progress;
                        });
                    });
                let result = runner.and_then(|runner| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(anyhow::Error::from)?;
                    runtime.block_on(runner.run(source, receiver))
                });
                match result {
                    Ok(result) => manager.update(&app, &id, |job| {
                        job.status = JobStatus::Completed;
                        job.channel = Some(result.channel);
                        job.summary = Some(RunSummaryDto::from(&result.summary));
                        job.progress.phase = "completed".to_owned();
                        job.progress.message = "Clipping job completed".to_owned();
                        job.progress.summary = job.summary.clone();
                        job.finished_at_ms = Some(now_ms());
                    }),
                    Err(error) if is_cancelled(&error) => manager.update(&app, &id, |job| {
                        job.status = JobStatus::Cancelled;
                        job.progress.phase = "cancelled".to_owned();
                        job.progress.message = "Clipping job cancelled".to_owned();
                        job.finished_at_ms = Some(now_ms());
                    }),
                    Err(error) => manager.update(&app, &id, |job| {
                        job.status = JobStatus::Failed;
                        job.progress.phase = "failed".to_owned();
                        job.progress.message = "Clipping job failed".to_owned();
                        job.error = Some(format!("{error:#}"));
                        job.finished_at_ms = Some(now_ms());
                    }),
                }
            })
            .map_err(|error| format!("start clipping worker: {error}"))?;

        Ok(snapshot)
    }

    fn update(&self, app: &AppHandle, id: &str, change: impl FnOnce(&mut JobSnapshot)) {
        let snapshot = self.jobs.lock().ok().and_then(|mut jobs| {
            let entry = jobs.get_mut(id)?;
            change(&mut entry.snapshot);
            Some(entry.snapshot.clone())
        });
        if let Some(snapshot) = snapshot {
            emit_snapshot(app, &snapshot);
        }
    }

    fn cancel(&self, app: &AppHandle, id: &str) -> Result<JobSnapshot, String> {
        let snapshot = {
            let mut jobs = self
                .jobs
                .lock()
                .map_err(|_| "job state is unavailable".to_owned())?;
            let entry = jobs
                .get_mut(id)
                .ok_or_else(|| format!("unknown clipping job {id}"))?;
            if matches!(
                entry.snapshot.status,
                JobStatus::Queued | JobStatus::Running
            ) {
                entry.cancellation.cancel();
                entry.snapshot.status = JobStatus::Cancelling;
                entry.snapshot.progress.phase = "cancelling".to_owned();
                entry.snapshot.progress.message = "Stopping clipping job".to_owned();
            }
            entry.snapshot.clone()
        };
        emit_snapshot(app, &snapshot);
        Ok(snapshot)
    }

    fn get(&self, id: &str) -> Result<JobSnapshot, String> {
        self.jobs
            .lock()
            .map_err(|_| "job state is unavailable".to_owned())?
            .get(id)
            .map(|entry| entry.snapshot.clone())
            .ok_or_else(|| format!("unknown clipping job {id}"))
    }

    fn list(&self) -> Result<Vec<JobSnapshot>, String> {
        let mut jobs = self
            .jobs
            .lock()
            .map_err(|_| "job state is unavailable".to_owned())?
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect::<Vec<_>>();
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at_ms));
        Ok(jobs)
    }
}

#[tauri::command]
pub fn start_channel_clipping_job(
    app: AppHandle,
    state: State<'_, JobManager>,
    channel: String,
    config: Config,
) -> Result<JobSnapshot, String> {
    state.start(app, JobSource::Channel { channel }, config)
}

#[tauri::command]
pub fn start_vod_clipping_job(
    app: AppHandle,
    state: State<'_, JobManager>,
    vod_url: String,
    config: Config,
) -> Result<JobSnapshot, String> {
    state.start(app, JobSource::Vod { url: vod_url }, config)
}

#[tauri::command]
pub fn cancel_clipping_job(
    app: AppHandle,
    state: State<'_, JobManager>,
    job_id: String,
) -> Result<JobSnapshot, String> {
    state.cancel(&app, &job_id)
}

#[tauri::command]
pub fn get_clipping_job(
    state: State<'_, JobManager>,
    job_id: String,
) -> Result<JobSnapshot, String> {
    state.get(&job_id)
}

#[tauri::command]
pub fn list_clipping_jobs(state: State<'_, JobManager>) -> Result<Vec<JobSnapshot>, String> {
    state.list()
}

pub fn manager_for(app: &AppHandle) -> Result<JobManager, Box<dyn std::error::Error>> {
    let model_paths = bundled_model_paths(app)?;
    let jobs_root = app.path().app_data_dir()?.join("clipfarmer").join("jobs");
    fs::create_dir_all(&jobs_root)?;
    Ok(JobManager::new(jobs_root, model_paths))
}

fn bundled_model_paths(app: &AppHandle) -> Result<ModelPaths, Box<dyn std::error::Error>> {
    const MODEL_DIR: &str = "scribble-models";
    let resolve = |name: &str| {
        app.path()
            .resolve(PathBuf::from(MODEL_DIR).join(name), BaseDirectory::Resource)
    };
    let paths = ModelPaths {
        transcription: resolve("ggml-large-v3-turbo-q5_0.bin")?,
        voice_activity_detection: resolve("ggml-silero-v6.2.0.bin")?,
    };
    for path in [&paths.transcription, &paths.voice_activity_detection] {
        if !path.is_file() {
            return Err(format!(
                "bundled ClipFarmer model is missing at {}; check src-tauri/resources/scribble-models",
                path.display()
            )
            .into());
        }
    }
    Ok(paths)
}

fn validate_source(source: &JobSource) -> Result<(), String> {
    match source {
        JobSource::Channel { channel }
            if !channel.is_empty()
                && channel
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_') =>
        {
            Ok(())
        }
        JobSource::Channel { .. } => Err("invalid Twitch channel login".to_owned()),
        JobSource::Vod { url }
            if url.starts_with("https://www.twitch.tv/videos/")
                || url.starts_with("https://twitch.tv/videos/") =>
        {
            Ok(())
        }
        JobSource::Vod { .. } => Err("invalid Twitch VOD URL".to_owned()),
        JobSource::VodSlice {
            url,
            start_ms,
            end_ms,
        } if (url.starts_with("https://www.twitch.tv/videos/")
            || url.starts_with("https://twitch.tv/videos/"))
            && *start_ms >= 0
            && *end_ms > *start_ms
            && end_ms.saturating_sub(*start_ms) >= 5_000 =>
        {
            Ok(())
        }
        JobSource::VodSlice { .. } => Err("invalid Twitch VOD slice".to_owned()),
    }
}

fn emit_snapshot(app: &AppHandle, snapshot: &JobSnapshot) {
    let _ = app.emit(JOB_PROGRESS_EVENT, snapshot);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_channel_and_vod_sources() {
        assert!(validate_source(&JobSource::Channel {
            channel: "twitch_dev".into()
        })
        .is_ok());
        assert!(validate_source(&JobSource::Channel {
            channel: "bad/name".into()
        })
        .is_err());
        assert!(validate_source(&JobSource::Vod {
            url: "https://www.twitch.tv/videos/123".into()
        })
        .is_ok());
        assert!(validate_source(&JobSource::Vod {
            url: "https://example.com/123".into()
        })
        .is_err());
        assert!(validate_source(&JobSource::VodSlice {
            url: "https://www.twitch.tv/videos/123".into(),
            start_ms: 30_000,
            end_ms: 45_000,
        })
        .is_ok());
        assert!(validate_source(&JobSource::VodSlice {
            url: "https://www.twitch.tv/videos/123".into(),
            start_ms: 30_000,
            end_ms: 34_999,
        })
        .is_err());
    }
}
