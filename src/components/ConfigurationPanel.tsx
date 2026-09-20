import type { Dispatch, FormEvent, SetStateAction } from "react";
import { DEFAULT_CONFIG } from "../config";
import type { ConfigState, OpenSections, PublisherAccount, PublisherPlatform, SourceMode } from "../types";
import { Field, NumberInput, Section, Toggle } from "./FormControls";
import { Icon } from "./Icon";

type Props = {
  config: ConfigState;
  setConfig: Dispatch<SetStateAction<ConfigState>>;
  sourceMode: SourceMode;
  sourceValue: string;
  sourceValid: boolean;
  invalidWindow: boolean;
  starting: boolean;
  openSections: OpenSections;
  onSourceMode: (mode: SourceMode) => void;
  onSourceValue: (value: string) => void;
  onToggleSection: (section: keyof OpenSections) => void;
  onSubmit: (event: FormEvent) => void;
  accounts: PublisherAccount[];
  accountsLoaded: boolean;
  connectionPrompt: PublisherPlatform | null;
  onPublisherChange: (platform: PublisherPlatform, enabled: boolean) => void;
  onOpenSettings: () => void;
  onDismissConnectionPrompt: () => void;
};

const PUBLISHERS: Array<{ platform: PublisherPlatform; key: "youtube" | "instagram" | "tiktokDrafts" | "twitchClips"; label: string; monogram: string }> = [
  { platform: "youtube", key: "youtube", label: "YouTube Shorts", monogram: "YT" },
  { platform: "instagram", key: "instagram", label: "Instagram Reels", monogram: "IG" },
  { platform: "tiktok", key: "tiktokDrafts", label: "TikTok drafts", monogram: "TT" },
  { platform: "twitch", key: "twitchClips", label: "Twitch clips", monogram: "TW" },
];

const PLATFORM_NAMES: Record<PublisherPlatform, string> = {
  youtube: "YouTube",
  tiktok: "TikTok",
  instagram: "Instagram",
  twitch: "Twitch",
};

export function ConfigurationPanel(props: Props) {
  const { config, setConfig, sourceMode, sourceValue, sourceValid, invalidWindow, starting, openSections } = props;
  const set = <K extends keyof ConfigState>(key: K, value: ConfigState[K]) => setConfig((current) => ({ ...current, [key]: value }));
  const platformCount = [config.youtube, config.instagram, config.tiktokDrafts, config.twitchClips].filter(Boolean).length;

  return <form className="configuration-panel" onSubmit={props.onSubmit}>
    <div className="source-card">
      <div className="card-heading"><h2>Source</h2></div>
      <div className="segmented"><button type="button" className={sourceMode === "channel" ? "active" : ""} onClick={() => props.onSourceMode("channel")}>Live channel</button><button type="button" className={sourceMode === "vod" ? "active" : ""} onClick={() => props.onSourceMode("vod")}>Twitch VOD</button></div>
      <label className="source-input"><span>{sourceMode === "channel" ? "twitch.tv/" : "URL"}</span><input value={sourceValue} onChange={(event) => props.onSourceValue(event.target.value)} aria-label={sourceMode === "channel" ? "Twitch channel" : "Twitch VOD URL"}/><span className={`validity ${sourceValid ? "valid" : ""}`}><Icon name={sourceValid ? "check" : "x"} size={15}/></span></label>
    </div>

    <div className="config-header"><h2>Workflow</h2><button type="button" className="reset-button" onClick={() => setConfig({ ...DEFAULT_CONFIG, youtube: props.accounts.some((account) => account.platform === "youtube" && account.connected) })}><Icon name="refresh" size={15}/>Reset defaults</button></div>
    <div className="sections">
      <Section icon="sliders" title="Capture and timing" description={`${config.observerWindowSeconds}s window, ${config.ringMinutes}m buffer`} open={openSections.capture} onToggle={() => props.onToggleSection("capture")}>
        <div className="field-grid"><Field label="Observer window" hint="Context sent for each decision"><NumberInput value={config.observerWindowSeconds} onChange={(value) => set("observerWindowSeconds", value)} suffix="sec"/></Field><Field label="Observer step" hint="How often a window advances"><NumberInput value={config.observerStepSeconds} onChange={(value) => set("observerStepSeconds", value)} suffix="sec"/></Field><Field label="Maturation delay"><NumberInput value={config.maturationDelaySeconds} onChange={(value) => set("maturationDelaySeconds", value)} suffix="sec"/></Field><Field label="Rolling buffer"><NumberInput value={config.ringMinutes} onChange={(value) => set("ringMinutes", value)} suffix="min"/></Field><Field label="Live poll interval"><NumberInput value={config.pollSeconds} onChange={(value) => set("pollSeconds", value)} min={5} suffix="sec"/></Field><Field label="Visual sample interval"><NumberInput value={config.frameIntervalSeconds} onChange={(value) => set("frameIntervalSeconds", value)} min={2} suffix="sec"/></Field></div>
        {invalidWindow && <p className="validation-error">Observer window must be at least one observer step.</p>}
      </Section>

      <Section icon="brain" title="AI review" description={`${config.deterministicModels ? "Deterministic offline models" : config.provider === "gemini" ? "Gemini 3.7 Flash" : "OpenAI editorial stack"}, ${config.scribbleModel === "large_turbo" ? "Large Turbo" : "Tiny"} Scribble`} open={openSections.ai} onToggle={() => props.onToggleSection("ai")}>
        <div className="toggle-field prominent"><div><strong>Deterministic models</strong><small>Replace hosted editorial and audio calls with offline test implementations</small></div><Toggle label="Deterministic models" checked={config.deterministicModels} onChange={(value) => set("deterministicModels", value)}/></div>
        <Field label="Editorial model provider"><div className="provider-choice"><button type="button" disabled={config.deterministicModels} className={config.provider === "gemini" ? "selected" : ""} onClick={() => { set("provider", "gemini"); set("apiKeyEnv", "GEMINI_API_KEY"); }}><span className="provider-glyph">G</span><span><strong>Gemini</strong><small>3.7 Flash across all roles</small></span><i/></button><button type="button" disabled={config.deterministicModels} className={config.provider === "openai" ? "selected" : ""} onClick={() => { set("provider", "openai"); set("apiKeyEnv", "OPENAI_API_KEY"); }}><span className="provider-glyph">O</span><span><strong>OpenAI</strong><small>Terra editor, Sol critic</small></span><i/></button></div></Field>
        <Field label="Scribble transcription model"><div className="scribble-choice"><button type="button" className={config.scribbleModel === "large_turbo" ? "selected" : ""} onClick={() => set("scribbleModel", "large_turbo")}><span><strong>Large Turbo</strong><small>Best accuracy, higher memory use</small></span><i/></button><button type="button" className={config.scribbleModel === "tiny" ? "selected" : ""} onClick={() => set("scribbleModel", "tiny")}><span><strong>Tiny</strong><small>Fastest, lowest memory use</small></span><i/></button></div></Field>
        <div className="field-grid compact"><Field label="API key environment variable"><input disabled={config.deterministicModels} value={config.apiKeyEnv} onChange={(event) => set("apiKeyEnv", event.target.value)}/></Field><Field label="Transcription language"><select value={config.language} onChange={(event) => set("language", event.target.value)}><option value="auto">Auto-detect</option><option value="en">English</option><option value="es">Spanish</option><option value="fr">French</option><option value="de">German</option><option value="ja">Japanese</option><option value="ko">Korean</option></select></Field><Field label="Transcription window"><NumberInput value={config.incrementalMinWindowSeconds} onChange={(value) => set("incrementalMinWindowSeconds", value)} suffix="sec"/></Field><div className="toggle-field"><div><strong>Voice activity detection</strong><small>Skip silent audio before transcription</small></div><Toggle label="Voice activity detection" checked={config.enableVad} onChange={(value) => set("enableVad", value)}/></div></div>
      </Section>

      <Section icon="send" title="Publishing" description={`${config.dryRun ? "Dry run" : "Live"}, ${platformCount} destination${platformCount === 1 ? "" : "s"}`} open={openSections.publishing} onToggle={() => props.onToggleSection("publishing")}>
        <div className="toggle-field prominent"><div><strong>Dry run</strong><small>Render outputs without posting to connected platforms</small></div><Toggle label="Dry run" checked={config.dryRun} onChange={(value) => set("dryRun", value)}/></div>
        {props.connectionPrompt && <div className="connection-prompt" role="alert"><span className="connection-prompt-icon"><Icon name="settings" size={17}/></span><div><strong>Connect {PLATFORM_NAMES[props.connectionPrompt]} first</strong><p>Publishing is still off for this destination. Connect your account in Settings, then try again.</p><button type="button" onClick={props.onOpenSettings}>Go to Settings <Icon name="chevron" size={13}/></button></div><button type="button" className="connection-prompt-close" aria-label="Dismiss account connection prompt" onClick={props.onDismissConnectionPrompt}><Icon name="x" size={15}/></button></div>}
        <div className="platform-list">{PUBLISHERS.map(({ platform, key, label, monogram }) => {
          const connected = props.accounts.some((account) => account.platform === platform && account.connected);
          return <div className="platform-row" key={key}><span className={`platform-icon ${key}`}>{monogram}</span><span className="platform-name">{label}<small>{!props.accountsLoaded ? "Checking account…" : connected ? "Connected" : "Account required"}</small></span><Toggle label={`${label} publishing`} checked={config[key]} onChange={(value) => props.onPublisherChange(platform, value)}/></div>;
        })}</div>
      </Section>

      <Section icon="settings" title="Advanced" description={`${config.stagingProvider} staging, ${config.maxAttempts} attempts`} open={openSections.advanced} onToggle={() => props.onToggleSection("advanced")}>
        <div className="field-grid"><Field label="Staging provider"><select value={config.stagingProvider} onChange={(event) => set("stagingProvider", event.target.value as "local" | "s3")}><option value="local">Local</option><option value="s3">S3-compatible</option></select></Field><Field label="Max attempts"><NumberInput value={config.maxAttempts} onChange={(value) => set("maxAttempts", value)}/></Field><Field label="Queue capacity"><NumberInput value={config.queueCapacity} onChange={(value) => set("queueCapacity", value)}/></Field><Field label="Artifact prefix"><input value={config.prefix} onChange={(event) => set("prefix", event.target.value)}/></Field>{config.stagingProvider === "s3" && <><Field label="Bucket"><input value={config.bucket} onChange={(event) => set("bucket", event.target.value)}/></Field><Field label="Public base URL"><input placeholder="https://cdn.example.com" value={config.publicBaseUrl} onChange={(event) => set("publicBaseUrl", event.target.value)}/></Field><Field label="Endpoint environment variable"><input value={config.endpointEnv} onChange={(event) => set("endpointEnv", event.target.value)}/></Field></>}</div>
        <details className="tool-paths"><summary>Media tool paths</summary><div className="field-grid"><Field label="FFmpeg"><input value={config.ffmpegPath} onChange={(event) => set("ffmpegPath", event.target.value)}/></Field><Field label="FFprobe"><input value={config.ffprobePath} onChange={(event) => set("ffprobePath", event.target.value)}/></Field><Field label="Streamlink"><input value={config.streamlinkPath} onChange={(event) => set("streamlinkPath", event.target.value)}/></Field><Field label="Chat downloader"><input value={config.chatDownloaderPath} onChange={(event) => set("chatDownloaderPath", event.target.value)}/></Field></div></details>
      </Section>
    </div>
    <div className="run-bar"><div><Icon name="check" size={15}/><span><strong>Ready</strong><small>{sourceMode === "channel" ? `twitch.tv/${sourceValue}` : "One VOD"}</small></span></div><button className="run-button" disabled={starting || !sourceValid || invalidWindow}><Icon name="play" size={17}/>{starting ? "Starting…" : "Start clipping"}</button></div>
  </form>;
}
