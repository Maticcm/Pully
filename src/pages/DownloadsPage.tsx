import { ChevronLeft, ChevronRight } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { DownloadList } from "../components/DownloadList";
import { interactionSpring, pageTransition, pageVariants } from "../lib/animation";
import type { DownloadProgress } from "../types/media";

type Props = {
  items: DownloadProgress[];
  onCancel(id: string): void;
  onRetry(id: string): void;
  onRemove(id: string): void;
  onOpen(id: string): void;
  onReveal(id: string): void;
};

const pageSize = 3;

export function DownloadsPage(props: Props) {
  const [page, setPage] = useState(0);
  const pages = Math.max(1, Math.ceil(props.items.length / pageSize));
  useEffect(() => { if (page >= pages) setPage(pages - 1); }, [page, pages]);
  const visible = useMemo(() => props.items.slice(page * pageSize, (page + 1) * pageSize), [page, props.items]);

  return <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" transition={pageTransition} className="mx-auto flex h-[calc(100vh-128px)] max-w-4xl flex-col px-6 pb-6 pt-2">
    <div className="mb-5 flex items-end justify-between"><div><h1 className="text-3xl font-bold tracking-[-.04em]">Downloads</h1><p className="mt-1 text-sm text-black/45 dark:text-white/40">Your active queue and recently completed files.</p></div><AnimatePresence initial={false}>{props.items.length > pageSize && <motion.div initial={{ opacity: 0, scale: 0.97 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.97 }} className="flex items-center gap-2"><motion.button whileHover={{ scale: 1.04 }} whileTap={{ scale: 0.92 }} transition={interactionSpring} aria-label="Previous page" className="pager-button" disabled={page === 0} onClick={() => setPage((value) => value - 1)}><ChevronLeft size={16}/></motion.button><span className="text-xs tabular-nums text-black/45 dark:text-white/40">{page + 1} / {pages}</span><motion.button whileHover={{ scale: 1.04 }} whileTap={{ scale: 0.92 }} transition={interactionSpring} aria-label="Next page" className="pager-button" disabled={page + 1 === pages} onClick={() => setPage((value) => value + 1)}><ChevronRight size={16}/></motion.button></motion.div>}</AnimatePresence></div>
    <div className="min-h-0 flex-1"><DownloadList items={visible} onCancel={props.onCancel} onRetry={props.onRetry} onRemove={props.onRemove} onOpen={props.onOpen} onReveal={props.onReveal}/></div>
  </motion.div>;
}
