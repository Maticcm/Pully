export type MediaFormat = {
  id: string;
  extension: string;
  resolution?: string;
  width?: number;
  height?: number;
  fps?: number;
  videoCodec?: string;
  audioCodec?: string;
  fileSize?: number;
  note?: string;
  hasVideo: boolean;
  hasAudio: boolean;
};

export type SubtitleTrack = { language: string; name?: string; automatic: boolean; extensions: string[] };

export type PlaylistItem = { id: string; title: string; url: string; thumbnail?: string; duration?: number };

export type MediaInfo = {
  id: string;
  url: string;
  title: string;
  creator?: string;
  duration?: number;
  thumbnail?: string;
  source: string;
  isPlaylist: boolean;
  playlistCount?: number;
  playlistItems: PlaylistItem[];
  formats: MediaFormat[];
  subtitles: SubtitleTrack[];
};

export type DownloadRequest = {
  url: string;
  title: string;
  creator?: string;
  thumbnail?: string;
  mode: "video" | "audio";
  formatId?: string;
  formatHasAudio?: boolean;
  targetFps?: number;
  outputFormat: string;
  outputDirectory?: string;
  filenameTemplate?: string;
  isPlaylist: boolean;
  playlistFolder: boolean;
  existingFileBehavior: "skip" | "overwrite";
  embedMetadata: boolean;
  embedThumbnail: boolean;
  saveThumbnail: boolean;
  downloadSubtitles: boolean;
  embedSubtitles: boolean;
};

export type DownloadStatus = "waiting" | "downloading" | "processing" | "completed" | "failed" | "cancelled";

export type DownloadProgress = {
  id: string;
  title: string;
  thumbnail?: string;
  status: DownloadStatus;
  percent: number;
  downloadedBytes?: number;
  totalBytes?: number;
  speed?: string;
  eta?: string;
  outputPath?: string;
  error?: string;
};

export type DependencyInfo = { pully: string; ytDlp?: string; ffmpeg?: string; spotiFlac?: string; ready: boolean; issues: string[] };

export type SetupProgress = { tool: "yt-dlp" | "ffmpeg"; percent: number; stage: "downloading" | "extracting" | "done" | "error"; message?: string };

export type BrowserName = "chrome" | "brave" | "edge" | "other";

/** A tab Pully currently knows about from a connected browser extension.
 * `key` (browserInstanceId:tabId) is the only safe unique identifier —
 * `tabId` alone repeats across browser sessions. */
export type DetectedTab = {
  key: string;
  browserInstanceId: string;
  browser: BrowserName;
  tabId: string;
  windowId?: string;
  url: string;
  title: string;
  active: boolean;
  favIconUrl?: string;
};

export type BrowserConnectionStatus = { browser: BrowserName; connected: boolean };
