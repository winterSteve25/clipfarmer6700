import { FormEvent, ReactNode, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type SourceMode = "channel" | "vod";
type Provider = "gemini" | "openai";
type ScribbleModel = "large_turbo" | "tiny";
type PreviewTab = "approved" | "rejected";
type NavItem = "new" | "jobs" | "presets";
type IconName = "spark" | "plus" | "history" | "layers" | "settings" | "sliders" | "brain" | "send" | "play" | "check" | "x" | "clock" | "film" | "chevron" | "copy" | "radio" | "refresh";

type RunSummary = {
  windowsObserved: number; candidatesReviewed: number; candidatesAccepted: number;
  candidatesRejected: number; postsCompleted: number; publishFailures: number;
};

type JobSnapshot = {
  id: string;
  source: { type: SourceMode; channel?: string; url?: string };
  channel: string | null;
  outputDir: string;
  status: "queued" | "running" | "cancelling" | "completed" | "cancelled" | "failed";
  progress: { phase: string; message: string; elapsedMs: number; capturedMs: number | null; summary: RunSummary | null };
  summary: RunSummary | null;
  error: string | null;
  createdAtMs: number;
  finishedAtMs: number | null;
};

type ConfigState = {
  provider: Provider; apiKeyEnv: string; pollSeconds: number; maxAttempts: number;
  ringMinutes: number; observerWindowSeconds: number; observerStepSeconds: number;
  maturationDelaySeconds: number; queueCapacity: number; frameIntervalSeconds: number;
  scribbleModel: ScribbleModel; enableVad: boolean; language: string; incrementalMinWindowSeconds: number;
  dryRun: boolean; youtube: boolean; instagram: boolean; tiktokDrafts: boolean; twitchClips: boolean;
  stagingProvider: "local" | "s3"; bucket: string; prefix: string; publicBaseUrl: string; endpointEnv: string;
  ffmpegPath: string; ffprobePath: string; streamlinkPath: string; chatDownloaderPath: string;
};

const DEFAULT_CONFIG: ConfigState = {
  provider: "gemini", apiKeyEnv: "GEMINI_API_KEY", pollSeconds: 10, maxAttempts: 4,
  ringMinutes: 10, observerWindowSeconds: 12, observerStepSeconds: 6, maturationDelaySeconds: 20,
  queueCapacity: 256, frameIntervalSeconds: 1, scribbleModel: "large_turbo", enableVad: true, language: "auto",
  incrementalMinWindowSeconds: 30, dryRun: true, youtube: true, instagram: false,
  tiktokDrafts: false, twitchClips: false, stagingProvider: "local", bucket: "clipfarmer-artifacts",
  prefix: "staged", publicBaseUrl: "", endpointEnv: "S3_ENDPOINT", ffmpegPath: "ffmpeg",
  ffprobePath: "ffprobe", streamlinkPath: "streamlink", chatDownloaderPath: "chat_downloader",
};

const isTauri = () => "__TAURI_INTERNALS__" in window;

function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  const paths: Record<IconName, ReactNode> = {
    spark: <><path d="m12 3 1.2 3.8L17 8l-3.8 1.2L12 13l-1.2-3.8L7 8l3.8-1.2L12 3Z"/><path d="m5 13 .8 2.2L8 16l-2.2.8L5 19l-.8-2.2L2 16l2.2-.8L5 13Z"/></>,
    plus: <><path d="M12 5v14M5 12h14"/></>,
    history: <><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5M12 7v5l3 2"/></>,
    layers: <><path d="m12 3-9 5 9 5 9-5-9-5Z"/><path d="m3 12 9 5 9-5M3 16l9 5 9-5"/></>,
    settings: <><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3A1.7 1.7 0 0 0 10 3V2.8h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z"/></>,
    sliders: <><path d="M4 7h10M18 7h2M4 17h2M10 17h10M14 4v6M6 14v6"/></>,
    brain: <><path d="M9.5 4.5A3 3 0 0 0 4 6.2 3.5 3.5 0 0 0 3.5 13 3.5 3.5 0 0 0 7 18.5 3 3 0 0 0 12 17V7a2.5 2.5 0 0 0-2.5-2.5ZM14.5 4.5A3 3 0 0 1 20 6.2a3.5 3.5 0 0 1 .5 6.8 3.5 3.5 0 0 1-3.5 5.5 3 3 0 0 1-5-1.5V7a2.5 2.5 0 0 1 2.5-2.5Z"/><path d="M8 9h4M12 13h4"/></>,
    send: <><path d="m21 3-7.3 18-3.9-7-6.8-4L21 3Z"/><path d="m9.8 14 4-4"/></>,
    play: <><path d="m8 5 11 7-11 7V5Z"/></>, check: <><path d="m5 12 4 4L19 6"/></>, x: <><path d="m6 6 12 12M18 6 6 18"/></>,
    clock: <><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></>,
    film: <><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 4v16M17 4v16M3 9h4M17 9h4M3 15h4M17 15h4"/></>,
    chevron: <><path d="m9 18 6-6-6-6"/></>,
    copy: <><rect x="8" y="8" width="11" height="11" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/></>,
    radio: <><circle cx="12" cy="12" r="2"/><path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4M4.2 4.2a11 11 0 0 0 0 15.6M19.8 4.2a11 11 0 0 1 0 15.6"/></>,
    refresh: <><path d="M20 7v5h-5M4 17v-5h5"/><path d="M6.1 8A7 7 0 0 1 18 6l2 6M18 16a7 7 0 0 1-11.9 2L4 12"/></>,
  };
  return <svg className="icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (value: boolean) => void; label: string }) {
  return <button type="button" role="switch" aria-checked={checked} aria-label={label} className={`toggle ${checked ? "on" : ""}`} onClick={() => onChange(!checked)}><span /></button>;
}
function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return <label className="field"><span className="field-label">{label}</span>{children}{hint && <small>{hint}</small>}</label>;
}
function NumberInput({ value, onChange, min = 1, max, suffix }: { value: number; onChange: (value: number) => void; min?: number; max?: number; suffix?: string }) {
  return <div className="number-input"><input type="number" value={value} min={min} max={max} onChange={(event) => onChange(Number(event.target.value))}/>{suffix && <span>{suffix}</span>}</div>;
}
function Section({ icon, title, description, open, onToggle, children }: { icon: IconName; title: string; description: string; open: boolean; onToggle: () => void; children: ReactNode }) {
  return <section className={`config-section ${open ? "open" : ""}`}><button className="section-head" type="button" onClick={onToggle} aria-expanded={open}><span className="section-icon"><Icon name={icon}/></span><span><strong>{title}</strong><small>{description}</small></span><span className="section-chevron"><Icon name="chevron" size={16}/></span></button>{open && <div className="section-content">{children}</div>}</section>;
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

function App() {
  const [nav, setNav] = useState<NavItem>("new");
  const [sourceMode, setSourceMode] = useState<SourceMode>("channel");
  const [sourceValue, setSourceValue] = useState("synthcity_live");
  const [config, setConfig] = useState<ConfigState>(() => { try { return { ...DEFAULT_CONFIG, ...JSON.parse(localStorage.getItem("clipfarmer-config") || "{}") }; } catch { return DEFAULT_CONFIG; } });
  const [previewTab, setPreviewTab] = useState<PreviewTab>("approved");
  const [openSections, setOpenSections] = useState({ capture: true, ai: true, publishing: true, advanced: false });
  const [jobs, setJobs] = useState<JobSnapshot[]>([]);
  const [starting, setStarting] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const set = <K extends keyof ConfigState>(key: K, value: ConfigState[K]) => setConfig((current) => ({ ...current, [key]: value }));
  const toggleSection = (key: keyof typeof openSections) => setOpenSections((current) => ({ ...current, [key]: !current[key] }));

  useEffect(() => { localStorage.setItem("clipfarmer-config", JSON.stringify(config)); }, [config]);
  useEffect(() => {
    if (!isTauri()) return;
    invoke<JobSnapshot[]>("list_clipping_jobs").then(setJobs).catch((error) => setNotice(String(error)));
    let dispose: (() => void) | undefined;
    listen<JobSnapshot>("clipfarmer-job-progress", (event) => setJobs((current) => [event.payload, ...current.filter((job) => job.id !== event.payload.id)])).then((unlisten) => { dispose = unlisten; });
    return () => dispose?.();
  }, []);

  const backendConfig = useMemo(() => ({
    worker: { pollSeconds: config.pollSeconds, maxAttempts: config.maxAttempts, ringMinutes: config.ringMinutes, observerWindowSeconds: config.observerWindowSeconds, observerStepSeconds: config.observerStepSeconds, maturationDelaySeconds: config.maturationDelaySeconds, queueCapacity: config.queueCapacity },
    media: { ffmpegPath: config.ffmpegPath, ffprobePath: config.ffprobePath, streamlinkPath: config.streamlinkPath, chatDownloaderPath: config.chatDownloaderPath, frameIntervalSeconds: config.frameIntervalSeconds },
    scribble: { modelVariant: config.scribbleModel, enableVad: config.enableVad, language: config.language, incrementalMinWindowSeconds: config.incrementalMinWindowSeconds },
    models: { provider: config.provider },
    [config.provider]: { apiKeyEnv: config.apiKeyEnv, observerModel: config.provider === "gemini" ? "gemini-3.7-flash" : "gpt-5.6-terra", directorModel: config.provider === "gemini" ? "gemini-3.7-flash" : "gpt-5.6-sol", editorModel: config.provider === "gemini" ? "gemini-3.7-flash" : "gpt-5.6-sol", criticModel: config.provider === "gemini" ? "gemini-3.7-flash" : "gpt-5.6-sol", audioModel: config.provider === "gemini" ? "gemini-3.7-flash" : "gpt-audio-1.5" },
    staging: { provider: config.stagingProvider, bucket: config.bucket, prefix: config.prefix, publicBaseUrl: config.publicBaseUrl || null, endpointEnv: config.endpointEnv },
    publishers: { dryRun: config.dryRun, youtube: config.youtube, instagram: config.instagram, tiktokDrafts: config.tiktokDrafts, twitchClips: config.twitchClips },
  }), [config]);

  const outputPreview = useMemo(() => {
    const accepted = previewTab === "approved";
    return { candidateId: accepted ? "clip_8f2c1" : "clip_3ad90", status: accepted ? "accepted" : "rejected", confidence: accepted ? 0.94 : 0.41, title: accepted ? "The impossible comeback" : "Quiet inventory management", startMs: accepted ? 1842000 : 2478000, endMs: accepted ? 1876500 : 2501000, hookText: accepted ? "Nobody thought this run was recoverable." : null, layout: accepted ? "vertical_focus" : null, rationale: accepted ? `Strong payoff, readable reaction, and a clear ${Math.round(config.observerWindowSeconds)}s narrative arc.` : `No distinct hook or payoff after the ${config.maturationDelaySeconds}s maturation window.`, publish: accepted ? (config.dryRun ? "dry_run" : "queued") : "skipped" };
  }, [previewTab, config.observerWindowSeconds, config.maturationDelaySeconds, config.dryRun]);

  const invalidWindow = config.observerWindowSeconds < config.observerStepSeconds;
  const sourceValid = sourceMode === "channel" ? /^[A-Za-z0-9_]+$/.test(sourceValue) : /^https:\/\/(www\.)?twitch\.tv\/videos\//.test(sourceValue);
  const platformCount = [config.youtube, config.instagram, config.tiktokDrafts, config.twitchClips].filter(Boolean).length;

  async function startRun(event: FormEvent) {
    event.preventDefault();
    if (!sourceValid || invalidWindow) return;
    if (!isTauri()) { setNotice("Run controls connect when this interface is opened in the ClipFarmer desktop app."); return; }
    setStarting(true); setNotice(null);
    try {
      const command = sourceMode === "channel" ? "start_channel_clipping_job" : "start_vod_clipping_job";
      const args = sourceMode === "channel" ? { channel: sourceValue, config: backendConfig } : { vodUrl: sourceValue, config: backendConfig };
      const job = await invoke<JobSnapshot>(command, args);
      setJobs((current) => [job, ...current.filter((item) => item.id !== job.id)]); setNav("jobs");
    } catch (error) { setNotice(String(error)); } finally { setStarting(false); }
  }
  async function cancelJob(id: string) { try { await invoke("cancel_clipping_job", { jobId: id }); } catch (error) { setNotice(String(error)); } }
  async function copyOutput() { await navigator.clipboard.writeText(JSON.stringify(outputPreview, null, 2)); setCopied(true); window.setTimeout(() => setCopied(false), 1400); }

  return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark"><Icon name="spark" size={22}/></span><span><strong>ClipFarmer</strong><small>Editorial console</small></span></div>
      <nav aria-label="Main navigation"><button className={nav === "new" ? "active" : ""} onClick={() => setNav("new")}><Icon name="plus"/><span>New run</span></button><button className={nav === "jobs" ? "active" : ""} onClick={() => setNav("jobs")}><Icon name="history"/><span>Run history</span>{jobs.length > 0 && <em>{jobs.length}</em>}</button><button className={nav === "presets" ? "active" : ""} onClick={() => setNav("presets")}><Icon name="layers"/><span>Presets</span></button></nav>
      <div className="sidebar-spacer"/><div className="system-card"><span className="system-dot"/><div><strong>Backend ready</strong><small>{isTauri() ? "Tauri connection active" : "Browser preview mode"}</small></div></div><button className="sidebar-settings"><Icon name="settings"/><span>Settings</span></button><div className="version">ClipFarmer · v0.1.0</div>
    </aside>

    <main className="workspace">
      <header className="topbar"><div><p>WORKSPACE / {nav === "new" ? "NEW RUN" : nav === "jobs" ? "RUN HISTORY" : "PRESETS"}</p><h1>{nav === "new" ? "Configure a clipping run" : nav === "jobs" ? "Run history" : "Configuration presets"}</h1></div><div className="topbar-actions"><span className="mode-badge"><span/>{config.dryRun ? "Safe mode" : "Publishing live"}</span><button className="icon-button" title="Settings"><Icon name="settings"/></button></div></header>
      {notice && <div className="notice"><span>{notice}</span><button aria-label="Dismiss" onClick={() => setNotice(null)}><Icon name="x" size={16}/></button></div>}

      {nav === "new" && <div className="editor-layout">
        <form className="configuration-panel" onSubmit={startRun}>
          <div className="source-card"><div className="card-heading"><div><span className="eyebrow">01 · SOURCE</span><h2>Choose what to watch</h2></div><Icon name="radio" size={22}/></div><div className="segmented"><button type="button" className={sourceMode === "channel" ? "active" : ""} onClick={() => { setSourceMode("channel"); setSourceValue("synthcity_live"); }}>Live channel</button><button type="button" className={sourceMode === "vod" ? "active" : ""} onClick={() => { setSourceMode("vod"); setSourceValue("https://www.twitch.tv/videos/"); }}>Twitch VOD</button></div><label className="source-input"><span>{sourceMode === "channel" ? "twitch.tv/" : "URL"}</span><input value={sourceValue} onChange={(event) => setSourceValue(event.target.value)} aria-label={sourceMode === "channel" ? "Twitch channel" : "Twitch VOD URL"}/><span className={`validity ${sourceValid ? "valid" : ""}`}><Icon name={sourceValid ? "check" : "x"} size={15}/></span></label></div>
          <div className="config-header"><div><span className="eyebrow">02 · CONFIGURATION</span><h2>Shape the editorial pipeline</h2></div><button type="button" className="reset-button" onClick={() => setConfig(DEFAULT_CONFIG)}><Icon name="refresh" size={15}/>Reset defaults</button></div>
          <div className="sections">
            <Section icon="sliders" title="Capture & timing" description={`${config.observerWindowSeconds}s window · ${config.ringMinutes}m buffer`} open={openSections.capture} onToggle={() => toggleSection("capture")}><div className="field-grid"><Field label="Observer window" hint="Context sent for each decision"><NumberInput value={config.observerWindowSeconds} onChange={(v) => set("observerWindowSeconds", v)} suffix="sec"/></Field><Field label="Observer step" hint="How often a window advances"><NumberInput value={config.observerStepSeconds} onChange={(v) => set("observerStepSeconds", v)} suffix="sec"/></Field><Field label="Maturation delay"><NumberInput value={config.maturationDelaySeconds} onChange={(v) => set("maturationDelaySeconds", v)} suffix="sec"/></Field><Field label="Rolling buffer"><NumberInput value={config.ringMinutes} onChange={(v) => set("ringMinutes", v)} suffix="min"/></Field><Field label="Live poll interval"><NumberInput value={config.pollSeconds} onChange={(v) => set("pollSeconds", v)} min={5} suffix="sec"/></Field><Field label="Visual sample interval"><NumberInput value={config.frameIntervalSeconds} onChange={(v) => set("frameIntervalSeconds", v)} suffix="sec"/></Field></div>{invalidWindow && <p className="validation-error">Observer window must be at least one observer step.</p>}</Section>
            <Section icon="brain" title="AI review" description={`${config.provider === "gemini" ? "Gemini 3.7 Flash" : "OpenAI editorial stack"} · ${config.scribbleModel === "large_turbo" ? "Large Turbo" : "Tiny"} Scribble`} open={openSections.ai} onToggle={() => toggleSection("ai")}><Field label="Editorial model provider"><div className="provider-choice"><button type="button" className={config.provider === "gemini" ? "selected" : ""} onClick={() => { set("provider", "gemini"); set("apiKeyEnv", "GEMINI_API_KEY"); }}><span>✦</span><span><strong>Gemini</strong><small>3.7 Flash across all roles</small></span><i/></button><button type="button" className={config.provider === "openai" ? "selected" : ""} onClick={() => { set("provider", "openai"); set("apiKeyEnv", "OPENAI_API_KEY"); }}><span>◎</span><span><strong>OpenAI</strong><small>Terra observer · Sol editorial</small></span><i/></button></div></Field><Field label="Scribble transcription model"><div className="scribble-choice"><button type="button" className={config.scribbleModel === "large_turbo" ? "selected" : ""} onClick={() => set("scribbleModel", "large_turbo")}><span><strong>Large Turbo</strong><small>Best accuracy · higher memory use</small></span><i/></button><button type="button" className={config.scribbleModel === "tiny" ? "selected" : ""} onClick={() => set("scribbleModel", "tiny")}><span><strong>Tiny</strong><small>Fastest · lowest memory use</small></span><i/></button></div></Field><div className="field-grid compact"><Field label="API key environment variable"><input value={config.apiKeyEnv} onChange={(e) => set("apiKeyEnv", e.target.value)}/></Field><Field label="Transcription language"><select value={config.language} onChange={(e) => set("language", e.target.value)}><option value="auto">Auto-detect</option><option value="en">English</option><option value="es">Spanish</option><option value="fr">French</option><option value="de">German</option><option value="ja">Japanese</option><option value="ko">Korean</option></select></Field><Field label="Transcription window"><NumberInput value={config.incrementalMinWindowSeconds} onChange={(v) => set("incrementalMinWindowSeconds", v)} suffix="sec"/></Field><div className="toggle-field"><div><strong>Voice activity detection</strong><small>Skip silent audio before transcription</small></div><Toggle label="Voice activity detection" checked={config.enableVad} onChange={(v) => set("enableVad", v)}/></div></div></Section>
            <Section icon="send" title="Publishing" description={`${config.dryRun ? "Dry run" : "Live"} · ${platformCount} destination${platformCount === 1 ? "" : "s"}`} open={openSections.publishing} onToggle={() => toggleSection("publishing")}><div className="toggle-field prominent"><div><strong>Dry run</strong><small>Render outputs without posting to connected platforms</small></div><Toggle label="Dry run" checked={config.dryRun} onChange={(v) => set("dryRun", v)}/></div><div className="platform-list">{([ ["youtube", "YouTube Shorts", "YT", config.youtube], ["instagram", "Instagram Reels", "IG", config.instagram], ["tiktokDrafts", "TikTok drafts", "TT", config.tiktokDrafts], ["twitchClips", "Twitch clips", "TW", config.twitchClips] ] as const).map(([key, label, monogram, checked]) => <div className="platform-row" key={key}><span className={`platform-icon ${key}`}>{monogram}</span><span>{label}</span><Toggle label={label} checked={checked} onChange={(v) => set(key, v)}/></div>)}</div></Section>
            <Section icon="settings" title="Advanced" description={`${config.stagingProvider} staging · ${config.maxAttempts} attempts`} open={openSections.advanced} onToggle={() => toggleSection("advanced")}><div className="field-grid"><Field label="Staging provider"><select value={config.stagingProvider} onChange={(e) => set("stagingProvider", e.target.value as "local" | "s3")}><option value="local">Local</option><option value="s3">S3-compatible</option></select></Field><Field label="Max attempts"><NumberInput value={config.maxAttempts} onChange={(v) => set("maxAttempts", v)}/></Field><Field label="Queue capacity"><NumberInput value={config.queueCapacity} onChange={(v) => set("queueCapacity", v)}/></Field><Field label="Artifact prefix"><input value={config.prefix} onChange={(e) => set("prefix", e.target.value)}/></Field>{config.stagingProvider === "s3" && <><Field label="Bucket"><input value={config.bucket} onChange={(e) => set("bucket", e.target.value)}/></Field><Field label="Public base URL"><input placeholder="https://cdn.example.com" value={config.publicBaseUrl} onChange={(e) => set("publicBaseUrl", e.target.value)}/></Field><Field label="Endpoint environment variable"><input value={config.endpointEnv} onChange={(e) => set("endpointEnv", e.target.value)}/></Field></>}</div><details className="tool-paths"><summary>Media tool paths</summary><div className="field-grid"><Field label="FFmpeg"><input value={config.ffmpegPath} onChange={(e) => set("ffmpegPath", e.target.value)}/></Field><Field label="FFprobe"><input value={config.ffprobePath} onChange={(e) => set("ffprobePath", e.target.value)}/></Field><Field label="Streamlink"><input value={config.streamlinkPath} onChange={(e) => set("streamlinkPath", e.target.value)}/></Field><Field label="Chat downloader"><input value={config.chatDownloaderPath} onChange={(e) => set("chatDownloaderPath", e.target.value)}/></Field></div></details></Section>
          </div>
          <div className="run-bar"><div><span className="ready-dot"/><span><strong>Ready to run</strong><small>{sourceMode === "channel" ? `Watching twitch.tv/${sourceValue}` : "Analyzing one VOD"}</small></span></div><button className="run-button" disabled={starting || !sourceValid || invalidWindow}><Icon name="play" size={18}/>{starting ? "Starting…" : "Start clipping"}</button></div>
        </form>

        <aside className="preview-panel"><div className="preview-header"><div><span className="eyebrow">OUTPUT PREVIEW</span><h2>Editorial decision</h2></div><span className="preview-label">Sample</span></div><p className="preview-explainer">A representative decision using your current settings. Real run totals appear in history.</p><div className="preview-tabs" role="tablist"><button type="button" role="tab" aria-selected={previewTab === "approved"} className={previewTab === "approved" ? "active" : ""} onClick={() => setPreviewTab("approved")}><span className="mini-check"><Icon name="check" size={13}/></span>Approved</button><button type="button" role="tab" aria-selected={previewTab === "rejected"} className={previewTab === "rejected" ? "active rejected" : ""} onClick={() => setPreviewTab("rejected")}><span className="mini-x"><Icon name="x" size={13}/></span>Rejected</button></div>
          <article className={`decision-card ${previewTab}`}><div className="video-preview"><div className="video-noise"/><div className="stream-hud"><span>LIVE</span><span>12.4K</span></div><div className="game-scene"><span className="scene-glow one"/><span className="scene-glow two"/><span className="scene-horizon"/><span className="scene-avatar"/></div><div className="caption-line">{previewTab === "approved" ? "NO WAY—THAT ACTUALLY WORKED" : "okay, let me sort this out…"}</div><div className="play-disc"><Icon name="play" size={20}/></div><div className="video-time">{previewTab === "approved" ? "00:34" : "00:23"}</div></div><div className="decision-body"><div className="decision-status-row"><span className={`decision-status ${previewTab}`}><Icon name={previewTab === "approved" ? "check" : "x"} size={14}/>{previewTab === "approved" ? "APPROVED" : "REJECTED"}</span><span className="confidence">{Math.round(outputPreview.confidence * 100)}% confidence</span></div><h3>{outputPreview.title}</h3><p>{outputPreview.rationale}</p><div className="timeline"><span style={{ width: previewTab === "approved" ? "82%" : "41%" }}/><i style={{ left: previewTab === "approved" ? "72%" : "31%" }}/></div><div className="decision-meta"><span><Icon name="clock" size={14}/>{Math.round((outputPreview.endMs - outputPreview.startMs) / 1000)} seconds</span><span><Icon name="film" size={14}/>{outputPreview.layout ?? "No render"}</span></div></div></article>
          <div className="reasoning-block"><div className="reasoning-head"><span>Decision trace</span><span>{config.provider === "gemini" ? "Gemini 3.7 Flash" : "OpenAI stack"}</span></div>{previewTab === "approved" ? <ol><li className="passed"><span><Icon name="check" size={12}/></span><div><strong>Observer</strong><small>Reaction spike detected</small></div></li><li className="passed"><span><Icon name="check" size={12}/></span><div><strong>Director</strong><small>Clear setup and payoff</small></div></li><li className="passed"><span><Icon name="check" size={12}/></span><div><strong>Critic</strong><small>Strong standalone clip</small></div></li></ol> : <ol><li className="passed"><span><Icon name="check" size={12}/></span><div><strong>Observer</strong><small>Candidate sent for review</small></div></li><li className="failed"><span><Icon name="x" size={12}/></span><div><strong>Director</strong><small>No distinct narrative beat</small></div></li><li className="muted"><span>—</span><div><strong>Critic</strong><small>Not reached</small></div></li></ol>}</div>
          <div className="json-block"><div className="json-head"><span>Output format</span><button type="button" onClick={copyOutput}><Icon name={copied ? "check" : "copy"} size={14}/>{copied ? "Copied" : "Copy JSON"}</button></div><pre>{JSON.stringify(outputPreview, null, 2)}</pre></div>
        </aside>
      </div>}

      {nav === "jobs" && <section className="history-view"><div className="history-summary"><SummaryPill label="Runs" value={jobs.length}/><SummaryPill label="Approved" value={jobs.reduce((sum, job) => sum + (job.summary?.candidatesAccepted ?? job.progress.summary?.candidatesAccepted ?? 0), 0)} tone="good"/><SummaryPill label="Rejected" value={jobs.reduce((sum, job) => sum + (job.summary?.candidatesRejected ?? job.progress.summary?.candidatesRejected ?? 0), 0)} tone="bad"/></div><div className="history-card"><div className="history-card-head"><div><span className="eyebrow">RECENT ACTIVITY</span><h2>Clipping runs</h2></div><button className="primary-small" onClick={() => setNav("new")}><Icon name="plus" size={15}/>New run</button></div>{jobs.length ? <div className="job-list">{jobs.map((job) => <JobRow key={job.id} job={job} onCancel={cancelJob}/>)}</div> : <div className="empty-state"><span><Icon name="film" size={28}/></span><h3>No runs yet</h3><p>Start a live channel or VOD run to see progress and result totals here.</p><button onClick={() => setNav("new")}>Configure first run</button></div>}</div></section>}
      {nav === "presets" && <section className="presets-view"><div className="preset-card featured"><span className="preset-mark"><Icon name="spark"/></span><div><span className="eyebrow">CURRENT</span><h2>Balanced editorial</h2><p>12-second observation windows, automatic language detection, and a 20-second maturation delay. Tuned for general Twitch streams.</p><div className="preset-tags"><span>{config.provider === "gemini" ? "Gemini" : "OpenAI"}</span><span>{config.ringMinutes}m buffer</span><span>{config.dryRun ? "Safe mode" : "Live publish"}</span></div></div></div><div className="preset-card"><span className="preset-mark violet"><Icon name="radio"/></span><div><span className="eyebrow">SUGGESTED</span><h2>Fast reactions</h2><p>Short windows and faster maturation for high-energy gameplay and rapid chat spikes.</p><button onClick={() => { setConfig((c) => ({ ...c, observerWindowSeconds: 8, observerStepSeconds: 4, maturationDelaySeconds: 12, ringMinutes: 6 })); setNav("new"); }}>Use preset</button></div></div></section>}
    </main>
  </div>;
}

export default App;
