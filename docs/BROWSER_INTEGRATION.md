# Browser Integration

Lets Pully see supported pages you already have open in a browser, so they
show up under **"Detected in your browser"** on the home screen without you
copying a link. Off by default — you opt in twice: install the extension,
and turn the feature on in Pully's own Settings.

```
Browser
  |
  Pully browser extension (background service worker)
  |  chrome.runtime.connectNative — Chromium's native-messaging framing
  v
pully-native-host.exe  (one process per browser connection)
  |  local named pipe, \\.\pipe\app.pully.desktop.browser-bridge
  v
Pully desktop app
  |  existing extractors::route -> yt-dlp / SpotiFLAC providers
  |  existing analyze / Quick Mode / DownloadManager
  v
Pully UI
```

A detected tab is **only ever a URL**. Clicking it (or its Download /
Quick Download buttons) calls the exact same `api.analyze(url)` /
Quick-Mode path a manually pasted link uses — nothing about analysis,
provider routing, or downloading is duplicated for browser-sourced links.

## Reliability and speed

Two things made early builds of this feature feel laggy or flaky, and both
are addressed structurally rather than by tuning a timeout:

1. **Reconnecting to Pully was opportunistic.** `pully-native-host` used to
   only try reconnecting its pipe to Pully when it happened to have a
   message to forward, and only for a few attempts right at its own
   startup. If Pully wasn't up yet at that exact moment, later messages
   could sit dropped until something coincidentally retried. Now a
   background thread polls for Pully's pipe every 300ms for the whole
   lifetime of the host process — reconnection isn't gated on browser
   activity at all.
2. **Nothing told the extension to resync after reconnecting.** Even once
   the pipe was back, Pully wouldn't actually have current data until the
   next tab event happened to fire. Now the moment the background thread
   establishes a *new* pipe connection, it sends the extension a
   `requestState` message (over native messaging, never over the pipe —
   Pully never sees it), and the extension responds with an immediate full
   `browserState` snapshot. In practice this means detection catches up
   within roughly half a second of Pully becoming reachable, not "whenever
   you next touch a tab."

On top of that: the popup's "Connected" status now pings immediately on
connect instead of waiting for the first scheduled 15s tick, the per-tab
update debounce dropped from 250ms to 150ms, and a lightweight full-state
resync every 60s acts as a safety net in case any single message is ever
lost, without adding meaningful traffic.

## What actually ships here

- `browser-extension/` — a Manifest V3 TypeScript extension (esbuild, no
  framework). Tracks open `http(s)` tabs via `chrome.tabs` events, relays
  them over native messaging, and does nothing else. It never downloads
  media itself.
- `src-tauri/src/browser_integration/` — the Rust side: the wire protocol
  (`messages.rs`), validation of everything inbound (`validation.rs`), the
  in-memory (never persisted) tab store (`state.rs`), the local named-pipe
  server (`ipc.rs`), and native-messaging-host registration
  (`native_messaging.rs`).
- `src-tauri/src/bin/pully-native-host.rs` — the actual native-messaging
  host binary Chromium spawns. A thin relay: stdin (from the browser) to the
  pipe (to Pully), answering `ping` locally.

## Pipe authentication

`reject_remote_clients(true)` on the named pipe (`ipc.rs`) blocks
network-remote connections, but doesn't by itself restrict *which local
process* can open it — any process running as the same Windows user could
otherwise connect directly and send crafted messages, bypassing Chrome's
`allowed_origins` check entirely (that check only constrains who can
*launch* `pully-native-host`, not who can talk to Pully's pipe once it
exists). To close that gap: Pully generates a fresh random token on every
startup (`browser_integration::pipe_auth::issue_token`) and writes it to
`%LOCALAPPDATA%\Pully\pipe-token.txt` (a per-user, NTFS-ACL'd location).
`pully-native-host` reads it (`pipe_auth::read_token`) and sends it as the
literal first frame on every new pipe connection, before anything else.
Pully's connection handler reads that first frame, compares it, and closes
the connection immediately on any mismatch — no `Envelope` is ever parsed,
no peer is ever registered, for an unauthenticated connection. This isn't
meant to resist a determined attacker with equal-or-higher local
privilege (who could read the same token file just as easily); it closes
the specific "another local process impersonates the extension" gap.

## Privacy

- **Opt-in, twice.** Installing the extension does nothing until you also
  enable "Browser Integration" in Pully's Settings. While disabled, Pully's
  pipe server drains incoming messages without storing or displaying
  anything (`BrowserIntegration::enabled`, checked before any message is
  applied to state).
- **Never uploaded.** Nothing here talks to a remote server. There's no
  telemetry, no analytics, and no code path capable of sending a detected
  URL anywhere except Pully's own local analysis pipeline.
- **Not page contents.** The extension reads `chrome.tabs` metadata (url,
  title, favicon) via the `tabs` permission — never DOM content, cookies,
  form values, passwords, or browsing history.
- **Not persisted.** `BrowserTabStore` lives in memory only. Closing a tab
  removes it; closing Pully drops all of it; nothing is written to disk
  except what a user explicitly downloads (which becomes a normal Pully
  download, exactly like a pasted link).
- **Filtered before it ever leaves the browser.** Only `http://`/`https://`
  URLs are ever considered (`isTrackableUrl` in the extension,
  re-enforced by `validation::validate_url` in Pully) — `chrome://`,
  `edge://`, `brave://`, `about:`, `file://`, `chrome-extension://`, etc.
  never get sent.

## Permissions the extension requests, and why

| Permission | Why |
| --- | --- |
| `tabs` | Read `url`/`title`/`favIconUrl` for open tabs and get open/close/update/activate events. This alone is enough (no `host_permissions` or `<all_urls>` needed) — Chrome grants tab metadata visibility through this permission by design. |
| `nativeMessaging` | Talk to `pully-native-host` at all. |
| `contextMenus` | The right-click "Open page/link in Pully" / "Quick Download with Pully" entries. |
| `storage` | `chrome.storage.session` only, to keep a random per-session instance id stable across the service worker being killed and restarted — cleared when the browser closes, never written to disk. |

No `host_permissions`, no `<all_urls>`, no content scripts.

## The wire protocol

One JSON envelope shape, tagged by `type`, shared (by hand — see below) by
`browser-extension/src/messaging/protocol.ts` and
`src-tauri/src/browser_integration/messages.rs`: `hello`, `ping`/`pong`,
`requestState` (sent host → extension only, the instant the host's pipe
connection to Pully comes up, to trigger an immediate resync — see
"Reliability and speed" below), `browserState` (full snapshot),
`tabAdded`/`tabUpdated`/`tabRemoved`, `activeTabChanged`, `openInPully` (the
one browser-originated message allowed to focus Pully's window — everything
else stays silent in the background), `protocolMismatch`, and `themeUpdate`
(Pully → extension only — see "Theme sync" below). Every message carries
`protocolVersion` (currently `1`); a mismatch gets a `protocolMismatch`
reply with a "your extension needs updating" message instead of being
silently misparsed. Limits (URL/title length, tabs tracked per browser,
frame size) live in `messages::limits` (Rust) and mirrored in
`protocol.ts`'s `LIMITS`.

**There is no code generator keeping the TypeScript and Rust shapes in
sync — if you change one, change the other by hand.** This was a
deliberate scope call: adding a schema/codegen pipeline for a message
protocol this small would be more machinery than the protocol itself.

## Theme sync

The extension's popup mirrors Pully's actual applied theme — accent color,
base color, font, and light/dark — rather than a fixed look-alike palette.
This needed the pipe to become bidirectional (it was host → Pully only
before):

- `App.tsx` already resolves its theme into CSS custom properties on
  `document.documentElement` (`--color-accent`, `--color-canvas`, etc. —
  see `colorChannels`/the base-color and dark-mode CSS rules in
  `styles.css`). A `useEffect` reads those resolved values back out via
  `getComputedStyle` and calls the `push_theme` Tauri command with them —
  once immediately on any theme-affecting change, and then every 5s on an
  interval, so a browser session that (re)connects at any point still picks
  up the current theme promptly rather than waiting on the next actual
  settings change.
- `push_theme` (`commands.rs`) broadcasts a `ThemeUpdate` envelope to every
  currently-connected `pully-native-host` session via
  `ipc::PeerRegistry` — the named-pipe server now keeps the *write* half of
  each connection (split via `tokio::io::split`) specifically so Pully can
  push to it, not just read from it.
- `pully-native-host` relays whatever Pully sends down the pipe straight to
  stdout, unparsed — it doesn't need to understand `ThemeUpdate` any more
  than it understands `TabAdded`.
- The service worker stores the last received theme in
  `chrome.storage.session` (not applied there — the background has no DOM);
  the popup reads it on open and overrides its CSS custom properties
  directly. No theme received yet (Pully never connected) → the popup keeps
  its static defaults, which already match Pully's own out-of-the-box look.

**Known limitation:** Rust never interprets any of this — it's opaque
`String`s relayed verbatim, by design (theme *derivation* stays entirely in
`App.tsx`, the single place that already knows how to turn a base color +
accent + dark mode into concrete colors). That means a change to Pully's
color/font system only requires updating `App.tsx`'s push effect and the
popup's `applyTheme`, never Rust.

## Tray mode and launch-on-demand

Pully can run entirely in the system tray, and the extension can launch it
if it isn't running at all — so "Quick Download" from the browser works
regardless of whether Pully happens to be open:

- **Closing the window doesn't quit Pully** while Settings → "Run in system
  tray" is on (the default). The window hides; Pully keeps running (pipe
  server, browser detection, everything) with a tray icon (Show Pully /
  Quit Pully). This is a Rust-side `WindowEvent::CloseRequested` handler
  (`lib.rs`), not a frontend behavior — closing works the same way whether
  or not the window currently has focus.
- **Quick Download never shows the window.** `ipc.rs`'s `OpenInPully`
  handler only calls `window.show()`/`set_focus()` when `quick` is `false`.
  A quick download processes with the window exactly as visible (or hidden)
  as it already was — Tauri's webview keeps running JS in the background
  regardless of window visibility, so this needs no special handling on the
  frontend beyond the normal Quick Mode code path.
- **If Pully isn't running at all**, `pully-native-host` tries to forward
  the message over the pipe as usual; if that fails, and the message was an
  explicit `OpenInPully` (never a passive tab update), it launches Pully
  itself (`browser_integration::launcher::launch`) — with `--tray` when the
  action was a Quick Download (so it comes up hidden), or without it for a
  normal "Open in Pully" (so it comes up showing the window, exactly like
  double-clicking the app). Pully records its own executable path to
  `%LOCALAPPDATA%\Pully\app-path.txt` on every startup (`launcher::record_app_path`)
  specifically so this lookup never has to guess an install location.
- If Pully has genuinely never been run once (no breadcrumb file yet), the
  native host can't safely guess a path and gives up with a stderr message
  — there's no silent fallback that could launch the wrong thing.
- The `--tray` launch flag is parsed alongside the URL in `startup.rs` and
  only ever hides the window in `setup()`; it carries no other meaning and
  is never interpreted as a shell/process instruction beyond that.

## Extension identity (why there's a `keys/` folder)

Chrome computes an extension's id from its public key. Without a pinned
key, "Load unpacked" gives a different random id every time you reload the
extension, which would make the native-messaging manifest's
`allowed_origins` impossible to keep correct during development.
`browser-extension/keys/pully-extension.pem` is a real, checked-out-of-git
RSA keypair generated for this purpose; its public half is embedded as the
`key` field in `manifest.json`, which fixes the extension's id at
`englocjbholjnjfkjkbbklebfhllfmfd` forever (`src-tauri/src/browser_integration/native_messaging.rs::EXTENSION_ID`
must match it). The private key is only needed if you ever package/sign
the extension yourself — **never commit it** (it's already gitignored).
If it's ever lost or rotated, regenerate it and update `EXTENSION_ID` to
match the new id.

## Windows-first

The named pipe (`ipc.rs`) and native-messaging-host registration
(`native_messaging.rs`) are implemented for Windows only, gated behind
`#[cfg(windows)]`, with a `#[cfg(not(windows))]` stub that logs the gap
instead of pretending to work. macOS/Linux would need a Unix domain socket
carrying the exact same `Envelope` protocol and the exact same
`BrowserTabStore` — the protocol and state layers are already
platform-agnostic, only `ipc.rs`'s transport needs a second implementation.

## Setting it up for development

There is no published Chrome Web Store listing — you load the extension
unpacked, same as any extension under development. This is a real,
inherent limitation of not having a public listing yet, not a shortcut:
production auto-registration of the extension itself isn't possible until
it's published. What Pully's installer *can* (and does) fully automate is
the native-messaging-host side (`native_messaging::register`, run on every
Pully startup) — that part needs no manual registry editing at all,
on a dev machine or an end user's.

1. **Build the extension**
   ```powershell
   cd browser-extension
   npm install
   npm run build      # -> browser-extension/dist
   ```
2. **Load it unpacked** — identical steps in Chrome, Brave, and Edge (all
   Chromium):
   - Go to `chrome://extensions` (`brave://extensions`, `edge://extensions`).
   - Turn on "Developer mode".
   - "Load unpacked" -> select `browser-extension/dist`.
   - Because `manifest.json` has a pinned `key`, the id will be
     `englocjbholjnjfkjkbbklebfhllfmfd` every time you do this.
3. **Build and run Pully** (`npm run tauri dev` from the repo root, or a
   release build). On startup Pully writes the native-messaging manifest to
   `%LOCALAPPDATA%\Pully\native-messaging\com.pully.native_host.json` and
   points Chrome/Brave/Edge's `NativeMessagingHosts` registry keys at it —
   automatically, no manual step. `cargo build`/`cargo tauri dev` already
   produce `pully-native-host.exe` right next to `pully.exe`, so debug runs
   find it without any extra copy step.
4. **Turn the feature on**: open Pully → Settings → Browser Integration →
   enable it (and leave "Show open browser tabs" on).
5. **Verify the connection**: open a supported page (e.g. a YouTube video)
   in the browser you loaded the extension into, then switch to Pully. It
   should appear under "Detected in your browser" on the home screen, and
   Settings → Browser Integration → Connection status should show that
   browser as Connected.

### Release builds

`scripts/prepare-sidecars.ps1` copies `yt-dlp`/`ffmpeg`/`spotiflac` into
`src-tauri/binaries/` before `npm run tauri build`. Do the same for the
native host so the bundled resources (`bundle.resources: ["binaries/*"]`)
pick it up:
```powershell
cargo build --release --bin pully-native-host --manifest-path src-tauri/Cargo.toml
Copy-Item src-tauri/target/release/pully-native-host.exe src-tauri/binaries/pully-native-host.exe -Force
.\scripts\prepare-sidecars.ps1 -YtDlp <path> -Ffmpeg <path>
npm run tauri build
```

## Manual test checklist

**Honest status: this checklist has not been run against a real browser.**
This environment has no interactive browser to click through — everything
above was verified the ways that *are* available here: `cargo check`/`test`/
`fmt` for the Rust side (protocol round-trips, validation edge cases,
per-browser-instance tab tracking, connection-status logic — see the unit
tests in `browser_integration/*.rs`), and `tsc`/`npm run build` for the
extension (typechecks cleanly, bundles to a loadable `dist/`). The
architecture and every message path are real and complete, not stubbed —
but the checklist below is what to actually run through by hand in Chrome,
Brave, and Edge before considering this feature verified end to end.

- [ ] YouTube tab → appears under "Detected in your browser"; clicking it
      runs it through the normal yt-dlp analysis screen.
- [ ] Spotify tab → same, routed through the SpotiFLAC provider.
- [ ] TikTok / Reddit tab → detected generically (no site-specific code in
      the extension); an unsupported site still shows a card (the
      extension does cheap `http(s)` filtering only) but fails analysis
      the same way a pasted link to it would.
- [ ] Multiple tabs, including duplicate URLs across tabs → one card, "Open
      in N tabs"; closing one duplicate keeps the card while others remain.
- [ ] Tab navigation / title change → card updates in place (debounced).
- [ ] Tab close → card disappears.
- [ ] Active-tab switching → "Only show active tab" setting filters live;
      switching to an already-open tab that was never navigated/reloaded in
      this session (e.g. it predates the extension connecting) still shows
      up immediately — `onActivated`/`onFocusChanged` re-upsert the tab
      being switched to rather than relying on `onCreated`/`onUpdated` alone.
- [ ] Multiple browser windows, and multiple browsers (e.g. Chrome + Brave)
      at once → tabs from each appear without id collisions
      (`browserInstanceId:tabId` keys).
- [ ] Pully already running when the browser opens → tabs appear once the
      extension connects.
- [ ] Browser opened before Pully → tabs appear within about a second of
      Pully starting: `pully-native-host` keeps a background thread polling
      for Pully's pipe every 300ms (not a one-shot startup retry), and the
      moment it connects, sends the extension a `requestState` message that
      triggers an immediate full resync rather than waiting on the next
      incidental tab event.
- [ ] Pully closed while browser stays open → card list clears; extension
      keeps retrying in the background; no crash, no repeated popups.
- [ ] Extension reloaded → new native-messaging connection, full
      `browserState` resync.
- [ ] Pully not running, click Quick Download in the popup → Pully launches
      hidden in the tray, the download starts, no window ever appears.
- [ ] Pully not running, click "Open in Pully" (non-quick) → Pully launches
      and shows its window with the link ready to analyze.
- [ ] Pully closed to the tray (window hidden, X button, "Run in system
      tray" on), click Quick Download → download proceeds, window stays
      hidden.
- [ ] Same as above but click "Open in Pully" (non-quick) → window is
      restored and focused.
- [ ] "Run in system tray" turned off in Settings → the window's X button
      quits Pully normally again (no tray icon behavior).
- [ ] Right-click a page → "Open page in Pully" / "Quick Download with
      Pully" both work and bring Pully's window forward (the only
      browser-triggered action allowed to).
- [ ] Right-click a link → "Open link in Pully" uses the link's URL, not
      the page's.
- [ ] Popup on a supported tab → shows title/domain, "Connected to Pully"
      when the native host is reachable, action buttons enabled.
- [ ] Popup reflects Pully's current accent color, font, base color, and
      light/dark — change any of them in Pully's Settings, reopen the
      popup within ~5s, confirm it updated (no extension reload needed).
- [ ] Popup before ever connecting to Pully (fresh install) → falls back to
      Pully's default look rather than an unstyled/broken page.
- [ ] Popup on an unsupported tab (e.g. `chrome://newtab`) → "No supported
      page open", action buttons disabled.
- [ ] Popup with the native host unreachable (Pully never run,
      registration missing) → "Pully isn't connected."
- [ ] Malformed / oversized messages → rejected by `validation.rs`
      (see its unit tests) without crashing the pipe connection.
- [ ] Unicode title, URL containing `&`, `?`, `#`, `%` → survives the round
      trip intact (JSON handles this natively; no manual escaping anywhere
      in the pipeline).
- [ ] Non-http URL crafted at any layer → rejected (`isTrackableUrl` in the
      extension, `validate_url` in Rust — defense in depth).
- [ ] 100+ open tabs → capped at `LIMITS.maxTrackedTabs` / `MAX_TABS_PER_BROWSER`
      (500) rather than growing unbounded.
- [ ] Rapid tab changes → coalesced by the extension's 250ms per-tab
      debounce instead of flooding the pipe.
