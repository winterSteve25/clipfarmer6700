import type { Icon as PhosphorIcon } from "@phosphor-icons/react";
import {
  ArrowCounterClockwiseIcon,
  BroadcastIcon,
  BrainIcon,
  CaretRightIcon,
  CheckIcon,
  ClockCounterClockwiseIcon,
  ClockIcon,
  CopyIcon,
  FilmStripIcon,
  GearSixIcon,
  PaperPlaneTiltIcon,
  PlayIcon,
  PlusIcon,
  SlidersHorizontalIcon,
  SparkleIcon,
  StackIcon,
  XIcon,
} from "@phosphor-icons/react";

export type IconName = "spark" | "plus" | "history" | "layers" | "settings" | "sliders" | "brain" | "send" | "play" | "check" | "x" | "clock" | "film" | "chevron" | "copy" | "radio" | "refresh";

const icons: Record<IconName, PhosphorIcon> = {
  spark: SparkleIcon,
  plus: PlusIcon,
  history: ClockCounterClockwiseIcon,
  layers: StackIcon,
  settings: GearSixIcon,
  sliders: SlidersHorizontalIcon,
  brain: BrainIcon,
  send: PaperPlaneTiltIcon,
  play: PlayIcon,
  check: CheckIcon,
  x: XIcon,
  clock: ClockIcon,
  film: FilmStripIcon,
  chevron: CaretRightIcon,
  copy: CopyIcon,
  radio: BroadcastIcon,
  refresh: ArrowCounterClockwiseIcon,
};

export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  const Glyph = icons[name];
  return <Glyph className="icon" size={size} weight="regular" aria-hidden="true"/>;
}

export function Logomark({ size = 18 }: { size?: number }) {
  return <svg className="icon" width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true">
    <path d="M4.5 8.5V6a1.5 1.5 0 0 1 1.5-1.5h2.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"/>
    <path d="M19.5 8.5V6A1.5 1.5 0 0 0 18 4.5h-2.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"/>
    <path d="M4.5 15.5V18A1.5 1.5 0 0 0 6 19.5h2.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"/>
    <path d="M19.5 15.5V18a1.5 1.5 0 0 1-1.5 1.5h-2.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"/>
    <circle cx="12" cy="12" r="2.25" fill="currentColor"/>
  </svg>;
}
