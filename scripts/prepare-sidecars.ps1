param(
  [Parameter(Mandatory=$true)][string]$YtDlp,
  [Parameter(Mandatory=$true)][string]$Ffmpeg,
  # Optional: a SpotiFLAC-compatible CLI implementing docs/SPOTIFLAC.md's contract.
  # Only needed to support Spotify links; everything else works without it.
  [Parameter(Mandatory=$false)][string]$SpotiFlac
)
$ErrorActionPreference = 'Stop'
$target = Join-Path $PSScriptRoot '..\src-tauri\binaries'
New-Item -ItemType Directory -Force -Path $target | Out-Null
Copy-Item -LiteralPath $YtDlp -Destination (Join-Path $target 'yt-dlp.exe') -Force
Copy-Item -LiteralPath $Ffmpeg -Destination (Join-Path $target 'ffmpeg.exe') -Force
if ($SpotiFlac) {
  Copy-Item -LiteralPath $SpotiFlac -Destination (Join-Path $target 'spotiflac.exe') -Force
}
Write-Host "Prepared verified sidecars in $target"
