# Pully features

A tour of what Pully currently does, grouped by area. For setup and build instructions see [README.md](README.md); for the Spotify provider's integration boundary see [docs/SPOTIFLAC.md](docs/SPOTIFLAC.md); for the browser extension/native-messaging bridge see [docs/BROWSER_INTEGRATION.md](docs/BROWSER_INTEGRATION.md).

## Link analysis

- **Paste-and-analyze** — paste a URL, press Enter or click Analyze, and Pully fetches title, creator, duration, thumbnail, available formats, subtitles, and playlist/album info before you commit to a download.
- **Clipboard detection** — the paste button reads the clipboard and fills the link field only when it looks like an `http(s)` URL.
- **Drag and drop** — drag a link from a browser (address bar, a page link) straight onto the Pully window to fill it in; an optional small floating drop target (Settings → General) reinforces where to drop it, and a full-window overlay confirms the drop.
- **Provider routing** — every URL is routed to exactly one backend *before* analysis or download: Spotify links (`open.spotify.com` and subdomains) go through an isolated SpotiFLAC adapter; everything else goes through yt-dlp. The two providers are interchangeable behind one interface, so the rest of the app never has provider-specific logic.
- **Playlist, album, and channel detection** — a link that resolves to multiple items is analyzed as a collection (item count shown), not just a single file.
- **Friendly error messages** — invalid links, missing dependencies, and unsupported/unavailable media are translated into plain-language explanations instead of raw process output.

## Browser integration

- **"Detected in your browser"** — with the optional Pully browser extension installed and Browser Integration enabled (Settings → Browser Integration, off by default), pages you already have open in Chrome, Brave, or Edge appear automatically on Pully's home screen — no copying a link. Clicking a card runs it through the exact same analysis/Quick Mode path as a manually pasted URL.
- **Right-click actions in the browser** — "Open page in Pully", "Open link in Pully", and "Quick Download with Pully" context-menu entries, plus a small extension popup showing the current tab and a live connection status.
- **Multi-browser and multi-tab aware** — tabs from several browsers at once are tracked without id collisions, duplicate open tabs collapse into one card ("Open in 3 tabs") without losing track of the underlying tabs, and "Only show active tab" narrows the list to what's currently focused.
- **Local-only, opt-in twice, nothing persisted** — communication goes through a local native-messaging host and named pipe, never a network request; tab data lives in memory only and disappears when a tab closes or Pully exits. See [docs/BROWSER_INTEGRATION.md](docs/BROWSER_INTEGRATION.md) for the full privacy and permissions breakdown.

## Windows shell integration

- **"Download with Pully"** — right-clicking a saved Internet Shortcut (`.url`) file in Explorer offers a "Download with Pully" command that launches Pully (or hands the link to an already-running instance) with the link pre-filled.
- **Single-instance aware** — opening a second link this way while Pully is already running forwards the link to the existing window instead of opening a duplicate.

## System tray

- **Run entirely in the background** — with "Run in system tray" on (Settings → General, on by default), closing Pully's window keeps it running in the tray instead of quitting — downloads, browser detection, and everything else keep working. A tray icon offers "Show Pully" and "Quit Pully".
- **Quick Download never interrupts you** — triggering Quick Download from the browser extension processes the download without ever showing or focusing the window, whether Pully was already running in the tray or not running at all (it launches straight into the tray for a quick download, silently).
- **"Open in Pully" always shows the window** — the non-quick action from the extension's popup or context menu launches Pully (if needed) and brings its window forward, ready to review the link, exactly like opening it normally.

## Video downloads

- **Quality selection** — choose a specific resolution (from the source's own available formats) or "Best".
- **Frame-rate conversion** — optionally re-encode down to a lower frame rate than the source (24/25/30/60fps or the source rate), only offered when it's actually possible.
- **Output container** — MP4, WebM, MKV, MOV, or Original (no remux).
- **Automatic audio pairing** — a video-only format is automatically paired with the best matching audio track and merged.

## Audio downloads

- **Format options** — MP3, M4A, AAC, Opus, OGG Vorbis, FLAC, ALAC, WAV, or Original.
- **Lossless-aware hints** — a note explains that choosing a lossless container doesn't restore quality a lossy source never had.
- **Audio-only sources** — sources that only expose audio (e.g. SpotiFLAC tracks) automatically hide the Video tab; sources with no audio-capable formats hide Audio. You can never pick a mode the source doesn't support.

## Metadata, thumbnails, and subtitles

- **Embed metadata** — writes supported source metadata (title, artist, etc.) into the downloaded file.
- **Embed thumbnail** — embeds the source thumbnail into the file, automatically converted to a compatible image format first so embedding doesn't silently fail on containers that reject the source's native thumbnail format.
- **Save thumbnail separately** — keeps the thumbnail image alongside the download.
- **Subtitles** — download available manual and automatic-caption subtitle tracks, and optionally embed them directly into the video file.

## Playlists and albums

- **Batch download in one job** — downloading a playlist, album, or channel queues one download entry that expands into all of its items.
- **Optional per-collection folder** — keep a playlist/album's files grouped in their own subfolder, or drop them flat into the download folder.
- **Partial-failure tolerance** — if some items in a collection fail (region-locked, removed, etc.), the rest still download; the job finishes as completed with a note listing which items failed, instead of losing the whole batch.

## Quick mode

- **One-click, hands-off downloading** — turn on Quick mode (a toggle in the header) and pasting a link immediately analyzes and queues it with a dedicated preset, no review screen in the way.
- **Instant reset for the next link** — the link field clears as soon as a download is queued, so you can paste the next link right away without waiting for the previous one to finish.
- **Independent preset** — Quick mode has its own settings (video vs. audio, quality, output format, metadata/thumbnail/subtitles) separate from your normal interactive defaults, configured in Settings → Quick mode.
- **Automatic fallback** — if a link doesn't support the preset's chosen mode (e.g. an audio-only source with "Video" selected), Quick mode silently falls back to what's actually available instead of failing.

## Download queue

- **Configurable concurrency** — run 1–6 downloads in parallel.
- **Live progress** — percent, downloaded/total size, speed, and ETA update in real time from the underlying process's own progress stream.
- **Clear status states** — Waiting, Downloading, Processing (post-download work like merging/embedding, shown with a moving indicator so it never looks frozen), Completed, Failed, Cancelled.
- **Cancel, retry, remove** — stop an in-progress download, re-queue a failed or cancelled one with the same settings, or clear a finished entry from the list.
- **Open or reveal** — open a completed file directly or reveal it in the system file browser.
- **Existing-file behavior** — choose to skip or overwrite when a file with the same destination name already exists.

## File naming and storage

- **Token-based filename templates** — build filenames from `{title}`, `{creator}`, `{id}`, `{resolution}`, and `{ext}`, with a live preview.
- **Safe by construction** — templates are validated so they can't escape the chosen download folder, use unsafe characters, or reference unsupported tokens, regardless of which provider produced the file.
- **Configurable download folder** — defaults to `Downloads/Pully`, or pick any folder via a native folder picker.

## Appearance and personalization

- **Theme** — light, dark, or follow the system setting.
- **Font, base color, and accent color** — font options preview in their own actual typeface right in the dropdown; accent color has quick presets plus a custom color picker (saturation/hue picker with a hex input) for any color.
- **Smooth, deliberate motion** — spring-based transitions throughout (progress bars, tabs, dropdowns, dialogs, list changes) built with Radix primitives for accessible behavior and Motion for animation, respecting the OS's reduced-motion setting.

## Window and navigation

- **Custom titlebar** — back/forward history navigation, refresh (disabled while downloads are active), minimize/maximize/close, and quick access to Settings.
- **In-app history stack** — Home, Downloads, and Settings pages keep their own back/forward navigation state.
- **Right-click menu on downloads** — a queue item's context menu offers the same Open/Reveal/Retry/Cancel/Remove actions as its icon buttons; the OS's default browser-chrome right-click menu is suppressed everywhere else in the app.

## Reliability, diagnostics, and trust

- **Dependency diagnostics** — Settings → Status reports whether yt-dlp, FFmpeg, and (optionally) SpotiFLAC are installed and their versions; the app clearly flags what's missing instead of failing silently. SpotiFLAC is optional and only needed for Spotify links — its absence never blocks the rest of the app.
- **No shell execution** — every external tool (yt-dlp, FFmpeg, the SpotiFLAC adapter) is invoked with argument arrays, never through a shell, so link/title/format values can't be used for command injection.
- **No access-control bypass** — Pully does not implement or embed any DRM, authentication, or subscription bypass for any provider; it only orchestrates locally installed tools that you provide.
- **Locally persisted settings** — all preferences are stored on-device; there's no account and no remote sync.
