import type { JobSnapshot } from "../types";
import { Icon } from "../components/Icon";

export function HistoryView({ jobs, onCancel, onNewRun }: { jobs: JobSnapshot[]; onCancel: (id: string) => void; onNewRun: () => void }) {
  const approved = jobs.reduce((sum, job) => sum + (job.summary?.candidatesAccepted ?? job.progress.summary?.candidatesAccepted ?? 0), 0);
  const rejected = jobs.reduce((sum, job) => sum + (job.summary?.candidatesRejected ?? job.progress.summary?.candidatesRejected ?? 0), 0);
  return <section className="history-view">
    <div className="history-summary"><SummaryPill label="Runs" value={jobs.length}/><SummaryPill label="Approved" value={approved} tone="good"/><SummaryPill label="Rejected" value={rejected} tone="bad"/></div>
    <div className="history-card">
      <div className="history-card-head"><div><span className="eyebrow">RECENT ACTIVITY</span><h2>Clipping runs</h2></div><button className="primary-small" onClick={onNewRun}><Icon name="plus" size={15}/>New run</button></div>
      {jobs.length ? <div className="job-list">{jobs.map((job) => <JobRow key={job.id} job={job} onCancel={onCancel}/>)}</div> : <div className="empty-state"><span><Icon name="film" size={28}/></span><h3>No runs yet</h3><p>Start a live channel or VOD run to see progress and result totals here.</p><button onClick={onNewRun}>Configure first run</button></div>}
    </div>
  </section>;
}

function SummaryPill({ label, value, tone }: { label: string; value: number; tone?: "good" | "bad" }) {
  return <div className={`summary-pill ${tone ?? ""}`}><strong>{value}</strong><span>{label}</span></div>;
}

function JobRow({ job, onCancel }: { job: JobSnapshot; onCancel: (id: string) => void }) {
  const summary = job.summary ?? job.progress.summary;
  const active = ["queued", "running", "cancelling"].includes(job.status);
  const name = job.channel || (job.source.type === "vod" ? "Twitch VOD" : job.source.channel) || "Untitled run";
  return <article className="job-row"><span className={`job-state ${job.status}`}><Icon name={active ? "radio" : job.status === "completed" ? "check" : "x"}/></span><div className="job-main"><strong>{name}</strong><span>{job.progress.message}</span></div>{summary && <div className="job-counts"><span>{summary.candidatesAccepted} approved</span><span>{summary.candidatesRejected} rejected</span></div>}<span className={`status-badge ${job.status}`}>{job.status}</span>{active && <button className="text-button danger" type="button" disabled={job.status === "cancelling"} onClick={() => onCancel(job.id)}>{job.status === "cancelling" ? "Stopping" : "Stop"}</button>}</article>;
}
