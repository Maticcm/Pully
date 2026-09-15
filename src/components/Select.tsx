import * as SelectPrimitive from "@radix-ui/react-select";
import { Check, ChevronDown } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useMemo, useState } from "react";
import type { CSSProperties } from "react";
import { interactionSpring } from "../lib/animation";

export type SelectOption = { value: string; label: string; detail?: string; style?: CSSProperties };

type Props = {
  value: string;
  options: SelectOption[];
  onChange(value: string): void;
  ariaLabel: string;
  disabled?: boolean;
  columns?: 1 | 2;
};

export function Select({ value, options, onChange, ariaLabel, disabled, columns = 1 }: Props) {
  const [open, setOpen] = useState(false);
  const selected = options.find((option) => option.value === value) ?? options[0];
  const resolvedColumns = useMemo(() => {
    const adaptive = options.length >= 8 ? 3 : options.length >= 5 ? 2 : 1;
    return Math.max(columns, adaptive);
  }, [columns, options.length]);
  const contentStyle = {
    minWidth: "var(--radix-select-trigger-width)",
    width: resolvedColumns === 1 ? "var(--radix-select-trigger-width)" : `${resolvedColumns * 184}px`,
    maxWidth: "calc(100vw - 16px)",
  } as CSSProperties;

  return <div className="custom-select">
    <SelectPrimitive.Root value={selected?.value} onValueChange={onChange} open={open} onOpenChange={setOpen} disabled={disabled}>
      <SelectPrimitive.Trigger aria-label={ariaLabel} className="select-trigger">
        <SelectPrimitive.Value><span className="block min-w-0 truncate" style={selected?.style}>{selected?.label}</span></SelectPrimitive.Value>
        <SelectPrimitive.Icon asChild><ChevronDown size={15} className={`shrink-0 transition-transform duration-150 ${open ? "rotate-180" : ""}`}/></SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <AnimatePresence>
          {open && <SelectPrimitive.Content forceMount asChild position="popper" align="start" sideOffset={6} collisionPadding={8}>
            <motion.div initial={{ opacity: 0, y: -4, scale: 0.985 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: -3, scale: 0.99 }} transition={interactionSpring} style={contentStyle} className="select-menu">
              <SelectPrimitive.Viewport className="grid gap-1 p-1.5" style={{ gridTemplateColumns: `repeat(${resolvedColumns}, minmax(0, 1fr))` }}>
                {options.map((option) => <SelectPrimitive.Item value={option.value} key={option.value} className="select-option">
                  <SelectPrimitive.ItemText><span className="block min-w-0 flex-1 text-left"><span className="block truncate" style={option.style}>{option.label}</span>{option.detail && <span className="mt-0.5 block truncate text-[10px] font-normal opacity-50">{option.detail}</span>}</span></SelectPrimitive.ItemText>
                  <SelectPrimitive.ItemIndicator className="ml-auto shrink-0"><Check size={14}/></SelectPrimitive.ItemIndicator>
                </SelectPrimitive.Item>)}
              </SelectPrimitive.Viewport>
            </motion.div>
          </SelectPrimitive.Content>}
        </AnimatePresence>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  </div>;
}
