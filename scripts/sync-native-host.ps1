$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$profile = if ($env:TAURI_ENV_DEBUG -eq 'true') { 'debug' } else { 'release' }
$source = Join-Path $projectRoot "src-tauri\target\$profile\pully-native-host.exe"
$destination = Join-Path $projectRoot 'src-tauri\binaries\pully-native-host.exe'
if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
  throw "The native messaging host was not built: $source"
}
Copy-Item -LiteralPath $source -Destination $destination -Force
Write-Host "Bundling current native messaging host from $source"
