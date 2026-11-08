# SpotiFLAC provider (beta / experimental)

Pully routes Spotify track, album, and playlist URLs to a locally installed
`spotiflac` executable. Other media providers continue to use yt-dlp.

**Status: beta.** This provider works, but it's newer and less battle-tested
than the yt-dlp path, and it depends entirely on a third-party CLI (and the
provider extension registry it points at) that Pully doesn't control or
audit. Treat Spotify support as experimental — expect rougher edges than
everything else in Pully.

## Supported CLI

Pully uses the public SpotiFLAC command-line interface:

```text
spotiflac [options] <spotify-url> <output-directory>
```

Pully requires the current Python module CLI (SpotiFLAC 4.1.2 or newer):

```text
python -m pip install --upgrade "SpotiFLAC>=4.1.2"
```

The **Settings → Status → Install SpotiFLAC** button performs this install
inside a Pully-owned Python virtual environment. Python 3 must already be
installed, but the button does not change packages in the system Python
environment. Pully detects the new CLI immediately; restarting is unnecessary.
Each install or repair creates a separate environment at its final path and checks
the CLI before activating it. The previous environment remains available if
installation fails or activation is interrupted. Old environments are retained
for recovery and may use disk space. Verification checks the CLI contract; it
does not confirm that an external provider can retrieve a particular track.
Packages are installed by pip from its configured package index; Pully does not
independently pin package hashes. If the CLI cannot start, Pully reports its
startup error separately from missing options.

Analysis reads Spotify's public metadata endpoint directly, so the card shows
the real title, creator, duration, and artwork before download. For downloads,
Pully requests `LOSSLESS` explicitly and enables JSON reporting. Older CLIs
that lack `--quality`, `--registries`, or `--json` are rejected rather than
silently producing a lossy substitute.

The maintained module uses opt-in provider extensions. Pully passes the
reviewable SpotiFLAC extension registry and selects its Tidal lossless provider:

```text
https://raw.githubusercontent.com/zarzet/SpotiFLAC-Extension/main/registry.json
```

SpotiFLAC writes into a unique Pully-managed temporary directory. When the
process exits, Pully recursively discovers the produced audio files, applies
the configured filename and existing-file rules, moves them into the selected
download directory, and removes the temporary directory. Nested album and
playlist output is supported. Cover images and working files are not moved
into the final directory.

The supported SpotiFLAC output is FLAC, so Spotify cards expose only
`Original` and `FLAC`. Pully verifies the FLAC stream signature before moving
the result into the user's download folder. Video, frame-rate, subtitle, and
yt-dlp conversion settings do not apply to this provider.

## Privacy and provider boundary

Pully does not implement credentials, decryption, or access-control bypasses.
It launches the external CLI as a subprocess with an argument array and never
sends Spotify URLs or metadata to yt-dlp. Any provider network activity belongs
to the installed SpotiFLAC tool and the explicitly selected extension.

## Cancellation and progress

Cancellation terminates the SpotiFLAC process tree and removes its temporary
directory. Builds that emit newline-delimited JSON progress events remain
compatible, but JSON output is not required. The standard public CLI shows an
indeterminate downloading state until its output files are finalized.

## Playlist synchronization

New Spotify subscriptions use the CLI's `--playlist` mode, rather than Spotify's Web API. Pully schedules each run through its download queue. SpotiFLAC indexes a dedicated folder beneath the chosen destination and attempts only missing tracks; it can also generate an M3U8 file. This requires a CLI with `--playlist`, `--m3u`, `--no-fallback`, `--subscribe`, and `--check-subscriptions`; the integration has been checked against version 5.0.6. Older Spotify subscriptions without a destination are paused; remove and re-add one to select a folder. Pully does not delete local files when tracks disappear from a playlist.

Pully asks SpotiFLAC for an ordered snapshot of track IDs using its subscription commands and an isolated temporary database. Pully compares snapshots to report additions, removals and reorderings. SpotiFLAC is a third-party provider; availability, matching, and lossless output depend on its services. A successful real-provider playlist download and restart recovery have not yet been verified for this integration.

SpotiFLAC's subscription and playlist modes de-duplicate repeated tracks, so Pully currently compares and writes the order of distinct tracks rather than preserving repeated playlist entries.
