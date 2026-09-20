import type { ConfigState, DecisionOutput, PreviewTab } from "../types";
import { Icon } from "./Icon";

type Props = {
  config: ConfigState;
  output: DecisionOutput;
  tab: PreviewTab;
  copied: boolean;
  onTab: (tab: PreviewTab) => void;
  onCopy: () => void;
};

export function DecisionPreview({ config, output, tab, copied, onTab, onCopy }: Props) {
  const approved = tab === "approved";
  return <aside className="preview-panel">
    <div className="preview-header"><div><span className="eyebrow">OUTPUT PREVIEW</span><h2>Editorial decision</h2></div><span className="preview-label">Sample</span></div>
    <p className="preview-explainer">A representative decision using your current settings. Real run totals appear in history.</p>
    <div className="preview-tabs" role="tablist"><button type="button" role="tab" aria-selected={approved} className={approved ? "active" : ""} onClick={() => onTab("approved")}><span className="mini-check"><Icon name="check" size={13}/></span>Approved</button><button type="button" role="tab" aria-selected={!approved} className={!approved ? "active rejected" : ""} onClick={() => onTab("rejected")}><span className="mini-x"><Icon name="x" size={13}/></span>Rejected</button></div>

    <article className={`decision-card ${tab}`}>
      <div className="video-preview"><div className="video-noise"/><div className="stream-hud"><span>LIVE</span><span>12.4K</span></div><div className="game-scene"><span className="scene-glow one"/><span className="scene-glow two"/><span className="scene-horizon"/><span className="scene-avatar"/></div><div className="caption-line">{approved ? "NO WAY—THAT ACTUALLY WORKED" : "okay, let me sort this out…"}</div><div className="play-disc"><Icon name="play" size={20}/></div><div className="video-time">{approved ? "00:34" : "00:23"}</div></div>
      <div className="decision-body"><div className="decision-status-row"><span className={`decision-status ${tab}`}><Icon name={approved ? "check" : "x"} size={14}/>{approved ? "APPROVED" : "REJECTED"}</span><span className="confidence">{Math.round(output.confidence * 100)}% confidence</span></div><h3>{output.title}</h3><p>{output.rationale}</p><div className="timeline"><span style={{ width: approved ? "82%" : "41%" }}/><i style={{ left: approved ? "72%" : "31%" }}/></div><div className="decision-meta"><span><Icon name="clock" size={14}/>{Math.round((output.endMs - output.startMs) / 1000)} seconds</span><span><Icon name="film" size={14}/>{output.layout ?? "No render"}</span></div></div>
    </article>

    <div className="reasoning-block"><div className="reasoning-head"><span>Decision trace</span><span>{config.provider === "gemini" ? "Gemini 3.7 Flash" : "OpenAI stack"}</span></div>{approved ? <ol><Trace state="passed" label="Observer" detail="Reaction spike detected"/><Trace state="passed" label="Director" detail="Clear setup and payoff"/><Trace state="passed" label="Critic" detail="Strong standalone clip"/></ol> : <ol><Trace state="passed" label="Observer" detail="Candidate sent for review"/><Trace state="failed" label="Director" detail="No distinct narrative beat"/><Trace state="muted" label="Critic" detail="Not reached"/></ol>}</div>
    <div className="json-block"><div className="json-head"><span>Output format</span><button type="button" onClick={onCopy}><Icon name={copied ? "check" : "copy"} size={14}/>{copied ? "Copied" : "Copy JSON"}</button></div><pre>{JSON.stringify(output, null, 2)}</pre></div>
  </aside>;
}

function Trace({ state, label, detail }: { state: "passed" | "failed" | "muted"; label: string; detail: string }) {
  return <li className={state}><span>{state === "muted" ? "—" : <Icon name={state === "passed" ? "check" : "x"} size={12}/>}</span><div><strong>{label}</strong><small>{detail}</small></div></li>;
}
