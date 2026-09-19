import type { Dispatch, SetStateAction } from "react";
import type { ConfigState } from "../types";
import { Icon } from "../components/Icon";

export function PresetsView({ config, setConfig, onUse }: { config: ConfigState; setConfig: Dispatch<SetStateAction<ConfigState>>; onUse: () => void }) {
  function useFastReactions() {
    setConfig((current) => ({ ...current, observerWindowSeconds: 8, observerStepSeconds: 4, maturationDelaySeconds: 12, ringMinutes: 6 }));
    onUse();
  }

  return <section className="presets-view">
    <div className="preset-card featured"><span className="preset-mark"><Icon name="spark"/></span><div><span className="eyebrow">CURRENT</span><h2>Balanced editorial</h2><p>12-second observation windows, automatic language detection, and a 20-second maturation delay. Tuned for general Twitch streams.</p><div className="preset-tags"><span>{config.provider === "gemini" ? "Gemini" : "OpenAI"}</span><span>{config.ringMinutes}m buffer</span><span>{config.dryRun ? "Safe mode" : "Live publish"}</span></div></div></div>
    <div className="preset-card"><span className="preset-mark violet"><Icon name="radio"/></span><div><span className="eyebrow">SUGGESTED</span><h2>Fast reactions</h2><p>Short windows and faster maturation for high-energy gameplay and rapid chat spikes.</p><button onClick={useFastReactions}>Use preset</button></div></div>
  </section>;
}
