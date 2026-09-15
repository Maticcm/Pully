import type { Browser, ThemeSnapshot } from "../messaging/protocol";

type PopupState = {
  connected: boolean;
  browser: Browser;
  tab?: { title: string; url: string; favIconUrl?: string };
};

const browserLabel: Record<Browser, string> = { chrome: "Chrome", brave: "Brave", edge: "Edge", other: "Browser" };

/** Pully's own fixed light/dark text-color pair (not user-customizable, so
 * unlike accent/canvas/surface/font — which come from the live `theme`
 * snapshot — these two are just hardcoded to match). */
const INK = { light: "25 34 28", dark: "238 242 234" };
const MUTED = { light: "107 113 102", dark: "154 161 146" };
const BORDER = { light: "rgb(0 0 0 / 0.08)", dark: "rgb(255 255 255 / 0.1)" };

/** Applies Pully's actual live theme (accent, canvas/surface, font, and
 * light/dark) to this popup, overriding the static defaults in popup.css.
 * Falls back to those defaults (which already look like Pully's own
 * default theme) when nothing has been received yet — e.g. Pully has never
 * been connected — so the popup never looks broken, just slightly generic. */
function applyTheme(theme: ThemeSnapshot | undefined) {
  if (!theme) return;
  const root = document.documentElement.style;
  root.setProperty("--color-accent", theme.accentColor);
  root.setProperty("--color-accent-foreground", theme.accentForeground);
  root.setProperty("--color-canvas", theme.canvas);
  root.setProperty("--color-surface", theme.surface);
  root.setProperty("--font-ui", theme.fontFamily);
  root.setProperty("--color-ink", theme.dark ? INK.dark : INK.light);
  root.setProperty("--color-muted", theme.dark ? MUTED.dark : MUTED.light);
  root.setProperty("--border", theme.dark ? BORDER.dark : BORDER.light);
}

function domainOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

function byId<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el as T;
}

async function refresh() {
  const titleEl = byId("page-title");
  const domainEl = byId("page-domain");
  const badgeEl = byId("browser-badge");
  const faviconImg = byId<HTMLImageElement>("favicon-img");
  const faviconFallback = byId("favicon-fallback");
  const openBtn = byId<HTMLButtonElement>("open-btn");
  const quickBtn = byId<HTMLButtonElement>("quick-btn");
  const statusDot = byId("status-dot");
  const statusText = byId("status-text");

  const state = (await chrome.runtime.sendMessage({ type: "getPopupState" }).catch(() => undefined)) as
    | PopupState
    | undefined;

  if (!state) {
    statusDot.className = "dot dot-off";
    statusText.textContent = "Pully isn't connected.";
    return;
  }

  badgeEl.textContent = browserLabel[state.browser];
  badgeEl.hidden = false;

  if (state.tab) {
    titleEl.textContent = state.tab.title;
    domainEl.textContent = domainOf(state.tab.url);
    if (state.tab.favIconUrl) {
      faviconImg.src = state.tab.favIconUrl;
      faviconImg.hidden = false;
      faviconFallback.hidden = true;
    } else {
      faviconImg.hidden = true;
      faviconFallback.hidden = false;
    }
    openBtn.disabled = false;
    quickBtn.disabled = false;
  } else {
    titleEl.textContent = "No supported page open";
    domainEl.textContent = "";
    faviconImg.hidden = true;
    faviconFallback.hidden = false;
    openBtn.disabled = true;
    quickBtn.disabled = true;
  }

  if (state.connected) {
    statusDot.className = "dot dot-on";
    statusText.textContent = "Connected to Pully";
  } else {
    statusDot.className = "dot dot-off";
    statusText.textContent = "Pully isn't connected — open the Pully app.";
  }
}

async function act(quick: boolean) {
  await chrome.runtime.sendMessage({ type: "openInPully", quick });
  window.close();
}

byId<HTMLButtonElement>("open-btn").addEventListener("click", () => void act(false));
byId<HTMLButtonElement>("quick-btn").addEventListener("click", () => void act(true));

void chrome.storage.session.get("theme").then((stored) => applyTheme(stored.theme as ThemeSnapshot | undefined));
void refresh();
