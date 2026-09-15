# Third-party licenses

This is a summary of what Pully bundles or invokes and under what license,
produced as part of a pre-release audit. It is not a copy-paste of
every dependency's full license text (with 484 Rust crates and ~70 npm
packages, that would be an unmaintainable multi-thousand-line file) — it's
a categorized summary plus exactly how to regenerate the full list yourself.

**Pully's own source code license is not yet chosen.** That's the project
owner's decision, not something picked on your behalf here — see the note
at the bottom.

## Runtime tools Pully invokes (not bundled by Pully's own build)

Pully never links against these — it only shells out to them as separate
processes (`Command::new(...)`, argument arrays, never a shell). Mere
process invocation of a GPL/LGPL program doesn't impose its copyleft on
Pully's own source; the obligation described below only applies to whoever
*bundles* a specific binary into a release installer.

| Tool | License | Bundled by Pully's build? |
|---|---|---|
| **yt-dlp** | [Unlicense](https://github.com/yt-dlp/yt-dlp/blob/master/LICENSE) (public domain) | No — via `scripts/prepare-sidecars.ps1`, `PATH`, or downloaded at runtime, only if the user clicks "Install automatically" (`src-tauri/src/setup.rs`), straight from yt-dlp's own GitHub releases |
| **FFmpeg** | **LGPL-2.1+ or GPL-2+/3+, depending on how the specific build was configured** — see below | No — same three paths as yt-dlp; the in-app downloader fetches gyan.dev's "essentials" Windows build, which is LGPL (no `--enable-gpl`) |
| **spotiflac** (optional) | Unknown — a third-party tool outside this repository. Not audited here; see `docs/SPOTIFLAC.md` for the isolation boundary. | No, and never auto-downloaded — see `docs/SPOTIFLAC.md` for why |

### FFmpeg: what a maintainer needs to do before shipping one

FFmpeg's license depends on its build configuration, not on FFmpeg the
project as a whole. A build compiled **without** `--enable-gpl` and without
any GPL-only external libraries is LGPL-2.1+; enabling `--enable-gpl`
(or certain external libraries) makes that specific build GPL-2+/3+.

**Before bundling any `ffmpeg.exe` into a release installer:**
1. Know which build you're using and confirm its actual license (check the
   build's own `-version`/license output, or the source of the build you
   downloaded).
2. Prefer an LGPL build (no `--enable-gpl`) — it keeps obligations simple
   and doesn't require Pully's own installer to do anything beyond
   attribution.
3. Include FFmpeg's license text and copyright notice alongside the
   bundled binary in the installer (a `NOTICE`/`THIRD_PARTY` folder next to
   the installed binaries is sufficient — FFmpeg doesn't require source
   distribution for LGPL use as a standalone, swappable executable, which
   this already is).

This is not a new requirement invented for this audit — it's the actual,
correct legal analysis for a "shell out to a user-supplied ffmpeg.exe"
architecture, and it was previously undocumented. It's now called out
explicitly in `README.md`'s "Production dependencies" section too.

## Rust dependencies (bundled, statically linked)

Audited via `cargo license` (484 resolved crates) as part of this audit:

| License family | Crate count |
|---|---:|
| Apache-2.0 OR MIT (dual-licensed, pick either) | 295 |
| MIT | 111 |
| Apache-2.0 OR MIT OR Zlib | 20 |
| Unicode-3.0 | 18 |
| MIT OR Unlicense | 11 |
| MPL-2.0 | 5 |
| Apache-2.0 (+ LLVM-exception variants) | ~8 |
| BSD-3-Clause (and dual variants) | ~5 |
| Zlib | 2 |
| ISC | 1 |
| 0BSD OR Apache-2.0 OR MIT | 1 |
| Apache-2.0 OR CC0-1.0 OR MIT-0 | 1 |

**No GPL or AGPL dependency anywhere in the Rust dependency tree.** The 5
MPL-2.0 crates are used unmodified as dependencies (not forked/edited),
which only requires that *if you modify an MPL-licensed file itself* you
share those specific file changes — using the crate as-is carries no
additional obligation beyond the standard "include the license" notice
already satisfied by this file plus `Cargo.lock`.

Regenerate the exact, current, full list yourself at any time:
```powershell
cargo install cargo-license --locked
cd src-tauri
cargo license
```

## npm dependencies — frontend (`package.json`)

| License | Count |
|---|---:|
| MIT | 62 |
| OFL-1.1 (the bundled `@fontsource-variable/*` fonts) | 5 |
| Apache-2.0 / MIT dual | 2 |
| ISC | 1 |
| 0BSD | 1 |

OFL-1.1 (the fonts: Inter, Manrope, Space Grotesk, IBM Plex Sans,
JetBrains Mono) permits bundling inside an application freely; its only
real restriction is that the font files themselves can't be **sold
standalone** and a modified font must be renamed — neither applies here.

Regenerate: `npx license-checker --production --summary` from the repo root.

## npm dependencies — browser extension (`browser-extension/package.json`)

All dev-only (esbuild, typescript, @types/chrome) — used to build the
extension, never shipped inside `dist/`. MIT (6) and Apache-2.0 (1).

## Notable individually-licensed components already covered above

Tauri, React, React DOM, Motion, all `@radix-ui/*` packages, and
`lucide-react` are all MIT or Apache-2.0/MIT dual — included in the counts
above, called out by name since they're the largest/most visible
dependencies.

## Pully's own license

**GPL-3.0-or-later**, chosen by the project owner. `license = "GPL-3.0-or-later"` is now set in `src-tauri/Cargo.toml`, `package.json`, and `browser-extension/package.json`. The `LICENSE` file text itself is the project owner's own addition (not generated as part of this audit) — add the canonical GPLv3 text at the repo root as `LICENSE` to complete this.

Practical effect of this choice: since none of Pully's own Rust/npm dependencies are GPL-incompatible (see the permissive/weak-copyleft summary above), there's no license conflict to resolve there. It also simplifies the FFmpeg guidance below slightly — because Pully's own code is now GPL-3.0, bundling a GPL-licensed FFmpeg build (not just an LGPL one) alongside it is straightforwardly compatible; LGPL is still the simpler choice if you'd rather minimize bundling obligations, but it's no longer required to avoid a license clash with Pully itself.
