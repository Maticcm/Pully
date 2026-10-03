import * as CheckboxPrimitive from "@radix-ui/react-checkbox";
import { Check } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { interactionSpring } from "../lib/animation";

type Props = { checked: boolean; onChange(checked: boolean): void; children: ReactNode; disabled?: boolean };

export function Checkbox({ checked, onChange, children, disabled = false }: Props) {
  return <label className={`group flex items-center gap-2.5 py-0.5 text-left text-xs text-black/60 transition-colors dark:text-white/55 ${disabled ? "cursor-not-allowed opacity-45" : "cursor-pointer hover:text-black dark:hover:text-white"}`}>
    <CheckboxPrimitive.Root checked={checked} disabled={disabled} onCheckedChange={(value) => onChange(value === true)} className={`grid h-[18px] w-[18px] shrink-0 place-items-center rounded-[6px] border outline-none transition-colors focus-visible:ring-2 focus-visible:ring-lime/35 ${checked ? "border-lime bg-lime text-accentForeground" : "border-black/15 bg-black/[.025] group-hover:border-black/30 dark:border-white/20 dark:bg-white/[.04] dark:group-hover:border-white/35"}`}>
      <CheckboxPrimitive.Indicator forceMount asChild>
        <span><AnimatePresence initial={false}>{checked && <motion.span className="grid place-items-center" initial={{ opacity: 0, scale: 0.55 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.7 }} transition={interactionSpring}><Check size={12} strokeWidth={3}/></motion.span>}</AnimatePresence></span>
      </CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
    <span className="flex items-center gap-1.5">{children}</span>
  </label>;
}
