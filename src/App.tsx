import { listen } from "@tauri-apps/api/event";
import { Check, DownloadCloud, Link2, Moon, Sun, Zap } from "lucide-react";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import type { DragEvent } from "react";
import { Brand } from "./components/Brand";
import { TitleBar } from "./components/TitleBar";
import { Toast } from "./components/Toast";
import { useBrowserTabs } from "./hooks/useBrowserTabs";
import { useDownloads } from "./hooks/useDownloads";
import { useSettings } from "./hooks/useSettings";
import { api } from "./lib/tauri";
import { buildQuickDownloadRequest } from "./lib/formats";
import { fontStacks } from "./lib/fonts";
import { interactionSpring } from "./lib/animation";
import { HomePage } from "./pages/HomePage";
import { SettingsPage } from "./pages/SettingsPage";
import { DownloadsPage } from "./pages/DownloadsPage";
import type { DependencyInfo, DownloadRequest, MediaInfo, SetupProgress } from "./types/media";

function colorChannels(hex: string) {
  const match = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!match) return { color: "200 241 105", foreground: "25 34 28" };
  const value = Number.parseInt(match[1], 16);
  const red = value >> 16;
  const green = (value >> 8) & 255;
  const blue = value & 255;
  const luminance = (red * 299 + green * 587 + blue * 114) / 1000;
  return { color: `${red} ${green} ${blue}`, foreground: luminance > 145 ? "25 34 28" : "255 255 255" };
}

function webUrlFromText(raw: string) {
  const shortcutUrl = /^URL=(.+)$/im.exec(raw)?.[1]?.trim();
  const candidates = [
    shortcutUrl,
    ...raw.split(/\r?\n/).map((line) => line.trim()).filter((line) => line && !line.startsWith("#")),
    /https?:\/\/[^\s<>"']+/i.exec(raw)?.[0],
  ];

  for (const candidate of candidates) {
    if (!candidate) continue;
    try {
      const parsed = new URL(candidate);
      if (parsed.protocol === "http:" || parsed.protocol === "https:") return parsed.href;
    } catch { /* Keep looking through the remaining drag representations. */ }
  }
  return undefined;
}

async function droppedUrl(dataTransfer: DataTransfer) {
  // Read all synchronous drag data before yielding: WebView data-transfer
  // stores are no longer guaranteed to be readable after the drop callback.
  const entries = Array.from(dataTransfer.types, (type) => ({
    type: type.toLowerCase(),
    value: dataTransfer.getData(type),
  }));
  const files = Array.from(dataTransfer.files);

  for (const type of ["text/uri-list", "text/x-moz-url", "url", "text/plain"]) {
    const match = entries.find((entry) => entry.type === type);
    const link = match && webUrlFromText(match.value);
    if (link) return link;
  }

  const html = entries.find((entry) => entry.type === "text/html")?.value;
  if (html) {
    const href = new DOMParser().parseFromString(html, "text/html").querySelector("a[href]")?.getAttribute("href");
    const link = href && webUrlFromText(href);
    if (link) return link;
  }

  // Chromium-based browsers may expose an address-bar drag as a virtual
  // Windows Internet Shortcut instead of ordinary text.
  for (const file of files) {
    if (!/\.(url|webloc)$/i.test(file.name) && !file.type.startsWith("text/")) continue;
    try {
      const link = webUrlFromText(await file.text());
      if (link) return link;
    } catch { /* An unreadable virtual file is simply not a usable link. */ }
  }
  return undefined;
}

function friendlyError(value: unknown) {
  const text = String(value);
  if (text.includes("InvalidUrl")) return "That doesn't look like a valid web link.";
  if (text.includes("DependencyMissing")) return "Pully's download engine isn't available.";
  if (text.includes("Unsupported")) return "Pully couldn't find downloadable media at this link.";
  return text.replace(/^.*?: /, "") || "Something went wrong while analyzing this link.";
}

export default function App() {
  const [navigation, setNavigation] = useState<{ entries: ("home" | "downloads" | "settings")[]; index: number }>({ entries: ["home"], index: 0 });
  const page = navigation.entries[navigation.index];
  const [url, setUrl] = useState("");
  const [media, setMedia] = useState<MediaInfo>();
  const [analyzing, setAnalyzing] = useState(false);
  const [quickBusy, setQuickBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [dependencies, setDependencies] = useState<DependencyInfo>();
  const [installing, setInstalling] = useState(false);
  const [installProgress, setInstallProgress] = useState<Record<string, SetupProgress>>({});
  const [installError, setInstallError] = useState<string>();
  const [toast, setToast] = useState<string>();
  const [systemDark, setSystemDark] = useState(() => matchMedia("(prefers-color-scheme: dark)").matches);
  const [dropState, setDropState] = useState<"idle" | "hover" | "success">("idle");
  const dragHideTimer = useRef<number | undefined>(undefined);
  const dropSuccessTimer = useRef<number | undefined>(undefined);
  const downloads = useDownloads();
  const { settings, save: saveSettings, reset: resetSettings } = useSettings();
  const browserTabs = useBrowserTabs(settings.browserIntegrationEnabled && settings.browserShowTabs);
  const dark = settings.theme === "dark" || (settings.theme === "system" && systemDark);
  const hasActiveDownloads = downloads.downloads.some((item) => ["waiting", "downloading", "processing"].includes(item.status));

  useEffect(() => {
    const query = matchMedia("(prefers-color-scheme: dark)");
    const update = () => setSystemDark(query.matches);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  useEffect(() => { document.documentElement.classList.toggle("dark", dark); }, [dark]);
  useEffect(() => {
    const root = document.documentElement;
    const accent = colorChannels(settings.accentColor);
    root.dataset.baseColor = settings.baseColor;
    root.style.setProperty("--font-ui", fontStacks[settings.font] ?? fontStacks.manrope);
    root.style.setProperty("--color-accent", accent.color);
    root.style.setProperty("--color-accent-foreground", accent.foreground);
  }, [settings.accentColor, settings.baseColor, settings.font]);
  useEffect(() => {
    // Keeps the browser extension's popup in sync with Pully's actual
    // applied theme (not just its defaults) — pushed on an interval rather
    // than only once, so a newly (re)connected extension picks it up
    // quickly too, without Pully needing to track who's currently connected.
    const pushTheme = () => {
      const styles = getComputedStyle(document.documentElement);
      void api.pushTheme({
        accentColor: styles.getPropertyValue("--color-accent").trim(),
        accentForeground: styles.getPropertyValue("--color-accent-foreground").trim(),
        canvas: styles.getPropertyValue("--color-canvas").trim(),
        surface: styles.getPropertyValue("--color-surface").trim(),
        dark,
        fontFamily: styles.getPropertyValue("--font-ui").trim(),
      }).catch(() => undefined);
    };
    pushTheme();
    const interval = window.setInterval(pushTheme, 5000);
    return () => window.clearInterval(interval);
  }, [dark, settings.accentColor, settings.baseColor, settings.font]);
  useEffect(() => { void api.dependencies().then(setDependencies).catch(() => undefined); }, []);
  useEffect(() => {
    const unlisten = listen<SetupProgress>("setup-progress", ({ payload }) => {
      setInstallProgress((current) => ({ ...current, [payload.tool]: payload }));
    });
    return () => void unlisten.then((fn) => fn());
  }, []);
  const installDependencies = async () => {
    setInstalling(true);
    setInstallError(undefined);
    setInstallProgress({});
    try {
      setDependencies(await api.installDependencies());
    } catch (caught) {
      setInstallError(friendlyError(caught));
    } finally {
      setInstalling(false);
    }
  };
  useEffect(() => {
    if (!toast) return;
    const timeout = window.setTimeout(() => setToast(undefined), 2600);
    return () => window.clearTimeout(timeout);
  }, [toast]);
  const analyze = async (target?: string) => {
    const value = (target ?? url).trim();
    if (!value) return;
    navigate("home");
    setUrl(value);
    setError(undefined);
    setMedia(undefined);
    setAnalyzing(true);
    try { setMedia(await api.analyze(value)); }
    catch (caught) { setError(friendlyError(caught)); }
    finally { setAnalyzing(false); }
  };
  const quickDownload = async (target?: string) => {
    const value = (target ?? url).trim();
    if (!value || quickBusy) return;
    navigate("home");
    setError(undefined);
    setMedia(undefined);
    setUrl("");
    setQuickBusy(true);
    try {
      const info = await api.analyze(value);
      await downloads.start(buildQuickDownloadRequest(info, settings));
      setToast(`Queued "${info.title}"`);
    } catch (caught) {
      setError(friendlyError(caught));
      setUrl(value);
    } finally {
      setQuickBusy(false);
    }
  };
  const submitUrl = () => settings.quickMode ? void quickDownload() : void analyze();
  // Kept fresh every render so the mount-only effect below always calls the
  // latest `analyze`/`quickDownload` (which close over `settings`/`quickBusy`
  // etc.) instead of the stale versions from whenever the effect first ran.
  const analyzeRef = useRef(analyze);
  analyzeRef.current = analyze;
  const quickDownloadRef = useRef(quickDownload);
  quickDownloadRef.current = quickDownload;

  useEffect(() => {
    // A link Pully was launched with directly, via a "Download with Pully"
    // shell action on a .url file, or via pully-native-host launching Pully
    // on the browser's behalf for a Quick Download that arrived while Pully
    // wasn't running (see startup::first_url_from_args / browser_integration::launcher).
    void api.takePendingUrl().then((value) => {
      if (!value) return;
      if (value.quick) void quickDownloadRef.current(value.url);
      else void analyzeRef.current(value.url);
    }).catch(() => undefined);
    // A later relaunch while already running (same shell action) arrives as
    // an event instead, since the frontend is already mounted by then.
    const unlistenExternal = listen<string>("external-url", ({ payload }) => void analyzeRef.current(payload));
    // An explicit "Open in Pully" / "Quick Download" action from the
    // browser extension's popup or context menu.
    const unlistenBrowserOpen = listen<{ url: string; quick: boolean }>("browser-open-request", ({ payload }) => {
      if (payload.quick) void quickDownloadRef.current(payload.url);
      else void analyzeRef.current(payload.url);
    });
    return () => {
      void unlistenExternal.then((fn) => fn());
      void unlistenBrowserOpen.then((fn) => fn());
    };
  }, []);
  const toggleQuickMode = () => { setMedia(undefined); setError(undefined); saveSettings({ ...settings, quickMode: !settings.quickMode }); };
  const paste = async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (/^https?:\/\//i.test(text.trim())) setUrl(text.trim());
    } catch { /* Clipboard permission is optional. */ }
  };
  const toggleTheme = () => saveSettings({ ...settings, theme: dark ? "light" : "dark" });
  const navigate = (target: "home" | "downloads" | "settings") => setNavigation((current) => {
    if (current.entries[current.index] === target) return current;
    return { entries: [...current.entries.slice(0, current.index + 1), target], index: current.index + 1 };
  });
  const goBack = () => setNavigation((current) => ({ ...current, index: Math.max(0, current.index - 1) }));
  const goForward = () => setNavigation((current) => ({ ...current, index: Math.min(current.entries.length - 1, current.index + 1) }));
  const showDownloads = () => navigate("downloads");
  const goHome = () => {
    setUrl("");
    setMedia(undefined);
    setError(undefined);
    navigate("home");
  };
  const queueDownload = async (request: DownloadRequest) => {
    await downloads.start(request);
    setToast("Added to downloads");
    navigate("downloads");
  };
  const savePreferences = (next: Parameters<typeof saveSettings>[0]) => {
    saveSettings(next);
    setToast("Settings saved");
  };
  const isLinkDrag = (dataTransfer: DataTransfer) => {
    const types = Array.from(dataTransfer.types, (type) => type.toLowerCase());
    return ["text/uri-list", "text/x-moz-url", "url", "text/plain", "text/html", "files"]
      .some((type) => types.includes(type));
  };
  const clearDragHide = () => {
    window.clearTimeout(dragHideTimer.current);
    dragHideTimer.current = undefined;
  };
  // The browser fires dragover continuously (many times a second) as long as the
  // cursor is over the window, including while it's over child elements — so this
  // alone drives a single, flicker-free show/hide instead of trying to pair up
  // dragenter/dragleave across every nested element the cursor happens to cross.
  const onDragOver = (event: DragEvent) => {
    if (!isLinkDrag(event.dataTransfer)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    setDropState((state) => (state === "success" ? state : "hover"));
    clearDragHide();
    dragHideTimer.current = window.setTimeout(() => setDropState((state) => (state === "success" ? state : "idle")), 150);
  };
  const onDrop = async (event: DragEvent) => {
    clearDragHide();
    if (!isLinkDrag(event.dataTransfer)) {
      setDropState("idle");
      return;
    }
    event.preventDefault();
    const link = await droppedUrl(event.dataTransfer);
    if (!link) {
      setDropState("idle");
      return;
    }
    setUrl(link);
    navigate("home");
    // A drag that starts outside the window can reach `drop` within the same
    // frame as the first `dragover`, so the "hover" state alone might never get
    // painted. Hold a distinct "success" state for a beat instead of hiding
    // immediately, so the confirmation is always actually seen.
    setDropState("success");
    window.clearTimeout(dropSuccessTimer.current);
    dropSuccessTimer.current = window.setTimeout(() => setDropState("idle"), 260);
  };

  return <MotionConfig reducedMotion="user" transition={interactionSpring}>
    <main onDragOver={onDragOver} onDrop={onDrop} className="relative min-h-screen bg-canvas pt-10 text-ink transition-colors duration-200 dark:text-[#eef2ea]">
      <TitleBar canGoBack={navigation.index > 0} canGoForward={navigation.index < navigation.entries.length - 1} reloadDisabled={hasActiveDownloads} settingsActive={page === "settings"} onBack={goBack} onForward={goForward} onSettings={() => navigate("settings")}/>
      <header className="mx-auto flex max-w-5xl items-center justify-between px-7 py-6">
        <motion.button whileHover={{ y: -1 }} whileTap={{ scale: 0.97 }} transition={interactionSpring} className="brand-button" onClick={goHome} aria-label="Home"><Brand /></motion.button>
        <div className="flex items-center gap-2">
          <motion.button whileHover={{ scale: 1.04 }} whileTap={{ scale: 0.92 }} transition={interactionSpring} className={`header-button ${settings.quickMode ? "bg-lime/25 text-ink dark:text-white" : ""}`} title={settings.quickMode ? "Quick mode on — downloads start instantly with your preset" : "Turn on quick mode"} aria-label="Toggle quick mode" aria-pressed={settings.quickMode} onClick={toggleQuickMode}><Zap size={18} fill={settings.quickMode ? "currentColor" : "none"}/></motion.button>
          <motion.button whileHover={{ scale: 1.04 }} whileTap={{ scale: 0.92 }} transition={interactionSpring} className="header-button" title="Downloads" aria-label="Downloads" onClick={showDownloads}><DownloadCloud size={18}/></motion.button>
          <motion.button whileHover={{ scale: 1.04 }} whileTap={{ scale: 0.92 }} transition={interactionSpring} className="header-button" title="Toggle theme" aria-label="Toggle theme" onClick={toggleTheme}><AnimatePresence mode="wait" initial={false}><motion.span key={dark ? "sun" : "moon"} initial={{ opacity: 0, rotate: -20, scale: 0.8 }} animate={{ opacity: 1, rotate: 0, scale: 1 }} exit={{ opacity: 0, rotate: 20, scale: 0.8 }} transition={{ duration: 0.14 }} className="grid place-items-center">{dark ? <Sun size={18}/> : <Moon size={18}/>}</motion.span></AnimatePresence></motion.button>
        </div>
      </header>

      <AnimatePresence mode="wait" initial={false}>
        {page === "settings" ? <SettingsPage key="settings" settings={settings} dependencies={dependencies} onSave={savePreferences} onReset={resetSettings}/> : page === "downloads" ? <DownloadsPage key="downloads" items={downloads.downloads} onCancel={(id) => void downloads.cancel(id)} onRetry={(id) => void downloads.retry(id)} onRemove={(id) => void downloads.remove(id)} onOpen={(id) => void downloads.openFile(id)} onReveal={(id) => void downloads.revealFile(id)}/> : <HomePage key="home" url={url} analyzing={analyzing} quickBusy={quickBusy} error={error} dependencies={dependencies} installing={installing} installProgress={installProgress} installError={installError} media={media} settings={settings} browserTabs={browserTabs} onUrlChange={setUrl} onSubmit={submitUrl} onPaste={() => void paste()} onInstall={() => void installDependencies()} onAnalyze={(target) => void analyze(target)} onQuickDownload={(target) => void quickDownload(target)} onDownload={queueDownload}/>} 
      </AnimatePresence>
      <AnimatePresence>{toast && <Toast key={toast} message={toast} onClose={() => setToast(undefined)}/>}</AnimatePresence>
      <AnimatePresence>
        {dropState !== "idle" && <motion.div key="drop-overlay" className="pointer-events-none fixed inset-0 z-[80] grid place-items-center bg-canvas/85" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.08, ease: [0.22, 1, 0.36, 1] }}>
          <motion.div layout transition={{ duration: 0.08 }} className={`flex flex-col items-center gap-3 rounded-[28px] border-2 px-14 py-11 shadow-float ${dropState === "success" ? "border-solid border-lime bg-surface" : "border-dashed border-lime/70 bg-surface"}`}>
            <AnimatePresence initial={false}>
              {dropState === "success" ? <motion.div key="success" initial={{ opacity: 0, scale: 0.92 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.08 }} className="flex flex-col items-center gap-3">
                <span className="grid h-12 w-12 place-items-center rounded-2xl bg-lime text-accentForeground"><Check size={22}/></span>
                <p className="text-base font-bold tracking-[-.02em]">Link added</p>
                <p className="text-xs text-black/45 dark:text-white/40">Ready to analyze or download</p>
              </motion.div> : <motion.div key="hover" initial={{ opacity: 0, scale: 0.96 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.08 }} className="flex flex-col items-center gap-3">
                <span className="grid h-12 w-12 place-items-center rounded-2xl bg-lime/25 text-ink dark:text-white"><Link2 size={22}/></span>
                <p className="text-base font-bold tracking-[-.02em]">Drop the link here</p>
                <p className="text-xs text-black/45 dark:text-white/40">Pully will fill it in for you</p>
              </motion.div>}
            </AnimatePresence>
          </motion.div>
        </motion.div>}
      </AnimatePresence>
    </main>
  </MotionConfig>;
}
