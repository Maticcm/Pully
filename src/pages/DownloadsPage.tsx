import { ChevronLeft, ChevronRight } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { DownloadList } from "../components/DownloadList";
import { interactionSpring, pageTransition, pageVariants } from "../lib/animation";
import type { DownloadProgress } from "../types/media";

type Props = {
  items: DownloadProgress[];
  library?: boolean;
  onCancel(id: string): void;
  onRetry(id: string): void;
  onRemove(id: string): void;
  onOpen(id: string): void;
  onReveal(id: string): void;
};

const pageSize = 10;

export function DownloadsPage(props: Props) {
  const [page, setPage] = useState(0);
  const [filter, setFilter] = useState<"all" | "active" | "completed" | "failed">("all");
  const [search, setSearch] = useState("");
  const filtered = useMemo(() => props.items.filter((item) => {
    if (props.library && item.status !== "completed") return false;
    if (!item.title.toLowerCase().includes(search.toLowerCase())) return false;
    if (filter === "active") return ["waiting", "downloading", "processing"].includes(item.status);
    if (filter === "completed") return item.status === "completed";
    if (filter === "failed") return ["failed", "cancelled"].includes(item.status);
    return true;
  }), [props.items, props.library, filter, search]);
  const pages = Math.max(1, Math.ceil(filtered.length / pageSize));
  useEffect(() => { if (page >= pages) setPage(pages - 1); }, [page, pages]);
  const visible = useMemo(() => filtered.slice(page * pageSize, (page + 1) * pageSize), [filtered, page]);

  return <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" transition={pageTransition} className="downloads-page mx-auto flex h-full max-w-5xl flex-col px-6 pb-6 pt-7 sm:px-9">
    <div className="mb-6 flex flex-wrap items-end justify-between gap-4"><div><h1 className="text-2xl font-bold tracking-[-.035em]">{props.library ? "Library" : "Downloads"}</h1><p className="mt-1 text-sm text-black/50 dark:text-white/45">{props.library ? "Completed files from this session. History is not saved after Pully closes." : "Follow your queue and manage completed downloads."}</p></div><AnimatePresence initial={false}>{filtered.length > pageSize && <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} className="flex items-center gap-2"><motion.button whileTap={{ scale: 0.92 }} transition={interactionSpring} aria-label="Previous page" className="pager-button" disabled={page === 0} onClick={() => setPage((value) => value - 1)}><ChevronLeft size={16}/></motion.button><span className="text-xs tabular-nums text-black/45 dark:text-white/40">{page + 1} / {pages}</span><motion.button whileTap={{ scale: 0.92 }} transition={interactionSpring} aria-label="Next page" className="pager-button" disabled={page + 1 === pages} onClick={() => setPage((value) => value + 1)}><ChevronRight size={16}/></motion.button></motion.div>}</AnimatePresence></div>
    <div className="downloads-toolbar"><div className="downloads-filters" aria-label="Filter downloads">{!props.library && ([ ["all", "All"], ["active", "Active"], ["completed", "Completed"], ["failed", "Failed"] ] as const).map(([value, label]) => <button key={value} aria-pressed={filter === value} onClick={() => { setFilter(value); setPage(0); }}>{label}</button>)}</div><input aria-label="Search downloads" placeholder="Search downloads" value={search} onChange={(event) => { setSearch(event.target.value); setPage(0); }}/></div>
    <div className="min-h-0 flex-1 overflow-y-auto pb-4">{visible.length ? <DownloadList items={visible} onCancel={props.onCancel} onRetry={props.onRetry} onRemove={props.onRemove} onOpen={props.onOpen} onReveal={props.onReveal}/> : <div className="downloads-empty"><p>{props.items.length && (search || filter !== "all") ? "No downloads match your filters." : props.library ? "No completed downloads in this session." : "Your download queue is empty."}</p><span>Downloads you start will appear here.</span></div>}</div>
  </motion.div>;
}
