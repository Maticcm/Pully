import { AnimatePresence, motion } from "motion/react";
import { Download, Globe, Zap } from "lucide-react";
import { useMemo } from "react";
import { groupByUrl } from "../hooks/useBrowserTabs";
import { interactionSpring, layoutSpring } from "../lib/animation";
import type { AppSettings } from "../types/settings";
import type { BrowserName, DetectedTab } from "../types/media";

type Props = {
  tabs: DetectedTab[];
  settings: AppSettings;
  onAnalyze(url: string): void;
  onQuickDownload(url: string): void;
};

const browserLabel: Record<BrowserName, string> = { chrome: "Chrome", brave: "Brave", edge: "Edge", other: "Browser" };

function domainOf(url: string) {
  try { return new URL(url).hostname.replace(/^www\./, ""); } catch { return url; }
}

export function DetectedTabs({ tabs, settings, onAnalyze, onQuickDownload }: Props) {
  const visible = useMemo(() => {
    const filtered = settings.browserOnlyActiveTab ? tabs.filter((tab) => tab.active) : tabs;
    return groupByUrl(filtered).sort((a, b) => Number(b.representative.active) - Number(a.representative.active));
  }, [tabs, settings.browserOnlyActiveTab]);

  if (visible.length === 0) return null;

  return <motion.div layout initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={layoutSpring} className="mt-6">
    <h2 className="mb-3 text-xs font-semibold uppercase tracking-[.1em] text-black/45 dark:text-white/40">Detected in your browser</h2>
    <div className="space-y-2.5">
      <AnimatePresence initial={false} mode="popLayout">
        {visible.map(({ key, representative, count }) => <motion.article layout key={key} initial={{ opacity: 0, y: 6, scale: 0.99 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, x: 8, scale: 0.98 }} transition={layoutSpring} onClick={() => onAnalyze(representative.url)} role="button" tabIndex={0} onKeyDown={(event) => event.key === "Enter" && onAnalyze(representative.url)} className="download-row flex cursor-pointer items-center gap-3.5 rounded-2xl border border-black/[.07] bg-surface p-3.5 shadow-sm dark:border-white/[.08]">
          <div className="relative grid h-10 w-10 shrink-0 place-items-center overflow-hidden rounded-xl bg-black/[.045] dark:bg-white/[.06]">
            {representative.favIconUrl ? <img src={representative.favIconUrl} alt="" className="h-5 w-5 object-contain" /> : <Globe size={18} className="text-black/35 dark:text-white/35" />}
            {representative.active && <span className="absolute bottom-0.5 right-0.5 h-2 w-2 rounded-full bg-lime ring-2 ring-surface" />}
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-semibold">{representative.title}</p>
            <p className="mt-0.5 flex items-center gap-1.5 truncate text-xs text-black/45 dark:text-white/40">
              <span className="truncate">{domainOf(representative.url)}</span>
              {settings.browserShowBrowserName && <span className="shrink-0 rounded-full bg-black/[.05] px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-black/40 dark:bg-white/[.08] dark:text-white/35">{browserLabel[representative.browser]}</span>}
              {count > 1 && <span className="shrink-0 text-black/35 dark:text-white/30">· Open in {count} tabs</span>}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            <motion.button whileHover={{ scale: 1.06 }} whileTap={{ scale: 0.92 }} transition={interactionSpring} title="Quick download" aria-label="Quick download" onClick={(event) => { event.stopPropagation(); onQuickDownload(representative.url); }} className="icon-button"><Zap size={15}/></motion.button>
            <motion.button whileHover={{ y: -1 }} whileTap={{ scale: 0.97 }} transition={interactionSpring} onClick={(event) => { event.stopPropagation(); onAnalyze(representative.url); }} className="flex h-8 items-center gap-1.5 rounded-lg bg-lime px-3 text-xs font-bold text-accentForeground"><Download size={13}/> Download</motion.button>
          </div>
        </motion.article>)}
      </AnimatePresence>
    </div>
  </motion.div>;
}
