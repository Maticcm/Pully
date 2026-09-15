import * as Progress from "@radix-ui/react-progress";
import { ArrowDownToLine, Check, CircleX, FolderOpen, LoaderCircle, RotateCcw, Trash2, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import { interactionSpring, layoutSpring } from "../lib/animation";
import type { DownloadProgress } from "../types/media";

type Props = { items: DownloadProgress[]; onCancel(id: string): void; onRetry(id: string): void; onRemove(id: string): void; onOpen(id: string): void; onReveal(id: string): void };

const statusLabel: Record<DownloadProgress["status"], string> = { waiting: "Waiting", downloading: "Downloading", processing: "Processing", completed: "Completed", failed: "Failed", cancelled: "Cancelled" };

function bytes(value?: number) {
  if (value == null) return "";
  const units = ["B", "KB", "MB", "GB"]; let n = value; let i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i += 1; }
  return `${n.toFixed(i ? 1 : 0)} ${units[i]}`;
}

function ActionButton({ title, onClick, children }: { title: string; onClick(): void; children: ReactNode }) {
  return <motion.button layout whileHover={{ scale: 1.06 }} whileTap={{ scale: 0.9 }} transition={interactionSpring} className="icon-button" title={title} aria-label={title} onClick={onClick}>{children}</motion.button>;
}

function menuItems(item: DownloadProgress, handlers: Pick<Props, "onCancel" | "onRetry" | "onRemove" | "onOpen" | "onReveal">): ContextMenuItem[] {
  const items: ContextMenuItem[] = [];
  if (item.status === "completed") {
    items.push({ key: "open", label: "Open file", icon: <Check size={14}/>, onSelect: () => handlers.onOpen(item.id) });
    items.push({ key: "reveal", label: "Show in folder", icon: <FolderOpen size={14}/>, onSelect: () => handlers.onReveal(item.id) });
  }
  if (item.status === "failed" || item.status === "cancelled") {
    items.push({ key: "retry", label: "Retry", icon: <RotateCcw size={14}/>, onSelect: () => handlers.onRetry(item.id) });
  }
  if (item.status === "downloading" || item.status === "waiting") {
    items.push({ key: "cancel", label: "Cancel", icon: <X size={14}/>, onSelect: () => handlers.onCancel(item.id), destructive: true });
  }
  if (!(["downloading", "processing", "waiting"] as DownloadProgress["status"][]).includes(item.status)) {
    items.push({ key: "remove", label: "Remove", icon: <Trash2 size={14}/>, onSelect: () => handlers.onRemove(item.id), destructive: true });
  }
  return items;
}

export function DownloadList({ items, onCancel, onRetry, onRemove, onOpen, onReveal }: Props) {
  if (!items.length) return <motion.div initial={{ opacity: 0, scale: 0.99 }} animate={{ opacity: 1, scale: 1 }} transition={layoutSpring} className="rounded-3xl border border-dashed border-black/10 px-6 py-10 text-center dark:border-white/10"><span className="mx-auto grid h-12 w-12 place-items-center rounded-2xl bg-lime/25 text-ink dark:text-white"><ArrowDownToLine size={21}/></span><p className="mt-3 text-sm font-semibold">Nothing downloading yet</p><p className="mt-1 text-xs text-black/40 dark:text-white/35">Analyze a link and your queue will appear here.</p></motion.div>;

  return <motion.div layout className="space-y-3">
    <AnimatePresence initial={false} mode="popLayout">
      {items.map((item) => <ContextMenu key={item.id} items={menuItems(item, { onCancel, onRetry, onRemove, onOpen, onReveal })}><motion.article layout initial={{ opacity: 0, y: 8, scale: 0.992 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, x: 10, scale: 0.985 }} transition={layoutSpring} className="download-row rounded-2xl border border-black/[.07] bg-surface p-4 shadow-sm dark:border-white/[.08]">
        <div className="flex items-center gap-4">
          <div className="h-14 w-20 shrink-0 overflow-hidden rounded-xl bg-black/5 dark:bg-white/10">{item.thumbnail && <img src={item.thumbnail} className="h-full w-full object-cover" />}</div>
          <div className="min-w-0 flex-1">
            <div className="flex items-start justify-between gap-4"><div className="min-w-0"><p className="truncate text-sm font-semibold">{item.title}</p><AnimatePresence mode="wait" initial={false}><motion.p key={item.status} initial={{ opacity: 0, y: 2 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -2 }} transition={{ duration: 0.13 }} className="mt-1 flex items-center gap-1.5 text-xs text-black/50 dark:text-white/45">{item.status === "completed" ? <Check size={13} /> : item.status === "failed" ? <CircleX size={13} /> : <LoaderCircle size={13} className={item.status === "downloading" || item.status === "processing" ? "animate-spin" : ""} />}{statusLabel[item.status]}{item.speed && ` · ${item.speed}`}{item.eta && ` · ${item.eta} left`}</motion.p></AnimatePresence></div><motion.span key={Math.round(item.percent)} initial={{ opacity: 0.65 }} animate={{ opacity: 1 }} className="text-sm font-semibold tabular-nums">{Math.round(item.percent)}%</motion.span></div>
            <Progress.Root value={item.percent} max={100} aria-label={`${item.title} download progress`} className="relative mt-3 h-1.5 overflow-hidden rounded-full bg-black/[.06] dark:bg-white/10">
              <Progress.Indicator asChild><motion.div className={`h-full origin-left rounded-full ${item.status === "failed" ? "bg-red-500" : "bg-lime"}`} initial={false} animate={{ scaleX: Math.max(0, Math.min(100, item.percent)) / 100 }} transition={{ type: "spring", stiffness: 180, damping: 28, mass: 0.8 }}/></Progress.Indicator>
              {item.status === "processing" && <motion.div aria-hidden className="absolute inset-y-0 w-1/3 rounded-full bg-lime/70" initial={{ x: "-100%" }} animate={{ x: "300%" }} transition={{ duration: 1.1, repeat: Infinity, ease: "easeInOut" }}/>}
            </Progress.Root>
            <div className="mt-2 flex items-center justify-between"><span className="text-[11px] text-black/40 dark:text-white/35">{bytes(item.downloadedBytes)}{item.totalBytes ? ` of ${bytes(item.totalBytes)}` : ""}</span><motion.div layout className="flex gap-1">
              <AnimatePresence initial={false} mode="popLayout">
                {(item.status === "downloading" || item.status === "waiting") && <ActionButton key="cancel" title="Cancel" onClick={() => onCancel(item.id)}><X size={15}/></ActionButton>}
                {(item.status === "failed" || item.status === "cancelled") && <ActionButton key="retry" title="Retry" onClick={() => onRetry(item.id)}><RotateCcw size={14}/></ActionButton>}
                {item.status === "completed" && <ActionButton key="open" title="Open file" onClick={() => onOpen(item.id)}><Check size={14}/></ActionButton>}
                {item.status === "completed" && <ActionButton key="reveal" title="Show in folder" onClick={() => onReveal(item.id)}><FolderOpen size={14}/></ActionButton>}
                {!(["downloading", "processing", "waiting"] as DownloadProgress["status"][]).includes(item.status) && <ActionButton key="remove" title="Remove" onClick={() => onRemove(item.id)}><Trash2 size={14}/></ActionButton>}
              </AnimatePresence>
            </motion.div></div>
            <AnimatePresence initial={false}>{item.error && <motion.details initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }} className="mt-2 overflow-hidden text-xs text-red-600"><summary className="cursor-pointer">Technical details</summary><p className="mt-1 whitespace-pre-wrap rounded-lg bg-red-50 p-2 dark:bg-red-950/30">{item.error}</p></motion.details>}</AnimatePresence>
          </div>
        </div>
      </motion.article></ContextMenu>)}
    </AnimatePresence>
  </motion.div>;
}
