use crate::{
    adapters::{ObjectStore, Publisher, Renderer, SignalExtractor, Transcriber, VisualSampler},
    config::Config,
    domain::{
        Candidate, ChannelProfile, ChatEvent, ClipState, EditorialDecision, EditorialStage,
        EvidenceWindow, LocalSignals, Outcome, SignalEvidence, TranscriptSegment,
    },
    editorial::{CandidateAudioAnalyzer, EditorialModel, ReviewResult},
    evidence::EvidenceRing,
    manifest::build_manifest,
    progress::{self, Step},
    store::Store,
    timeline::{CandidateTracker, TranscriptDeduper},
};
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path, sync::Arc};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSummary {
    pub windows_observed: usize,
    pub candidates_reviewed: usize,
    pub candidates_accepted: usize,
    pub candidates_rejected: usize,
    pub posts_completed: usize,
    pub publish_failures: usize,
}

struct EvidenceSlice<'a> {
    transcripts: &'a [TranscriptSegment],
    chat: &'a [ChatEvent],
    visuals: &'a [crate::domain::VisualSample],
    profile: &'a ChannelProfile,
    local_signals: Option<LocalSignals>,
}

pub struct Service {
    pub cfg: Config,
    pub store: Store,
    pub evidence_ring: EvidenceRing,
    pub transcriber: Arc<dyn Transcriber>,
    pub visual_sampler: Arc<dyn VisualSampler>,
    pub signal_extractor: Arc<dyn SignalExtractor>,
    pub editorial: Arc<dyn EditorialModel>,
    pub audio_analyzer: Arc<dyn CandidateAudioAnalyzer>,
    pub renderer: Arc<dyn Renderer>,
    pub object_store: Arc<dyn ObjectStore>,
    pub publishers: Vec<Arc<dyn Publisher>>,
}

pub struct ServiceDependencies {
    pub transcriber: Arc<dyn Transcriber>,
    pub visual_sampler: Arc<dyn VisualSampler>,
    pub signal_extractor: Arc<dyn SignalExtractor>,
    pub editorial: Arc<dyn EditorialModel>,
    pub audio_analyzer: Arc<dyn CandidateAudioAnalyzer>,
    pub renderer: Arc<dyn Renderer>,
    pub object_store: Arc<dyn ObjectStore>,
    pub publishers: Vec<Arc<dyn Publisher>>,
}

impl Service {
    pub fn new(cfg: Config, dependencies: ServiceDependencies) -> Result<Self> {
        cfg.validate()?;
        let store = Store::open(cfg.db_path())?;
        let evidence_ring = EvidenceRing::new(cfg.worker.queue_capacity);
        Ok(Self {
            cfg,
            store,
            evidence_ring,
            transcriber: dependencies.transcriber,
            visual_sampler: dependencies.visual_sampler,
            signal_extractor: dependencies.signal_extractor,
            editorial: dependencies.editorial,
            audio_analyzer: dependencies.audio_analyzer,
            renderer: dependencies.renderer,
            object_store: dependencies.object_store,
            publishers: dependencies.publishers,
        })
    }

    /// Replays a VOD or a captured live file through the same observer and final editorial path.
    pub async fn replay_file(
        &self,
        channel_id: &str,
        input_path: &str,
        duration_ms: i64,
    ) -> Result<RunSummary> {
        self.scan_file(channel_id, input_path, duration_ms, 0).await
    }

    /// Analyze a growing capture from `scan_start_ms`; callers should include at least 120 seconds
    /// of overlap so a developing moment can retain setup and delayed reaction context.
    pub async fn scan_file(
        &self,
        channel_id: &str,
        input_path: &str,
        duration_ms: i64,
        scan_start_ms: i64,
    ) -> Result<RunSummary> {
        ensure!(duration_ms >= 5_000, "input is too short to contain a clip");
        ensure!(Path::new(input_path).exists(), "input media does not exist");

        progress::section(format!("Analyzing {channel_id}"));
        progress::info(format!(
            "Source: {input_path} • duration {} • starting at {}",
            progress::timestamp(duration_ms),
            progress::timestamp(scan_start_ms)
        ));

        let session_id = uuid::Uuid::new_v4().to_string();
        let session_step = Step::start("Creating analysis session");
        self.store
            .create_session(&session_id, channel_id, input_path)?;
        let source_id = stable_source_id(input_path)?;
        let mut profile = self
            .store
            .profile(channel_id)?
            .unwrap_or_else(|| ChannelProfile::empty(channel_id));
        session_step.done(format!("session {}", short_id(&session_id)));
        let step_ms = self.cfg.worker.observer_step_seconds as i64 * 1_000;
        let window_ms = self.cfg.worker.observer_window_seconds as i64 * 1_000;
        let mut cursor = (scan_start_ms.max(0) + step_ms).min(duration_ms);
        let mut tracker = CandidateTracker::new(self.cfg.worker.maturation_delay_seconds);
        let mut transcripts = TranscriptDeduper::default();
        let mut visuals = Vec::new();
        let chat_step = Step::start("Loading timestamped chat");
        let chat = load_chat_sidecar(input_path, &session_id)?;
        chat_step.done(format!("{} messages", chat.len()));
        let mut summary = RunSummary::default();
        let mut audio_energy_total = 0.0;
        let total_windows = ((duration_ms - cursor + step_ms - 1) / step_ms + 1).max(1);
        let mut window_number = 0_i64;

        while cursor <= duration_ms {
            window_number += 1;
            let window_start = cursor.saturating_sub(window_ms);
            let window_end = cursor;
            progress::section(format!(
                "Window {window_number}/{total_windows} • {} → {}",
                progress::timestamp(window_start),
                progress::timestamp(window_end)
            ));

            let transcription_step = Step::start("Transcribing audio");
            let segments = self
                .transcriber
                .transcribe(input_path, &session_id, window_start, window_end)
                .await?;
            let segment_count = segments.len();
            let mut new_segment_count = 0;
            for segment in segments {
                if transcripts.insert(segment.clone()) {
                    self.store.record_transcript(&segment)?;
                    new_segment_count += 1;
                }
            }
            transcription_step.done(format!(
                "{new_segment_count} new / {segment_count} found, {} total",
                transcripts.segments().len()
            ));

            let frame_dir = self
                .cfg
                .data_dir
                .join("frames")
                .join(&session_id)
                .join(format!("{window_start}-{window_end}"));
            let visuals_step = Step::start("Sampling video frames");
            let new_visuals = self
                .visual_sampler
                .sample(
                    input_path,
                    &frame_dir,
                    window_start,
                    window_end,
                    self.cfg.media.frame_interval_seconds,
                )
                .await?;
            visuals_step.done(format!("{} frames", new_visuals.len()));
            visuals.extend(new_visuals);

            let signals_step = Step::start("Measuring audio and scene changes");
            let local_signals = self
                .signal_extractor
                .extract(input_path, window_start, window_end)
                .await?;
            signals_step.done(format!(
                "energy {:.2}, scene changes {:.2}/s",
                local_signals.audio_energy, local_signals.scene_change_rate
            ));
            audio_energy_total += local_signals.audio_energy;
            let window = evidence_window(
                &session_id,
                channel_id,
                window_start,
                window_end,
                &EvidenceSlice {
                    transcripts: transcripts.segments(),
                    chat: &chat,
                    visuals: &visuals,
                    profile: &profile,
                    local_signals: Some(local_signals),
                },
            );
            let observer_step = Step::start(format!(
                "Running observer ({})",
                self.editorial.model_name(EditorialStage::Observer)
            ));
            let observer = self
                .editorial
                .decide(EditorialStage::Observer, &window, None, &[], None)
                .await?;
            observer_step.done(format!(
                "{} at {:.0}% confidence",
                if observer.accept {
                    "candidate detected"
                } else {
                    "no candidate"
                },
                observer.confidence * 100.0
            ));
            summary.windows_observed += 1;
            self.record_ring("observer", &session_id, serde_json::to_value(&observer)?)?;
            if observer.accept
                && let Some(candidate) = candidate_from_observer(
                    &session_id,
                    channel_id,
                    &source_id,
                    transcripts.segments(),
                    &observer,
                    duration_ms,
                )
            {
                progress::info(format!(
                    "Tracking candidate {} ({} → {})",
                    short_id(&candidate.id),
                    progress::timestamp(candidate.start_ms),
                    progress::timestamp(candidate.end_ms)
                ));
                tracker.observe(candidate);
            }
            let ready = tracker.mature(cursor);
            self.review_ready(
                input_path,
                ready,
                &EvidenceSlice {
                    transcripts: transcripts.segments(),
                    chat: &chat,
                    visuals: &visuals,
                    profile: &profile,
                    local_signals: Some(local_signals),
                },
                &mut summary,
            )
            .await?;
            if cursor == duration_ms {
                break;
            }
            cursor = (cursor + step_ms).min(duration_ms);
        }

        progress::section("Finalizing candidates");
        let ready =
            tracker.mature(duration_ms + self.cfg.worker.maturation_delay_seconds as i64 * 1_000);
        self.review_ready(
            input_path,
            ready,
            &EvidenceSlice {
                transcripts: transcripts.segments(),
                chat: &chat,
                visuals: &visuals,
                profile: &profile,
                local_signals: None,
            },
            &mut summary,
        )
        .await?;

        let recovery = self.retry_pending(input_path).await?;
        if recovery.posts_completed > 0 || recovery.publish_failures > 0 {
            progress::success(format!(
                "Publish recovery finished: {} posts, {} failures",
                recovery.posts_completed, recovery.publish_failures
            ));
        }
        summary.posts_completed += recovery.posts_completed;
        summary.publish_failures += recovery.publish_failures;

        let profile_step = Step::start("Updating channel profile");
        update_profile_from_session(&mut profile, transcripts.segments(), &chat, duration_ms);
        if summary.windows_observed > 0 {
            profile.normal_audio_energy = audio_energy_total / summary.windows_observed as f64;
        }
        self.store.upsert_profile(&profile)?;
        self.store.finish_session(&session_id, "completed")?;
        profile_step.done(format!("version {}", profile.version));
        progress::success(format!(
            "Analysis complete: {} windows, {} accepted, {} rejected, {} posts",
            summary.windows_observed,
            summary.candidates_accepted,
            summary.candidates_rejected,
            summary.posts_completed
        ));
        Ok(summary)
    }

    async fn review_ready(
        &self,
        input_path: &str,
        ready: Vec<Candidate>,
        context: &EvidenceSlice<'_>,
        summary: &mut RunSummary,
    ) -> Result<()> {
        for candidate in ready {
            if !self.store.upsert_candidate(&candidate)? {
                progress::info(format!(
                    "Candidate {} was already processed; skipping",
                    short_id(&candidate.id)
                ));
                continue;
            }
            progress::section(format!(
                "Reviewing candidate {} • {} → {}",
                short_id(&candidate.id),
                progress::timestamp(candidate.start_ms),
                progress::timestamp(candidate.end_ms)
            ));
            summary.candidates_reviewed += 1;
            let evidence = evidence_window(
                &candidate.session_id,
                &candidate.channel_id,
                candidate.start_ms.saturating_sub(30_000).max(0),
                candidate.end_ms.saturating_add(30_000),
                context,
            );
            let review = self
                .review_candidate(input_path, candidate, &evidence)
                .await?;
            if review.accepted {
                summary.candidates_accepted += 1;
                progress::success(format!(
                    "Candidate accepted: {}",
                    review.final_decision.title
                ));
                match self
                    .render_and_publish(input_path, &review, context.transcripts)
                    .await
                {
                    Ok(outcomes) => summary.posts_completed += outcomes.len(),
                    Err(error) => {
                        summary.publish_failures += 1;
                        progress::warning(format!(
                            "Publishing candidate {} failed and will be retried: {error:#}",
                            short_id(&review.candidate.id)
                        ));
                        self.record_ring(
                            "publish_retryable",
                            &review.candidate.session_id,
                            serde_json::json!({
                                "candidate_id":review.candidate.id,
                                "error":error.to_string()
                            }),
                        )?;
                    }
                }
            } else {
                summary.candidates_rejected += 1;
                progress::info(format!(
                    "Candidate rejected: {}",
                    review.final_decision.rationale
                ));
            }
        }
        Ok(())
    }

    /// Retries only unfinished platform jobs. Completed remote posts are never submitted again.
    pub async fn retry_pending(&self, input_path: &str) -> Result<RunSummary> {
        let mut summary = RunSummary::default();
        let pending = self.store.pending_candidates(100)?;
        if pending.is_empty() {
            progress::info("No unfinished publish jobs");
        } else {
            progress::info(format!("Retrying {} unfinished candidates", pending.len()));
        }
        for candidate in pending {
            progress::section(format!("Recovering candidate {}", short_id(&candidate.id)));
            let attempts = self.store.note_candidate_attempt(&candidate.id)?;
            if attempts > self.cfg.worker.max_attempts {
                self.store
                    .fail_candidate(&candidate.id, "retry budget exhausted")?;
                summary.publish_failures += 1;
                progress::warning("Retry budget exhausted; candidate marked failed");
                continue;
            }
            let Some(final_decision) = self.store.final_decision(&candidate.id)? else {
                self.store
                    .fail_candidate(&candidate.id, "stored critic decision is missing")?;
                summary.publish_failures += 1;
                progress::warning("Stored critic decision is missing; candidate marked failed");
                continue;
            };
            let transcripts = self.store.transcripts_for_candidate(&candidate)?;
            let review = ReviewResult {
                candidate,
                decisions: vec![final_decision.clone()],
                audio: crate::domain::AudioAnnotation {
                    emotional_arc: "recovered publish job".to_owned(),
                    nonverbal_events: Vec::new(),
                    hook_ms: None,
                    payoff_ms: None,
                    confidence: 0.0,
                },
                accepted: true,
                final_decision,
            };
            match self
                .render_and_publish(input_path, &review, &transcripts)
                .await
            {
                Ok(outcomes) => summary.posts_completed += outcomes.len(),
                Err(error) => {
                    summary.publish_failures += 1;
                    progress::warning(format!("Recovery attempt failed: {error:#}"));
                }
            }
        }
        Ok(summary)
    }

    pub async fn review_candidate(
        &self,
        input_path: &str,
        mut candidate: Candidate,
        evidence: &EvidenceWindow,
    ) -> Result<ReviewResult> {
        candidate.validate()?;
        ensure!(
            candidate.state == ClipState::Ready,
            "candidate is not ready"
        );

        let audio_step = Step::start("Analyzing candidate audio");
        let audio = self.audio_analyzer.annotate(input_path, &candidate).await?;
        audio_step.done(format!(
            "{:.0}% confidence, {} detected events",
            audio.confidence * 100.0,
            audio.nonverbal_events.len()
        ));
        let mut decisions = Vec::new();
        for stage in [
            EditorialStage::Director,
            EditorialStage::Editor,
            EditorialStage::Critic,
        ] {
            let editorial_step = Step::start(format!(
                "Running {stage} ({})",
                self.editorial.model_name(stage)
            ));
            let decision = self
                .editorial
                .decide(stage, evidence, Some(&candidate), &decisions, Some(&audio))
                .await?;
            crate::editorial::validate_decision(&decision, candidate.start_ms, candidate.end_ms)?;
            self.store.record_decision(
                &candidate.id,
                self.editorial.model_name(stage),
                &decision,
            )?;
            editorial_step.done(format!(
                "{} at {:.0}% confidence",
                if decision.accept {
                    "accepted"
                } else {
                    "rejected"
                },
                decision.confidence * 100.0
            ));
            decisions.push(decision);
        }
        let final_decision = decisions.last().cloned().expect("three decisions");
        let accepted = final_decision.accept;
        let next = if accepted {
            ClipState::Accepted
        } else {
            ClipState::Rejected
        };
        let _ = self
            .store
            .transition(&candidate.id, ClipState::Ready, next.clone())?;
        candidate.state = next;
        Ok(ReviewResult {
            candidate,
            decisions,
            audio,
            accepted,
            final_decision,
        })
    }

    pub async fn render_and_publish(
        &self,
        input_path: &str,
        review: &ReviewResult,
        transcripts: &[TranscriptSegment],
    ) -> Result<Vec<Outcome>> {
        ensure!(review.accepted, "cannot render a rejected candidate");
        let candidate = &review.candidate;
        progress::section(format!("Producing candidate {}", short_id(&candidate.id)));
        let output_path = self
            .cfg
            .data_dir
            .join("outputs")
            .join(format!("{}.mp4", candidate.id));

        let manifest_step = Step::start("Building edit manifest and captions");
        let manifest = build_manifest(
            candidate,
            &review.final_decision,
            input_path,
            &output_path.display().to_string(),
            transcripts,
        )?;
        self.store.record_manifest(&manifest)?;
        manifest_step.done(format!(
            "{} captions, {} layout",
            manifest.captions.len(),
            manifest.layout
        ));

        let render_step = Step::start("Rendering vertical clip with ffmpeg");
        let rendered = self.renderer.render(&manifest).await?;
        let _ = self
            .store
            .transition(&candidate.id, ClipState::Accepted, ClipState::Rendered)?;
        render_step.done(rendered.clone());

        let object_key = format!("{}/{}.mp4", candidate.channel_id, candidate.id);
        let staging_step = Step::start("Staging rendered clip");
        let staged = self.object_store.stage(&rendered, &object_key).await?;
        let _ = self
            .store
            .transition(&candidate.id, ClipState::Rendered, ClipState::Staged)?;
        staging_step.done(display_location(&staged));

        let queue_step = Step::start("Creating idempotent publish jobs");
        for publisher in &self.publishers {
            let job_id = format!("{}-{}", publisher.platform(), candidate.id);
            let key = format!("{}:{}", publisher.platform(), candidate.idempotency_key());
            self.store
                .enqueue_publish_job(&job_id, &candidate.id, publisher.platform(), &key)?;
        }
        let _ = self
            .store
            .transition(&candidate.id, ClipState::Staged, ClipState::Publishing)?;
        queue_step.done(format!("{} platforms", self.publishers.len()));

        let mut outcomes = Vec::new();
        let mut failures = Vec::new();
        for publisher in &self.publishers {
            if self
                .store
                .publish_job_done(&candidate.id, publisher.platform())?
            {
                progress::info(format!(
                    "{} was already published; skipping",
                    publisher.platform()
                ));
                continue;
            }
            let key = format!("{}:{}", publisher.platform(), candidate.idempotency_key());
            let publish_step = Step::start(format!("Publishing to {}", publisher.platform()));
            match publisher
                .publish(
                    candidate,
                    &rendered,
                    &staged,
                    &review.final_decision.title,
                    &key,
                )
                .await
            {
                Ok(outcome) => {
                    self.store.complete_publish_job(&outcome)?;
                    publish_step.done(format!("status {}", outcome.status));
                    outcomes.push(outcome);
                }
                Err(error) => {
                    self.store.fail_publish_job(
                        &candidate.id,
                        publisher.platform(),
                        &error.to_string(),
                    )?;
                    publish_step.failed(error.to_string());
                    failures.push(format!("{}: {error}", publisher.platform()));
                }
            }
        }
        if self.store.all_publish_jobs_done(&candidate.id)? {
            let next = if self.store.candidate_has_awaiting_creator(&candidate.id)? {
                ClipState::AwaitingCreator
            } else {
                ClipState::Published
            };
            let _ = self
                .store
                .transition(&candidate.id, ClipState::Publishing, next)?;
        }
        if !failures.is_empty() {
            anyhow::bail!("one or more publishers failed: {}", failures.join("; "));
        }
        progress::success(format!(
            "Candidate {} finished with {} completed posts",
            short_id(&candidate.id),
            outcomes.len()
        ));
        Ok(outcomes)
    }

    fn record_ring(&self, kind: &str, subject: &str, detail: serde_json::Value) -> Result<()> {
        self.evidence_ring.record(kind, subject, detail.clone());
        self.store.record_timeline(subject, 0, kind, &detail)
    }
}

fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn display_location(location: &str) -> String {
    if location.starts_with("https://") {
        "remote HTTPS object ready".to_owned()
    } else {
        location.to_owned()
    }
}

fn candidate_from_observer(
    session_id: &str,
    channel_id: &str,
    source_id: &str,
    transcripts: &[TranscriptSegment],
    observer: &EditorialDecision,
    duration_ms: i64,
) -> Option<Candidate> {
    let start_ms = observer.start_ms.max(0);
    let mut end_ms = observer.end_ms.min(duration_ms);
    if end_ms - start_ms < 5_000 {
        end_ms = (start_ms + 5_000).min(duration_ms);
    }
    if !(5_000..=60_000).contains(&(end_ms - start_ms)) {
        return None;
    }
    let transcript = transcripts
        .iter()
        .filter(|segment| segment.end_ms > start_ms && segment.start_ms < end_ms)
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Some(Candidate {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_owned(),
        channel_id: channel_id.to_owned(),
        source: "twitch".to_owned(),
        source_id: source_id.to_owned(),
        start_ms,
        end_ms,
        payoff_ms: Some(end_ms),
        transcript,
        observer_confidence: observer.confidence,
        state: ClipState::Emerging,
    })
}

fn evidence_window(
    session_id: &str,
    channel_id: &str,
    start_ms: i64,
    end_ms: i64,
    context: &EvidenceSlice<'_>,
) -> EvidenceWindow {
    let window_transcripts = context
        .transcripts
        .iter()
        .filter(|segment| segment.end_ms > start_ms && segment.start_ms < end_ms)
        .cloned()
        .collect::<Vec<_>>();
    let window_chat = context
        .chat
        .iter()
        .filter(|event| event.at_ms >= start_ms && event.at_ms <= end_ms)
        .cloned()
        .collect::<Vec<_>>();
    let window_visuals = context
        .visuals
        .iter()
        .filter(|sample| sample.at_ms >= start_ms && sample.at_ms <= end_ms)
        .cloned()
        .collect::<Vec<_>>();
    let seconds = ((end_ms - start_ms).max(1) as f64) / 1_000.0;
    let chat_rate = window_chat.len() as f64 / seconds;
    let combined_text = window_transcripts
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let local_signals = context.local_signals.unwrap_or(LocalSignals {
        audio_energy: 0.0,
        scene_change_rate: 0.0,
    });
    EvidenceWindow {
        session_id: session_id.to_owned(),
        channel_id: channel_id.to_owned(),
        start_ms,
        end_ms,
        transcripts: window_transcripts,
        chat: window_chat,
        visuals: window_visuals,
        signals: vec![SignalEvidence {
            source: "local".to_owned(),
            at_ms: end_ms,
            chat_rate,
            visual_motion: local_signals.scene_change_rate,
            audio_energy: local_signals.audio_energy,
            text: combined_text,
        }],
        channel_profile: Some(context.profile.clone()),
    }
}

fn load_chat_sidecar(input_path: &str, session_id: &str) -> Result<Vec<ChatEvent>> {
    let sidecar = Path::new(input_path).with_extension("chat.jsonl");
    if !sidecar.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&sidecar)
        .with_context(|| format!("read chat sidecar {}", sidecar.display()))?;
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value: serde_json::Value = serde_json::from_str(line)?;
            if let Ok(mut event) = serde_json::from_value::<ChatEvent>(value.clone()) {
                event.session_id = session_id.to_owned();
                return Ok(event);
            }
            let at_ms = value
                .get("time_in_seconds")
                .and_then(serde_json::Value::as_f64)
                .map(|seconds| (seconds * 1_000.0) as i64)
                .or_else(|| value.get("at_ms").and_then(serde_json::Value::as_i64))
                .context("chat line has no timeline timestamp")?;
            let text = value
                .get("message")
                .or_else(|| value.get("text"))
                .and_then(serde_json::Value::as_str)
                .context("chat line has no message text")?
                .to_owned();
            let author = value
                .pointer("/author/name")
                .or_else(|| value.get("author"))
                .and_then(serde_json::Value::as_str)
                .map(hash_author);
            Ok(ChatEvent {
                session_id: session_id.to_owned(),
                at_ms,
                author_hash: author,
                text,
            })
        })
        .collect()
}

fn hash_author(author: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(author.as_bytes());
    format!("{:x}", hash.finalize())[..16].to_owned()
}

fn update_profile_from_session(
    profile: &mut ChannelProfile,
    transcripts: &[TranscriptSegment],
    chat: &[ChatEvent],
    duration_ms: i64,
) {
    use std::collections::HashMap;
    profile.normal_chat_rate = chat.len() as f64 / (duration_ms.max(1) as f64 / 1_000.0);
    let mut counts: HashMap<String, usize> = HashMap::new();
    for word in transcripts
        .iter()
        .flat_map(|segment| segment.text.split_whitespace())
    {
        let word = word
            .chars()
            .filter(|character| character.is_alphanumeric() || *character == '\'')
            .flat_map(char::to_lowercase)
            .collect::<String>();
        if word.len() >= 4 {
            *counts.entry(word).or_default() += 1;
        }
    }
    let mut words = counts.into_iter().collect::<Vec<_>>();
    words.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    profile.vocabulary = words.into_iter().take(20).map(|(word, _)| word).collect();
    profile.version = profile.version.saturating_add(1);
}

fn stable_source_id(input_path: &str) -> Result<String> {
    use sha2::{Digest, Sha256};
    let canonical = fs::canonicalize(input_path)?;
    let mut hash = Sha256::new();
    hash.update(canonical.to_string_lossy().as_bytes());
    Ok(format!("{:x}", hash.finalize()))
}
