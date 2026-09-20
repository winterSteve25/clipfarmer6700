import type { ConfigState, NavItem } from "../types";
import { Icon, Logomark } from "./Icon";

export function Sidebar({ nav, jobsCount, onNavigate }: { nav: NavItem; jobsCount: number; onNavigate: (item: NavItem) => void }) {
  return <aside className="sidebar">
    <div className="brand"><span className="brand-mark"><Logomark size={17}/></span><strong><span>Clip</span>Farmer</strong></div>
    <nav aria-label="Main navigation">
      <button aria-current={nav === "new" ? "page" : undefined} className={nav === "new" ? "active" : ""} onClick={() => onNavigate("new")}><Icon name="plus"/><span>New run</span></button>
      <button aria-current={nav === "jobs" ? "page" : undefined} className={nav === "jobs" ? "active" : ""} onClick={() => onNavigate("jobs")}><Icon name="history"/><span>Run history</span>{jobsCount > 0 && <em>{jobsCount}</em>}</button>
      <button aria-current={nav === "presets" ? "page" : undefined} className={nav === "presets" ? "active" : ""} onClick={() => onNavigate("presets")}><Icon name="layers"/><span>Presets</span></button>
    </nav>
    <div className="sidebar-spacer"/>
    <button aria-current={nav === "settings" ? "page" : undefined} className={`sidebar-settings ${nav === "settings" ? "active" : ""}`} onClick={() => onNavigate("settings")}><Icon name="settings"/><span>Settings</span></button>
  </aside>;
}

const pageCopy: Record<NavItem, { crumb: string; title: string }> = {
  new: { crumb: "New run", title: "Configure a clipping run" },
  jobs: { crumb: "Activity", title: "Run history" },
  presets: { crumb: "Workflow", title: "Configuration presets" },
  settings: { crumb: "Publishing", title: "Account connections" },
};

export function Topbar({ nav, config, onOpenSettings }: { nav: NavItem; config: ConfigState; onOpenSettings: () => void }) {
  const copy = pageCopy[nav];
  return <header className="topbar"><div><span className="page-context">{copy.crumb}</span><h1>{copy.title}</h1></div><div className="topbar-actions"><span className={`mode-badge ${config.dryRun ? "safe" : "live"}`}><Icon name={config.dryRun ? "check" : "radio"} size={14}/>{config.dryRun ? "Safe mode" : "Publishing live"}</span><button className={`icon-button ${nav === "settings" ? "active" : ""}`} aria-label="Open settings" title="Settings" onClick={onOpenSettings}><Icon name="settings"/></button></div></header>;
}

export function Notice({ message, onDismiss }: { message: string; onDismiss: () => void }) {
  return <div className="notice"><span>{message}</span><button aria-label="Dismiss" onClick={onDismiss}><Icon name="x" size={16}/></button></div>;
}
