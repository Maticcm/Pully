import { AlertTriangle, ArrowDownToLine, AudioLines, Clipboard, Link2, Music2, Youtube } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { AnalyzeSkeleton } from "../components/AnalyzeSkeleton";
import { DetectedTabs } from "../components/DetectedTabs";
import { MediaCard } from "../components/MediaCard";
import { interactionSpring, layoutSpring, pageTransition, pageVariants } from "../lib/animation";
import type { DependencyInfo, DetectedTab, DownloadRequest, MediaInfo, SetupProgress } from "../types/media";
import type { AppSettings } from "../types/settings";
import type { UpdateStatus } from "../hooks/useAppUpdater";

type Props = {
  updateStatus?: UpdateStatus;
  url: string;
  analyzing: boolean;
  quickBusy: boolean;
  error?: string;
  dependencies?: DependencyInfo;
  installing: boolean;
  installProgress: Record<string, SetupProgress>;
  installError?: string;
  media?: MediaInfo;
  settings: AppSettings;
  browserTabs: DetectedTab[];
  onUrlChange(value: string): void;
  onSubmit(): void;
  onPaste(): void;
  onInstall(): void;
  onAnalyze(url: string): void;
  onQuickDownload(url: string): void;
  onDownload(request: DownloadRequest): Promise<void>;
};

type ComposerProps = Pick<Props, "url" | "analyzing" | "quickBusy" | "settings" | "onUrlChange" | "onSubmit" | "onPaste"> & {
  compact?: boolean;
};

function BrandHeading({ compact = false, updateStatus }: { compact?: boolean; updateStatus?: UpdateStatus }) {
  const updateAvailable = updateStatus && ["waiting", "downloading", "installing"].includes(updateStatus.phase);
  return <div className={`home-heading ${compact ? "home-heading--compact" : ""}`}>
    <div className="home-wordmark">
      <span className="home-brand-mark" aria-hidden="true"><ArrowDownToLine strokeWidth={2.3}/></span>
      <h1>Pully<span className="home-wordmark-dot" aria-hidden="true">.</span>{updateAvailable && <span className="home-update-dot" role="status" title={`Update ${updateStatus.version ?? ""} available`.replace("  ", " ")}><span className="sr-only">App update available{updateStatus.version ? `: version ${updateStatus.version}` : ""}.</span></span>}</h1>
    </div>
    <p className="home-heading-description">Videos, music, and playlists. Saved your way.</p>
    {!compact && <div className="home-platforms" aria-label="Supported platforms include YouTube, Spotify, SoundCloud, and more">
      <span><Youtube size={16} className="text-red-500" aria-hidden="true"/>YouTube</span>
      <span><Music2 size={15} className="text-emerald-500" aria-hidden="true"/>Spotify</span>
      <span><AudioLines size={16} className="text-orange-500" aria-hidden="true"/>SoundCloud</span>
      <span className="home-platforms-more">+ more</span>
    </div>}
  </div>;
}

function LinkComposer({ url, analyzing, quickBusy, settings, onUrlChange, onSubmit, onPaste, compact = false }: ComposerProps) {
  const busy = analyzing || quickBusy;
  const action = settings.quickMode ? (quickBusy ? "Queuing..." : "Download") : (analyzing ? "Pulling..." : "Pull");
  const placeholder = settings.quickMode ? "Paste a link to download instantly..." : "Paste a video, playlist, or channel link...";

  return <motion.div layoutId="link-composer" transition={layoutSpring} className={`home-link-bar ${compact ? "" : "home-link-bar--landing"}`}>
    <Link2 className="shrink-0 text-black/30 dark:text-white/30" size={18}/>
    <input autoFocus={!compact} aria-label="Media link" value={url} onChange={(event) => onUrlChange(event.target.value)} onKeyDown={(event) => event.key === "Enter" && url.trim() && onSubmit()} placeholder={placeholder} className={`${compact ? "h-11" : "h-12"} min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-black/30 dark:placeholder:text-white/25`}/>
    <motion.button whileHover={{ scale: 1.06 }} whileTap={{ scale: 0.9 }} transition={interactionSpring} onClick={onPaste} title="Paste from clipboard" aria-label="Paste from clipboard" className="home-paste-button"><Clipboard size={17}/></motion.button>
    <motion.button whileHover={!url.trim() || busy ? undefined : { y: -1 }} whileTap={!url.trim() || busy ? undefined : { scale: 0.98 }} transition={interactionSpring} onClick={onSubmit} disabled={!url.trim() || busy} className={`${compact ? "h-11" : "h-12"} premium-action shrink-0 rounded-xl bg-lime px-5 text-sm font-bold text-accentForeground disabled:cursor-not-allowed disabled:opacity-40`}>{busy && <span className="button-spinner"/>}{action}</motion.button>
  </motion.div>;
}

function SetupNotice({ dependencies, installing, installProgress, installError, onInstall }: Pick<Props, "dependencies" | "installing" | "installProgress" | "installError" | "onInstall">) {
  if (!dependencies || dependencies.ready) return null;
  return <motion.div layout initial={{ opacity: 0, height: 0, y: -3 }} animate={{ opacity: 1, height: "auto", y: 0 }} exit={{ opacity: 0, height: 0, y: -3 }} transition={layoutSpring} className="mt-3 flex overflow-hidden gap-3 rounded-xl border border-amber-500/20 bg-amber-50 px-4 py-3 text-sm text-amber-900 dark:bg-amber-950/25 dark:text-amber-200">
    <AlertTriangle className="mt-0.5 shrink-0" size={17}/><div className="min-w-0 flex-1">
      <p className="font-semibold">Download engine setup needed</p>
      <p className="mt-0.5 text-xs opacity-75">{dependencies.issues.join(" ")} Development builds also detect tools installed on PATH.</p>
      {installing ? <div className="mt-2.5 space-y-1.5">{Object.values(installProgress).map((item) => <div key={item.tool} className="flex items-center gap-2 text-xs"><span className="w-14 shrink-0 font-mono">{item.tool}</span><div className="h-1.5 flex-1 overflow-hidden rounded-full bg-amber-900/10 dark:bg-white/10"><motion.div className="h-full rounded-full bg-amber-600 dark:bg-amber-300" animate={{ width: `${item.percent}%` }} transition={{ duration: 0.2 }}/></div><span className="w-20 shrink-0 opacity-70">{item.stage === "extracting" ? "extracting..." : item.stage === "done" ? "done" : `${Math.round(item.percent)}%`}</span></div>)}{Object.keys(installProgress).length === 0 && <p className="text-xs opacity-70">Starting...</p>}</div>
        : <button onClick={onInstall} className="mt-2.5 rounded-lg bg-amber-600 px-3 py-1.5 text-xs font-bold text-white transition hover:bg-amber-700 dark:bg-amber-300 dark:text-amber-950 dark:hover:bg-amber-200">Install automatically</button>}
      {installError && <p className="mt-2 text-xs font-medium text-red-700 dark:text-red-300">{installError}</p>}
    </div>
  </motion.div>;
}

export function HomePage(props: Props) {
  const { media, analyzing, error, dependencies, settings, browserTabs } = props;
  const hasDetectedTabs = settings.browserIntegrationEnabled && settings.browserShowTabs && browserTabs.length > 0;
  const workspace = Boolean(media) || analyzing || hasDetectedTabs || Boolean(error) || Boolean(dependencies && !dependencies.ready);

  return <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" transition={pageTransition} className="mx-auto h-[calc(100vh-128px)] max-w-5xl scrollbar-hidden overflow-y-auto px-7 pb-6">
    <AnimatePresence mode="wait" initial={false}>
      {!workspace ? <motion.div key="landing" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -5 }} transition={layoutSpring} className="flex h-full flex-col items-center justify-center pb-20">
        <BrandHeading updateStatus={props.updateStatus}/>
        <div className="mt-8 w-full max-w-3xl"><LinkComposer {...props}/></div>
      </motion.div> : <motion.div key="workspace" initial={{ opacity: 0, y: 5 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={layoutSpring} className="h-full">
        <div className="mb-4"><BrandHeading compact updateStatus={props.updateStatus}/></div>
        <LinkComposer {...props} compact/>
        <AnimatePresence initial={false}>
          <SetupNotice {...props}/>
          {error && <motion.div layout initial={{ opacity: 0, height: 0, y: -3 }} animate={{ opacity: 1, height: "auto", y: 0 }} exit={{ opacity: 0, height: 0, y: -3 }} transition={layoutSpring} className="mt-3 overflow-hidden rounded-xl border border-red-500/15 bg-red-50 px-4 py-3 text-sm text-red-700 dark:bg-red-950/30 dark:text-red-300">{error}</motion.div>}
        </AnimatePresence>
        <AnimatePresence mode="wait" initial={false}>
          {analyzing ? <motion.div key="analyzing" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.14 }}><AnalyzeSkeleton/></motion.div> : media ? <motion.div layout="position" key={media.id} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={layoutSpring} className="mt-4 min-h-0"><MediaCard media={media} settings={settings} busy={false} onDownload={props.onDownload}/></motion.div> : hasDetectedTabs ? <DetectedTabs key="detected-tabs" tabs={browserTabs} settings={settings} onAnalyze={props.onAnalyze} onQuickDownload={props.onQuickDownload}/> : null}
        </AnimatePresence>
      </motion.div>}
    </AnimatePresence>
  </motion.div>;
}
