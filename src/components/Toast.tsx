import { CheckCircle2, X } from "lucide-react";
import { motion } from "motion/react";
import { interactionSpring } from "../lib/animation";

type Props = { message: string; onClose(): void };

export function Toast({ message, onClose }: Props) {
  return <motion.div role="status" initial={{ opacity: 0, x: "-50%", y: 12, scale: 0.97 }} animate={{ opacity: 1, x: "-50%", y: 0, scale: 1 }} exit={{ opacity: 0, x: "-50%", y: 8, scale: 0.98 }} transition={interactionSpring} className="fixed bottom-5 left-1/2 z-[70] flex items-center gap-3 rounded-2xl border border-black/10 bg-surface px-4 py-3 shadow-float dark:border-white/10">
    <span className="grid h-7 w-7 place-items-center rounded-lg bg-lime text-accentForeground"><CheckCircle2 size={16}/></span>
    <span className="whitespace-nowrap text-sm font-semibold">{message}</span>
    <motion.button whileHover={{ scale: 1.06 }} whileTap={{ scale: 0.92 }} transition={interactionSpring} className="ml-1 grid h-7 w-7 place-items-center rounded-lg text-black/35 transition-colors hover:bg-black/[.05] hover:text-black dark:text-white/35 dark:hover:bg-white/[.07] dark:hover:text-white" onClick={onClose} aria-label="Dismiss"><X size={14}/></motion.button>
  </motion.div>;
}
