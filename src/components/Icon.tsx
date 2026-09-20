import type { ReactNode } from "react";

export type IconName = "spark" | "plus" | "history" | "layers" | "settings" | "sliders" | "brain" | "send" | "play" | "check" | "x" | "clock" | "film" | "chevron" | "copy" | "radio" | "refresh";

const paths: Record<IconName, ReactNode> = {
  spark: <><path d="m12 3 1.2 3.8L17 8l-3.8 1.2L12 13l-1.2-3.8L7 8l3.8-1.2L12 3Z"/><path d="m5 13 .8 2.2L8 16l-2.2.8L5 19l-.8-2.2L2 16l2.2-.8L5 13Z"/></>,
  plus: <path d="M12 5v14M5 12h14"/>,
  history: <><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5M12 7v5l3 2"/></>,
  layers: <><path d="m12 3-9 5 9 5 9-5-9-5Z"/><path d="m3 12 9 5 9-5M3 16l9 5 9-5"/></>,
  settings: <><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3A1.7 1.7 0 0 0 10 3V2.8h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z"/></>,
  sliders: <><path d="M4 7h10M18 7h2M4 17h2M10 17h10M14 4v6M6 14v6"/></>,
  brain: <><path d="M9.5 4.5A3 3 0 0 0 4 6.2 3.5 3.5 0 0 0 3.5 13 3.5 3.5 0 0 0 7 18.5 3 3 0 0 0 12 17V7a2.5 2.5 0 0 0-2.5-2.5ZM14.5 4.5A3 3 0 0 1 20 6.2a3.5 3.5 0 0 1 .5 6.8 3.5 3.5 0 0 1-3.5 5.5 3 3 0 0 1-5-1.5V7a2.5 2.5 0 0 1 2.5-2.5Z"/><path d="M8 9h4M12 13h4"/></>,
  send: <><path d="m21 3-7.3 18-3.9-7-6.8-4L21 3Z"/><path d="m9.8 14 4-4"/></>,
  play: <path d="m8 5 11 7-11 7V5Z"/>, check: <path d="m5 12 4 4L19 6"/>, x: <path d="m6 6 12 12M18 6 6 18"/>,
  clock: <><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></>,
  film: <><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 4v16M17 4v16M3 9h4M17 9h4M3 15h4M17 15h4"/></>,
  chevron: <path d="m9 18 6-6-6-6"/>,
  copy: <><rect x="8" y="8" width="11" height="11" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/></>,
  radio: <><circle cx="12" cy="12" r="2"/><path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4M4.2 4.2a11 11 0 0 0 0 15.6M19.8 4.2a11 11 0 0 1 0 15.6"/></>,
  refresh: <><path d="M20 7v5h-5M4 17v-5h5"/><path d="M6.1 8A7 7 0 0 1 18 6l2 6M18 16a7 7 0 0 1-11.9 2L4 12"/></>,
};

export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  return <svg className="icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}
