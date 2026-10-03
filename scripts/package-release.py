"""Package a built, signed Windows release without including private keys."""

import hashlib
import json
import shutil
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSION = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["version"]
TARGET = ROOT / "src-tauri/target/release"
OUTPUT = ROOT / "src-tauri/target/release-assets"
OUTPUT.mkdir(parents=True, exist_ok=True)

installer = TARGET / f"bundle/nsis/Pully_{VERSION}_x64-setup.exe"
signature = installer.with_suffix(".exe.sig")
manifest = TARGET / "bundle/nsis/latest.json"
msi = TARGET / f"bundle/msi/Pully_{VERSION}_x64_en-US.msi"
for source in (installer, signature, manifest, msi):
    if not source.is_file():
        raise RuntimeError(f"Missing release file: {source.name}")
    shutil.copy2(source, OUTPUT / source.name)
feed = json.loads(manifest.read_text(encoding="utf-8"))
if feed["version"] != VERSION or feed["platforms"]["windows-x86_64"]["signature"] != signature.read_text().strip():
    raise RuntimeError("Update manifest and installer signature do not match")
if signature.stat().st_mtime < installer.stat().st_mtime:
    raise RuntimeError("The installer must be signed again after rebuilding")

standalone = OUTPUT / f"Pully_{VERSION}_x64-standalone.zip"
with zipfile.ZipFile(standalone, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
    archive.write(TARGET / "pully.exe", "pully.exe")
    for source in sorted((ROOT / "src-tauri/binaries").iterdir()):
        if source.is_file() and not source.name.startswith("."):
            archive.write(source, "binaries/" + source.name)
    archive.write(ROOT / "LICENSE", "LICENSE.txt")
    archive.writestr("README.txt", f"Pully {VERSION} for Windows x64\n\nExtract this entire ZIP into a folder and run pully.exe. Keep the binaries folder beside it.\nNo installer is required. Windows WebView2 is required and is included in current Windows installations.\nPreferences and optional SpotiFLAC tools are stored in Pully's app data folder.\nSpotiFLAC can be installed from Settings > Status and requires Python 3.\nApp updates use the signed NSIS installer when idle; disable automatic updates in Settings > Status if you prefer to replace this folder manually.\n\nhttps://github.com/Maticcm/Pully\n")

extension_version = json.loads((ROOT / "browser-extension/package.json").read_text())["version"]
extension = OUTPUT / f"Pully-browser-extension-{extension_version}.zip"
extension_dist = ROOT / "browser-extension/dist"
with zipfile.ZipFile(extension, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
    for source in sorted(extension_dist.rglob("*")):
        if source.is_file():
            archive.write(source, source.relative_to(extension_dist).as_posix())
    archive.write(ROOT / "LICENSE", "LICENSE.txt")
    archive.write(ROOT / "browser-extension/README.md", "README.md")

with zipfile.ZipFile(standalone) as archive:
    required = {"pully.exe", "binaries/yt-dlp.exe", "binaries/ffmpeg.exe", "binaries/deno.exe", "binaries/pully-native-host.exe", "LICENSE.txt"}
    if not required.issubset(archive.namelist()) or archive.testzip() is not None:
        raise RuntimeError("Standalone ZIP validation failed")
with zipfile.ZipFile(extension) as archive:
    if "manifest.json" not in archive.namelist() or any("keys/" in name or "node_modules/" in name for name in archive.namelist()) or archive.testzip() is not None:
        raise RuntimeError("Extension ZIP validation failed")

assets = [OUTPUT / source.name for source in (installer, signature, msi, standalone, extension, manifest)]
lines = []
for asset in sorted(assets):
    with asset.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    lines.append(f"{digest}  {asset.name}")
(OUTPUT / "SHA256SUMS.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
for asset in assets + [OUTPUT / "SHA256SUMS.txt"]:
    print(f"{asset.name}: {asset.stat().st_size} bytes")
