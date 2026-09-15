// The extension's only job: track which http(s) tabs are currently open and
// relay that (plus explicit "open in Pully" actions) to the native
// messaging host. It never fetches page content, never downloads anything,
// and never talks to any server other than the local native host.
import {
  Browser,
  Envelope,
  isLikelyMediaHost,
  isSafeFavicon,
  isTrackableUrl,
  LIMITS,
  NATIVE_HOST_ID,
  PROTOCOL_VERSION,
  TabRecord,
  ThemeSnapshot,
} from "../messaging/protocol";

let port: chrome.runtime.Port | null = null;
let connected = false;
let browserInstanceId = "";
let browserFamily: Browser = "chrome";
let reconnectDelay = 1000;
let lastPongAt = 0;
let pingTimer: ReturnType<typeof setInterval> | undefined;
let resyncTimer: ReturnType<typeof setInterval> | undefined;

const trackedTabs = new Map<number, TabRecord>();
const pendingUpdates = new Map<number, ReturnType<typeof setTimeout>>();

async function detectBrowser(): Promise<Browser> {
  // Brave intentionally makes itself hard to fingerprint, so the official
  // (best-effort) way to detect it is its own `navigator.brave.isBrave()`
  // API. Everything else falls back to user-agent sniffing, which Edge and
  // Chrome both support reliably enough for a cosmetic label.
  const nav = navigator as Navigator & { brave?: { isBrave: () => Promise<boolean> } };
  try {
    if (nav.brave && (await nav.brave.isBrave())) return "brave";
  } catch {
    // Not Brave, or the API changed shape — fall through to UA sniffing.
  }
  const ua = navigator.userAgent;
  if (ua.includes("Edg/")) return "edge";
  if (ua.includes("Chrome/")) return "chrome";
  return "other";
}

async function getInstanceId(): Promise<string> {
  // `storage.session` lives only for the current browser session (cleared
  // on browser close, never written to disk) and survives this MV3 service
  // worker being killed and restarted while the browser stays open.
  const stored = await chrome.storage.session.get("browserInstanceId");
  if (typeof stored.browserInstanceId === "string") return stored.browserInstanceId;
  const id = crypto.randomUUID();
  await chrome.storage.session.set({ browserInstanceId: id });
  return id;
}

function send(envelope: Envelope) {
  if (!port) return;
  try {
    port.postMessage(envelope);
  } catch {
    // The port died between our null-check and this call; onDisconnect will
    // fire and drive reconnection — nothing else to do here.
  }
}

function toTabRecord(tab: chrome.tabs.Tab): TabRecord | undefined {
  if (tab.id == null || !isTrackableUrl(tab.url) || !isLikelyMediaHost(tab.url)) return undefined;
  return {
    tabId: String(tab.id),
    windowId: tab.windowId != null ? String(tab.windowId) : undefined,
    url: tab.url,
    title: (tab.title ?? "").slice(0, LIMITS.maxTitleLength),
    active: Boolean(tab.active),
    favIconUrl: isSafeFavicon(tab.favIconUrl) ? tab.favIconUrl : undefined,
  };
}

function sendTabUpsert(tab: chrome.tabs.Tab) {
  if (tab.id == null) return;
  const id = tab.id;
  const record = toTabRecord(tab);
  const wasTracked = trackedTabs.has(id);
  if (!record) {
    // The tab navigated to something we don't track (e.g. an internal
    // page) — if we'd previously reported it, tell Pully it's gone.
    if (wasTracked) {
      trackedTabs.delete(id);
      send({ type: "tabRemoved", protocolVersion: PROTOCOL_VERSION, browserInstanceId, tabId: String(id) });
    }
    return;
  }
  if (!wasTracked && trackedTabs.size >= LIMITS.maxTrackedTabs) return;
  trackedTabs.set(id, record);
  send({
    type: wasTracked ? "tabUpdated" : "tabAdded",
    protocolVersion: PROTOCOL_VERSION,
    browserInstanceId,
    browser: browserFamily,
    tab: record,
  });
}

/** Coalesces the handful of onUpdated events one navigation tends to fire
 * (url change, then title, then favicon, then status=complete) into a
 * single outgoing message instead of one per event. */
function scheduleUpdate(tab: chrome.tabs.Tab) {
  if (tab.id == null) return;
  const id = tab.id;
  const existing = pendingUpdates.get(id);
  if (existing) clearTimeout(existing);
  pendingUpdates.set(
    id,
    setTimeout(() => {
      pendingUpdates.delete(id);
      sendTabUpsert(tab);
    }, 150),
  );
}

async function sendFullState() {
  const tabs = await chrome.tabs.query({});
  trackedTabs.clear();
  const records: TabRecord[] = [];
  for (const tab of tabs) {
    if (records.length >= LIMITS.maxTrackedTabs) break;
    const record = toTabRecord(tab);
    if (record && tab.id != null) {
      records.push(record);
      trackedTabs.set(tab.id, record);
    }
  }
  send({
    type: "browserState",
    protocolVersion: PROTOCOL_VERSION,
    browserInstanceId,
    browser: browserFamily,
    tabs: records,
  });
}

function connect() {
  if (port) return;
  try {
    port = chrome.runtime.connectNative(NATIVE_HOST_ID);
  } catch {
    scheduleReconnect();
    return;
  }
  port.onMessage.addListener((message: Envelope) => {
    if (message.type === "pong") {
      lastPongAt = Date.now();
      connected = true;
    } else if (message.type === "requestState") {
      // pully-native-host sends this the instant its own connection to
      // Pully comes up (including right after Pully starts) — resync
      // immediately instead of waiting on the next tab event.
      void sendFullState();
    } else if (message.type === "themeUpdate") {
      // Stored, not applied here — the popup (a separate document that only
      // exists while open) reads this itself when it opens.
      const theme: ThemeSnapshot = {
        accentColor: message.accentColor,
        accentForeground: message.accentForeground,
        canvas: message.canvas,
        surface: message.surface,
        dark: message.dark,
        fontFamily: message.fontFamily,
      };
      void chrome.storage.session.set({ theme });
    } else if (message.type === "protocolMismatch") {
      console.warn(`[Pully] ${message.message}`);
    }
  });
  port.onDisconnect.addListener(() => {
    void chrome.runtime.lastError; // Nothing actionable, just acknowledged.
    port = null;
    connected = false;
    scheduleReconnect();
  });
  send({
    type: "hello",
    protocolVersion: PROTOCOL_VERSION,
    browser: browserFamily,
    browserInstanceId,
    extensionVersion: chrome.runtime.getManifest().version,
  });
  // Ping right away instead of waiting for the first scheduled tick (up to
  // 15s away) — a real pong from a local process comes back in milliseconds,
  // so this is what makes "Connected to Pully" accurate almost immediately
  // after connecting rather than eventually.
  send({ type: "ping", protocolVersion: PROTOCOL_VERSION });
  reconnectDelay = 1000;
  void sendFullState();
}

function scheduleReconnect() {
  setTimeout(connect, reconnectDelay);
  reconnectDelay = Math.min(reconnectDelay * 2, 30_000);
}

/** A lightweight liveness check for the native host process — not a
 * round-trip through Pully itself (see pully-native-host, which answers
 * Ping locally). Runs on an interval, not a tight poll, and does nothing
 * when there's no port to check. */
function startPingLoop() {
  if (pingTimer) return;
  pingTimer = setInterval(() => {
    if (!port) return;
    send({ type: "ping", protocolVersion: PROTOCOL_VERSION });
    if (lastPongAt && Date.now() - lastPongAt > 45_000) {
      port.disconnect();
      port = null;
      connected = false;
      connect();
    }
  }, 15_000);
}

/** A safety net on top of the immediate `requestState`-triggered resync —
 * covers the unlikely case of a dropped/lost message — without adding
 * meaningful chatter (one small snapshot roughly once a minute). */
function startResyncLoop() {
  if (resyncTimer) return;
  resyncTimer = setInterval(() => {
    if (port) void sendFullState();
  }, 60_000);
}

chrome.tabs.onCreated.addListener((tab) => scheduleUpdate(tab));
chrome.tabs.onUpdated.addListener((_tabId, changeInfo, tab) => {
  if (changeInfo.url || changeInfo.title || changeInfo.favIconUrl || changeInfo.status === "complete") {
    scheduleUpdate(tab);
  }
});
chrome.tabs.onRemoved.addListener((tabId) => {
  const pending = pendingUpdates.get(tabId);
  if (pending) clearTimeout(pending);
  pendingUpdates.delete(tabId);
  if (trackedTabs.delete(tabId)) {
    send({ type: "tabRemoved", protocolVersion: PROTOCOL_VERSION, browserInstanceId, tabId: String(tabId) });
  }
});
/** Switching to a tab is the single most common way a user "opens" something
 * Pully should see, but it doesn't itself change the tab's url/title/etc —
 * so onCreated/onUpdated may never have fired for it (a background tab that
 * predates this session, one the service worker missed while asleep, or one
 * that was simply never touched again after loading). Re-upserting it here,
 * every time, is what makes switching to an already-open tab show up
 * immediately instead of only after that tab happens to reload or navigate. */
function refreshActiveTab(tabId: number) {
  chrome.tabs.get(tabId, (tab) => {
    if (chrome.runtime.lastError || !tab) return;
    sendTabUpsert(tab);
  });
}

chrome.tabs.onActivated.addListener(({ tabId }) => {
  refreshActiveTab(tabId);
  send({ type: "activeTabChanged", protocolVersion: PROTOCOL_VERSION, browserInstanceId, tabId: String(tabId) });
});

// Switching to a different (already-focused-tab) browser *window* doesn't
// fire onActivated at all — this covers that case the same way.
chrome.windows.onFocusChanged.addListener((windowId) => {
  if (windowId === chrome.windows.WINDOW_ID_NONE) return;
  chrome.tabs.query({ active: true, windowId }, (tabs) => {
    const tab = tabs[0];
    if (tab?.id != null) refreshActiveTab(tab.id);
  });
});

const CONTEXT_MENU_ITEMS: chrome.contextMenus.CreateProperties[] = [
  { id: "pully-open-page", title: "Open page in Pully", contexts: ["page"] },
  { id: "pully-quick-page", title: "Quick Download with Pully", contexts: ["page"] },
  { id: "pully-open-link", title: "Open link in Pully", contexts: ["link"] },
  { id: "pully-quick-link", title: "Quick Download with Pully", contexts: ["link"] },
];

/** Context menu items persist across service worker restarts (Chrome owns
 * them, not the worker's memory) — this only needs to run once per install,
 * not on every wake. Each `create` gets its own callback so an expected
 * "duplicate id" from a racing/second call is consumed instead of surfacing
 * as an unchecked `runtime.lastError` in chrome://extensions. */
function setupContextMenus() {
  chrome.contextMenus.removeAll(() => {
    for (const item of CONTEXT_MENU_ITEMS) {
      chrome.contextMenus.create(item, () => void chrome.runtime.lastError);
    }
  });
}

chrome.contextMenus.onClicked.addListener((info) => {
  const isLinkTarget = info.menuItemId === "pully-open-link" || info.menuItemId === "pully-quick-link";
  const url = isLinkTarget ? info.linkUrl : info.pageUrl;
  if (!isTrackableUrl(url)) return;
  const quick = info.menuItemId === "pully-quick-page" || info.menuItemId === "pully-quick-link";
  send({ type: "openInPully", protocolVersion: PROTOCOL_VERSION, url, quick });
});

type PopupRequest = { type: "getPopupState" } | { type: "openInPully"; quick: boolean };

chrome.runtime.onMessage.addListener((message: PopupRequest, _sender, sendResponse) => {
  void (async () => {
    if (message.type === "getPopupState") {
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      sendResponse({
        connected,
        browser: browserFamily,
        tab: tab && isTrackableUrl(tab.url)
          ? { title: tab.title ?? tab.url, url: tab.url, favIconUrl: isSafeFavicon(tab.favIconUrl) ? tab.favIconUrl : undefined }
          : undefined,
      });
      return;
    }
    if (message.type === "openInPully") {
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      if (tab && isTrackableUrl(tab.url)) {
        send({ type: "openInPully", protocolVersion: PROTOCOL_VERSION, url: tab.url, quick: message.quick });
        sendResponse({ ok: true });
      } else {
        sendResponse({ ok: false });
      }
    }
  })();
  return true; // Keep the message channel open for the async response above.
});

async function init() {
  browserInstanceId = await getInstanceId();
  browserFamily = await detectBrowser();
  connect();
  startPingLoop();
  startResyncLoop();
}

// Context menu items persist across service worker restarts, so this only
// needs to run once per install/update — running it on every wake (onStartup,
// and the top-level `init()` below) is what caused duplicate-id errors.
chrome.runtime.onInstalled.addListener(() => setupContextMenus());
chrome.runtime.onInstalled.addListener(() => void init());
chrome.runtime.onStartup.addListener(() => void init());
void init();
