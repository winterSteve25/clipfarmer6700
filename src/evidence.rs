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
