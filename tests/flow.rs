use anyhow::Result;
use async_trait::async_trait;
use clipfarmer6700::{
    Service, ServiceDependencies,
    adapters::{
        DryRunPublisher, LocalObjectStore, ManifestRenderer, Publisher, SignalExtractor,
        Transcriber, VisualSampler,
    },
    config::{
        Config, GeminiConfig, MediaConfig, ModelSelectionConfig, OpenAiConfig, PublisherConfig,
        ScribbleConfig, StagingConfig, WorkerConfig,
    },
    domain::{
        AudioAnnotation, Candidate, ChannelProfile, ClipState, EditorialDecision, EditorialStage,
        EvidenceWindow, LocalSignals, Outcome, TranscriptSegment, VisualSample,
    },
    editorial::{DeterministicAudioAnnotation, DeterministicEditorial, EditorialModel},
    pipeline::RunSummary,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Debug)]
struct FakeTranscriber;

#[async_trait]
impl Transcriber for FakeTranscriber {
    async fn transcribe(
        &self,
        _input_path: &str,
        session_id: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<TranscriptSegment>> {
        Ok(vec![TranscriptSegment {
            session_id: session_id.to_owned(),
            start_ms,
            end_ms,
            text: format!("setup and payoff at {end_ms}"),
            confidence: Some(0.95),
            no_speech_probability: Some(0.01),
            is_final: true,
        }])
    }
}

#[derive(Debug)]
struct FakeSampler;

#[async_trait]
impl VisualSampler for FakeSampler {
    async fn sample(
        &self,
        _input_path: &str,
        _output_dir: &Path,
        start_ms: i64,
        _end_ms: i64,
        _interval_seconds: u64,
    ) -> Result<Vec<VisualSample>> {
        Ok(vec![VisualSample {
            at_ms: start_ms,
            path: "unused-in-deterministic-model.jpg".to_owned(),
            region: "full_frame".to_owned(),
            reason: "test".to_owned(),
        }])
    }
}

#[derive(Debug)]
struct FakeSignals;

#[async_trait]
impl SignalExtractor for FakeSignals {
    async fn extract(
        &self,
        _input_path: &str,
        _start_ms: i64,
        _end_ms: i64,
    ) -> Result<LocalSignals> {
        Ok(LocalSignals {
            audio_energy: 0.7,
            scene_change_rate: 0.2,
        })
    }
}

fn config(root: PathBuf) -> Config {
    let worker = WorkerConfig {
        maturation_delay_seconds: 20,
        ..WorkerConfig::default()
    };
    Config {
        data_dir: root,
        worker,
        media: MediaConfig::default(),
        scribble: ScribbleConfig::default(),
        models: ModelSelectionConfig::default(),
        openai: OpenAiConfig::default(),
        gemini: GeminiConfig::default(),
        staging: StagingConfig::default(),
        publishers: PublisherConfig::default(),
    }
}

fn service(root: &Path, publishers: Vec<Arc<dyn Publisher>>) -> Service {
    service_with_editorial(
        root,
        publishers,
        Arc::new(DeterministicEditorial { accept: true }),
    )
}

fn service_with_editorial(
    root: &Path,
    publishers: Vec<Arc<dyn Publisher>>,
    editorial: Arc<dyn EditorialModel>,
) -> Service {
    Service::new(
        config(root.to_owned()),
        ServiceDependencies {
            transcriber: Arc::new(FakeTranscriber),
            visual_sampler: Arc::new(FakeSampler),
            signal_extractor: Arc::new(FakeSignals),
            editorial,
            audio_analyzer: Arc::new(DeterministicAudioAnnotation),
            renderer: Arc::new(ManifestRenderer),
            object_store: Arc::new(LocalObjectStore {
                root: root.join("objects"),
                bucket: "bucket".to_owned(),
                prefix: "clips".to_owned(),
                public_base_url: Some("https://media.example.test".to_owned()),
            }),
            publishers,
        },
    )
    .unwrap()
}

#[derive(Debug)]
struct CriticRejects;

#[async_trait]
impl EditorialModel for CriticRejects {
    fn model_name(&self, _stage: EditorialStage) -> &str {
        "critic-rejects-test"
    }

    async fn decide(
        &self,
        stage: EditorialStage,
        evidence: &EvidenceWindow,
        candidate: Option<&Candidate>,
        _prior: &[EditorialDecision],
        _audio: Option<&AudioAnnotation>,
    ) -> Result<EditorialDecision> {
        let (start_ms, end_ms) = candidate
            .map(|candidate| (candidate.start_ms, candidate.end_ms))
            .unwrap_or((evidence.start_ms, evidence.end_ms));
        Ok(EditorialDecision {
            stage,
            accept: stage != EditorialStage::Critic,
            confidence: 0.9,
            rationale: "critic found the moment depends on missing context".to_owned(),
            title: "Rejected".to_owned(),
            start_ms,
            end_ms,
            hook_text: None,
            layout: Some("fit_blur".to_owned()),
            alternatives: Vec::new(),
        })
    }
}

#[tokio::test]
async fn independent_critic_can_veto_every_prior_stage() {
    let root = std::env::temp_dir().join(format!("clipfarmer-veto-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("input.mp4");
    std::fs::write(&input, b"test media placeholder").unwrap();
    let publisher_calls = Arc::new(AtomicUsize::new(0));
    let service = service_with_editorial(
        &root,
        vec![Arc::new(CountingPublisher {
            name: "youtube".to_owned(),
            calls: publisher_calls.clone(),
            failures_before_success: 0,
        })],
        Arc::new(CriticRejects),
    );
    let summary = service
        .replay_file("channel", input.to_str().unwrap(), 30_000)
        .await
        .unwrap();
    assert_eq!(summary.candidates_rejected, 1);
    assert_eq!(summary.posts_completed, 0);
    assert_eq!(publisher_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn replay_runs_observer_director_editor_critic_and_publishes() {
    let root = std::env::temp_dir().join(format!("clipfarmer-flow-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("input.mp4");
    std::fs::write(&input, b"test media placeholder").unwrap();
    let publishers: Vec<Arc<dyn Publisher>> = vec![
        Arc::new(DryRunPublisher {
            name: "youtube".to_owned(),
            draft_only: false,
        }),
        Arc::new(DryRunPublisher {
            name: "instagram".to_owned(),
            draft_only: false,
        }),
        Arc::new(DryRunPublisher {
            name: "tiktok".to_owned(),
            draft_only: true,
        }),
    ];
    let service = service(&root, publishers);
    service
        .store
        .upsert_profile(&ChannelProfile::empty("channel"))
        .unwrap();
    let summary = service
        .replay_file("channel", input.to_str().unwrap(), 60_000)
        .await
        .unwrap();
    assert_eq!(
        summary,
        RunSummary {
            windows_observed: 10,
            candidates_reviewed: 1,
            candidates_accepted: 1,
            candidates_rejected: 0,
            posts_completed: 3,
            publish_failures: 0,
        }
    );
    let counts = service.store.status_counts().unwrap();
    assert!(
        counts
            .iter()
            .any(|(state, count)| state == "awaiting_creator" && *count == 1)
    );
}

#[derive(Debug)]
struct CountingPublisher {
    name: String,
    calls: Arc<AtomicUsize>,
    failures_before_success: usize,
}

#[async_trait]
impl Publisher for CountingPublisher {
    fn platform(&self) -> &str {
        &self.name
    }

    async fn publish(
        &self,
        candidate: &Candidate,
        _local_asset: &str,
        _staged_asset: &str,
        _title: &str,
        idempotency_key: &str,
    ) -> Result<Outcome> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call < self.failures_before_success {
            anyhow::bail!("injected transient failure")
        }
        Ok(Outcome {
            candidate_id: candidate.id.clone(),
            platform: self.name.clone(),
            remote_id: format!("remote-{}", self.name),
            status: "published".to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            url: None,
        })
    }
}

fn accepted_review(candidate: Candidate) -> clipfarmer6700::editorial::ReviewResult {
    let decision = EditorialDecision {
        stage: EditorialStage::Critic,
        accept: true,
        confidence: 0.9,
        rationale: "survived critique".to_owned(),
        title: "Clip".to_owned(),
        start_ms: candidate.start_ms,
        end_ms: candidate.end_ms,
        hook_text: None,
        layout: Some("fit_blur".to_owned()),
        alternatives: Vec::new(),
    };
    clipfarmer6700::editorial::ReviewResult {
        candidate,
        decisions: vec![decision.clone()],
        audio: AudioAnnotation {
            emotional_arc: "rise".to_owned(),
            nonverbal_events: vec![],
            hook_ms: None,
            payoff_ms: None,
            confidence: 0.8,
        },
        accepted: true,
        final_decision: decision,
    }
}

#[tokio::test]
async fn partial_publish_retry_skips_already_successful_platform() {
    let root = std::env::temp_dir().join(format!("clipfarmer-retry-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("input.mp4");
    std::fs::write(&input, b"test media placeholder").unwrap();
    let youtube_calls = Arc::new(AtomicUsize::new(0));
    let instagram_calls = Arc::new(AtomicUsize::new(0));
    let publishers: Vec<Arc<dyn Publisher>> = vec![
        Arc::new(CountingPublisher {
            name: "youtube".to_owned(),
            calls: youtube_calls.clone(),
            failures_before_success: 0,
        }),
        Arc::new(CountingPublisher {
            name: "instagram".to_owned(),
            calls: instagram_calls.clone(),
            failures_before_success: 1,
        }),
    ];
    let service = service(&root, publishers);
    service
        .store
        .create_session("session", "channel", "input")
        .unwrap();
    let candidate = Candidate {
        id: "candidate".to_owned(),
        session_id: "session".to_owned(),
        channel_id: "channel".to_owned(),
        source: "twitch".to_owned(),
        source_id: "source".to_owned(),
        start_ms: 0,
        end_ms: 30_000,
        payoff_ms: Some(25_000),
        transcript: "moment".to_owned(),
        observer_confidence: 0.9,
        state: ClipState::Accepted,
    };
    service.store.upsert_candidate(&candidate).unwrap();
    let review = accepted_review(candidate);
    assert!(
        service
            .render_and_publish(input.to_str().unwrap(), &review, &[])
            .await
            .is_err()
    );
    assert_eq!(youtube_calls.load(Ordering::SeqCst), 1);
    assert_eq!(instagram_calls.load(Ordering::SeqCst), 1);

    let outcomes = service
        .render_and_publish(input.to_str().unwrap(), &review, &[])
        .await
        .unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(youtube_calls.load(Ordering::SeqCst), 1);
    assert_eq!(instagram_calls.load(Ordering::SeqCst), 2);
    assert!(
        service
            .store
            .publish_job_done("candidate", "youtube")
            .unwrap()
    );
    assert!(
        service
            .store
            .publish_job_done("candidate", "instagram")
            .unwrap()
    );
}
