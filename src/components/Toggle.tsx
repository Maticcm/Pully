import * as SwitchPrimitive from "@radix-ui/react-switch";
import { motion } from "motion/react";
import { interactionSpring } from "../lib/animation";

type Props = { checked: boolean; onChange(checked: boolean): void; label: string; description?: string; disabled?: boolean };

export function Toggle({ checked, onChange, label, description, disabled }: Props) {
  return <label className={`flex items-start justify-between gap-4 py-2 ${disabled ? "opacity-45" : "cursor-pointer"}`}>
    <span><span className="block text-sm font-medium">{label}</span>{description && <span className="mt-0.5 block text-xs leading-relaxed text-black/45 dark:text-white/40">{description}</span>}</span>
    <SwitchPrimitive.Root checked={checked} onCheckedChange={onChange} disabled={disabled} className={`relative mt-0.5 h-6 w-11 shrink-0 rounded-full outline-none transition-colors focus-visible:ring-2 focus-visible:ring-lime/35 ${checked ? "bg-lime" : "bg-black/10 dark:bg-white/15"}`}>
      <SwitchPrimitive.Thumb asChild><motion.span animate={{ x: checked ? 20 : 0 }} transition={interactionSpring} className={`absolute left-1 top-1 block h-4 w-4 rounded-full shadow-sm ${checked ? "bg-accentForeground" : "bg-white"}`}/></SwitchPrimitive.Thumb>
    </SwitchPrimitive.Root>
  </label>;
}
