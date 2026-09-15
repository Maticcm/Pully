import * as PopoverPrimitive from "@radix-ui/react-popover";
import { ChevronDown } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { interactionSpring } from "../lib/animation";
import { hexToHsv, hsvToHex, hueToHex, type Hsv } from "../lib/color";

type Props = { value: string; onChange(hex: string): void; ariaLabel: string };

const hueTrack = "linear-gradient(to right, #f00 0%, #ff0 17%, #0f0 33%, #0ff 50%, #00f 67%, #f0f 83%, #f00 100%)";

function clamp01(value: number) {
  return Math.max(0, Math.min(1, value));
}

export function ColorPicker({ value, onChange, ariaLabel }: Props) {
  const [open, setOpen] = useState(false);
  const [hsv, setHsv] = useState<Hsv>(() => hexToHsv(value) ?? { h: 90, s: 0.6, v: 0.9 });
  const [hexDraft, setHexDraft] = useState(value);
  const svRef = useRef<HTMLDivElement>(null);
  const hueRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) {
      const parsed = hexToHsv(value);
      if (parsed) setHsv(parsed);
      setHexDraft(value);
    }
  }, [open, value]);

  const commit = (next: Hsv) => {
    setHsv(next);
    const hex = hsvToHex(next);
    setHexDraft(hex);
    onChange(hex);
  };

  const dragSv = (event: ReactPointerEvent<HTMLDivElement>) => {
    const el = svRef.current;
    if (!el) return;
    el.setPointerCapture(event.pointerId);
    const update = (clientX: number, clientY: number) => {
      const rect = el.getBoundingClientRect();
      const s = clamp01((clientX - rect.left) / rect.width);
      const v = 1 - clamp01((clientY - rect.top) / rect.height);
      commit({ ...hsv, s, v });
    };
    update(event.clientX, event.clientY);
    const move = (moveEvent: PointerEvent) => update(moveEvent.clientX, moveEvent.clientY);
    const stop = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", stop);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", stop);
  };

  const dragHue = (event: ReactPointerEvent<HTMLDivElement>) => {
    const el = hueRef.current;
    if (!el) return;
    el.setPointerCapture(event.pointerId);
    const update = (clientX: number) => {
      const rect = el.getBoundingClientRect();
      const h = clamp01((clientX - rect.left) / rect.width) * 360;
      commit({ ...hsv, h });
    };
    update(event.clientX);
    const move = (moveEvent: PointerEvent) => update(moveEvent.clientX);
    const stop = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", stop);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", stop);
  };

  const submitHex = (raw: string) => {
    setHexDraft(raw);
    const parsed = hexToHsv(raw);
    if (parsed) {
      setHsv(parsed);
      onChange(hsvToHex(parsed));
    }
  };

  const swatchHex = hsvToHex(hsv);

  return <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
    <PopoverPrimitive.Trigger asChild>
      <button type="button" aria-label={ariaLabel} className="flex h-10 items-center gap-2 rounded-xl border border-black/10 px-3 text-xs font-semibold outline-none transition-colors hover:border-black/25 focus-visible:ring-2 focus-visible:ring-lime/30 dark:border-white/10 dark:hover:border-white/25">
        <span className="h-5 w-5 rounded-lg border border-black/10 dark:border-white/15" style={{ backgroundColor: value }}/>
        Custom
        <span className="font-mono text-black/40 dark:text-white/35">{value}</span>
        <ChevronDown size={13} className={`text-black/30 transition-transform dark:text-white/30 ${open ? "rotate-180" : ""}`}/>
      </button>
    </PopoverPrimitive.Trigger>
    <PopoverPrimitive.Portal>
      <AnimatePresence>
        {open && <PopoverPrimitive.Content forceMount asChild align="start" sideOffset={8} collisionPadding={8}>
          <motion.div initial={{ opacity: 0, y: -4, scale: 0.98 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: -3, scale: 0.99 }} transition={interactionSpring} className="select-menu z-[100] w-60 p-3.5">
            <div ref={svRef} onPointerDown={dragSv} className="relative h-36 w-full touch-none overflow-hidden rounded-xl" style={{ backgroundColor: hueToHex(hsv.h), cursor: "crosshair" }}>
              <div className="absolute inset-0" style={{ background: "linear-gradient(to right, #fff, rgba(255,255,255,0))" }}/>
              <div className="absolute inset-0" style={{ background: "linear-gradient(to top, #000, rgba(0,0,0,0))" }}/>
              <span className="pointer-events-none absolute h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,.25)]" style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%`, backgroundColor: swatchHex }}/>
            </div>
            <div ref={hueRef} onPointerDown={dragHue} className="relative mt-3 h-3.5 w-full touch-none rounded-full" style={{ background: hueTrack, cursor: "pointer" }}>
              <span className="pointer-events-none absolute top-1/2 h-4 w-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,.25)]" style={{ left: `${(hsv.h / 360) * 100}%`, backgroundColor: hueToHex(hsv.h) }}/>
            </div>
            <div className="mt-3 flex items-center gap-2">
              <span className="h-8 w-8 shrink-0 rounded-lg border border-black/10 dark:border-white/15" style={{ backgroundColor: swatchHex }}/>
              <input value={hexDraft} onChange={(event) => submitHex(event.target.value)} onBlur={() => setHexDraft(swatchHex)} spellCheck={false} className="h-8 min-w-0 flex-1 rounded-lg border border-black/[.09] bg-black/[.025] px-2 font-mono text-xs outline-none focus:border-lime dark:border-white/10 dark:bg-white/[.045]"/>
            </div>
          </motion.div>
        </PopoverPrimitive.Content>}
      </AnimatePresence>
    </PopoverPrimitive.Portal>
  </PopoverPrimitive.Root>;
}
