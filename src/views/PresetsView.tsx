import type { Dispatch, SetStateAction } from "react";
import type { ConfigState } from "../types";
import { Icon } from "../components/Icon";

export function PresetsView({ config, setConfig, onUse }: { config: ConfigState; setConfig: Dispatch<SetStateAction<ConfigState>>; onUse: () => void }) {
  function useFastReactions() {
    setConfig((current) => ({ ...current, observerWindowSeconds: 8, observerStepSeconds: 4, maturationDelaySeconds: 12, ringMinutes: 6 }));
    onUse();
  }

  return <section className="presets-view-wrap">
    <div className="settings-intro">
      <h2>Configuration presets</h2>
      <p>Presets are starting points for the workflow config, not locked-in settings. Apply one, then keep tuning any field on the New run page.</p>
    </div>
    <div className="presets-view">
      <div className="preset-card featured"><span className="preset-mark"><Icon name="spark"/></span><div><span className="preset-state">Current</span><h2>Balanced editorial</h2><p>12-second observation windows, automatic language detection, and a 20-second maturation delay. Tuned for general Twitch streams.</p><div className="preset-tags"><span>{config.deterministicModels ? "Deterministic" : config.provider === "gemini" ? "Gemini" : "OpenAI"}</span><span>{config.ringMinutes}m buffer</span><span>{config.dryRun ? "Safe mode" : "Live publish"}</span></div></div></div>
      <div className="preset-card"><span className="preset-mark"><Icon name="radio"/></span><div><span className="preset-state">Suggested</span><h2>Fast reactions</h2><p>Short windows and faster maturation for high-energy gameplay and rapid chat spikes.</p><div className="preset-tags"><span>8s window</span><span>6m buffer</span><span>12s maturation</span></div><button onClick={useFastReactions}>Use preset</button></div></div>
      <div className="preset-card ghost"><span className="preset-mark"><Icon name="plus"/></span><div><span className="preset-state">Coming soon</span><h2>Save your own preset</h2><p>Capture the workflow config from any run and reuse it here as a one-click starting point.</p></div></div>
    </div>
  </section>;
}
