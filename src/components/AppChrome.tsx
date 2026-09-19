import type { ConfigState, NavItem } from "../types";
import { Icon } from "./Icon";

export function Sidebar({ nav, jobsCount, onNavigate }: { nav: NavItem; jobsCount: number; onNavigate: (item: NavItem) => void }) {
  return <aside className="sidebar">
    <div className="brand"><span className="brand-mark"><Icon name="spark" size={22}/></span><span><strong>ClipFarmer</strong><small>Editorial console</small></span></div>
    <nav aria-label="Main navigation">
      <button className={nav === "new" ? "active" : ""} onClick={() => onNavigate("new")}><Icon name="plus"/><span>New run</span></button>
      <button className={nav === "jobs" ? "active" : ""} onClick={() => onNavigate("jobs")}><Icon name="history"/><span>Run history</span>{jobsCount > 0 && <em>{jobsCount}</em>}</button>
      <button className={nav === "presets" ? "active" : ""} onClick={() => onNavigate("presets")}><Icon name="layers"/><span>Presets</span></button>
    </nav>
    <div className="sidebar-spacer"/>
    <button className="sidebar-settings"><Icon name="settings"/><span>Settings</span></button>
  </aside>;
}

const pageCopy: Record<NavItem, { crumb: string; title: string }> = {
  new: { crumb: "NEW RUN", title: "Configure a clipping run" },
  jobs: { crumb: "RUN HISTORY", title: "Run history" },
  presets: { crumb: "PRESETS", title: "Configuration presets" },
};

export function Topbar({ nav, config }: { nav: NavItem; config: ConfigState }) {
  const copy = pageCopy[nav];
  return <header className="topbar"><div><p>WORKSPACE / {copy.crumb}</p><h1>{copy.title}</h1></div><div className="topbar-actions"><span className="mode-badge"><span/>{config.dryRun ? "Safe mode" : "Publishing live"}</span><button className="icon-button" title="Settings"><Icon name="settings"/></button></div></header>;
}

export function Notice({ message, onDismiss }: { message: string; onDismiss: () => void }) {
  return <div className="notice"><span>{message}</span><button aria-label="Dismiss" onClick={onDismiss}><Icon name="x" size={16}/></button></div>;
}
