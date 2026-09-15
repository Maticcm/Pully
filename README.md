# Pully

Pully is a local-first desktop media downloader built with Tauri 2, React, TypeScript, Tailwind CSS, Rust, yt-dlp, and FFmpeg.

**Platform: Windows only, for now.** The installer, the browser-integration named pipe, native-messaging registry entries, and the `.url`-file shell integration are all Windows-specific. See `docs/BROWSER_INTEGRATION.md` for the (documented, not yet implemented) macOS/Linux gap in that one feature specifically.

URLs are routed to one of two isolated providers before analysis or download:
Spotify links go to a SpotiFLAC-compatible adapter (**beta/experimental** —
works, but less tested than the yt-dlp path and dependent on a third-party
CLI you install yourself), everything else keeps going through yt-dlp. See
[docs/SPOTIFLAC.md](docs/SPOTIFLAC.md) for the provider boundary, setup, and
the exact integration contract.

An optional browser extension lets Pully see supported pages you already
have open (opt-in, local-only, nothing uploaded) — see
[docs/BROWSER_INTEGRATION.md](docs/BROWSER_INTEGRATION.md) for the
architecture, privacy details, and how to build/load it for development.

See [features.md](features.md) for a full tour of what's implemented.

## Development

Requirements: Node.js, Rust, yt-dlp, and FFmpeg available on `PATH`. A `spotiflac` executable is optional and only needed for Spotify links — see [docs/SPOTIFLAC.md](docs/SPOTIFLAC.md).

```powershell
npm install
npm run tauri dev
```

Pully validates URLs in Rust, invokes executables with argument arrays (never a shell), converts yt-dlp JSON to its own typed models, and sends structured progress events to the UI. Downloads default to `Downloads/Pully`.

## Production dependencies

Release builds look for `yt-dlp` and `ffmpeg` in three places, in order: the bundled `binaries` resource folder, Pully's own app-data folder (see below), then `PATH`.

**Option A — bundle trusted binaries at build time.** Put verified Windows binaries in `src-tauri/binaries` with:

```powershell
.\scripts\prepare-sidecars.ps1 -YtDlp C:\path\to\yt-dlp.exe -Ffmpeg C:\path\to\ffmpeg.exe -SpotiFlac C:\path\to\spotiflac.exe
npm run tauri build
```

`-SpotiFlac` is optional; omit it and Spotify links simply report SpotiFLAC as missing (Settings → Status) while every other link keeps working.

**Option B — let the user install them from inside the app.** If a release ships without bundled binaries, Pully's home screen shows a "Download engine setup needed" banner with an "Install automatically" button. Clicking it downloads the latest `yt-dlp.exe` from yt-dlp's own GitHub releases and `ffmpeg.exe` from gyan.dev's public Windows FFmpeg builds, straight from their official sources over HTTPS, into Pully's own app-data folder (a `tools` subfolder under Tauri's standard per-app data directory) — see `src-tauri/src/setup.rs`. This **never** happens automatically or silently; it only runs in response to that explicit click, keeping Pully's "no network calls you didn't ask for" stance (`docs/PRIVACY.md`) intact. SpotiFLAC is never auto-installed this way — see `docs/SPOTIFLAC.md` for why.

The build scripts themselves still never fetch anything — the supply chain for what ships *inside* an installer stays explicit either way.

**If you bundle FFmpeg into a release installer, its license depends on how that specific binary was built** (LGPL vs GPL) — see `THIRD_PARTY_LICENSES.md` for what that means and what to include alongside it. Pully invoking FFmpeg as a subprocess doesn't affect Pully's own license either way; this only matters for whoever ships a specific `ffmpeg.exe` inside an installer.

## Checks

```powershell
npm run lint
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

## Current MVP scope

Implemented: URL analysis, typed media/format/subtitle metadata, playlist detection, video and audio downloads, FFmpeg merging/conversion, metadata/thumbnail/subtitle options, a configurable queue, real yt-dlp progress, cancellation/retry/removal, dependency diagnostics, opening completed files, locally persisted settings for paths, formats, naming, extras, appearance, and existing-file behavior, a one-click quick-download mode with its own preset, drag-and-drop link import, a Windows "Download with Pully" shell action for `.url` files, provider routing so Spotify links go through an isolated SpotiFLAC adapter instead of yt-dlp (see [docs/SPOTIFLAC.md](docs/SPOTIFLAC.md)), and an opt-in browser extension that surfaces open, supported browser tabs on the home screen (see [docs/BROWSER_INTEGRATION.md](docs/BROWSER_INTEGRATION.md)).

Not yet implemented: playlist item selection UI, persistent download history, pause/resume, and macOS/Linux support. These are intentionally not represented as working controls.

## More documentation

- [SECURITY.md](SECURITY.md) — how to report a vulnerability, and the actual security model.
- [docs/PRIVACY.md](docs/PRIVACY.md) — exactly what Pully does and doesn't do over the network.
- [CONTRIBUTING.md](CONTRIBUTING.md) — setup, checks, and architecture rules for PRs.
- [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) — license audit of everything bundled or invoked.
- [docs/SPOTIFLAC.md](docs/SPOTIFLAC.md) — the SpotiFLAC provider's status, setup, and integration contract.
