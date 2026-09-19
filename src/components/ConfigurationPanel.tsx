import type { Dispatch, FormEvent, SetStateAction } from "react";
import { DEFAULT_CONFIG } from "../config";
import type { ConfigState, OpenSections, SourceMode } from "../types";
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
};

export function ConfigurationPanel(props: Props) {
  const { config, setConfig, sourceMode, sourceValue, sourceValid, invalidWindow, starting, openSections } = props;
  const set = <K extends keyof ConfigState>(key: K, value: ConfigState[K]) => setConfig((current) => ({ ...current, [key]: value }));
  const platformCount = [config.youtube, config.instagram, config.tiktokDrafts, config.twitchClips].filter(Boolean).length;

  return <form className="configuration-panel" onSubmit={props.onSubmit}>
    <div className="source-card">
      <div className="card-heading"><div><span className="eyebrow">01 · SOURCE</span><h2>Choose what to watch</h2></div><Icon name="radio" size={22}/></div>
      <div className="segmented"><button type="button" className={sourceMode === "channel" ? "active" : ""} onClick={() => props.onSourceMode("channel")}>Live channel</button><button type="button" className={sourceMode === "vod" ? "active" : ""} onClick={() => props.onSourceMode("vod")}>Twitch VOD</button></div>
      <label className="source-input"><span>{sourceMode === "channel" ? "twitch.tv/" : "URL"}</span><input value={sourceValue} onChange={(event) => props.onSourceValue(event.target.value)} aria-label={sourceMode === "channel" ? "Twitch channel" : "Twitch VOD URL"}/><span className={`validity ${sourceValid ? "valid" : ""}`}><Icon name={sourceValid ? "check" : "x"} size={15}/></span></label>
    </div>

    <div className="config-header"><div><span className="eyebrow">02 · CONFIGURATION</span><h2>Shape the editorial pipeline</h2></div><button type="button" className="reset-button" onClick={() => setConfig(DEFAULT_CONFIG)}><Icon name="refresh" size={15}/>Reset defaults</button></div>
    <div className="sections">
      <Section icon="sliders" title="Capture & timing" description={`${config.observerWindowSeconds}s window · ${config.ringMinutes}m buffer`} open={openSections.capture} onToggle={() => props.onToggleSection("capture")}>
        <div className="field-grid"><Field label="Observer window" hint="Context sent for each decision"><NumberInput value={config.observerWindowSeconds} onChange={(value) => set("observerWindowSeconds", value)} suffix="sec"/></Field><Field label="Observer step" hint="How often a window advances"><NumberInput value={config.observerStepSeconds} onChange={(value) => set("observerStepSeconds", value)} suffix="sec"/></Field><Field label="Maturation delay"><NumberInput value={config.maturationDelaySeconds} onChange={(value) => set("maturationDelaySeconds", value)} suffix="sec"/></Field><Field label="Rolling buffer"><NumberInput value={config.ringMinutes} onChange={(value) => set("ringMinutes", value)} suffix="min"/></Field><Field label="Live poll interval"><NumberInput value={config.pollSeconds} onChange={(value) => set("pollSeconds", value)} min={5} suffix="sec"/></Field><Field label="Visual sample interval"><NumberInput value={config.frameIntervalSeconds} onChange={(value) => set("frameIntervalSeconds", value)} suffix="sec"/></Field></div>
        {invalidWindow && <p className="validation-error">Observer window must be at least one observer step.</p>}
      </Section>

      <Section icon="brain" title="AI review" description={`${config.provider === "gemini" ? "Gemini 3.7 Flash" : "OpenAI editorial stack"} · ${config.scribbleModel === "large_turbo" ? "Large Turbo" : "Tiny"} Scribble`} open={openSections.ai} onToggle={() => props.onToggleSection("ai")}>
        <Field label="Editorial model provider"><div className="provider-choice"><button type="button" className={config.provider === "gemini" ? "selected" : ""} onClick={() => { set("provider", "gemini"); set("apiKeyEnv", "GEMINI_API_KEY"); }}><span>✦</span><span><strong>Gemini</strong><small>3.7 Flash across all roles</small></span><i/></button><button type="button" className={config.provider === "openai" ? "selected" : ""} onClick={() => { set("provider", "openai"); set("apiKeyEnv", "OPENAI_API_KEY"); }}><span>◎</span><span><strong>OpenAI</strong><small>Terra observer · Sol editorial</small></span><i/></button></div></Field>
        <Field label="Scribble transcription model"><div className="scribble-choice"><button type="button" className={config.scribbleModel === "large_turbo" ? "selected" : ""} onClick={() => set("scribbleModel", "large_turbo")}><span><strong>Large Turbo</strong><small>Best accuracy · higher memory use</small></span><i/></button><button type="button" className={config.scribbleModel === "tiny" ? "selected" : ""} onClick={() => set("scribbleModel", "tiny")}><span><strong>Tiny</strong><small>Fastest · lowest memory use</small></span><i/></button></div></Field>
        <div className="field-grid compact"><Field label="API key environment variable"><input value={config.apiKeyEnv} onChange={(event) => set("apiKeyEnv", event.target.value)}/></Field><Field label="Transcription language"><select value={config.language} onChange={(event) => set("language", event.target.value)}><option value="auto">Auto-detect</option><option value="en">English</option><option value="es">Spanish</option><option value="fr">French</option><option value="de">German</option><option value="ja">Japanese</option><option value="ko">Korean</option></select></Field><Field label="Transcription window"><NumberInput value={config.incrementalMinWindowSeconds} onChange={(value) => set("incrementalMinWindowSeconds", value)} suffix="sec"/></Field><div className="toggle-field"><div><strong>Voice activity detection</strong><small>Skip silent audio before transcription</small></div><Toggle label="Voice activity detection" checked={config.enableVad} onChange={(value) => set("enableVad", value)}/></div></div>
      </Section>

      <Section icon="send" title="Publishing" description={`${config.dryRun ? "Dry run" : "Live"} · ${platformCount} destination${platformCount === 1 ? "" : "s"}`} open={openSections.publishing} onToggle={() => props.onToggleSection("publishing")}>
        <div className="toggle-field prominent"><div><strong>Dry run</strong><small>Render outputs without posting to connected platforms</small></div><Toggle label="Dry run" checked={config.dryRun} onChange={(value) => set("dryRun", value)}/></div>
        <div className="platform-list">{([ ["youtube", "YouTube Shorts", "YT", config.youtube], ["instagram", "Instagram Reels", "IG", config.instagram], ["tiktokDrafts", "TikTok drafts", "TT", config.tiktokDrafts], ["twitchClips", "Twitch clips", "TW", config.twitchClips] ] as const).map(([key, label, monogram, checked]) => <div className="platform-row" key={key}><span className={`platform-icon ${key}`}>{monogram}</span><span>{label}</span><Toggle label={label} checked={checked} onChange={(value) => set(key, value)}/></div>)}</div>
      </Section>

      <Section icon="settings" title="Advanced" description={`${config.stagingProvider} staging · ${config.maxAttempts} attempts`} open={openSections.advanced} onToggle={() => props.onToggleSection("advanced")}>
        <div className="field-grid"><Field label="Staging provider"><select value={config.stagingProvider} onChange={(event) => set("stagingProvider", event.target.value as "local" | "s3")}><option value="local">Local</option><option value="s3">S3-compatible</option></select></Field><Field label="Max attempts"><NumberInput value={config.maxAttempts} onChange={(value) => set("maxAttempts", value)}/></Field><Field label="Queue capacity"><NumberInput value={config.queueCapacity} onChange={(value) => set("queueCapacity", value)}/></Field><Field label="Artifact prefix"><input value={config.prefix} onChange={(event) => set("prefix", event.target.value)}/></Field>{config.stagingProvider === "s3" && <><Field label="Bucket"><input value={config.bucket} onChange={(event) => set("bucket", event.target.value)}/></Field><Field label="Public base URL"><input placeholder="https://cdn.example.com" value={config.publicBaseUrl} onChange={(event) => set("publicBaseUrl", event.target.value)}/></Field><Field label="Endpoint environment variable"><input value={config.endpointEnv} onChange={(event) => set("endpointEnv", event.target.value)}/></Field></>}</div>
        <details className="tool-paths"><summary>Media tool paths</summary><div className="field-grid"><Field label="FFmpeg"><input value={config.ffmpegPath} onChange={(event) => set("ffmpegPath", event.target.value)}/></Field><Field label="FFprobe"><input value={config.ffprobePath} onChange={(event) => set("ffprobePath", event.target.value)}/></Field><Field label="Streamlink"><input value={config.streamlinkPath} onChange={(event) => set("streamlinkPath", event.target.value)}/></Field><Field label="Chat downloader"><input value={config.chatDownloaderPath} onChange={(event) => set("chatDownloaderPath", event.target.value)}/></Field></div></details>
      </Section>
    </div>
    <div className="run-bar"><div><span className="ready-dot"/><span><strong>Ready to run</strong><small>{sourceMode === "channel" ? `Watching twitch.tv/${sourceValue}` : "Analyzing one VOD"}</small></span></div><button className="run-button" disabled={starting || !sourceValid || invalidWindow}><Icon name="play" size={18}/>{starting ? "Starting…" : "Start clipping"}</button></div>
  </form>;
}
