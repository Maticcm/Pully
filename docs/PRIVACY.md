# Privacy

Pully doesn't have a privacy policy in the legal-boilerplate sense because
it doesn't collect anything to have a policy about. This is a factual
description of what actually happens, verified as part of a pre-release
pre-release audit rather than aspirational.

## What Pully's own code does over the network

Automatic app updates (enabled by default) check the Pully GitHub Releases
manifest hosted on raw.githubusercontent.com on startup and daily, then install a signed update when Pully is idle.
This can be disabled in Settings > Status. The frontend still
makes zero direct outbound network requests — enforced by the CSP:
the app's Content Security Policy (`src-tauri/tauri.conf.json`) restricts
`connect-src` to Tauri's own local IPC channel, so the webview cannot
`fetch()` anything even if it tried. There is no analytics SDK, no crash
reporter or telemetry.

The one exception: the Rust backend can download `yt-dlp.exe` (from
`github.com/yt-dlp/yt-dlp`'s GitHub releases) and `ffmpeg.exe` (from
`gyan.dev`'s public Windows FFmpeg builds) straight from their official
sources over HTTPS, writing them into Pully's own app-data folder. This
**only** runs when you click "Install automatically" on the
missing-dependency banner — never on startup, never silently, never
without that click. See `src-tauri/src/setup.rs`. SpotiFLAC can be installed
separately from Settings > Status — see `docs/SPOTIFLAC.md`.

## What happens when you actually use Pully

- **Analyzing/downloading a link**: yt-dlp (or SpotiFLAC, for Spotify
  links) contacts whatever site the link points to, and that site's own
  media hosts/CDN — inherent to fetching the media you asked for. This is
  the *external tool's* network activity, not code Pully wrote.
- **Browser integration** (off by default, opt-in): the browser extension
  sends tab metadata (URL, title, favicon) to a local native-messaging
  host process, which relays it to Pully over a local named pipe with a
  same-user auth token. This never leaves your machine — there is no
  server involved anywhere in this path. See `docs/BROWSER_INTEGRATION.md`
  for the full protocol and message-validation details.
- **SpotiFLAC** (optional, installed from Settings or supplied locally): its network behavior is
  outside Pully's control — Pully only shells out to it and parses its
  output. See `docs/SPOTIFLAC.md`.

## What's stored, and where

- **Settings** (theme, download folder, format defaults, etc.) — browser
  `localStorage`, on your device only.
- **Detected browser tabs** — kept in memory only, never written to disk,
  cleared when a tab closes, the browser disconnects, or Pully exits.
- **Download history** — not persisted at all. Once you clear a completed
  download from the queue (or close Pully), there's no record of it kept
  anywhere.
- App update checks contact GitHub Releases. No account or download history is sent with them.

## Browser extension permissions

The extension requests exactly `tabs`, `nativeMessaging`, `contextMenus`,
and `storage` — no `host_permissions`, no `<all_urls>`, no content
scripts. It reads tab URL/title/favicon via the `tabs` API only; it cannot
read page content, cookies, form data, or browsing history, because it
never asks for permission to and has no code path that would let it.
`storage` is used only for a random per-session instance id
(`chrome.storage.session`, cleared when the browser closes).

## If something here turns out to be inaccurate

Open an issue. This document is meant to be checkable against the actual
source, not just trusted.
