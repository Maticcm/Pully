param(
    [string]$Repository = 'Maticcm/Pully',
    [string]$BundleDirectory = 'src-tauri/target/release/bundle/nsis'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$bundle = Join-Path $root $BundleDirectory
$version = (Get-Content -LiteralPath (Join-Path $root 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
$name = "Pully_${version}_x64-setup.exe"
$installer = Join-Path $bundle $name
$signatureFile = "$installer.sig"
if (-not (Test-Path -LiteralPath $installer) -or -not (Test-Path -LiteralPath $signatureFile)) {
    throw "Signed installer or signature missing in $bundle"
}

$signature = (Get-Content -LiteralPath $signatureFile -Raw).Trim()
$manifest = [ordered]@{
    version = $version
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            url = "https://github.com/$Repository/releases/download/v$version/$name"
            signature = $signature
        }
    }
}
$output = Join-Path $bundle 'latest.json'
[System.IO.File]::WriteAllText($output, ($manifest | ConvertTo-Json -Depth 5), [System.Text.UTF8Encoding]::new($false))
$feedDirectory = Join-Path $root 'updates'
New-Item -ItemType Directory -Force -Path $feedDirectory | Out-Null
[System.IO.File]::WriteAllText((Join-Path $feedDirectory 'latest.json'), ($manifest | ConvertTo-Json -Depth 5), [System.Text.UTF8Encoding]::new($false))
Write-Output $output
