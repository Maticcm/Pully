# Security

Pully is a local desktop app that shells out to external tools (yt-dlp,
FFmpeg, and optionally a SpotiFLAC-compatible binary and a browser
extension). This document explains the actual security model, not a
generic template.

## Reporting a vulnerability

Please open a private security advisory on GitHub (Security tab → "Report
a vulnerability") rather than a public issue, so a fix can go out before
details are public. Include what you found, how to reproduce it, and what
you think the realistic impact is. There's no bug bounty — this is a small
open-source project — but reports are taken seriously and credited.

## What's actually in scope

- Pully's Rust backend and Tauri configuration (`src-tauri/`)
- Pully's React frontend (`src/`)
- The browser extension and native-messaging host (`browser-extension/`, `src-tauri/src/browser_integration/`, `src-tauri/src/bin/pully-native-host.rs`)

## What's out of scope

- Vulnerabilities in yt-dlp, FFmpeg, or a third-party SpotiFLAC binary
  themselves — report those upstream. Pully invokes them as separate
  processes with argument arrays (never a shell), so it doesn't inherit
  their internals, but it also can't fix bugs in them.
- Anything that requires first bypassing a platform's own DRM/access
  controls — Pully doesn't implement or want that, and won't accept
  reports asking it to.
- Social engineering, physical access, or attacks that require the
  attacker to already have arbitrary code execution as the same user
  Pully runs as (at that point they don't need Pully at all).

## The actual security model, briefly

- **The frontend has no direct filesystem, shell, or process access.**
  Everything goes through a small, explicit set of Tauri commands
  (`src-tauri/capabilities/default.json` grants only window controls and
  the folder-picker dialog beyond Tauri's own IPC). A compromised or
  malicious frontend can only call the commands Pully itself defined, each
  of which validates its own input.
- **External processes are always invoked with argument arrays**, never a
  shell (`cmd`/`powershell`/`sh`) — checked explicitly as part of the
  pre-release audit (`AUDIT.md`). Titles, URLs, and filenames can't inject
  extra command-line flags.
- **Filenames are sanitized and path-escape is rejected** before any file
  is written — see `src-tauri/src/filesystem.rs`. A malicious media title
  cannot write outside the configured download folder.
- **The local named pipe used for browser-extension communication requires
  a same-user auth token** (issued fresh at every Pully startup,
  `src-tauri/src/browser_integration/pipe_auth.rs`) before any message is
  processed — see `docs/BROWSER_INTEGRATION.md` for the full protocol.
- **Everything received from a browser extension or a provider's own
  output is treated as untrusted** and re-validated on Pully's side
  regardless of what the sender already checked
  (`src-tauri/src/browser_integration/validation.rs`).

## What Pully will not add

- Automatic execution of a downloaded/updated binary from a remote URL
  (there is currently no auto-update system at all — see `README.md`).
- Any mechanism to bypass DRM, authentication, or a platform's access
  controls, for any provider.
