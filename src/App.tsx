import { type FormEvent, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Notice, Sidebar, Topbar } from "./components/AppChrome";
import { ConfigurationPanel } from "./components/ConfigurationPanel";
import { DecisionPreview } from "./components/DecisionPreview";
import { buildBackendConfig, buildOutputPreview, isSourceValid, isTauri, loadConfig } from "./config";
import type { JobSnapshot, NavItem, OpenSections, PreviewTab, SourceMode } from "./types";
import { HistoryView } from "./views/HistoryView";
import { PresetsView } from "./views/PresetsView";
import "./App.css";

const DEFAULT_OPEN_SECTIONS: OpenSections = {
  capture: true,
  ai: true,
  publishing: true,
  advanced: false,
};

function App() {
  const [nav, setNav] = useState<NavItem>("new");
  const [sourceMode, setSourceMode] = useState<SourceMode>("channel");
  const [sourceValue, setSourceValue] = useState("synthcity_live");
  const [config, setConfig] = useState(loadConfig);
  const [previewTab, setPreviewTab] = useState<PreviewTab>("approved");
  const [openSections, setOpenSections] = useState(DEFAULT_OPEN_SECTIONS);
  const [jobs, setJobs] = useState<JobSnapshot[]>([]);
  const [starting, setStarting] = useState(false);
  const [retryingJobId, setRetryingJobId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    localStorage.setItem("clipfarmer-config", JSON.stringify(config));
  }, [config]);

  useEffect(() => {
    if (!isTauri()) return;

    invoke<JobSnapshot[]>("list_clipping_jobs")
      .then(setJobs)
      .catch((error) => setNotice(String(error)));

    let dispose: (() => void) | undefined;
    listen<JobSnapshot>("clipfarmer-job-progress", (event) => {
      setJobs((current) => [
        event.payload,
        ...current.filter((job) => job.id !== event.payload.id),
      ]);
    }).then((unlisten) => { dispose = unlisten; });

    return () => dispose?.();
  }, []);

  const backendConfig = useMemo(() => buildBackendConfig(config), [config]);
  const outputPreview = useMemo(
    () => buildOutputPreview(previewTab, config),
    [previewTab, config],
  );
  const invalidWindow = config.observerWindowSeconds < config.observerStepSeconds;
  const sourceValid = isSourceValid(sourceMode, sourceValue);

  function changeSourceMode(mode: SourceMode) {
    setSourceMode(mode);
    setSourceValue(mode === "channel" ? "synthcity_live" : "https://www.twitch.tv/videos/");
  }

  function toggleSection(section: keyof OpenSections) {
    setOpenSections((current) => ({ ...current, [section]: !current[section] }));
  }

  async function startRun(event: FormEvent) {
    event.preventDefault();
    if (!sourceValid || invalidWindow) return;
    if (!isTauri()) {
      setNotice("Run controls connect when this interface is opened in the ClipFarmer desktop app.");
      return;
    }

    setStarting(true);
    setNotice(null);
    try {
      const command = sourceMode === "channel"
        ? "start_channel_clipping_job"
        : "start_vod_clipping_job";
      const args = sourceMode === "channel"
        ? { channel: sourceValue, config: backendConfig, deterministicModels: config.deterministicModels }
        : { vodUrl: sourceValue, config: backendConfig, deterministicModels: config.deterministicModels };
      const job = await invoke<JobSnapshot>(command, args);
      setJobs((current) => [job, ...current.filter((item) => item.id !== job.id)]);
      setNav("jobs");
    } catch (error) {
      setNotice(String(error));
    } finally {
      setStarting(false);
    }
  }

  async function cancelJob(id: string) {
    try {
      await invoke("cancel_clipping_job", { jobId: id });
    } catch (error) {
      setNotice(String(error));
    }
  }

  async function retryJob(id: string) {
    setRetryingJobId(id);
    setNotice(null);
    try {
      const job = await invoke<JobSnapshot>("retry_clipping_job", { jobId: id });
      setJobs((current) => [job, ...current.filter((item) => item.id !== job.id)]);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setRetryingJobId(null);
    }
  }

  async function copyOutput() {
    await navigator.clipboard.writeText(JSON.stringify(outputPreview, null, 2));
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  }

  return <div className="app-shell">
    <Sidebar nav={nav} jobsCount={jobs.length} onNavigate={setNav}/>
    <main className="workspace">
      <Topbar nav={nav} config={config}/>
      {notice && (
        <Notice message={notice} onDismiss={() => setNotice(null)}/>
      )}

      {nav === "new" && <div className="editor-layout">
        <ConfigurationPanel
          config={config}
          setConfig={setConfig}
          sourceMode={sourceMode}
          sourceValue={sourceValue}
          sourceValid={sourceValid}
          invalidWindow={invalidWindow}
          starting={starting}
          openSections={openSections}
          onSourceMode={changeSourceMode}
          onSourceValue={setSourceValue}
          onToggleSection={toggleSection}
          onSubmit={startRun}
        />
        <DecisionPreview
          config={config}
          output={outputPreview}
          tab={previewTab}
          copied={copied}
          onTab={setPreviewTab}
          onCopy={copyOutput}
        />
      </div>}

      {nav === "jobs" && (
        <HistoryView jobs={jobs} retryingJobId={retryingJobId} onCancel={cancelJob} onRetry={retryJob} onNewRun={() => setNav("new")}/>
      )}
      {nav === "presets" && (
        <PresetsView config={config} setConfig={setConfig} onUse={() => setNav("new")}/>
      )}
    </main>
  </div>;
}

export default App;
