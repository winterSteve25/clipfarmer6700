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
    <div className="preview-header"><h2>Example output</h2></div>
    <p className="preview-explainer">Preview how the current workflow evaluates a candidate.</p>
    <div className="preview-tabs" role="tablist"><button type="button" role="tab" aria-selected={approved} className={approved ? "active" : ""} onClick={() => onTab("approved")}>Approved</button><button type="button" role="tab" aria-selected={!approved} className={!approved ? "active rejected" : ""} onClick={() => onTab("rejected")}>Rejected</button></div>

    <article className={`decision-card ${tab}`}>
      <div className="decision-body">
        <div className="decision-status-row"><span className={`decision-status ${tab}`}><Icon name={approved ? "check" : "x"} size={14}/>{approved ? "Approved" : "Rejected"}</span><span className="candidate-id">{output.candidateId}</span></div>
        <div className="decision-hero"><div><h3>{output.title}</h3><p>{output.rationale}</p></div><div className="decision-score" aria-label={`${Math.round(output.confidence * 100)} percent confidence`}><strong>{Math.round(output.confidence * 100)}</strong><span>% confidence</span></div></div>
        {output.hookText && <blockquote>{output.hookText}</blockquote>}
        <dl className="decision-facts"><div><dt>Duration</dt><dd>{Math.round((output.endMs - output.startMs) / 1000)} seconds</dd></div><div><dt>Layout</dt><dd>{output.layout ?? "No render"}</dd></div></dl>
      </div>
    </article>

    <div className="reasoning-block"><div className="reasoning-head"><span>Review stages</span><span>{config.provider === "gemini" ? "Gemini 3.7 Flash" : "OpenAI stack"}</span></div>{approved ? <ol><Trace state="passed" label="Observer" detail="Reaction spike detected"/><Trace state="passed" label="Director" detail="Clear setup and payoff"/><Trace state="passed" label="Critic" detail="Strong standalone clip"/></ol> : <ol><Trace state="passed" label="Observer" detail="Candidate sent for review"/><Trace state="failed" label="Director" detail="No distinct narrative beat"/><Trace state="muted" label="Critic" detail="Not reached"/></ol>}</div>
    <div className="json-block"><div className="json-head"><span>JSON</span><button type="button" onClick={onCopy}><Icon name={copied ? "check" : "copy"} size={14}/>{copied ? "Copied" : "Copy"}</button></div><pre>{JSON.stringify(output, null, 2)}</pre></div>
  </aside>;
}

function Trace({ state, label, detail }: { state: "passed" | "failed" | "muted"; label: string; detail: string }) {
  return <li className={state}><span>{state === "muted" ? "-" : <Icon name={state === "passed" ? "check" : "x"} size={12}/>}</span><div><strong>{label}</strong><small>{detail}</small></div></li>;
}
