//! Bounded in-memory storage for recent pipeline evidence and diagnostic events.
//! This module makes recent backend activity available without unbounded memory growth.
//! It complements durable timeline records with a lightweight snapshot for inspection and debugging.

use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub at_ms: u128,
    pub kind: String,
    pub subject: String,
    pub detail: serde_json::Value,
}
#[derive(Clone)]
pub struct EvidenceRing {
    inner: Arc<Mutex<VecDeque<Evidence>>>,
    cap: usize,
}
impl EvidenceRing {
    pub fn new(cap: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(cap))),
            cap,
        }
    }
    pub fn record(
        &self,
        kind: impl Into<String>,
        subject: impl Into<String>,
        detail: serde_json::Value,
    ) {
        let mut q = self.inner.lock().expect("evidence mutex");
        if q.len() == self.cap {
            q.pop_front();
        }
        q.push_back(Evidence {
            at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            kind: kind.into(),
            subject: subject.into(),
            detail,
        });
    }
    pub fn snapshot(&self) -> Vec<Evidence> {
        self.inner
            .lock()
            .expect("evidence mutex")
            .iter()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_newest_entries_when_capacity_is_reached() {
        let ring = EvidenceRing::new(2);
        ring.record("first", "session", serde_json::json!({"value": 1}));
        ring.record("second", "session", serde_json::json!({"value": 2}));
        ring.record("third", "session", serde_json::json!({"value": 3}));

        let snapshot = ring.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].kind, "second");
        assert_eq!(snapshot[1].kind, "third");
    }

    #[test]
    fn cloned_rings_share_the_same_event_buffer() {
        let ring = EvidenceRing::new(1);
        let clone = ring.clone();

        clone.record("shared", "session", serde_json::json!({}));

        assert_eq!(ring.snapshot()[0].subject, "session");
    }
}
