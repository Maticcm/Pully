import * as ContextMenuPrimitive from "@radix-ui/react-context-menu";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import type { ReactNode } from "react";
import { interactionSpring } from "../lib/animation";

export type ContextMenuItem = {
  key: string;
  label: string;
  icon?: ReactNode;
  onSelect(): void;
  disabled?: boolean;
  destructive?: boolean;
};

type Props = { items: ContextMenuItem[]; children: ReactNode };

export function ContextMenu({ items, children }: Props) {
  const [open, setOpen] = useState(false);
  return <ContextMenuPrimitive.Root onOpenChange={setOpen}>
    <ContextMenuPrimitive.Trigger asChild>{children}</ContextMenuPrimitive.Trigger>
    <ContextMenuPrimitive.Portal>
      <AnimatePresence>
        {open && <ContextMenuPrimitive.Content forceMount asChild collisionPadding={8} alignOffset={4}>
          <motion.div initial={{ opacity: 0, scale: 0.96 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.97 }} transition={interactionSpring} className="select-menu z-[100] min-w-[170px] p-1.5">
            {items.map((item) => <ContextMenuPrimitive.Item key={item.key} disabled={item.disabled} onSelect={item.onSelect} className={`select-option cursor-default data-[disabled]:pointer-events-none data-[disabled]:opacity-40 ${item.destructive ? "data-[highlighted]:!bg-red-500/10 data-[highlighted]:!text-red-600 dark:data-[highlighted]:!text-red-400" : ""}`}>
              {item.icon && <span className="grid h-4 w-4 shrink-0 place-items-center opacity-70">{item.icon}</span>}
              <span className="flex-1 text-left">{item.label}</span>
            </ContextMenuPrimitive.Item>)}
          </motion.div>
        </ContextMenuPrimitive.Content>}
      </AnimatePresence>
    </ContextMenuPrimitive.Portal>
  </ContextMenuPrimitive.Root>;
}
