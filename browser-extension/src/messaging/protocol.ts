// Mirrors src-tauri/src/browser_integration/messages.rs. Keep the two in
// sync by hand — there's no generator for this yet (see
// docs/BROWSER_INTEGRATION.md). `chrome.runtime.connectNative` handles
// Chromium's length-prefixed native-messaging framing itself; the extension
// only ever deals with plain objects shaped like `Envelope`.

export const PROTOCOL_VERSION = 1;
export const NATIVE_HOST_ID = "com.pully.native_host";

/** Hard caps mirrored from the Rust side — enforced again there regardless. */
export const LIMITS = {
  maxUrlLength: 4096,
  maxTitleLength: 300,
  maxTrackedTabs: 500,
};

export type Browser = "chrome" | "brave" | "edge" | "other";

export type TabRecord = {
  tabId: string;
  windowId?: string;
  url: string;
  title: string;
  active: boolean;
  favIconUrl?: string;
};

export type Envelope =
  | { type: "hello"; protocolVersion: number; browser: Browser; browserInstanceId: string; extensionVersion: string }
  | { type: "ping"; protocolVersion: number }
  | { type: "pong"; protocolVersion: number }
  | { type: "requestState"; protocolVersion: number }
  | { type: "browserState"; protocolVersion: number; browserInstanceId: string; browser: Browser; tabs: TabRecord[] }
  | { type: "tabAdded"; protocolVersion: number; browserInstanceId: string; browser: Browser; tab: TabRecord }
  | { type: "tabUpdated"; protocolVersion: number; browserInstanceId: string; browser: Browser; tab: TabRecord }
  | { type: "tabRemoved"; protocolVersion: number; browserInstanceId: string; tabId: string }
  | { type: "activeTabChanged"; protocolVersion: number; browserInstanceId: string; tabId: string }
  | { type: "openInPully"; protocolVersion: number; url: string; quick: boolean }
  | { type: "protocolMismatch"; protocolVersion: number; message: string }
  | {
      type: "themeUpdate";
      protocolVersion: number;
      accentColor: string;
      accentForeground: string;
      canvas: string;
      surface: string;
      dark: boolean;
      fontFamily: string;
    };

/** The subset of a `themeUpdate` message the popup actually renders with —
 * stored as-is in `chrome.storage.session` so the popup (a separate
 * document/context) can read the last one received without needing a live
 * round trip to the background service worker. */
export type ThemeSnapshot = Omit<Extract<Envelope, { type: "themeUpdate" }>, "type" | "protocolVersion">;

/** Only http/https ever leave the extension — everything else (chrome://,
 * edge://, about:, file://, chrome-extension://, etc.) is filtered here,
 * before a message is even built, and re-validated again on Pully's side. */
export function isTrackableUrl(url: string | undefined): url is string {
  if (!url) return false;
  return (url.startsWith("http://") || url.startsWith("https://")) && url.length <= LIMITS.maxUrlLength;
}

export function isSafeFavicon(url: string | undefined): url is string {
  if (!url) return false;
  return url.startsWith("http://") || url.startsWith("https://") || url.startsWith("data:image/");
}

/** A cheap, deliberately non-exhaustive heuristic for "this tab is likely
 * something Pully can download" — used only to decide what shows up under
 * "Detected in your browser" (passive, automatic). It intentionally does
 * NOT gate explicit actions (right-click "Open in Pully", the popup's
 * buttons, or a manually pasted link) — those always go through Pully's
 * real provider system and let it decide, same as any other link. Without
 * this filter, every open http(s) tab (settings pages, docs, unrelated
 * sites) would clutter the detected list, since actually asking yt-dlp
 * about every open tab is explicitly too expensive to do automatically.
 * Extend this list freely; it only ever needs to be "reasonably broad", not
 * a complete mirror of every site yt-dlp/SpotiFLAC actually supports. */
const LIKELY_MEDIA_HOSTS = [
  "youtube.com",
  "youtu.be",
  "music.youtube.com",
  "vimeo.com",
  "dailymotion.com",
  "twitch.tv",
  "clips.twitch.tv",
  "soundcloud.com",
  "bandcamp.com",
  "spotify.com",
  "mixcloud.com",
  "audiomack.com",
  "tiktok.com",
  "instagram.com",
  "facebook.com",
  "fb.watch",
  "twitter.com",
  "x.com",
  "reddit.com",
  "redd.it",
  "streamable.com",
  "rumble.com",
  "odysee.com",
  "bilibili.com",
  "vk.com",
  "pinterest.com",
  "snapchat.com",
  "linkedin.com",
  "tumblr.com",
];

export function isLikelyMediaHost(url: string): boolean {
  let hostname: string;
  try {
    hostname = new URL(url).hostname.toLowerCase();
  } catch {
    return false;
  }
  return LIKELY_MEDIA_HOSTS.some((host) => hostname === host || hostname.endsWith(`.${host}`));
}
