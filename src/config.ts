import type { ConfigState, DecisionOutput, PreviewTab, SourceMode } from "./types";

export const DEFAULT_CONFIG: ConfigState = {
  provider: "gemini",
  apiKeyEnv: "GEMINI_API_KEY",
  pollSeconds: 10,
  maxAttempts: 4,
  ringMinutes: 10,
  observerWindowSeconds: 12,
  observerStepSeconds: 6,
  maturationDelaySeconds: 20,
  queueCapacity: 256,
  frameIntervalSeconds: 1,
  scribbleModel: "large_turbo",
  enableVad: true,
  language: "auto",
  incrementalMinWindowSeconds: 30,
  dryRun: true,
  youtube: true,
  instagram: false,
  tiktokDrafts: false,
  twitchClips: false,
  stagingProvider: "local",
  bucket: "clipfarmer-artifacts",
  prefix: "staged",
  publicBaseUrl: "",
  endpointEnv: "S3_ENDPOINT",
  ffmpegPath: "ffmpeg",
  ffprobePath: "ffprobe",
  streamlinkPath: "streamlink",
  chatDownloaderPath: "chat_downloader",
};

export function loadConfig(): ConfigState {
  try {
    return {
      ...DEFAULT_CONFIG,
      ...JSON.parse(localStorage.getItem("clipfarmer-config") || "{}"),
    };
  } catch {
    return DEFAULT_CONFIG;
  }
}

export function buildBackendConfig(config: ConfigState) {
  const isGemini = config.provider === "gemini";
  return {
    worker: {
      pollSeconds: config.pollSeconds,
      maxAttempts: config.maxAttempts,
      ringMinutes: config.ringMinutes,
      observerWindowSeconds: config.observerWindowSeconds,
      observerStepSeconds: config.observerStepSeconds,
      maturationDelaySeconds: config.maturationDelaySeconds,
      queueCapacity: config.queueCapacity,
    },
    media: {
      ffmpegPath: config.ffmpegPath,
      ffprobePath: config.ffprobePath,
      streamlinkPath: config.streamlinkPath,
      chatDownloaderPath: config.chatDownloaderPath,
      frameIntervalSeconds: config.frameIntervalSeconds,
    },
    scribble: {
      modelVariant: config.scribbleModel,
      enableVad: config.enableVad,
      language: config.language,
      incrementalMinWindowSeconds: config.incrementalMinWindowSeconds,
    },
    models: { provider: config.provider },
    [config.provider]: {
      apiKeyEnv: config.apiKeyEnv,
      observerModel: isGemini ? "gemini-3.7-flash" : "gpt-5.6-terra",
      directorModel: isGemini ? "gemini-3.7-flash" : "gpt-5.6-sol",
      editorModel: isGemini ? "gemini-3.7-flash" : "gpt-5.6-sol",
      criticModel: isGemini ? "gemini-3.7-flash" : "gpt-5.6-sol",
      audioModel: isGemini ? "gemini-3.7-flash" : "gpt-audio-1.5",
    },
    staging: {
      provider: config.stagingProvider,
      bucket: config.bucket,
      prefix: config.prefix,
      publicBaseUrl: config.publicBaseUrl || null,
      endpointEnv: config.endpointEnv,
    },
    publishers: {
      dryRun: config.dryRun,
      youtube: config.youtube,
      instagram: config.instagram,
      tiktokDrafts: config.tiktokDrafts,
      twitchClips: config.twitchClips,
    },
  };
}

export function buildOutputPreview(tab: PreviewTab, config: ConfigState): DecisionOutput {
  const accepted = tab === "approved";
  return {
    candidateId: accepted ? "clip_8f2c1" : "clip_3ad90",
    status: accepted ? "accepted" : "rejected",
    confidence: accepted ? 0.94 : 0.41,
    title: accepted ? "The impossible comeback" : "Quiet inventory management",
    startMs: accepted ? 1842000 : 2478000,
    endMs: accepted ? 1876500 : 2501000,
    hookText: accepted ? "Nobody thought this run was recoverable." : null,
    layout: accepted ? "vertical_focus" : null,
    rationale: accepted
      ? `Strong payoff, readable reaction, and a clear ${Math.round(config.observerWindowSeconds)}s narrative arc.`
      : `No distinct hook or payoff after the ${config.maturationDelaySeconds}s maturation window.`,
    publish: accepted ? (config.dryRun ? "dry_run" : "queued") : "skipped",
  };
}

export const isTauri = () => "__TAURI_INTERNALS__" in window;

export function isSourceValid(mode: SourceMode, value: string) {
  return mode === "channel"
    ? /^[A-Za-z0-9_]+$/.test(value)
    : /^https:\/\/(www\.)?twitch\.tv\/videos\//.test(value);
}
