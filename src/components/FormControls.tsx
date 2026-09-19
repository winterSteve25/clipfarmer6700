import type { ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (value: boolean) => void; label: string }) {
  return <button type="button" role="switch" aria-checked={checked} aria-label={label} className={`toggle ${checked ? "on" : ""}`} onClick={() => onChange(!checked)}><span/></button>;
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return <label className="field"><span className="field-label">{label}</span>{children}{hint && <small>{hint}</small>}</label>;
}

export function NumberInput({ value, onChange, min = 1, max, suffix }: { value: number; onChange: (value: number) => void; min?: number; max?: number; suffix?: string }) {
  return <div className="number-input"><input type="number" value={value} min={min} max={max} onChange={(event) => onChange(Number(event.target.value))}/>{suffix && <span>{suffix}</span>}</div>;
}

export function Section({ icon, title, description, open, onToggle, children }: { icon: IconName; title: string; description: string; open: boolean; onToggle: () => void; children: ReactNode }) {
  return <section className={`config-section ${open ? "open" : ""}`}><button className="section-head" type="button" onClick={onToggle} aria-expanded={open}><span className="section-icon"><Icon name={icon}/></span><span><strong>{title}</strong><small>{description}</small></span><span className="section-chevron"><Icon name="chevron" size={16}/></span></button>{open && <div className="section-content">{children}</div>}</section>;
}
