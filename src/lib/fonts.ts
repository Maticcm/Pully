import type { FontPreference } from "../types/settings";

export const fontStacks: Record<FontPreference, string> = {
  system: '"Segoe UI Variable", "Segoe UI", sans-serif',
  inter: '"Inter Variable", "Inter", sans-serif',
  manrope: '"Manrope Variable", "Manrope", sans-serif',
  "space-grotesk": '"Space Grotesk Variable", "Space Grotesk", sans-serif',
  "ibm-plex": '"IBM Plex Sans Variable", "IBM Plex Sans", sans-serif',
  jetbrains: '"JetBrains Mono Variable", "JetBrains Mono", monospace',
  rounded: '"Trebuchet MS", "Segoe UI", sans-serif',
  classic: 'Georgia, "Times New Roman", serif',
  mono: '"Cascadia Code", "SFMono-Regular", Consolas, monospace',
};
