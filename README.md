<div align="center">
  <img src="src-tauri/icons/icon.png" width="112" alt="Pully app icon">

  <h1>Pully</h1>

  <p><strong>Media, without the mess.</strong></p>
  <p>Paste a link, choose what you want, and save it locally.</p>

  <p>
    <a href="#building-from-source">Build from source</a> ·
    <a href="#documentation">Documentation</a> ·
    <a href="https://github.com/Maticcm/Pully/issues/new">Report a bug</a>
  </p>

  <p>
    <img alt="Platform: Windows" src="https://img.shields.io/badge/platform-Windows-1676D2?style=flat-square">
    <a href="LICENSE"><img alt="License: GPL-3.0-or-later" src="https://img.shields.io/badge/license-GPL--3.0--or--later-C8F169?style=flat-square&labelColor=172019"></a>
    <img alt="Status: pre-release" src="https://img.shields.io/badge/status-pre--release-E9B949?style=flat-square&labelColor=172019">
    <a href="https://tauri.app"><img alt="Built with Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white"></a>
  </p>
</div>

<!--
  Screenshot target: docs/assets/pully-home.png
  Add a current, real capture of the home screen here before the first public release.
-->

Pully replaces ad-heavy downloader pages with a focused desktop workflow. It shows the formats a source actually exposes, lets you choose the result, and saves it to a folder you control.

The React interface talks to a native Rust backend; provider tools run on your computer and downloaded media goes directly to local storage. There is no Pully account, analytics service, or cloud processing layer. Provider tools still connect to the source service when you ask Pully to analyze or download media.

Service-specific behavior stays behind a provider boundary. Most links use yt-dlp, while Spotify track, album, and playlist links are routed to an isolated, optional SpotiFLAC adapter.

## Features

| | |
|---|---|
| **Video**<br>Choose an available resolution, keep or reduce the source frame rate, pair video-only streams with audio, and output MP4, WebM, MKV, MOV, or the original container. | **Audio**<br>Extract or convert to MP3, M4A, AAC, Opus, Ogg Vorbis, FLAC, ALAC, WAV, or keep the original format. |
| **Playlists and collections**<br>Download playlists, albums, and channels as one queue job, optionally group them in a folder, and keep successful items when part of a collection fails. | **Metadata and extras**<br>Embed metadata and thumbnails, save artwork separately, and download or embed available manual and automatic subtitles. |
| **Quick Mode**<br>Analyze and queue a link in one action using a dedicated video/audio, quality, format, metadata, thumbnail, and subtitle preset. | **Download manager**<br>Run 1–6 jobs concurrently with live progress, size, speed, ETA, processing states, cancellation, retry, open, and reveal actions. |
| **Desktop workflow**<br>Paste or drag in links, send saved `.url` shortcuts from Explorer, keep Pully in the system tray, and continue downloads in the background. | **Personalization**<br>Choose light, dark, or system mode; select a bundled font and base color; and set any accent color. Motion respects the operating system's reduced-motion preference. |

See the [complete feature reference](features.md) for the full implemented scope and known limitations.

## Three steps

| 1. Paste | 2. Choose | 3. Pull |
|:---:|:---:|:---:|
| Add a media URL by paste, drag and drop, or browser integration. | Review the available mode, quality, frame rate, format, and extras. | Queue the download and let Pully handle merging, conversion, and file placement. |

## Supported services

Pully supports media sources available through its installed providers:

- **yt-dlp** handles non-Spotify links. Compatibility follows the installed yt-dlp version; see its [supported-sites reference](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md) for the current extractor list.
- **SpotiFLAC** handles Spotify track, album, and playlist URLs through a separate **beta/experimental** integration. Pully requires the current CLI contract, explicitly requests its lossless mode, and only accepts output with a valid FLAC signature. Setup and provider boundaries are documented in [SpotiFLAC integration](docs/SPOTIFLAC.md).

Availability can vary by source and media item. Pully does not implement DRM, authentication, subscription, or access-control bypasses. Only download media you are permitted to save.

## Browser integration

The optional Chromium extension removes the copy-and-paste step:

**Open supported media in Chrome, Brave, or Edge** → **Pully detects the tab locally** → **select it in Pully** → **review or Quick Download**

The extension communicates through a native-messaging host and a same-user local named pipe. Tab URLs, titles, and favicons are held in memory and are not sent to a Pully server. Browser integration is off by default and requires both the unpacked extension and the in-app setting; there is not yet a Chrome Web Store release.

The implementation and automated protocol tests are present, but the documented real-browser manual test checklist is still pending. See [Browser Integration](docs/BROWSER_INTEGRATION.md) for architecture, permissions, development setup, and current validation status.

## Private by design

- No account, advertising, analytics SDK, telemetry, crash reporter, or remote Pully service.
- The frontend Content Security Policy blocks outbound web requests; network access for requested media is performed by the selected provider tool.
- Preferences stay in local storage. Download history is not persisted, and detected browser tabs live in memory only.
- Missing yt-dlp and FFmpeg binaries are downloaded only after an explicit **Install automatically** action. SpotiFLAC is never installed automatically.

Read [Privacy](docs/PRIVACY.md) for the exact network, storage, and browser-extension behavior.

## Download Pully

Pully is currently pre-release. The project does **not** publish an official Windows installer on [GitHub Releases](https://github.com/Maticcm/Pully/releases) yet, so there is no binary download link to recommend. For now, run Pully from source.

## Building from source

### Requirements

- Windows
- Node.js with npm
- A Rust toolchain
- yt-dlp and FFmpeg available on `PATH` for development
- Optional: SpotiFLAC 4.1.2 or newer for Spotify links

```powershell
git clone https://github.com/Maticcm/Pully.git
cd Pully
npm install
npm run tauri dev
```

Development builds discover provider tools on `PATH`. Pully can also use trusted binaries placed in its bundled resources or app-data tools directory. SpotiFLAC is optional; without it, only Spotify links are unavailable.

For a release-style Windows build, prepare the provider sidecars explicitly, then build the Tauri bundle:

```powershell
.\scripts\prepare-sidecars.ps1 `
  -YtDlp C:\path\to\yt-dlp.exe `
  -Ffmpeg C:\path\to\ffmpeg.exe `
  -SpotiFlac C:\path\to\spotiflac.exe

npm run tauri build
```

`-SpotiFlac` is optional. Browser-integration releases also need the native-messaging host beside Pully; follow the [release build instructions](docs/BROWSER_INTEGRATION.md#release-builds). Maintainers bundling FFmpeg should review the applicable obligations in [Third-party licenses](THIRD_PARTY_LICENSES.md).

### Checks

```powershell
npm run lint
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
cargo test --manifest-path src-tauri/Cargo.toml
```

If you change the browser extension:

```powershell
cd browser-extension
npm install
npm run typecheck
npm run build
```

## Architecture

```mermaid
flowchart TD
    UI["Pully UI<br/>React + TypeScript"] -->|typed Tauri commands| Core["Native core<br/>Tauri 2 + Rust"]
    Browser["Optional browser extension"] --> Host["Native-messaging host"]
    Host -->|authenticated local pipe| Core
    Core --> Router{"Provider router"}
    Router -->|non-Spotify URL| YT["yt-dlp provider"]
    Router -->|Spotify URL| SF["SpotiFLAC adapter<br/>optional · beta"]
    YT --> Queue["Download manager"]
    SF --> Queue
    Queue --> Tools["FFmpeg + file safety layer"]
    Tools --> Files["Local filesystem"]
```

URLs are validated before routing. Providers expose a shared media model to the UI, while the download manager owns queueing, cancellation, progress, post-processing, and final file placement.

## Tech stack

| Layer | Technology |
|---|---|
| Desktop runtime | Tauri 2 |
| Native core | Rust, Tokio |
| Interface | React 18, TypeScript, Vite, Tailwind CSS |
| Interaction | Motion for React, Radix Primitives |
| Media providers | yt-dlp; optional SpotiFLAC adapter |
| Post-processing | FFmpeg |
| Browser bridge | Manifest V3 extension, native messaging, local named pipe |

## Documentation

- [Features](features.md) — implemented capabilities and limitations
- [Privacy](docs/PRIVACY.md) — network activity, local storage, and browser data
- [Browser Integration](docs/BROWSER_INTEGRATION.md) — architecture, permissions, setup, and test status
- [SpotiFLAC integration](docs/SPOTIFLAC.md) — supported CLI contract and provider boundary
- [Contributing](CONTRIBUTING.md) — setup, checks, and architecture rules
- [Security](SECURITY.md) — threat model and vulnerability reporting
- [Third-party licenses](THIRD_PARTY_LICENSES.md) — runtime tools and dependency licensing

## Contributing

Issues and pull requests are welcome. For non-trivial changes, open an issue first so the approach can be agreed before implementation. Read [CONTRIBUTING.md](CONTRIBUTING.md) and run the documented checks before submitting a PR.

## Security

Please do not disclose vulnerabilities in a public issue. Use GitHub's **Security** tab to [report a private vulnerability](https://github.com/Maticcm/Pully/security/advisories/new), following [SECURITY.md](SECURITY.md).

## License

Pully is licensed under [GPL-3.0-or-later](LICENSE). Runtime tools and third-party packages retain their own licenses; see [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

<p align="center"><sub>Pully — Media, without the mess.</sub></p>
