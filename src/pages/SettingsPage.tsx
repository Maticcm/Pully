import * as TabsPrimitive from "@radix-ui/react-tabs";
import { open } from "@tauri-apps/plugin-dialog";
import { Check, Download, FileText, FolderOpen, Info, Puzzle, RotateCcw, Save, SlidersHorizontal, Tag, Zap } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import type { BrowserConnectionStatus, DependencyInfo } from "../types/media";
import { defaultSettings, type AppSettings } from "../types/settings";
import { Toggle } from "../components/Toggle";
import { Select } from "../components/Select";
import { ColorPicker } from "../components/ColorPicker";
import { interactionSpring, pageTransition, pageVariants } from "../lib/animation";
import { fontStacks } from "../lib/fonts";
import { api } from "../lib/tauri";
import type { UpdateStatus } from "../hooks/useAppUpdater";

type Section = "general" | "downloads" | "quick" | "naming" | "extras" | "browser" | "status";
type Props = { settings: AppSettings; dependencies?: DependencyInfo; installingSpotiFlac: boolean; spotiFlacInstallError?: string; onInstallSpotiFlac(): void; updateStatus: UpdateStatus; onCheckForUpdates(): void; onSave(value: AppSettings): void; onReset(): void };

const sections: { id: Section; label: string; icon: typeof SlidersHorizontal }[] = [
  { id: "general", label: "General", icon: SlidersHorizontal },
  { id: "downloads", label: "Downloads", icon: Download },
  { id: "quick", label: "Quick mode", icon: Zap },
  { id: "naming", label: "Naming", icon: FileText },
  { id: "extras", label: "Extras", icon: Tag },
  { id: "browser", label: "Browser", icon: Puzzle },
  { id: "status", label: "Status", icon: Info },
];
const browserLabel: Record<BrowserConnectionStatus["browser"], string> = { chrome: "Chrome", brave: "Brave", edge: "Edge", other: "Other browser" };
const quickModeTypeOptions = [{ value: "video", label: "Video" }, { value: "audio", label: "Audio" }];

const tokens = ["{title}", "{creator}", "{id}", "{resolution}", "{ext}"];
const accentPresets = [
  { name: "Pully", value: "#c8f169" },
  { name: "Violet", value: "#8b5cf6" },
  { name: "Ocean", value: "#60a5fa" },
  { name: "Coral", value: "#fb7185" },
  { name: "Amber", value: "#fbbf24" },
];
const themeOptions = [{ value: "system", label: "Use system setting" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }];
const fontOptions = [
  { value: "manrope", label: "Manrope", detail: "Built in · recommended" },
  { value: "inter", label: "Inter", detail: "Built in · clean" },
  { value: "space-grotesk", label: "Space Grotesk", detail: "Built in · geometric" },
  { value: "ibm-plex", label: "IBM Plex Sans", detail: "Built in · technical" },
  { value: "jetbrains", label: "JetBrains Mono", detail: "Built in · monospace" },
  { value: "system", label: "System UI", detail: "Uses your operating system" },
  { value: "rounded", label: "Rounded", detail: "Uses Trebuchet MS" },
  { value: "classic", label: "Classic serif", detail: "Uses Georgia" },
  { value: "mono", label: "System monospace", detail: "Uses Cascadia or Consolas" },
].map((option) => ({ ...option, style: { fontFamily: fontStacks[option.value as AppSettings["font"]] } }));
const baseOptions = [{ value: "neutral", label: "Neutral" }, { value: "cool", label: "Cool" }, { value: "warm", label: "Warm" }];
const videoQualityOptions = [{ value: "best", label: "Best" }, { value: "2160", label: "4K · 2160p" }, { value: "1440", label: "QHD · 1440p" }, { value: "1080", label: "FHD · 1080p" }, { value: "720", label: "HD · 720p" }, { value: "480", label: "SD · 480p" }];
const videoFormatOptions = [
  { value: "mp4", label: "MP4", detail: "Widely compatible" },
  { value: "webm", label: "WebM", detail: "Open web container" },
  { value: "mkv", label: "MKV", detail: "Flexible container" },
  { value: "mov", label: "MOV", detail: "Apple-friendly container" },
  { value: "original", label: "Original", detail: "Keep the source container" },
];
const audioFormatOptions = [
  { value: "original", label: "Original", detail: "Keep the source format" },
  { value: "mp3", label: "MP3", detail: "Universal compatibility" },
  { value: "m4a", label: "M4A", detail: "Efficient AAC container" },
  { value: "aac", label: "AAC", detail: "Raw AAC audio" },
  { value: "opus", label: "Opus", detail: "Efficient modern codec" },
  { value: "vorbis", label: "OGG Vorbis", detail: "Open audio format" },
  { value: "flac", label: "FLAC", detail: "Lossless compression" },
  { value: "alac", label: "ALAC", detail: "Apple lossless" },
  { value: "wav", label: "WAV", detail: "Uncompressed audio" },
];
const concurrencyOptions = [1, 2, 3, 4, 5, 6].map((value) => ({ value: String(value), label: String(value) }));
const existingFileOptions = [{ value: "skip", label: "Skip existing file" }, { value: "overwrite", label: "Overwrite it" }];

export function SettingsPage({ settings, dependencies, installingSpotiFlac, spotiFlacInstallError, onInstallSpotiFlac, updateStatus, onCheckForUpdates, onSave, onReset }: Props) {
  const [draft, setDraft] = useState(settings);
  const [section, setSection] = useState<Section>("general");
  const [saved, setSaved] = useState(false);
  const [browserStatus, setBrowserStatus] = useState<BrowserConnectionStatus[]>([]);
  useEffect(() => setDraft(settings), [settings]);
  useEffect(() => {
    if (section !== "browser" || !settings.browserIntegrationEnabled) return;
    let cancelled = false;
    const poll = () => void api.browserConnectionStatus().then((value) => { if (!cancelled) setBrowserStatus(value); }).catch(() => undefined);
    poll();
    const interval = window.setInterval(poll, 4000);
    return () => { cancelled = true; window.clearInterval(interval); };
  }, [section, settings.browserIntegrationEnabled]);
  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => setDraft((current) => ({ ...current, [key]: value }));
  const changed = useMemo(() => JSON.stringify(draft) !== JSON.stringify(settings), [draft, settings]);
  const save = () => { onSave(draft); setSaved(true); window.setTimeout(() => setSaved(false), 1600); };
  const reset = () => { setDraft(defaultSettings); onReset(); };
  const browse = async () => { const selected = await open({ directory: true, multiple: false, title: "Choose download folder" }); if (typeof selected === "string") update("downloadDirectory", selected); };
  const addToken = (token: string) => update("filenameTemplate", `${draft.filenameTemplate}${token}`);

  return <TabsPrimitive.Root value={section} onValueChange={(value) => setSection(value as Section)} asChild>
    <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" transition={pageTransition} className="mx-auto flex h-[calc(100vh-128px)] max-w-5xl flex-col px-6 pb-6 pt-1">
    <div className="flex flex-wrap items-center justify-between gap-4"><div><h1 className="text-3xl font-bold tracking-[-.04em]">Settings</h1><p className="mt-1 text-sm text-black/45 dark:text-white/40">Make Pully work the way you do.</p></div><div className="flex gap-2"><button className="secondary-button" onClick={reset}><RotateCcw size={15}/> Reset</button><button className="primary-button" disabled={!changed} onClick={save}>{saved ? <Check size={16}/> : <Save size={16}/>} {saved ? "Saved" : "Save changes"}</button></div></div>
    <TabsPrimitive.List aria-label="Settings sections" className="mt-5 grid grid-cols-7 border-b border-black/[.07] dark:border-white/[.08]">{sections.map(({ id, label, icon: Icon }) => <TabsPrimitive.Trigger key={id} value={id} className={`settings-tab relative ${section === id ? "active" : ""}`}>{section === id && <motion.span layoutId="settings-tab-indicator" transition={interactionSpring} className="absolute inset-x-3 -bottom-0.5 h-0.5 rounded-full bg-lime"/>}<Icon size={16}/><span className="hidden sm:inline">{label}</span></TabsPrimitive.Trigger>)}</TabsPrimitive.List>
    <section className="settings-panel mt-5 min-h-0 flex-1 rounded-3xl border border-black/[.07] bg-surface p-5 shadow-sm dark:border-white/[.08] sm:p-6">
      <AnimatePresence mode="wait" initial={false}>
      <TabsPrimitive.Content forceMount value={section} key={section} asChild>
      <motion.div initial={{ opacity: 0, x: 4 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -4 }} transition={{ duration: 0.14 }}>
      {section === "general" && <SettingsGrid title="Appearance" description="Pully uses your operating system language and needs no account.">
        <SettingField label="Theme"><Select ariaLabel="Theme" value={draft.theme} options={themeOptions} onChange={(value) => update("theme", value as AppSettings["theme"])}/></SettingField>
        <SettingField label="Font"><Select ariaLabel="Font" value={draft.font} options={fontOptions} columns={2} onChange={(value) => update("font", value as AppSettings["font"])}/></SettingField>
        <SettingField label="Base color"><Select ariaLabel="Base color" value={draft.baseColor} options={baseOptions} onChange={(value) => update("baseColor", value as AppSettings["baseColor"])}/></SettingField>
        <div className="settings-note"><div className="h-9 w-9 shrink-0 rounded-xl bg-lime"/><div><p className="text-sm font-semibold">Interface preview</p><p className="text-xs text-black/45 dark:text-white/40">Changes apply across the app after saving.</p></div></div>
        <div className="sm:col-span-2"><span className="mb-3 block text-xs font-semibold text-black/55 dark:text-white/50">Accent color</span><div className="flex flex-wrap items-center gap-3">{accentPresets.map((preset) => <button key={preset.value} type="button" title={preset.name} aria-label={`${preset.name} accent`} onClick={() => update("accentColor", preset.value)} className={`grid h-10 w-10 place-items-center rounded-xl border transition hover:scale-105 ${draft.accentColor.toLowerCase() === preset.value ? "border-black/50 ring-2 ring-black/10 dark:border-white/60 dark:ring-white/10" : "border-black/10 dark:border-white/10"}`}><span className="h-6 w-6 rounded-lg" style={{ backgroundColor: preset.value }}/></button>)}<ColorPicker ariaLabel="Custom accent color" value={draft.accentColor} onChange={(value) => update("accentColor", value)}/></div></div>
        <div className="sm:col-span-2 border-t border-black/[.06] pt-4 dark:border-white/[.07]"><Toggle checked={draft.runInTray} onChange={(value) => update("runInTray", value)} label="Run in system tray" description="Closing the window keeps Pully running in the background so browser-triggered downloads and detection keep working. Quit from the tray icon to fully exit."/></div>
      </SettingsGrid>}
      {section === "downloads" && <SettingsGrid title="Download defaults" description="These choices are applied when you analyze new media.">
        <div className="sm:col-span-2"><SettingField label="Download folder"><div className="flex gap-2"><input className="settings-input min-w-0 flex-1" value={draft.downloadDirectory} readOnly placeholder="Downloads/Pully"/><button className="secondary-button" onClick={() => void browse()}><FolderOpen size={15}/> Browse</button></div></SettingField></div>
        <SettingField label="Default video quality"><Select ariaLabel="Default video quality" value={draft.defaultVideoQuality} options={videoQualityOptions} onChange={(value) => update("defaultVideoQuality", value as AppSettings["defaultVideoQuality"])}/></SettingField>
        <SettingField label="Default video format"><Select ariaLabel="Default video format" value={draft.defaultVideoFormat} options={videoFormatOptions} onChange={(value) => update("defaultVideoFormat", value as AppSettings["defaultVideoFormat"])}/></SettingField>
        <SettingField label="Default audio format"><Select ariaLabel="Default audio format" value={draft.defaultAudioFormat} options={audioFormatOptions} onChange={(value) => update("defaultAudioFormat", value as AppSettings["defaultAudioFormat"])}/></SettingField>
        <SettingField label="Concurrent downloads"><Select ariaLabel="Concurrent downloads" value={String(draft.concurrentDownloads)} options={concurrencyOptions} onChange={(value) => update("concurrentDownloads", Number(value))}/></SettingField>
        <div className="sm:col-span-2 border-t border-black/[.06] pt-4 dark:border-white/[.07]"><Toggle checked={draft.preferOriginalFormats} onChange={(value) => update("preferOriginalFormats", value)} label="Prefer original formats" description="Avoid remuxing unless your selected format requires it."/></div>
      </SettingsGrid>}
      {section === "quick" && <SettingsGrid title="Quick mode" description="When on, pasting a link downloads it immediately with this preset — no review step, ready for the next link right away.">
        <div className="sm:col-span-2 border-b border-black/[.06] pb-4 dark:border-white/[.07]"><Toggle checked={draft.quickMode} onChange={(value) => update("quickMode", value)} label="Enable quick mode" description="Toggle any time from the zap icon in the header."/></div>
        <SettingField label="Download as"><Select ariaLabel="Quick mode media type" value={draft.quickModeType} options={quickModeTypeOptions} onChange={(value) => update("quickModeType", value as AppSettings["quickModeType"])}/></SettingField>
        <div/>
        {draft.quickModeType === "video" ? <>
          <SettingField label="Video quality"><Select ariaLabel="Quick mode video quality" value={draft.quickModeVideoQuality} options={videoQualityOptions} onChange={(value) => update("quickModeVideoQuality", value as AppSettings["quickModeVideoQuality"])}/></SettingField>
          <SettingField label="Video format"><Select ariaLabel="Quick mode video format" value={draft.quickModeVideoFormat} options={videoFormatOptions} onChange={(value) => update("quickModeVideoFormat", value as AppSettings["quickModeVideoFormat"])}/></SettingField>
        </> : <SettingField label="Audio format"><Select ariaLabel="Quick mode audio format" value={draft.quickModeAudioFormat} options={audioFormatOptions} onChange={(value) => update("quickModeAudioFormat", value as AppSettings["quickModeAudioFormat"])}/></SettingField>}
        <div className="sm:col-span-2 border-t border-black/[.06] pt-4 dark:border-white/[.07]"><Toggle checked={draft.quickModeEmbedMetadata} onChange={(value) => update("quickModeEmbedMetadata", value)} label="Embed metadata" description="Applies to every quick-mode download."/></div>
        <Toggle checked={draft.quickModeEmbedThumbnail} onChange={(value) => update("quickModeEmbedThumbnail", value)} label="Embed thumbnail" description="Uses FFmpeg when the selected container supports it."/>
        <Toggle checked={draft.quickModeDownloadSubtitles} onChange={(value) => update("quickModeDownloadSubtitles", value)} label="Download subtitles" description="Includes available manual and automatic captions."/>
        <div className="sm:col-span-2 rounded-xl bg-black/[.035] px-4 py-3 text-xs text-black/45 dark:bg-white/[.045] dark:text-white/40">If the source doesn't support the selected type (for example, an audio-only source with "Video" chosen), quick mode automatically falls back to what's available. Folder, filename, and existing-file rules come from the Downloads and Naming tabs.</div>
      </SettingsGrid>}
      {section === "naming" && <SettingsGrid title="File naming" description="Use a small, safe set of metadata tokens. Folder separators and arbitrary yt-dlp expressions are not accepted.">
        <div className="sm:col-span-2"><SettingField label="Filename"><input className="settings-input font-mono" value={draft.filenameTemplate} onChange={(e) => update("filenameTemplate", e.target.value)}/></SettingField><div className="mt-3 flex flex-wrap gap-2">{tokens.map((token) => <button key={token} className="token-button" onClick={() => addToken(token)}>{token}</button>)}</div><div className="mt-4 rounded-xl bg-black/[.035] px-4 py-3 dark:bg-white/[.045]"><span className="text-[10px] font-bold uppercase tracking-wider text-black/35 dark:text-white/30">Preview</span><p className="mt-1 truncate font-mono text-sm">{draft.filenameTemplate.replace("{title}", "Golden").replace("{creator}", "HUNTR/X").replace("{id}", "a1b2c3").replace("{resolution}", "1080p").replace("{ext}", "mp4")}</p></div></div>
        <Toggle checked={draft.playlistFolder} onChange={(value) => update("playlistFolder", value)} label="Create a folder for playlists" description="Keeps multi-item downloads together."/>
        <SettingField label="If a file already exists"><Select ariaLabel="Existing file behavior" value={draft.existingFileBehavior} options={existingFileOptions} onChange={(value) => update("existingFileBehavior", value as AppSettings["existingFileBehavior"])}/></SettingField>
      </SettingsGrid>}
      {section === "extras" && <SettingsGrid title="Metadata and extras" description="Only options supported safely by yt-dlp and FFmpeg are shown.">
        <Toggle checked={draft.embedMetadata} onChange={(value) => update("embedMetadata", value)} label="Embed metadata" description="Writes supported source metadata into the output file."/>
        <Toggle checked={draft.embedThumbnail} onChange={(value) => update("embedThumbnail", value)} label="Embed thumbnail" description="Uses FFmpeg when the selected container supports it."/>
        <Toggle checked={draft.saveThumbnail} onChange={(value) => update("saveThumbnail", value)} label="Save thumbnail separately" description="Keeps the source thumbnail next to the download."/>
        <Toggle checked={draft.downloadSubtitles} onChange={(value) => update("downloadSubtitles", value)} label="Download subtitles" description="Includes available manual and automatic captions."/>
        <Toggle checked={draft.embedSubtitles} onChange={(value) => update("embedSubtitles", value)} label="Embed subtitles when compatible" description="Also downloads subtitles when enabled."/>
      </SettingsGrid>}
      {section === "browser" && <SettingsGrid title="Browser integration" description="Lets Pully see supported pages currently open in browsers with the Pully extension installed. This information stays on your device and is never uploaded by Pully.">
        <div className="sm:col-span-2 border-b border-black/[.06] pb-4 dark:border-white/[.07]"><Toggle checked={draft.browserIntegrationEnabled} onChange={(value) => update("browserIntegrationEnabled", value)} label="Enable browser integration" description="Off by default. Requires the Pully browser extension to be installed separately."/></div>
        <Toggle checked={draft.browserShowTabs} onChange={(value) => update("browserShowTabs", value)} label="Show open browser tabs in Pully" description={'Lists relevant open tabs under "Detected in your browser" on the home screen.'}/>
        <Toggle checked={draft.browserOnlyActiveTab} onChange={(value) => update("browserOnlyActiveTab", value)} label="Only show active tab" description="Shows just the tab currently focused in each browser instead of every open tab."/>
        <Toggle checked={draft.browserShowBrowserName} onChange={(value) => update("browserShowBrowserName", value)} label="Show browser name" description="Displays which browser (Chrome, Brave, Edge) a detected tab came from."/>
        <div className="sm:col-span-2 border-t border-black/[.06] pt-4 dark:border-white/[.07]">
          <span className="mb-3 block text-xs font-semibold text-black/55 dark:text-white/50">Connection status</span>
          {!draft.browserIntegrationEnabled ? <p className="text-xs text-black/45 dark:text-white/40">Turn on browser integration to see connected browsers.</p>
            : browserStatus.length === 0 ? <p className="text-xs text-black/45 dark:text-white/40">No browser has connected yet. Install the Pully extension and open a supported page.</p>
            : <div className="grid gap-2 sm:grid-cols-2">{browserStatus.map((status) => <div key={status.browser} className="flex items-center justify-between rounded-xl border border-black/[.06] px-4 py-3 dark:border-white/[.08]"><span className="text-sm font-semibold">{browserLabel[status.browser]}</span><span className={`rounded-full px-2.5 py-1 text-[10px] font-bold uppercase tracking-wider ${status.connected ? "bg-lime/40 text-ink dark:bg-lime dark:text-ink" : "bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-200"}`}>{status.connected ? "Connected" : "Not connected"}</span></div>)}</div>}
        </div>
      </SettingsGrid>}
      {section === "status" && <SettingsGrid title="Installed components" description="Pully checks these local executables before starting work.">
        <StatusRow label="Pully" value={dependencies?.pully ?? "0.1.1"} ready />
        <div className="sm:col-span-2 rounded-xl border border-black/[.06] px-4 py-3 dark:border-white/[.08]">
          <Toggle checked={draft.autoUpdate} onChange={(value) => update("autoUpdate", value)} label="Install app updates automatically" description="Checks on startup and daily. Installs signed updates after downloads and other work finish."/>
          <div className="mt-3 flex items-center gap-3"><button type="button" className="secondary-button" disabled={["checking", "downloading", "installing"].includes(updateStatus.phase)} onClick={onCheckForUpdates}>Check for updates</button><span className="text-xs text-black/45 dark:text-white/40">{updateStatus.phase === "checking" ? "Checking..." : updateStatus.phase === "current" ? "Pully is up to date" : updateStatus.phase === "waiting" ? `Version ${updateStatus.version} waiting for idle time` : updateStatus.phase === "downloading" ? `Downloading ${updateStatus.version}${updateStatus.progress == null ? "" : ` (${updateStatus.progress}%)`}` : updateStatus.phase === "installing" ? "Installing update..." : updateStatus.phase === "error" ? "Update check or install failed" : ""}</span></div>
          {updateStatus.phase === "error" && <p role="alert" className="mt-2 text-xs text-red-600 dark:text-red-300">{updateStatus.message}</p>}
        </div>
        <StatusRow label="yt-dlp" value={dependencies?.ytDlp ?? "Not found"} ready={Boolean(dependencies?.ytDlp)} />
        <StatusRow label="FFmpeg" value={dependencies?.ffmpeg ?? "Not found"} ready={Boolean(dependencies?.ffmpeg)} />
        <StatusRow label="SpotiFLAC (beta)" value={dependencies?.spotiFlac ?? "Not found — needed only for Spotify links"} ready={Boolean(dependencies?.spotiFlac)} />
        <div className="sm:col-span-2">
          <button type="button" className="secondary-button" disabled={installingSpotiFlac} onClick={onInstallSpotiFlac}>{installingSpotiFlac ? "Installing SpotiFLAC..." : dependencies?.spotiFlac ? "Update SpotiFLAC" : "Install SpotiFLAC"}</button>
          <p className="mt-2 text-xs text-black/45 dark:text-white/40">Uses Python 3 to install SpotiFLAC in Pully's app data folder. Required only for Spotify downloads.</p>
          {spotiFlacInstallError && <p role="alert" className="mt-2 text-xs text-red-600 dark:text-red-300">{spotiFlacInstallError}</p>}
        </div>
      </SettingsGrid>}
      </motion.div>
      </TabsPrimitive.Content>
      </AnimatePresence>
    </section>
  </motion.div>
  </TabsPrimitive.Root>;
}

function SettingsGrid({ title, description, children }: { title: string; description: string; children: ReactNode }) { return <><div className="mb-7"><h2 className="text-lg font-bold">{title}</h2><p className="mt-1 text-sm text-black/45 dark:text-white/40">{description}</p></div><div className="grid gap-x-10 gap-y-5 sm:grid-cols-2">{children}</div></>; }
function SettingField({ label, children }: { label: string; children: ReactNode }) { return <label className="block"><span className="mb-2 block text-xs font-semibold text-black/55 dark:text-white/50">{label}</span>{children}</label>; }
function StatusRow({ label, value, ready }: { label: string; value: string; ready: boolean }) { return <div className="flex items-center justify-between rounded-xl border border-black/[.06] px-4 py-3 dark:border-white/[.08]"><div><p className="text-sm font-semibold">{label}</p><p className="mt-0.5 max-w-[240px] truncate font-mono text-xs text-black/45 dark:text-white/40">{value}</p></div><span className={`rounded-full px-2.5 py-1 text-[10px] font-bold uppercase tracking-wider ${ready ? "bg-lime/40 text-ink dark:bg-lime dark:text-ink" : "bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-200"}`}>{ready ? "Ready" : "Missing"}</span></div>; }
