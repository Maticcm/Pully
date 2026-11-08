import { invoke } from "@tauri-apps/api/core";
import type { BrowserConnectionStatus, DependencyInfo, DetectedTab, DownloadRequest, MediaInfo, SyncedPlaylist } from "../types/media";

export type ThemeSnapshot = {
  accentColor: string;
  accentForeground: string;
  canvas: string;
  surface: string;
  dark: boolean;
  fontFamily: string;
};

export type PendingLaunch = { url: string; quick: boolean };

export const api = {
  listSyncedPlaylists: () => invoke<SyncedPlaylist[]>("list_synced_playlists"),
  addSyncedPlaylist: (request: DownloadRequest, intervalMinutes: number, writeM3u: boolean) => invoke<void>("add_synced_playlist", { request, intervalMinutes, writeM3u }),
  changeSyncedPlaylist: (id: string, action: "pause" | "resume" | "sync" | "remove" | "folder" | "root") => invoke<void>("change_synced_playlist", { id, action }),
  analyze: (url: string) => invoke<MediaInfo>("analyze_url", { url }),
  startDownload: (request: DownloadRequest) => invoke<string>("start_download", { request }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  removeDownload: (id: string) => invoke<void>("remove_download", { id }),
  retryDownload: (id: string) => invoke<string>("retry_download", { id }),
  openFile: (id: string) => invoke<void>("open_download", { id, reveal: false }),
  revealFile: (id: string) => invoke<void>("open_download", { id, reveal: true }),
  dependencies: () => invoke<DependencyInfo>("dependency_info"),
  setConcurrency: (concurrency: number) => invoke<void>("set_concurrency", { concurrency }),
  takePendingUrl: () => invoke<PendingLaunch | null>("take_pending_url"),
  browserTabs: () => invoke<DetectedTab[]>("browser_tabs"),
  browserConnectionStatus: () => invoke<BrowserConnectionStatus[]>("browser_connection_status"),
  setBrowserIntegrationEnabled: (enabled: boolean) => invoke<void>("set_browser_integration_enabled", { enabled }),
  pushTheme: (theme: ThemeSnapshot) => invoke<void>("push_theme", { theme }),
  setRunInTray: (enabled: boolean) => invoke<void>("set_run_in_tray", { enabled }),
  installDependencies: () => invoke<DependencyInfo>("install_dependencies"),
  installSpotiFlac: () => invoke<DependencyInfo>("install_spotiflac"),
};
