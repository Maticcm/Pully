import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

async function loadTypeScript(relativePath) {
  const source = await readFile(new URL(relativePath, import.meta.url), "utf8");
  const { outputText } = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2020 },
  });
  return import(`data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`);
}

const { buildQuickDownloadRequest } = await loadTypeScript("../src/lib/formats.ts");
const { defaultSettings } = await loadTypeScript("../src/types/settings.ts");
const media = {
  url: "https://open.spotify.com/track/example",
  title: "Track",
  source: "Spotify",
  isPlaylist: false,
  formats: [{ hasVideo: false, hasAudio: true }],
};

for (const format of ["original", "flac", "mp3", "m4a", "aac", "alac", "opus", "vorbis", "wav"]) {
  const settings = { ...defaultSettings, quickModeAudioFormat: format };
  const spotify = buildQuickDownloadRequest(media, settings);
  assert.equal(spotify.mode, "audio");
  assert.equal(spotify.outputFormat, ["original", "flac"].includes(format) ? format : "original");
  assert.equal(spotify.embedThumbnail, false);
  assert.equal(buildQuickDownloadRequest({ ...media, source: "SoundCloud" }, settings).outputFormat, format);
}
assert.equal(buildQuickDownloadRequest(media, defaultSettings).outputFormat, "original");
console.log("Passed: Spotify quick mode and non-Spotify audio format presets.");
