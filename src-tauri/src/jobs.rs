use clipfarmer::{
    config::Config,
    runtime::{is_cancelled, RunSummaryDto},
    CancellationHandle, JobProgress, JobSource, LibraryRunner, ModelPaths,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
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
    pub history: Vec<JobProgress>,
    pub summary: Option<RunSummaryDto>,
    pub error: Option<String>,
    pub created_at_ms: u64,
    pub finished_at_ms: Option<u64>,
}

struct JobEntry {
    snapshot: JobSnapshot,
    cancellation: CancellationHandle,
    config: Config,
    deterministic_models: bool,
    scan_start_ms: i64,
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
        deterministic_models: bool,
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
        let queued_progress = JobProgress {
            phase: "queued".to_owned(),
            message: "Waiting for the background worker to start".to_owned(),
            elapsed_ms: 0,
            captured_ms: None,
            completed_units: None,
            total_units: None,
            transferred_bytes: None,
            summary: None,
        };
        let snapshot = JobSnapshot {
            id: id.clone(),
            source: source.clone(),
            channel: match &source {
                JobSource::Channel { channel } => Some(channel.clone()),
                JobSource::Vod { .. } | JobSource::VodSlice { .. } => None,
            },
            output_dir,
            status: JobStatus::Queued,
            progress: queued_progress.clone(),
            history: vec![queued_progress],
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
                    config: config.clone(),
                    deterministic_models,
                    scan_start_ms: 0,
                },
            );
        emit_snapshot(&app, &snapshot);
        self.spawn_worker(
            app,
            id,
            source,
            config,
            deterministic_models,
            job_root,
            receiver,
            0,
            RunSummaryDto::default(),
        )?;

        Ok(snapshot)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_worker(
        &self,
        app: AppHandle,
        id: String,
        source: JobSource,
        config: Config,
        deterministic_models: bool,
        job_root: PathBuf,
        receiver: clipfarmer::Cancellation,
        resume_from_ms: i64,
        base_summary: RunSummaryDto,
    ) -> Result<(), String> {
        let manager = self.clone();
        let model_paths = self.model_paths.clone();
        std::thread::Builder::new()
            .name(format!("clipfarmer-{id}"))
            .spawn(move || {
                manager.update(&app, &id, |job| {
                    if job.status == JobStatus::Queued {
                        job.status = JobStatus::Running;
                        job.progress.phase = "starting".to_owned();
                        job.progress.message = if resume_from_ms > 0 {
                            format!("Resuming near {} seconds", resume_from_ms / 1_000)
                        } else {
                            "Initializing ClipFarmer".to_owned()
                        };
                        record_progress(job);
                    }
                });
                let event_manager = manager.clone();
                let event_app = app.clone();
                let event_id = id.clone();
                let progress_base = base_summary.clone();
                let runner = LibraryRunner::load(
                    config,
                    job_root,
                    model_paths,
                    deterministic_models,
                    move |mut progress| {
                        if let Some(summary) = progress.summary.as_mut() {
                            add_summary_dto(summary, &progress_base);
                        } else {
                            progress.summary = Some(progress_base.clone());
                        }
                        event_manager.update(&event_app, &event_id, |job| {
                            job.progress = progress;
                            record_progress(job);
                        });
                    },
                );
                let result = runner.and_then(|runner| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(anyhow::Error::from)?;
                    runtime.block_on(runner.run(source, receiver, resume_from_ms))
                });
                match result {
                    Ok(result) => manager.update(&app, &id, |job| {
                        let mut summary = RunSummaryDto::from(&result.summary);
                        add_summary_dto(&mut summary, &base_summary);
                        job.status = JobStatus::Completed;
                        job.channel = Some(result.channel);
                        job.summary = Some(summary);
                        job.progress.phase = "completed".to_owned();
                        job.progress.message = "Clipping job completed".to_owned();
                        job.progress.summary = job.summary.clone();
                        job.finished_at_ms = Some(now_ms());
                        record_progress(job);
                    }),
                    Err(error) if is_cancelled(&error) => manager.update(&app, &id, |job| {
                        job.status = JobStatus::Cancelled;
                        job.progress.phase = "cancelled".to_owned();
                        job.progress.message = "Clipping job cancelled".to_owned();
                        job.finished_at_ms = Some(now_ms());
                        record_progress(job);
                    }),
                    Err(error) => manager.update(&app, &id, |job| {
                        job.status = JobStatus::Failed;
                        job.progress.phase = "failed".to_owned();
                        job.progress.message = "Clipping job failed".to_owned();
                        job.error = Some(format!("{error:#}"));
                        job.finished_at_ms = Some(now_ms());
                        record_progress(job);
                    }),
                }
            })
            .map_err(|error| format!("start clipping worker: {error}"))?;
        Ok(())
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
                record_progress(&mut entry.snapshot);
            }
            entry.snapshot.clone()
        };
        emit_snapshot(app, &snapshot);
        Ok(snapshot)
    }

    fn retry(&self, app: AppHandle, id: &str) -> Result<JobSnapshot, String> {
        let (
            snapshot,
            source,
            config,
            deterministic_models,
            receiver,
            resume_from_ms,
            base_summary,
        ) = {
            let mut jobs = self
                .jobs
                .lock()
                .map_err(|_| "job state is unavailable".to_owned())?;
            let entry = jobs
                .get_mut(id)
                .ok_or_else(|| format!("unknown clipping job {id}"))?;
            if !matches!(
                entry.snapshot.status,
                JobStatus::Failed | JobStatus::Cancelled
            ) {
                return Err("only failed or cancelled jobs can be resumed".to_owned());
            }
            if !matches!(&entry.snapshot.source, JobSource::Vod { .. }) {
                return Err(
                    "live channel captures cannot be resumed; retry is available for VOD jobs"
                        .to_owned(),
                );
            }
            let base_summary = entry
                .snapshot
                .summary
                .clone()
                .or_else(|| entry.snapshot.progress.summary.clone())
                .unwrap_or_default();
            let resume_from_ms =
                resume_start_ms(&entry.snapshot.progress, &entry.config, entry.scan_start_ms);
            let (cancellation, receiver) = CancellationHandle::new();
            entry.cancellation = cancellation;
            entry.scan_start_ms = resume_from_ms;
            entry.snapshot.status = JobStatus::Queued;
            entry.snapshot.error = None;
            entry.snapshot.finished_at_ms = None;
            entry.snapshot.progress = JobProgress {
                phase: "queued".to_owned(),
                message: format!(
                    "Queued to resume near {} seconds using cached media",
                    resume_from_ms / 1_000
                ),
                elapsed_ms: 0,
                captured_ms: entry.snapshot.progress.captured_ms,
                completed_units: None,
                total_units: None,
                transferred_bytes: None,
                summary: Some(base_summary.clone()),
            };
            record_progress(&mut entry.snapshot);
            (
                entry.snapshot.clone(),
                entry.snapshot.source.clone(),
                entry.config.clone(),
                entry.deterministic_models,
                receiver,
                resume_from_ms,
                base_summary,
            )
        };
        emit_snapshot(&app, &snapshot);
        self.spawn_worker(
            app,
            id.to_owned(),
            source,
            config,
            deterministic_models,
            self.jobs_root.join(id),
            receiver,
            resume_from_ms,
            base_summary,
        )?;
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
    deterministic_models: Option<bool>,
) -> Result<JobSnapshot, String> {
    state.start(
        app,
        JobSource::Channel { channel },
        config,
        deterministic_models.unwrap_or(false),
    )
}

#[tauri::command]
pub fn start_vod_clipping_job(
    app: AppHandle,
    state: State<'_, JobManager>,
    vod_url: String,
    config: Config,
    deterministic_models: Option<bool>,
) -> Result<JobSnapshot, String> {
    state.start(
        app,
        JobSource::Vod { url: vod_url },
        config,
        deterministic_models.unwrap_or(false),
    )
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
pub fn retry_clipping_job(
    app: AppHandle,
    state: State<'_, JobManager>,
    job_id: String,
) -> Result<JobSnapshot, String> {
    state.retry(app, &job_id)
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
        large_turbo_transcription: resolve("ggml-large-v3-turbo-q5_0.bin")?,
        tiny_transcription: resolve("ggml-tiny-q5_1.bin")?,
        voice_activity_detection: resolve("ggml-silero-v6.2.0.bin")?,
    };
    for path in [
        &paths.large_turbo_transcription,
        &paths.tiny_transcription,
        &paths.voice_activity_detection,
    ] {
        validate_bundled_model(path)?;
    }
    Ok(paths)
}

fn validate_bundled_model(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !path.is_file() {
        return Err(format!(
            "bundled ClipFarmer model is missing at {}; run `git lfs pull` and restart the app",
            path.display()
        )
        .into());
    }
    let mut header = [0_u8; 64];
    let mut file = fs::File::open(path)?;
    let bytes_read = file.read(&mut header)?;
    let header = &header[..bytes_read];
    if header.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        return Err(format!(
            "bundled ClipFarmer model at {} is only a Git LFS pointer; run `git lfs pull` and restart the app",
            path.display()
        )
        .into());
    }
    if !header.starts_with(b"lmgg") {
        return Err(format!(
            "bundled ClipFarmer model at {} is invalid or incomplete; run `git lfs pull` and restart the app",
            path.display()
        )
        .into());
    }
    Ok(())
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

fn record_progress(job: &mut JobSnapshot) {
    if let Some(last) = job.history.last_mut() {
        if last.phase == job.progress.phase && last.message == job.progress.message {
            *last = job.progress.clone();
            return;
        }
    }
    job.history.push(job.progress.clone());
    const MAX_HISTORY: usize = 200;
    if job.history.len() > MAX_HISTORY {
        job.history.drain(..job.history.len() - MAX_HISTORY);
    }
}

fn resume_start_ms(progress: &JobProgress, config: &Config, scan_start_ms: i64) -> i64 {
    let completed = progress.completed_units.unwrap_or_default() as i64;
    let step_ms = config.worker.observer_step_seconds as i64 * 1_000;
    let overlap_ms = (config.worker.observer_window_seconds
        + config.worker.maturation_delay_seconds) as i64
        * 1_000;
    scan_start_ms
        .saturating_add(completed.saturating_mul(step_ms).saturating_sub(overlap_ms))
        .max(0)
}

fn add_summary_dto(total: &mut RunSummaryDto, additional: &RunSummaryDto) {
    total.windows_observed += additional.windows_observed;
    total.candidates_reviewed += additional.candidates_reviewed;
    total.candidates_accepted += additional.candidates_accepted;
    total.candidates_rejected += additional.candidates_rejected;
    total.posts_completed += additional.posts_completed;
    total.publish_failures += additional.publish_failures;
    if let Some(cost) = additional.estimated_api_cost_usd {
        total.estimated_api_cost_usd =
            Some(total.estimated_api_cost_usd.unwrap_or_default() + cost);
    }
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
    fn bundled_model_validation_rejects_git_lfs_pointers() {
        let root = std::env::temp_dir().join(format!(
            "clipfarmer-model-validation-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let model = root.join("model.bin");
        fs::write(
            &model,
            b"version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 123\n",
        )
        .unwrap();
        let error = validate_bundled_model(&model).unwrap_err().to_string();
        assert!(error.contains("Git LFS pointer"));
        assert!(error.contains("git lfs pull"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bundled_model_validation_accepts_ggml_header() {
        let root = std::env::temp_dir().join(format!(
            "clipfarmer-model-validation-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let model = root.join("model.bin");
        fs::write(&model, b"lmgg-model-data").unwrap();
        validate_bundled_model(&model).unwrap();
        let _ = fs::remove_dir_all(root);
    }

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

    #[test]
    fn resume_rewinds_only_context_from_last_completed_window() {
        let config = Config::default();
        let progress = JobProgress {
            phase: "failed".into(),
            message: String::new(),
            elapsed_ms: 0,
            captured_ms: Some(120_000),
            completed_units: Some(10),
            total_units: Some(20),
            transferred_bytes: None,
            summary: None,
        };

        assert_eq!(resume_start_ms(&progress, &config, 0), 28_000);
        assert_eq!(resume_start_ms(&progress, &config, 28_000), 56_000);
    }

    #[test]
    fn resumed_summary_preserves_prior_work_and_cost() {
        let mut resumed = RunSummaryDto {
            windows_observed: 2,
            estimated_api_cost_usd: Some(0.25),
            ..RunSummaryDto::default()
        };
        let previous = RunSummaryDto {
            windows_observed: 10,
            candidates_accepted: 1,
            estimated_api_cost_usd: Some(1.5),
            ..RunSummaryDto::default()
        };

        add_summary_dto(&mut resumed, &previous);

        assert_eq!(resumed.windows_observed, 12);
        assert_eq!(resumed.candidates_accepted, 1);
        assert_eq!(resumed.estimated_api_cost_usd, Some(1.75));
    }
}
