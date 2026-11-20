/**
 * The accent the whole window follows. The same seven as the Android app; the
 * CSS presets live in app.css under [data-accent='…'].
 */
export const ACCENTS = [
  { name: "inkblue", label: "Ink blue", light: "#2f5aa8", dark: "#8fb0ea" },
  { name: "teal", label: "Teal", light: "#0f7069", dark: "#6cc9be" },
  { name: "moss", label: "Moss", light: "#4a6e33", dark: "#a3c487" },
  { name: "violet", label: "Violet", light: "#6741c9", dark: "#b29bf5" },
  { name: "plum", label: "Plum", light: "#8a3a66", dark: "#db9bc0" },
  { name: "graphite", label: "Graphite", light: "#3f3f46", dark: "#cfcfd6" },
  { name: "terracotta", label: "Terracotta", light: "#b4471f", dark: "#dd8662" },
] as const;

export type AccentName = (typeof ACCENTS)[number]["name"];

export const DEFAULT_ACCENT: AccentName = "inkblue";

/** Presets from before the redesign, mapped to their nearest new accent. */
const LEGACY: Record<string, AccentName> = {
  purple: "violet",
  blue: "inkblue",
  green: "moss",
  pink: "plum",
  orange: "terracotta",
};

/** A stored accent name as one this version knows, defaulting to ink blue. */
export function normalizeAccent(stored: string | null | undefined): AccentName {
  if (!stored) return DEFAULT_ACCENT;
  if (ACCENTS.some((a) => a.name === stored)) return stored as AccentName;
  return LEGACY[stored] ?? DEFAULT_ACCENT;
}
