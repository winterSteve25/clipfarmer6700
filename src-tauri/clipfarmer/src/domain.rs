use serde::{Deserialize, Serialize};
use std::fmt;

/// All timestamps are milliseconds on the source session's monotonic timeline.
pub type TimestampMs = i64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClipState {
    Observing,
    Emerging,
    Maturing,
    Ready,
    Accepted,
    Rejected,
    Rendered,
    Staged,
    Publishing,
    Published,
    AwaitingCreator,
    Failed,
}

impl fmt::Display for ClipState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = serde_json::to_value(self).map_err(|_| fmt::Error)?;
        f.write_str(value.as_str().ok_or(fmt::Error)?)
    }
}

impl std::str::FromStr for ClipState {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        serde_json::from_value(serde_json::Value::String(value.to_owned()))
            .map_err(|_| format!("unknown clip state {value}"))
    }
}

impl ClipState {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        use ClipState::*;
        matches!(
            (self, next),
            (Observing, Emerging)
                | (Emerging, Maturing)
                | (Maturing, Ready)
                | (Ready, Accepted | Rejected)
                | (Accepted, Rendered)
                | (Rendered, Staged)
                | (Staged, Publishing)
                | (Publishing, Published | AwaitingCreator)
                | (AwaitingCreator, Published)
                | (_, Failed)
        )
    }

    pub fn terminal(&self) -> bool {
        matches!(self, Self::Rejected | Self::Published)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub session_id: String,
    pub channel_id: String,
    pub source: String,
    pub source_id: String,
    pub start_ms: TimestampMs,
    pub end_ms: TimestampMs,
    pub payoff_ms: Option<TimestampMs>,
    pub transcript: String,
    /// Observer confidence guides evidence density; it cannot accept a clip.
    pub observer_confidence: f64,
    pub state: ClipState,
}

impl Candidate {
    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (5_000..=60_000).contains(&self.duration_ms()),
            "candidate duration must be 5-60 seconds"
        );
        anyhow::ensure!(
            !self.id.is_empty() && !self.session_id.is_empty() && !self.source_id.is_empty(),
            "candidate identifiers must not be empty"
        );
        Ok(())
    }

    pub fn idempotency_key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.source, self.source_id, self.start_ms, self.end_ms
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptSegment {
    pub session_id: String,
    pub start_ms: TimestampMs,
    pub end_ms: TimestampMs,
    pub text: String,
    pub confidence: Option<f64>,
    pub no_speech_probability: Option<f64>,
    pub is_final: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatEvent {
    pub session_id: String,
    pub at_ms: TimestampMs,
    pub author_hash: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisualSample {
    pub at_ms: TimestampMs,
    pub path: String,
    pub region: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignalEvidence {
    pub source: String,
    pub at_ms: TimestampMs,
    pub chat_rate: f64,
    pub visual_motion: f64,
    pub audio_energy: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceWindow {
    pub session_id: String,
    pub channel_id: String,
    pub start_ms: TimestampMs,
    pub end_ms: TimestampMs,
    pub transcripts: Vec<TranscriptSegment>,
    pub chat: Vec<ChatEvent>,
    pub visuals: Vec<VisualSample>,
    pub signals: Vec<SignalEvidence>,
    pub channel_profile: Option<ChannelProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioAnnotation {
    pub emotional_arc: String,
    pub nonverbal_events: Vec<String>,
    pub hook_ms: Option<TimestampMs>,
    pub payoff_ms: Option<TimestampMs>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditorialStage {
    Observer,
    Director,
    Editor,
    Critic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CutVariant {
    pub start_ms: TimestampMs,
    pub end_ms: TimestampMs,
    pub rationale: String,
}

impl fmt::Display for EditorialStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Observer => "observer",
            Self::Director => "director",
            Self::Editor => "editor",
            Self::Critic => "critic",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditorialDecision {
    pub stage: EditorialStage,
    pub accept: bool,
    pub confidence: f64,
    pub rationale: String,
    pub title: String,
    pub start_ms: TimestampMs,
    pub end_ms: TimestampMs,
    pub hook_text: Option<String>,
    pub layout: Option<String>,
    #[serde(default)]
    pub alternatives: Vec<CutVariant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelProfile {
    pub channel_id: String,
    pub version: u32,
    pub summary: String,
    pub vocabulary: Vec<String>,
    pub cast: Vec<String>,
    pub recent_topics: Vec<String>,
    #[serde(default)]
    pub successful_examples: Vec<String>,
    #[serde(default)]
    pub failed_examples: Vec<String>,
    pub normal_chat_rate: f64,
    pub normal_audio_energy: f64,
    pub retention_baseline: Option<f64>,
    pub share_rate_baseline: Option<f64>,
}

impl ChannelProfile {
    pub fn empty(channel_id: impl Into<String>) -> Self {
        Self {
            channel_id: channel_id.into(),
            version: 1,
            summary: String::new(),
            vocabulary: Vec::new(),
            cast: Vec::new(),
            recent_topics: Vec::new(),
            successful_examples: Vec::new(),
            failed_examples: Vec::new(),
            normal_chat_rate: 0.0,
            normal_audio_energy: 0.0,
            retention_baseline: None,
            share_rate_baseline: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptionCue {
    pub start_ms: TimestampMs,
    pub end_ms: TimestampMs,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditManifest {
    pub version: u8,
    pub candidate_id: String,
    pub input_path: String,
    pub output_path: String,
    pub source_start_ms: TimestampMs,
    pub source_end_ms: TimestampMs,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub layout: String,
    pub hook_text: Option<String>,
    pub captions: Vec<CaptionCue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Outcome {
    pub candidate_id: String,
    pub platform: String,
    pub remote_id: String,
    pub status: String,
    pub idempotency_key: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutcomeMetrics {
    pub platform: String,
    pub remote_id: String,
    pub age_hours: u32,
    pub views: u64,
    pub average_watch_seconds: Option<f64>,
    pub completion_rate: Option<f64>,
    pub viewed_vs_swiped_away: Option<f64>,
    pub rewatches: Option<u64>,
    pub shares: u64,
    pub saves: u64,
    pub likes: u64,
    pub comments: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioFeatures {
    pub rms_db: f64,
    pub peak_db: f64,
    pub speech_ratio: f64,
    pub excitement: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LocalSignals {
    pub audio_energy: f64,
    pub scene_change_rate: f64,
}

/// Merge overlapping observer proposals without allowing a merged clip over sixty seconds.
pub fn merge_candidates(mut candidates: Vec<Candidate>, gap_ms: i64) -> Vec<Candidate> {
    candidates.sort_by_key(|candidate| candidate.start_ms);
    let mut merged: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        if let Some(previous) = merged.last_mut() {
            let same_event = candidate.session_id == previous.session_id
                && candidate.source_id == previous.source_id
                && candidate.start_ms <= previous.end_ms.saturating_add(gap_ms);
            let merged_duration = candidate.end_ms.max(previous.end_ms) - previous.start_ms;
            if same_event && merged_duration <= 60_000 {
                previous.end_ms = previous.end_ms.max(candidate.end_ms);
                previous.observer_confidence = previous
                    .observer_confidence
                    .max(candidate.observer_confidence);
                if candidate.payoff_ms.is_some() {
                    previous.payoff_ms = candidate.payoff_ms;
                }
                continue;
            }
        }
        merged.push(candidate);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, start_ms: i64, end_ms: i64) -> Candidate {
        Candidate {
            id: id.to_owned(),
            session_id: "session".to_owned(),
            channel_id: "channel".to_owned(),
            source: "twitch".to_owned(),
            source_id: "source".to_owned(),
            start_ms,
            end_ms,
            payoff_ms: Some(end_ms),
            transcript: String::new(),
            observer_confidence: 0.8,
            state: ClipState::Emerging,
        }
    }

    #[test]
    fn merges_same_event_but_never_over_sixty_seconds() {
        let merged = merge_candidates(
            vec![candidate("a", 0, 30_000), candidate("b", 29_000, 50_000)],
            2_000,
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].end_ms, 50_000);
        let not_merged = merge_candidates(
            vec![candidate("a", 0, 30_000), candidate("b", 29_000, 70_000)],
            2_000,
        );
        assert_eq!(not_merged.len(), 2);
    }

    #[test]
    fn lifecycle_rejects_skipping_the_critic_gate() {
        assert!(ClipState::Ready.can_transition_to(&ClipState::Accepted));
        assert!(!ClipState::Emerging.can_transition_to(&ClipState::Accepted));
        assert!(!ClipState::Ready.can_transition_to(&ClipState::Published));
    }
}
