export type ThemePreference = "system" | "light" | "dark";
export type ExistingFileBehavior = "skip" | "overwrite";
export type FontPreference = "system" | "inter" | "manrope" | "space-grotesk" | "ibm-plex" | "jetbrains" | "rounded" | "classic" | "mono";
export type BaseColor = "neutral" | "cool" | "warm";

export type AppSettings = {
  theme: ThemePreference;
  font: FontPreference;
  baseColor: BaseColor;
  accentColor: string;
  downloadDirectory: string;
  defaultVideoQuality: "best" | "2160" | "1440" | "1080" | "720" | "480";
  defaultVideoFormat: "mp4" | "webm" | "mkv" | "mov" | "original";
  defaultAudioFormat: "original" | "mp3" | "m4a" | "aac" | "flac" | "alac" | "opus" | "vorbis" | "wav";
  concurrentDownloads: number;
  filenameTemplate: string;
  playlistFolder: boolean;
  existingFileBehavior: ExistingFileBehavior;
  embedMetadata: boolean;
  embedThumbnail: boolean;
  saveThumbnail: boolean;
  downloadSubtitles: boolean;
  embedSubtitles: boolean;
  preferOriginalFormats: boolean;
  quickMode: boolean;
  quickModeType: "video" | "audio";
  quickModeVideoQuality: "best" | "2160" | "1440" | "1080" | "720" | "480";
  quickModeVideoFormat: "mp4" | "webm" | "mkv" | "mov" | "original";
  quickModeAudioFormat: "original" | "mp3" | "m4a" | "aac" | "flac" | "alac" | "opus" | "vorbis" | "wav";
  quickModeEmbedMetadata: boolean;
  quickModeEmbedThumbnail: boolean;
  quickModeDownloadSubtitles: boolean;
  browserIntegrationEnabled: boolean;
  browserShowTabs: boolean;
  browserOnlyActiveTab: boolean;
  browserShowBrowserName: boolean;
  runInTray: boolean;
  autoUpdate: boolean;
};

export const defaultSettings: AppSettings = {
  theme: "system",
  font: "manrope",
  baseColor: "neutral",
  accentColor: "#c8f169",
  downloadDirectory: "",
  defaultVideoQuality: "best",
  defaultVideoFormat: "mp4",
  defaultAudioFormat: "original",
  concurrentDownloads: 2,
  filenameTemplate: "{title} [{id}].{ext}",
  playlistFolder: true,
  existingFileBehavior: "skip",
  embedMetadata: true,
  embedThumbnail: true,
  saveThumbnail: false,
  downloadSubtitles: false,
  embedSubtitles: false,
  preferOriginalFormats: false,
  quickMode: false,
  quickModeType: "video",
  quickModeVideoQuality: "best",
  quickModeVideoFormat: "mp4",
  quickModeAudioFormat: "mp3",
  quickModeEmbedMetadata: true,
  quickModeEmbedThumbnail: true,
  quickModeDownloadSubtitles: false,
  browserIntegrationEnabled: false,
  browserShowTabs: true,
  browserOnlyActiveTab: false,
  browserShowBrowserName: true,
  runInTray: true,
  autoUpdate: true,
};
