# Contributing

Thanks for considering it. A few things specific to this repo.

## Before you start

- Pully is licensed under GPL-3.0-or-later (see `LICENSE`) — your
  contribution will be under the same license.
- For anything non-trivial, open an issue first. Pully has a fairly
  specific architecture (see below) and it's easier to agree on an
  approach before writing code than to rework a finished PR.

## Setup

```powershell
npm install
npm run tauri dev
```

Requires Node.js, Rust, and `yt-dlp`/`ffmpeg` on `PATH` for development
(they're not bundled in dev mode — see `README.md`). The browser extension
and its native-messaging host are optional for most work; see
`docs/BROWSER_INTEGRATION.md` if you're touching that area.

## Before opening a PR

Run the same checks CI (or you, manually) will run:

```powershell
npm run lint
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
cargo test --manifest-path src-tauri/Cargo.toml
```

If you touched the browser extension:
```powershell
cd browser-extension
npm run typecheck
npm run build
```

## Architecture notes that matter for contributions

- **Never shell out via `cmd`/`powershell`/`sh`.** Every external process
  invocation uses `Command::new(program).args([...])` — an argument array,
  never a shell string. This is load-bearing for security (see
  `SECURITY.md`); a PR that concatenates a shell command string will be
  rejected regardless of what it's trying to accomplish.
- **Filenames and paths go through `src-tauri/src/filesystem.rs`.** Don't
  build a destination path anywhere else — the sanitization and
  directory-escape checks live in exactly one place on purpose.
- **The frontend has no direct filesystem/shell/process access** — add a
  narrow, validated Tauri command instead of widening
  `src-tauri/capabilities/default.json`.
- **Provider isolation**: yt-dlp-specific and SpotiFLAC-specific logic
  stay in their own modules (`src-tauri/src/extractors/`,
  `src-tauri/src/downloader/`). Don't special-case a provider in shared
  code — route through `extractors::route()` instead.
- **No DRM/access-control bypass, for any provider, ever.** PRs that add
  this (including "just for one site") will be closed.

## Reporting a security issue

Don't open a public issue — see `SECURITY.md`.
