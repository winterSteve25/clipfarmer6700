import { type FormEvent, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Notice, Sidebar, Topbar } from "./components/AppChrome";
import { ConfigurationPanel } from "./components/ConfigurationPanel";
import { DecisionPreview } from "./components/DecisionPreview";
import { buildBackendConfig, buildOutputPreview, isSourceValid, isTauri, loadConfig } from "./config";
import type { JobSnapshot, NavItem, OpenSections, PreviewTab, PublisherAccount, PublisherCredentials, PublisherPlatform, SourceMode } from "./types";
import { HistoryView } from "./views/HistoryView";
import { PresetsView } from "./views/PresetsView";
import { SettingsView } from "./views/SettingsView";
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
  const [accounts, setAccounts] = useState<PublisherAccount[]>([]);
  const [accountsLoaded, setAccountsLoaded] = useState(false);
  const [busyPlatform, setBusyPlatform] = useState<PublisherPlatform | null>(null);
  const [connectionPrompt, setConnectionPrompt] = useState<PublisherPlatform | null>(null);

  useEffect(() => {
    localStorage.setItem("clipfarmer-config", JSON.stringify(config));
  }, [config]);

  useEffect(() => {
    if (!isTauri()) {
      setAccountsLoaded(true);
      setConfig((current) => ({ ...current, youtube: false, instagram: false, tiktokDrafts: false, twitchClips: false }));
      return;
    }

    invoke<JobSnapshot[]>("list_clipping_jobs")
      .then(setJobs)
      .catch((error) => setNotice(String(error)));

    invoke<PublisherAccount[]>("list_publisher_accounts")
      .then((nextAccounts) => {
        setAccounts(nextAccounts);
        const connected = (platform: PublisherPlatform) => nextAccounts.some((account) => account.platform === platform && account.connected);
        setConfig((current) => ({
          ...current,
          youtube: current.youtube && connected("youtube"),
          instagram: current.instagram && connected("instagram"),
          tiktokDrafts: current.tiktokDrafts && connected("tiktok"),
          twitchClips: current.twitchClips && connected("twitch"),
        }));
        setAccountsLoaded(true);
      })
      .catch((error) => {
        setAccountsLoaded(true);
        setNotice(String(error));
      });

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

  function isConnected(platform: PublisherPlatform) {
    return accounts.some((account) => account.platform === platform && account.connected);
  }

  function changePublisher(platform: PublisherPlatform, enabled: boolean) {
    const configKey = { youtube: "youtube", tiktok: "tiktokDrafts", instagram: "instagram", twitch: "twitchClips" }[platform] as "youtube" | "tiktokDrafts" | "instagram" | "twitchClips";
    if (enabled && accountsLoaded && !isConnected(platform)) {
      setConnectionPrompt(platform);
      return;
    }
    setConnectionPrompt(null);
    setConfig((current) => ({ ...current, [configKey]: enabled }));
  }

  function openSettings() {
    setConnectionPrompt(null);
    setNav("settings");
  }

  async function connectAccount(platform: PublisherPlatform, credentials: PublisherCredentials) {
    if (!isTauri()) {
      setNotice("Account connections are available in the ClipFarmer desktop app.");
      return false;
    }
    setBusyPlatform(platform);
    setNotice(null);
    try {
      const nextAccounts = await invoke<PublisherAccount[]>("connect_publisher_account", { platform, credentials });
      setAccounts(nextAccounts);
      return true;
    } catch (error) {
      setNotice(String(error));
      return false;
    } finally {
      setBusyPlatform(null);
    }
  }

  async function disconnectAccount(platform: PublisherPlatform) {
    setBusyPlatform(platform);
    setNotice(null);
    try {
      const nextAccounts = await invoke<PublisherAccount[]>("disconnect_publisher_account", { platform });
      setAccounts(nextAccounts);
      changePublisher(platform, false);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusyPlatform(null);
    }
  }

  return <div className="app-shell">
    <Sidebar nav={nav} jobsCount={jobs.length} onNavigate={setNav}/>
    <main className="workspace">
      <Topbar nav={nav} config={config} onOpenSettings={openSettings}/>
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
          accounts={accounts}
          accountsLoaded={accountsLoaded}
          connectionPrompt={connectionPrompt}
          onPublisherChange={changePublisher}
          onOpenSettings={openSettings}
          onDismissConnectionPrompt={() => setConnectionPrompt(null)}
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
      {nav === "settings" && (
        <SettingsView accounts={accounts} busyPlatform={busyPlatform} onConnect={connectAccount} onDisconnect={disconnectAccount}/>
      )}
    </main>
  </div>;
}

export default App;
