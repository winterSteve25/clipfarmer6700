import { useEffect, useState } from "react";
import type { JobProgress, JobSnapshot, RunSummary } from "../types";
import { Icon } from "../components/Icon";

const ACTIVE_STATUSES = new Set(["queued", "running", "cancelling"]);

const PHASE_LABELS: Record<string, string> = {
  queued: "Queued",
  starting: "Starting worker",
  starting_capture: "Starting video capture",
  starting_chat: "Starting chat capture",
  preparing_vod: "Preparing VOD",
  loading_transcription: "Loading transcription",
  configuring_models: "Configuring AI models",
  configuring_staging: "Configuring staging",
  configuring_publishers: "Configuring publishers",
  opening_database: "Opening database",
  capturing: "Capturing and analyzing",
  analyzing: "Analyzing media",
  cancelling: "Stopping",
  completed: "Completed",
  cancelled: "Cancelled",
  failed: "Failed",
};

function phaseLabel(phase: string) {
  return PHASE_LABELS[phase] ?? phase.replace(/_/g, " ");
}

function formatDuration(milliseconds: number | null | undefined) {
  if (milliseconds == null) return "-";
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours) return `${hours}h ${minutes}m ${seconds}s`;
  if (minutes) return `${minutes}m ${seconds}s`;
  return `${seconds}s`;
}

function formatDate(milliseconds: number) {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "medium",
  }).format(new Date(milliseconds));
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KiB", "MiB", "GiB", "TiB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value.toFixed(value >= 10 ? 1 : 2)} ${unit}`;
}

function formatCost(cost: number) {
  if (cost < 0.01) return `$${cost.toFixed(4)}`;
  return `$${cost.toFixed(2)}`;
}

function sourceLabel(job: JobSnapshot) {
  if (job.source.type === "channel") {
    return `twitch.tv/${job.source.channel ?? job.channel ?? "unknown"}`;
  }
  return job.source.url ?? "Twitch VOD";
}

function jobName(job: JobSnapshot) {
  return job.channel
    || (job.source.type === "channel" ? job.source.channel : "Twitch VOD")
    || "Untitled run";
}

function latestSummary(job: JobSnapshot): RunSummary | null {
  return job.summary ?? job.progress.summary;
}

function SummaryPill({ label, value, tone }: { label: string; value: number; tone?: "good" | "bad" }) {
  return <div className={`summary-pill ${tone ?? ""}`}><strong>{value}</strong><span>{label}</span></div>;
}

function Metric({ label, value, tone }: { label: string; value: number | string; tone?: "good" | "bad" }) {
  return <div className={`job-metric ${tone ?? ""}`}><span>{label}</span><strong>{value}</strong></div>;
}

function progressPercent(progress: JobProgress) {
  if (!progress.totalUnits || progress.completedUnits == null) return null;
  return Math.min(100, Math.max(0, Math.round(progress.completedUnits / progress.totalUnits * 100)));
}

function ProgressBar({ progress }: { progress: JobProgress }) {
  const percent = progressPercent(progress);
  return <div
    className={`job-progress ${percent == null ? "indeterminate" : ""}`}
    role="progressbar"
    aria-label={phaseLabel(progress.phase)}
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={percent ?? undefined}
  ><span style={percent == null ? undefined : { width: `${percent}%` }}/></div>;
}

function ActivityTimeline({ events }: { events: JobProgress[] }) {
  return <ol className="job-timeline">
    {events.map((event, index) => <li key={`${event.phase}-${event.elapsedMs}-${index}`}>
      <span className="timeline-marker"><Icon name={index === events.length - 1 ? "radio" : "check"} size={12}/></span>
      <div><strong>{phaseLabel(event.phase)}</strong><p>{event.message}</p></div>
      <time>{formatDuration(event.elapsedMs)}</time>
    </li>)}
  </ol>;
}

function JobCard({ job, now, retrying, onCancel, onRetry }: { job: JobSnapshot; now: number; retrying: boolean; onCancel: (id: string) => void; onRetry: (id: string) => void }) {
  const summary = latestSummary(job);
  const active = ACTIVE_STATUSES.has(job.status);
  const runtime = job.finishedAtMs
    ? job.finishedAtMs - job.createdAtMs
    : Math.max(job.progress.elapsedMs, active ? now - job.createdAtMs : 0);
  const history = job.history?.length ? job.history : [job.progress];
  const percent = progressPercent(job.progress);
  return <details className={`job-card ${job.status}`} open={active || job.status === "failed"}>
    <summary>
      <span className={`job-state ${job.status}`}><Icon name={active ? "radio" : job.status === "completed" ? "check" : "x"}/></span>
      <span className="job-main"><strong>{jobName(job)}</strong><span><b>{phaseLabel(job.progress.phase)}</b> - {job.progress.message}</span>{active && <ProgressBar progress={job.progress}/>}</span>
      <span className="job-quick-stats">{percent != null && <strong>{percent}%</strong>}{job.progress.transferredBytes != null && <strong>{formatBytes(job.progress.transferredBytes)} downloaded</strong>}<span>{formatDuration(runtime)} elapsed</span>{job.progress.capturedMs != null && <span>{formatDuration(job.progress.capturedMs)} captured</span>}</span>
      <span className={`status-badge ${job.status}`}>{job.status}</span>
      <span className="job-expand"><Icon name="chevron" size={16}/></span>
    </summary>
    <div className="job-details">
      {job.error && <section className="job-error" aria-label="Failure details"><div><Icon name="x" size={17}/><strong>Why this run failed</strong></div><pre>{job.error}</pre></section>}
      <div className="job-detail-grid">
        <section>
          <span className="detail-label">Current activity</span>
          <h3>{phaseLabel(job.progress.phase)}</h3>
          <p>{job.progress.message}</p>
        </section>
        <dl className="job-facts">
          <div><dt>Source</dt><dd title={sourceLabel(job)}>{sourceLabel(job)}</dd></div>
          <div><dt>Started</dt><dd>{formatDate(job.createdAtMs)}</dd></div>
          <div><dt>Run time</dt><dd>{formatDuration(runtime)}</dd></div>
          <div><dt>Captured media</dt><dd>{formatDuration(job.progress.capturedMs)}</dd></div>
          <div><dt>Job ID</dt><dd title={job.id}>{job.id}</dd></div>
          <div><dt>Output folder</dt><dd title={job.outputDir}>{job.outputDir}</dd></div>
        </dl>
      </div>
      <section className="job-results">
        <span className="detail-label">Pipeline totals</span>
        <div className="job-metrics">
          <Metric label="Windows observed" value={summary?.windowsObserved ?? 0}/>
          <Metric label="Candidates reviewed" value={summary?.candidatesReviewed ?? 0}/>
          <Metric label="Approved" value={summary?.candidatesAccepted ?? 0} tone="good"/>
          <Metric label="Rejected" value={summary?.candidatesRejected ?? 0}/>
          <Metric label="Posts completed" value={summary?.postsCompleted ?? 0} tone="good"/>
          <Metric label="Publish failures" value={summary?.publishFailures ?? 0} tone={(summary?.publishFailures ?? 0) > 0 ? "bad" : undefined}/>
          {summary?.estimatedApiCostUsd != null && <Metric label="Estimated API cost" value={formatCost(summary.estimatedApiCostUsd)}/>}
        </div>
      </section>
      <section className="job-activity">
        <span className="detail-label">Activity trace</span>
        <ActivityTimeline events={history}/>
      </section>
      {(active || ((job.status === "failed" || job.status === "cancelled") && job.source.type === "vod")) && <div className="job-actions">
        {active && <button className="text-button danger" type="button" disabled={job.status === "cancelling"} onClick={() => onCancel(job.id)}>{job.status === "cancelling" ? "Stopping…" : "Stop run"}</button>}
        {!active && job.source.type === "vod" && <button className="text-button retry" type="button" disabled={retrying} onClick={() => onRetry(job.id)}><Icon name="refresh" size={14}/>{retrying ? "Resuming…" : "Resume from checkpoint"}</button>}
      </div>}
    </div>
  </details>;
}

export function HistoryView({ jobs, retryingJobId, onCancel, onRetry, onNewRun }: { jobs: JobSnapshot[]; retryingJobId: string | null; onCancel: (id: string) => void; onRetry: (id: string) => void; onNewRun: () => void }) {
  const hasActiveJobs = jobs.some((job) => ACTIVE_STATUSES.has(job.status));
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (!hasActiveJobs) return;
    setNow(Date.now());
    const interval = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(interval);
  }, [hasActiveJobs]);
  const accepted = jobs.reduce((sum, job) => sum + (latestSummary(job)?.candidatesAccepted ?? 0), 0);
  const rejected = jobs.reduce((sum, job) => sum + (latestSummary(job)?.candidatesRejected ?? 0), 0);
  const failed = jobs.filter((job) => job.status === "failed").length;
  return <section className="history-view">
    <div className="history-summary"><SummaryPill label="Runs" value={jobs.length}/><SummaryPill label="Approved" value={accepted} tone="good"/><SummaryPill label="Rejected" value={rejected}/><SummaryPill label="Failed runs" value={failed} tone={failed ? "bad" : undefined}/></div>
    <div className="history-card">
      <div className="history-card-head"><h2>Clipping runs</h2><button className="primary-small" onClick={onNewRun}><Icon name="plus" size={15}/>New run</button></div>
      {jobs.length ? <div className="job-list">{jobs.map((job) => <JobCard key={job.id} job={job} now={now} retrying={retryingJobId === job.id} onCancel={onCancel} onRetry={onRetry}/>)}</div> : <div className="empty-state"><span><Icon name="film" size={28}/></span><h3>No runs yet</h3><p>Start a live channel or VOD run to see progress, diagnostics, and result totals here.</p><button onClick={onNewRun}>Configure first run</button></div>}
    </div>
  </section>;
}
