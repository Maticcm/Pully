import type { Transition, Variants } from "motion/react";

export const interactionSpring: Transition = {
  type: "spring",
  stiffness: 520,
  damping: 36,
  mass: 0.7,
};

export const layoutSpring: Transition = {
  type: "spring",
  stiffness: 420,
  damping: 38,
  mass: 0.85,
};

export const pageTransition: Transition = {
  duration: 0.2,
  ease: [0.22, 1, 0.36, 1],
};

export const pageVariants: Variants = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -4 },
};
