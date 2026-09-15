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
