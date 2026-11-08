import { Download, Search, Video, X } from "lucide-react";
import { memo, useDeferredValue, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { interactionSpring } from "../lib/animation";
import type { PlaylistItem } from "../types/media";

type Props = {
  items: PlaylistItem[];
  selected: Set<number>;
  busy: boolean;
  onToggle(index: number, checked: boolean): void;
  onSelectAll(): void;
  onClearAll(): void;
  onDownload(): void;
};

function duration(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
}

const CollectionRow = memo(function CollectionRow({ item, checked, busy, onToggle }: { item: PlaylistItem; checked: boolean; busy: boolean; onToggle(index: number, checked: boolean): void }) {
  return <button type="button" role="checkbox" aria-checked={checked} aria-label={item.title} disabled={busy} onClick={() => onToggle(item.index, !checked)} className="collection-row">
    <span className="collection-check" aria-hidden="true"><svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><path d="M20 6 9 17l-5-5" pathLength="1"/></svg></span>
    <span className="collection-art" aria-hidden="true">{item.thumbnail ? <img src={item.thumbnail} alt="" loading="lazy" decoding="async"/> : <Video size={18}/>}</span>
    <span className="min-w-0 flex-1"><span className="collection-title">{item.title}</span><span className="collection-index">Video {item.index}</span></span>
    {item.duration != null && <span className="collection-duration">{duration(item.duration)}</span>}
  </button>;
});

export function CollectionPicker({ items, selected, busy, onToggle, onSelectAll, onClearAll, onDownload }: Props) {
  const filterId = useId();
  const reduceMotion = useReducedMotion();
  const listRef = useRef<HTMLDivElement>(null);
  const scrollFrame = useRef<number>();
  const [scrollTop, setScrollTop] = useState(0);
  const [search, setSearch] = useState("");
  const deferredSearch = useDeferredValue(search);
  const [selectedOnly, setSelectedOnly] = useState(false);
  const visible = useMemo(() => {
    const query = deferredSearch.trim().toLocaleLowerCase();
    if (!selectedOnly && !query) return items;
    return items.filter(item => (!selectedOnly || selected.has(item.index)) && item.title.toLocaleLowerCase().includes(query));
  }, [items, selected, selectedOnly, deferredSearch]);
  const previousResults = useRef(visible);
  const resultsChanged = previousResults.current !== visible;
  const rowHeight = 64;
  const viewportHeight = 300;
  const clampedScroll = Math.min(scrollTop, Math.max(0, visible.length * rowHeight - viewportHeight));
  const start = Math.max(0, Math.floor(clampedScroll / rowHeight) - 3);
  const end = Math.min(visible.length, Math.ceil((clampedScroll + viewportHeight) / rowHeight) + 3);
  const rendered = visible.slice(start, end);

  useLayoutEffect(() => {
    previousResults.current = visible;
    if (listRef.current && listRef.current.scrollTop > clampedScroll) {
      listRef.current.scrollTo({ top: clampedScroll, behavior: "instant" });
      setScrollTop(clampedScroll);
    }
  }, [visible, clampedScroll]);
  useEffect(() => () => {
    if (scrollFrame.current != null) cancelAnimationFrame(scrollFrame.current);
  }, []);
  const handleScroll = () => {
    if (scrollFrame.current != null) return;
    scrollFrame.current = requestAnimationFrame(() => {
      scrollFrame.current = undefined;
      setScrollTop(listRef.current?.scrollTop ?? 0);
    });
  };

  return <div className="collection-picker">
    <div className="collection-toolbar">
      <div><h3 className="text-sm font-bold">Choose your videos</h3><p className="mt-1 text-xs text-black/45 dark:text-white/40">Keep everything, or pick just your favourites.</p></div>
      <div className="collection-filters" aria-label="Filter collection">
        <button type="button" aria-pressed={!selectedOnly} onClick={() => setSelectedOnly(false)}>{!selectedOnly && <motion.span layoutId={filterId} transition={reduceMotion ? { duration: 0 } : interactionSpring} className="collection-filter-indicator"/>}All <span className="collection-filter-count">{items.length}</span></button>
        <button type="button" aria-pressed={selectedOnly} onClick={() => setSelectedOnly(true)}>{selectedOnly && <motion.span layoutId={filterId} transition={reduceMotion ? { duration: 0 } : interactionSpring} className="collection-filter-indicator"/>}Selected <span className="collection-filter-count" style={{ minWidth: `${String(items.length).length}ch` }}>{selected.size}</span></button>
      </div>
    </div>
    <div className="collection-search-row">
      <div className="collection-search"><Search size={15} aria-hidden="true"/><input aria-label="Search collection" placeholder="Find a video…" value={search} onChange={event => setSearch(event.target.value)}/>{search && <button type="button" aria-label="Clear search" onClick={() => setSearch("")}><X size={14}/></button>}</div>
      <div className="flex shrink-0 gap-1"><button type="button" disabled={busy} className="collection-text-button" onClick={onSelectAll}>Select all</button><button type="button" disabled={busy || selected.size === 0} className="collection-text-button" onClick={onClearAll}>Clear all</button></div>
    </div>
    <div ref={listRef} className="collection-list scrollbar-hidden" aria-label="Collection items" tabIndex={0} onScroll={handleScroll}>
      <div className="collection-canvas" style={{ height: visible.length * rowHeight }}>
        <AnimatePresence initial={false} custom={resultsChanged}>
          {rendered.map((item, offset) => <motion.div key={item.index} className="collection-row-position" initial={reduceMotion || !resultsChanged ? false : { opacity: 0, y: (start + offset) * rowHeight + 8 }} animate={{ opacity: 1, y: (start + offset) * rowHeight }} variants={{ leave: (changing: boolean) => ({ opacity: 0, scale: reduceMotion ? 1 : .985, transition: { duration: reduceMotion || !changing ? 0 : .18 } }) }} exit="leave" transition={{ duration: reduceMotion ? 0 : .22, ease: [.22, 1, .36, 1] }}>
            <CollectionRow item={item} checked={selected.has(item.index)} busy={busy} onToggle={onToggle}/>
          </motion.div>)}
        </AnimatePresence>
      </div>
      {visible.length === 0 && <motion.div initial={reduceMotion ? false : { opacity: 0, y: 4 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: reduceMotion ? 0 : .2, delay: reduceMotion ? 0 : .08 }} className="collection-empty"><Search size={22}/><p>{search ? "No videos match your search." : "No videos selected yet."}</p><button type="button" onClick={() => { setSearch(""); setSelectedOnly(false); }}>Show all videos</button></motion.div>}
    </div>
    <div className="collection-footer">
      <div><p className="text-sm font-semibold"><motion.span key={selected.size} className="collection-selection-count" style={{ minWidth: `${String(items.length).length}ch` }} initial={reduceMotion ? false : { opacity: 0, y: 3 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: .16 }}>{selected.size}</motion.span> <span className="font-normal text-black/45 dark:text-white/40">of {items.length} selected</span></p><p className="mt-1 text-[11px] text-black/40 dark:text-white/35">{selected.size === 0 ? "Select a video to get started." : "Your format settings apply to every selected video."}</p></div>
      <motion.button type="button" whileHover={busy || selected.size === 0 || reduceMotion ? undefined : { y: -1 }} whileTap={busy || selected.size === 0 || reduceMotion ? undefined : { scale: .98 }} transition={interactionSpring} className="primary-button" disabled={busy || selected.size === 0} onClick={onDownload}><Download size={16}/>{selected.size === items.length ? "Download all" : "Download selected"}</motion.button>
    </div>
  </div>;
}
