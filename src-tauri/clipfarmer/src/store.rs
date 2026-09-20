use crate::domain::{
    Candidate, ChannelProfile, ClipState, EditManifest, EditorialDecision, Outcome, OutcomeMetrics,
    TranscriptSegment,
};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).context("open SQLite store")?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;
             CREATE TABLE IF NOT EXISTS schema_migrations(
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS sessions(
                id TEXT PRIMARY KEY,
                channel_id TEXT NOT NULL,
                source_uri TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                ended_at TEXT
             );
             CREATE TABLE IF NOT EXISTS discontinuities(
                id INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                epoch INTEGER NOT NULL,
                at_ms INTEGER NOT NULL,
                reason TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS timeline_events(
                id INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                at_ms INTEGER NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS transcripts(
                id INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                start_ms INTEGER NOT NULL,
                end_ms INTEGER NOT NULL,
                text TEXT NOT NULL,
                confidence REAL,
                no_speech_probability REAL,
                is_final INTEGER NOT NULL,
                UNIQUE(session_id,start_ms,end_ms,text)
             );
             CREATE TABLE IF NOT EXISTS chat_messages(
                id INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                at_ms INTEGER NOT NULL,
                author_hash TEXT,
                text TEXT NOT NULL,
                UNIQUE(session_id,at_ms,author_hash,text)
             );
             CREATE TABLE IF NOT EXISTS visual_samples(
                id INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                at_ms INTEGER NOT NULL,
                path TEXT NOT NULL,
                region TEXT NOT NULL,
                reason TEXT NOT NULL,
                UNIQUE(session_id,at_ms,path)
             );
             CREATE TABLE IF NOT EXISTS candidates(
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                channel_id TEXT NOT NULL,
                source TEXT NOT NULL,
                source_id TEXT NOT NULL,
                start_ms INTEGER NOT NULL,
                end_ms INTEGER NOT NULL,
                payoff_ms INTEGER,
                transcript TEXT NOT NULL,
                observer_confidence REAL NOT NULL,
                state TEXT NOT NULL,
                idempotency_key TEXT NOT NULL UNIQUE,
                attempts INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS decisions(
                id INTEGER PRIMARY KEY,
                candidate_id TEXT NOT NULL REFERENCES candidates(id),
                stage TEXT NOT NULL,
                model TEXT NOT NULL,
                decision TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(candidate_id,stage)
             );
             CREATE TABLE IF NOT EXISTS manifests(
                id INTEGER PRIMARY KEY,
                candidate_id TEXT NOT NULL REFERENCES candidates(id),
                version INTEGER NOT NULL,
                manifest TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(candidate_id,version)
             );
             CREATE TABLE IF NOT EXISTS publish_jobs(
                id TEXT PRIMARY KEY,
                candidate_id TEXT NOT NULL REFERENCES candidates(id),
                platform TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'queued',
                attempts INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                idempotency_key TEXT NOT NULL UNIQUE,
                remote_id TEXT,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(candidate_id,platform)
             );
             CREATE TABLE IF NOT EXISTS posts(
                id INTEGER PRIMARY KEY,
                candidate_id TEXT NOT NULL REFERENCES candidates(id),
                platform TEXT NOT NULL,
                remote_id TEXT NOT NULL,
                status TEXT NOT NULL,
                url TEXT,
                idempotency_key TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(platform,remote_id)
             );
             CREATE TABLE IF NOT EXISTS metrics(
                id INTEGER PRIMARY KEY,
                platform TEXT NOT NULL,
                remote_id TEXT NOT NULL,
                age_hours INTEGER NOT NULL,
                payload TEXT NOT NULL,
                observed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(platform,remote_id,age_hours)
             );
             CREATE TABLE IF NOT EXISTS model_calls(
                id TEXT PRIMARY KEY,
                session_id TEXT,
                candidate_id TEXT,
                stage TEXT NOT NULL,
                model TEXT NOT NULL,
                request_hash TEXT NOT NULL,
                response TEXT NOT NULL,
                latency_ms INTEGER,
                estimated_cost_usd REAL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(stage,model,request_hash)
             );
             CREATE TABLE IF NOT EXISTS channel_profiles(
                channel_id TEXT PRIMARY KEY,
                version INTEGER NOT NULL,
                profile TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             INSERT OR IGNORE INTO schema_migrations(version) VALUES(1);",
        )?;
        Ok(Self { conn })
    }

    pub fn create_session(&self, id: &str, channel_id: &str, source_uri: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions(id,channel_id,source_uri) VALUES(?1,?2,?3)",
            params![id, channel_id, source_uri],
        )?;
        Ok(())
    }

    pub fn finish_session(&self, id: &str, status: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET status=?2,ended_at=CURRENT_TIMESTAMP WHERE id=?1",
            params![id, status],
        )?;
        Ok(())
    }

    pub fn record_timeline(
        &self,
        session_id: &str,
        at_ms: i64,
        kind: &str,
        payload: &serde_json::Value,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO timeline_events(session_id,at_ms,kind,payload) VALUES(?1,?2,?3,?4)",
            params![session_id, at_ms, kind, payload.to_string()],
        )?;
        Ok(())
    }

    pub fn record_transcript(&self, segment: &TranscriptSegment) -> Result<bool> {
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO transcripts(session_id,start_ms,end_ms,text,confidence,no_speech_probability,is_final)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                segment.session_id,
                segment.start_ms,
                segment.end_ms,
                segment.text,
                segment.confidence,
                segment.no_speech_probability,
                segment.is_final
            ],
        )?;
        Ok(inserted == 1)
    }

    pub fn upsert_candidate(&self, candidate: &Candidate) -> Result<bool> {
        candidate.validate()?;
        let inserted = self.conn.execute(
            "INSERT INTO candidates(
                id,session_id,channel_id,source,source_id,start_ms,end_ms,payoff_ms,
                transcript,observer_confidence,state,idempotency_key
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(idempotency_key) DO NOTHING",
            params![
                candidate.id,
                candidate.session_id,
                candidate.channel_id,
                candidate.source,
                candidate.source_id,
                candidate.start_ms,
                candidate.end_ms,
                candidate.payoff_ms,
                candidate.transcript,
                candidate.observer_confidence,
                candidate.state.to_string(),
                candidate.idempotency_key()
            ],
        )?;
        Ok(inserted == 1)
    }

    pub fn transition(&self, id: &str, expected: ClipState, next: ClipState) -> Result<bool> {
        if !expected.can_transition_to(&next) {
            bail!("invalid transition {expected} -> {next}");
        }
        let changed = self.conn.execute(
            "UPDATE candidates SET state=?1,updated_at=CURRENT_TIMESTAMP WHERE id=?2 AND state=?3",
            params![next.to_string(), id, expected.to_string()],
        )?;
        Ok(changed == 1)
    }

    pub fn fail_candidate(&self, id: &str, error: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE candidates SET state='failed',last_error=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?1",
            params![id, error],
        )?;
        Ok(())
    }

    pub fn note_candidate_attempt(&self, id: &str) -> Result<u32> {
        self.conn.execute(
            "UPDATE candidates SET attempts=attempts+1,updated_at=CURRENT_TIMESTAMP WHERE id=?1",
            [id],
        )?;
        self.conn
            .query_row("SELECT attempts FROM candidates WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .context("candidate does not exist")
    }

    pub fn get_candidate(&self, id: &str) -> Result<Option<Candidate>> {
        self.conn
            .query_row(
                "SELECT id,session_id,channel_id,source,source_id,start_ms,end_ms,payoff_ms,
                        transcript,observer_confidence,state
                 FROM candidates WHERE id=?1",
                [id],
                row_to_candidate,
            )
            .optional()
            .context("load candidate")
    }

    pub fn pending_candidates(&self, limit: usize) -> Result<Vec<Candidate>> {
        let mut statement = self.conn.prepare(
            "SELECT id,session_id,channel_id,source,source_id,start_ms,end_ms,payoff_ms,
                    transcript,observer_confidence,state
             FROM candidates
             WHERE state IN ('accepted','rendered','staged','publishing')
             ORDER BY created_at LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], row_to_candidate)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("query pending candidates")
    }

    pub fn record_decision(
        &self,
        candidate_id: &str,
        model: &str,
        decision: &EditorialDecision,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO decisions(candidate_id,stage,model,decision)
             VALUES(?1,?2,?3,?4)
             ON CONFLICT(candidate_id,stage) DO UPDATE SET
                model=excluded.model,decision=excluded.decision,created_at=CURRENT_TIMESTAMP",
            params![
                candidate_id,
                decision.stage.to_string(),
                model,
                serde_json::to_string(decision)?
            ],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_model_call_at(
        db_path: &Path,
        session_id: Option<&str>,
        candidate_id: Option<&str>,
        stage: &str,
        model: &str,
        request_hash: &str,
        response: &serde_json::Value,
        latency_ms: u64,
        estimated_cost_usd: Option<f64>,
    ) -> Result<()> {
        let conn = Connection::open(db_path).context("open SQLite store for model usage")?;
        conn.execute(
            "INSERT INTO model_calls(
                id,session_id,candidate_id,stage,model,request_hash,response,latency_ms,estimated_cost_usd
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(stage,model,request_hash) DO UPDATE SET
                response=excluded.response,
                latency_ms=excluded.latency_ms,
                estimated_cost_usd=excluded.estimated_cost_usd",
            params![
                uuid::Uuid::new_v4().to_string(),
                session_id,
                candidate_id,
                stage,
                model,
                request_hash,
                serde_json::to_string(response)?,
                latency_ms.min(i64::MAX as u64) as i64,
                estimated_cost_usd,
            ],
        )?;
        Ok(())
    }

    pub fn estimated_model_cost_usd(&self, session_id: &str) -> Result<Option<f64>> {
        self.conn
            .query_row(
                "SELECT SUM(estimated_cost_usd) FROM model_calls WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .context("sum estimated model cost")
    }

    pub fn record_manifest(&self, manifest: &EditManifest) -> Result<()> {
        self.conn.execute(
            "INSERT INTO manifests(candidate_id,version,manifest) VALUES(?1,?2,?3)
             ON CONFLICT(candidate_id,version) DO UPDATE SET manifest=excluded.manifest",
            params![
                manifest.candidate_id,
                manifest.version,
                serde_json::to_string(manifest)?
            ],
        )?;
        Ok(())
    }

    pub fn final_decision(&self, candidate_id: &str) -> Result<Option<EditorialDecision>> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT decision FROM decisions WHERE candidate_id=?1 AND stage='critic'",
                [candidate_id],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|value| serde_json::from_str(&value).context("parse stored critic decision"))
            .transpose()
    }

    pub fn transcripts_for_candidate(
        &self,
        candidate: &Candidate,
    ) -> Result<Vec<TranscriptSegment>> {
        let mut statement = self.conn.prepare(
            "SELECT session_id,start_ms,end_ms,text,confidence,no_speech_probability,is_final
             FROM transcripts WHERE session_id=?1 AND end_ms>?2 AND start_ms<?3 ORDER BY start_ms",
        )?;
        let rows = statement.query_map(
            params![candidate.session_id, candidate.start_ms, candidate.end_ms],
            |row| {
                Ok(TranscriptSegment {
                    session_id: row.get(0)?,
                    start_ms: row.get(1)?,
                    end_ms: row.get(2)?,
                    text: row.get(3)?,
                    confidence: row.get(4)?,
                    no_speech_probability: row.get(5)?,
                    is_final: row.get(6)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn enqueue_publish_job(
        &self,
        id: &str,
        candidate_id: &str,
        platform: &str,
        idempotency_key: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO publish_jobs(id,candidate_id,platform,idempotency_key)
             VALUES(?1,?2,?3,?4)
             ON CONFLICT(candidate_id,platform) DO NOTHING",
            params![id, candidate_id, platform, idempotency_key],
        )?;
        Ok(())
    }

    pub fn publish_job_done(&self, candidate_id: &str, platform: &str) -> Result<bool> {
        let done = self
            .conn
            .query_row(
                "SELECT state IN ('published','awaiting_creator') FROM publish_jobs
                 WHERE candidate_id=?1 AND platform=?2",
                params![candidate_id, platform],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(false);
        Ok(done)
    }

    pub fn fail_publish_job(&self, candidate_id: &str, platform: &str, error: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE publish_jobs SET state='retryable',attempts=attempts+1,last_error=?3,
                    updated_at=CURRENT_TIMESTAMP WHERE candidate_id=?1 AND platform=?2",
            params![candidate_id, platform, error],
        )?;
        Ok(())
    }

    pub fn complete_publish_job(&self, outcome: &Outcome) -> Result<()> {
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "UPDATE publish_jobs SET state=?3,remote_id=?4,last_error=NULL,updated_at=CURRENT_TIMESTAMP
             WHERE candidate_id=?1 AND platform=?2",
            params![
                outcome.candidate_id,
                outcome.platform,
                outcome.status,
                outcome.remote_id
            ],
        )?;
        transaction.execute(
            "INSERT INTO posts(candidate_id,platform,remote_id,status,url,idempotency_key)
             VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(idempotency_key) DO UPDATE SET
                status=excluded.status,url=excluded.url,remote_id=excluded.remote_id",
            params![
                outcome.candidate_id,
                outcome.platform,
                outcome.remote_id,
                outcome.status,
                outcome.url,
                outcome.idempotency_key
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn all_publish_jobs_done(&self, candidate_id: &str) -> Result<bool> {
        let remaining: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM publish_jobs
             WHERE candidate_id=?1 AND state NOT IN ('published','awaiting_creator')",
            [candidate_id],
            |row| row.get(0),
        )?;
        Ok(remaining == 0)
    }

    pub fn candidate_has_awaiting_creator(&self, candidate_id: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM publish_jobs WHERE candidate_id=?1 AND state='awaiting_creator'",
            [candidate_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn record_metrics(&self, metrics: &OutcomeMetrics) -> Result<()> {
        self.conn.execute(
            "INSERT INTO metrics(platform,remote_id,age_hours,payload) VALUES(?1,?2,?3,?4)
             ON CONFLICT(platform,remote_id,age_hours) DO UPDATE SET
                payload=excluded.payload,observed_at=CURRENT_TIMESTAMP",
            params![
                metrics.platform,
                metrics.remote_id,
                metrics.age_hours,
                serde_json::to_string(metrics)?
            ],
        )?;
        Ok(())
    }

    pub fn upsert_profile(&self, profile: &ChannelProfile) -> Result<()> {
        self.conn.execute(
            "INSERT INTO channel_profiles(channel_id,version,profile) VALUES(?1,?2,?3)
             ON CONFLICT(channel_id) DO UPDATE SET
                version=excluded.version,profile=excluded.profile,updated_at=CURRENT_TIMESTAMP",
            params![
                profile.channel_id,
                profile.version,
                serde_json::to_string(profile)?
            ],
        )?;
        Ok(())
    }

    pub fn profile(&self, channel_id: &str) -> Result<Option<ChannelProfile>> {
        let profile: Option<String> = self
            .conn
            .query_row(
                "SELECT profile FROM channel_profiles WHERE channel_id=?1",
                [channel_id],
                |row| row.get(0),
            )
            .optional()?;
        profile
            .map(|value| serde_json::from_str(&value).context("parse channel profile"))
            .transpose()
    }

    pub fn status_counts(&self) -> Result<Vec<(String, u64)>> {
        let mut statement = self
            .conn
            .prepare("SELECT state,COUNT(*) FROM candidates GROUP BY state ORDER BY state")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn row_to_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<Candidate> {
    let state: String = row.get(10)?;
    Ok(Candidate {
        id: row.get(0)?,
        session_id: row.get(1)?,
        channel_id: row.get(2)?,
        source: row.get(3)?,
        source_id: row.get(4)?,
        start_ms: row.get(5)?,
        end_ms: row.get(6)?,
        payoff_ms: row.get(7)?,
        transcript: row.get(8)?,
        observer_confidence: row.get(9)?,
        state: state.parse().map_err(|error: String| {
            rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, error.into())
        })?,
    })
}
