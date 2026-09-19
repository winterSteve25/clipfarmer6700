use crate::domain::{Candidate, ClipState, TimestampMs, TranscriptSegment, merge_candidates};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineDiscontinuity {
    pub epoch: u32,
    pub at_ms: TimestampMs,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct StreamClock {
    session_id: String,
    epoch: u32,
    last_ms: TimestampMs,
    discontinuities: Vec<TimelineDiscontinuity>,
}

impl StreamClock {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            epoch: 0,
            last_ms: 0,
            discontinuities: Vec::new(),
        }
    }

    pub fn observe(&mut self, at_ms: TimestampMs) -> Result<()> {
        ensure!(at_ms >= self.last_ms, "timeline moved backwards");
        self.last_ms = at_ms;
        Ok(())
    }

    pub fn discontinuity(&mut self, at_ms: TimestampMs, reason: impl Into<String>) {
        self.epoch = self.epoch.saturating_add(1);
        self.last_ms = at_ms;
        self.discontinuities.push(TimelineDiscontinuity {
            epoch: self.epoch,
            at_ms,
            reason: reason.into(),
        });
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn epoch(&self) -> u32 {
        self.epoch
    }
    pub fn now_ms(&self) -> TimestampMs {
        self.last_ms
    }
    pub fn discontinuities(&self) -> &[TimelineDiscontinuity] {
        &self.discontinuities
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaSegment {
    pub epoch: u32,
    pub start_ms: TimestampMs,
    pub end_ms: TimestampMs,
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct MediaRing {
    retention_ms: i64,
    segments: VecDeque<MediaSegment>,
}

impl MediaRing {
    pub fn new(retention_minutes: u64) -> Self {
        Self {
            retention_ms: (retention_minutes as i64).saturating_mul(60_000),
            segments: VecDeque::new(),
        }
    }

    /// Adds metadata and returns expired files. The caller decides when it is safe to delete them.
    pub fn push(&mut self, segment: MediaSegment) -> Vec<PathBuf> {
        let cutoff = segment.end_ms.saturating_sub(self.retention_ms);
        self.segments.push_back(segment);
        let mut expired = Vec::new();
        while self
            .segments
            .front()
            .is_some_and(|front| front.end_ms < cutoff)
        {
            if let Some(segment) = self.segments.pop_front() {
                expired.push(segment.path);
            }
        }
        expired
    }

    pub fn range(&self, epoch: u32, start_ms: i64, end_ms: i64) -> Vec<MediaSegment> {
        self.segments
            .iter()
            .filter(|segment| {
                segment.epoch == epoch && segment.end_ms > start_ms && segment.start_ms < end_ms
            })
            .cloned()
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct TranscriptDeduper {
    finalized: Vec<TranscriptSegment>,
}

impl TranscriptDeduper {
    pub fn insert(&mut self, mut incoming: TranscriptSegment) -> bool {
        incoming.text = incoming
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if incoming.text.is_empty() || incoming.end_ms <= incoming.start_ms {
            return false;
        }
        let duplicate = self.finalized.iter().any(|existing| {
            let overlap =
                incoming.start_ms < existing.end_ms && incoming.end_ms > existing.start_ms;
            overlap && normalized(&incoming.text) == normalized(&existing.text)
        });
        if duplicate {
            return false;
        }
        self.finalized.push(incoming);
        self.finalized.sort_by_key(|segment| segment.start_ms);
        true
    }

    pub fn segments(&self) -> &[TranscriptSegment] {
        &self.finalized
    }
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric() || character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug)]
pub struct CandidateTracker {
    maturation_delay_ms: i64,
    active: Vec<Candidate>,
}

impl CandidateTracker {
    pub fn new(maturation_delay_seconds: u64) -> Self {
        Self {
            maturation_delay_ms: (maturation_delay_seconds as i64).saturating_mul(1_000),
            active: Vec::new(),
        }
    }

    pub fn observe(&mut self, candidate: Candidate) {
        self.active.push(candidate);
        self.active = merge_candidates(std::mem::take(&mut self.active), 2_000);
    }

    pub fn mature(&mut self, now_ms: i64) -> Vec<Candidate> {
        let mut ready = Vec::new();
        let mut waiting = Vec::new();
        for mut candidate in std::mem::take(&mut self.active) {
            let anchor = candidate.payoff_ms.unwrap_or(candidate.end_ms);
            if now_ms.saturating_sub(anchor) >= self.maturation_delay_ms {
                candidate.state = ClipState::Ready;
                ready.push(candidate);
            } else {
                waiting.push(candidate);
            }
        }
        self.active = waiting;
        ready
    }

    pub fn active(&self) -> &[Candidate] {
        &self.active
    }
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
            source_id: "vod".to_owned(),
            start_ms,
            end_ms,
            payoff_ms: Some(end_ms),
            transcript: String::new(),
            observer_confidence: 0.8,
            state: ClipState::Emerging,
        }
    }

    #[test]
    fn clock_requires_a_new_epoch_after_backwards_time() {
        let mut clock = StreamClock::new("s");
        clock.observe(10_000).unwrap();
        assert!(clock.observe(9_000).is_err());
        clock.discontinuity(0, "reconnect");
        clock.observe(100).unwrap();
        assert_eq!(clock.epoch(), 1);
    }

    #[test]
    fn overlapping_transcript_is_deduplicated() {
        let mut deduper = TranscriptDeduper::default();
        let mut segment = TranscriptSegment {
            session_id: "s".to_owned(),
            start_ms: 0,
            end_ms: 5_000,
            text: "No way!".to_owned(),
            confidence: Some(0.9),
            no_speech_probability: None,
            is_final: true,
        };
        assert!(deduper.insert(segment.clone()));
        segment.start_ms = 4_000;
        segment.end_ms = 7_000;
        segment.text = " no  way ".to_owned();
        assert!(!deduper.insert(segment));
    }

    #[test]
    fn candidate_waits_for_delayed_reaction() {
        let mut tracker = CandidateTracker::new(20);
        tracker.observe(candidate("a", 0, 30_000));
        assert!(tracker.mature(49_999).is_empty());
        assert_eq!(tracker.mature(50_000).len(), 1);
    }
}
