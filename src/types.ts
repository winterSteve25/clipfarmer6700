export type SourceMode = "channel" | "vod";
export type Provider = "gemini" | "openai";
export type ScribbleModel = "large_turbo" | "tiny";
export type PreviewTab = "approved" | "rejected";
export type NavItem = "new" | "jobs" | "presets";

export type RunSummary = {
  windowsObserved: number;
  candidatesReviewed: number;
  candidatesAccepted: number;
  candidatesRejected: number;
  postsCompleted: number;
  publishFailures: number;
};

export type JobProgress = {
  phase: string;
  message: string;
  elapsedMs: number;
  capturedMs: number | null;
  completedUnits?: number | null;
  totalUnits?: number | null;
  transferredBytes?: number | null;
  summary: RunSummary | null;
};

export type JobSnapshot = {
  id: string;
  source: { type: SourceMode; channel?: string; url?: string };
  channel: string | null;
  outputDir: string;
  status: "queued" | "running" | "cancelling" | "completed" | "cancelled" | "failed";
  progress: JobProgress;
  history?: JobProgress[];
  summary: RunSummary | null;
  error: string | null;
  createdAtMs: number;
  finishedAtMs: number | null;
};

export type ConfigState = {
  deterministicModels: boolean;
  provider: Provider;
  apiKeyEnv: string;
  pollSeconds: number;
  maxAttempts: number;
  ringMinutes: number;
  observerWindowSeconds: number;
  observerStepSeconds: number;
  maturationDelaySeconds: number;
  queueCapacity: number;
  frameIntervalSeconds: number;
  scribbleModel: ScribbleModel;
  enableVad: boolean;
  language: string;
  incrementalMinWindowSeconds: number;
  dryRun: boolean;
  youtube: boolean;
  instagram: boolean;
  tiktokDrafts: boolean;
  twitchClips: boolean;
  stagingProvider: "local" | "s3";
  bucket: string;
  prefix: string;
  publicBaseUrl: string;
  endpointEnv: string;
  ffmpegPath: string;
  ffprobePath: string;
  streamlinkPath: string;
  chatDownloaderPath: string;
};

export type OpenSections = {
  capture: boolean;
  ai: boolean;
  publishing: boolean;
  advanced: boolean;
};

export type DecisionOutput = {
  candidateId: string;
  status: string;
  confidence: number;
  title: string;
  startMs: number;
  endMs: number;
  hookText: string | null;
  layout: string | null;
  rationale: string;
  publish: string;
};
