import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { api } from "../lib/tauri";
import type { DetectedTab } from "../types/media";

/** Tracks the live set of browser tabs Pully has been told about, only while
 * `enabled` — when it flips off, the list is cleared immediately rather than
 * left stale (Pully's own backend also stops tracking anything at that
 * point; see `browser_integration::ipc`'s opt-in gate). */
export function useBrowserTabs(enabled: boolean) {
  const [tabs, setTabs] = useState<DetectedTab[]>([]);

  useEffect(() => {
    if (!enabled) {
      setTabs([]);
      return;
    }
    let disposed = false;
    void api.browserTabs().then((value) => { if (!disposed) setTabs(value); }).catch(() => undefined);
    const unlisten = listen<DetectedTab[]>("browser-tabs-changed", ({ payload }) => setTabs(payload));
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
    };
  }, [enabled]);

  return tabs;
}

export type DetectedTabGroup = { key: string; representative: DetectedTab; count: number };

/** Groups tabs that share the exact same URL (possibly across different
 * browsers) so the UI shows one card instead of a wall of duplicates, while
 * still tracking every underlying tab so closing one duplicate doesn't drop
 * the card while others remain open. */
export function groupByUrl(tabs: DetectedTab[]): DetectedTabGroup[] {
  const groups = new Map<string, DetectedTab[]>();
  for (const tab of tabs) {
    const existing = groups.get(tab.url);
    if (existing) existing.push(tab);
    else groups.set(tab.url, [tab]);
  }
  return [...groups.entries()].map(([url, members]) => {
    const representative = members.find((tab) => tab.active) ?? members[0]!;
    return { key: url, representative, count: members.length };
  });
}
