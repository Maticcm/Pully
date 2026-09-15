export function AnalyzeSkeleton() {
  return <div className="animate-rise mt-5 overflow-hidden rounded-[28px] border border-black/[.06] bg-surface p-5 shadow-sm dark:border-white/[.08]">
    <div className="flex gap-5"><div className="skeleton h-32 w-48 shrink-0 rounded-2xl"/><div className="flex flex-1 flex-col justify-center"><div className="skeleton h-3 w-20 rounded-full"/><div className="skeleton mt-4 h-5 w-4/5 rounded-full"/><div className="skeleton mt-2 h-4 w-2/5 rounded-full"/><div className="mt-6 flex gap-3"><div className="skeleton h-10 flex-1 rounded-xl"/><div className="skeleton h-10 flex-1 rounded-xl"/></div></div></div>
    <div className="mt-4 flex items-center gap-2 text-xs font-medium text-black/40 dark:text-white/35"><span className="scan-dot"/>Reading available media and formats...</div>
  </div>;
}
