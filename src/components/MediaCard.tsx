import * as ToggleGroup from "@radix-ui/react-toggle-group";
import { Captions, ChevronDown, Download, ListVideo, Music2, Video } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useId, useMemo, useState } from "react";
import { Checkbox } from "./Checkbox";
import { Select } from "./Select";
import { interactionSpring, layoutSpring } from "../lib/animation";
import { canEmbedThumbnail, frameRateValue, qualityValue, sourceFormatFor, videoFormats } from "../lib/formats";
import type { DownloadRequest, MediaFormat, MediaInfo } from "../types/media";
import type { AppSettings } from "../types/settings";

type Props = { media: MediaInfo; settings: AppSettings; busy: boolean; onDownload(request: DownloadRequest): Promise<void> };

function duration(seconds?: number) { if (!seconds) return ""; const h = Math.floor(seconds / 3600); const m = Math.floor(seconds % 3600 / 60); const s = Math.floor(seconds % 60); return h ? `${h}:${String(m).padStart(2,"0")}:${String(s).padStart(2,"0")}` : `${m}:${String(s).padStart(2,"0")}`; }
function qualityLabel(height: number) {
  const tier = height >= 4320 ? "8K" : height >= 2160 ? "4K" : height >= 1440 ? "QHD" : height >= 1080 ? "FHD" : height >= 720 ? "HD" : height >= 360 ? "SD" : "Low";
  return `${tier} · ${height}p`;
}
function lowerFrameRatesFor(format?: MediaFormat) {
  const sourceRate = format && frameRateValue(format);
  if (!sourceRate) return [];
  const source = Number(sourceRate);
  return [...new Set([source, 60, 30, 25, 24].filter((rate) => rate <= source))].map(String);
}

export function MediaCard({ media, settings, busy, onDownload }: Props) {
  const advancedId = useId();
  const reduceMotion = useReducedMotion();
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const videos = useMemo(() => videoFormats(media.formats), [media]);
  const qualityFormats = useMemo(() => videos.filter((format, index) => index === videos.findIndex((candidate) => candidate.height === format.height)), [videos]);
  const hasVideoFormats = videos.length > 0;
  const hasAudioFormats = media.formats.some((format) => format.hasAudio);
  const isSpotify = media.source.toLowerCase() === "spotify";
  const defaultAudioFormat = isSpotify && !["original", "flac"].includes(settings.defaultAudioFormat) ? "original" : settings.defaultAudioFormat;
  const availableModes = useMemo(() => (["video", "audio"] as const).filter((value) => value === "video" ? hasVideoFormats : hasAudioFormats), [hasVideoFormats, hasAudioFormats]);
  const [mode, setMode] = useState<"video"|"audio">(hasVideoFormats ? "video" : "audio");
  const preferredQuality = qualityFormats.some((format) => format.height === Number(settings.defaultVideoQuality)) ? settings.defaultVideoQuality : "best";
  const [quality, setQuality] = useState<string>(preferredQuality);
  const [frameRate, setFrameRate] = useState("best");
  const [outputFormat, setOutputFormat] = useState<string>(hasVideoFormats ? (settings.preferOriginalFormats ? "original" : settings.defaultVideoFormat) : defaultAudioFormat);
  const [metadata, setMetadata] = useState(settings.embedMetadata);
  const [thumbnail, setThumbnail] = useState(settings.embedThumbnail);
  const [subtitles, setSubtitles] = useState(settings.downloadSubtitles || settings.embedSubtitles);
  const thumbnailSupported = !isSpotify && canEmbedThumbnail(mode, outputFormat);
  const videoOptions = useMemo(() => [{ value: "best", label: "Best" }, ...qualityFormats.map((format) => ({ value: qualityValue(format), label: format.height ? qualityLabel(format.height) : format.note || "Original quality" }))], [qualityFormats]);
  const sourceFormat = useMemo(() => sourceFormatFor(videos, quality), [quality, videos]);
  const availableFrameRates = useMemo(() => lowerFrameRatesFor(sourceFormat), [sourceFormat]);
  const frameRateOptions = useMemo(() => [{ value: "best", label: "Best" }, ...availableFrameRates.map((value) => ({ value, label: `${value} fps` }))], [availableFrameRates]);
  const sourceFrameRate = sourceFormat && frameRateValue(sourceFormat);
  const convertsFrameRate = frameRate !== "best" && Boolean(sourceFrameRate) && Number(frameRate) < Number(sourceFrameRate);
  const outputOptions = mode === "video"
    ? [{ value: "mp4", label: "MP4" }, { value: "webm", label: "WebM" }, { value: "mkv", label: "MKV" }, { value: "mov", label: "MOV" }, ...(!convertsFrameRate ? [{ value: "original", label: "Original" }] : [])]
    : isSpotify ? [{ value: "original", label: "Original" }, { value: "flac", label: "FLAC" }] : [{ value: "original", label: "Original" }, { value: "mp3", label: "MP3" }, { value: "m4a", label: "M4A" }, { value: "aac", label: "AAC" }, { value: "opus", label: "Opus" }, { value: "vorbis", label: "OGG Vorbis" }, { value: "flac", label: "FLAC" }, { value: "alac", label: "ALAC" }, { value: "wav", label: "WAV" }];

  const submit = () => {
    const selectedFormat = quality === "best" && frameRate === "best" ? undefined : sourceFormat;
    return onDownload({ url: media.url, title: media.title, creator: media.creator, thumbnail: media.thumbnail, mode, formatId: mode === "video" ? selectedFormat?.id ?? "best" : undefined, formatHasAudio: mode === "video" ? selectedFormat?.hasAudio : undefined, targetFps: mode === "video" && convertsFrameRate ? Number(frameRate) : undefined, outputFormat, outputDirectory: settings.downloadDirectory || undefined, filenameTemplate: settings.filenameTemplate, isPlaylist: media.isPlaylist, playlistFolder: settings.playlistFolder, existingFileBehavior: settings.existingFileBehavior, embedMetadata: metadata, embedThumbnail: thumbnail && thumbnailSupported, saveThumbnail: settings.saveThumbnail, downloadSubtitles: subtitles, embedSubtitles: settings.embedSubtitles });
  };

  const changeQuality = (value: string) => {
    setQuality(value);
    if (frameRate === "best") return;
    const nextSource = sourceFormatFor(videos, value);
    const nextRates = lowerFrameRatesFor(nextSource);
    if (!nextRates.includes(frameRate)) {
      setFrameRate("best");
    } else if (Number(frameRate) < Number(frameRateValue(nextSource)) && outputFormat === "original") {
      setOutputFormat("mp4");
    }
  };

  const changeFrameRate = (value: string) => {
    setFrameRate(value);
    if (value !== "best" && sourceFrameRate && Number(value) < Number(sourceFrameRate) && outputFormat === "original") {
      setOutputFormat("mp4");
    }
  };

  const changeMode = (value: string) => {
    if (value !== "video" && value !== "audio") return;
    setMode(value);
    setOutputFormat(value === "video" ? (settings.preferOriginalFormats ? "original" : settings.defaultVideoFormat) : defaultAudioFormat);
  };

  return <motion.section layout initial={{ opacity: 0, y: 8, scale: 0.995 }} animate={{ opacity: 1, y: 0, scale: 1 }} transition={layoutSpring} whileHover={{ y: -1 }} className="media-card overflow-hidden rounded-[28px] border border-black/[.07] bg-surface shadow-float dark:border-white/[.08]">
    <div className="grid sm:grid-cols-[220px_1fr]">
      <div className="media-thumbnail relative aspect-video bg-black/5 sm:aspect-auto">{media.thumbnail ? <img src={media.thumbnail} className="h-full w-full object-cover transition-transform duration-700" /> : <div className="grid h-full place-items-center text-black/20"><Video /></div>}<span className="pointer-events-none absolute inset-0 bg-gradient-to-t from-black/20 to-transparent opacity-60"/> {media.duration && <span className="absolute bottom-3 right-3 rounded-lg border border-white/10 bg-black/70 px-2 py-1 text-[11px] font-medium text-white shadow-sm backdrop-blur">{duration(media.duration)}</span>}</div>
      <div className="p-6"><div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[.13em] text-black/40 dark:text-white/40"><span>{media.source}</span>{media.isPlaylist && <><span>·</span><ListVideo size={13}/><span>{media.playlistCount} items</span></>}</div><h2 className="mt-2 line-clamp-2 text-xl font-bold tracking-[-.025em]">{media.title}</h2>{media.creator && <p className="mt-1 text-sm text-black/50 dark:text-white/45">{media.creator}</p>}
        {availableModes.length > 1 && <ToggleGroup.Root type="single" value={mode} onValueChange={changeMode} aria-label="Download type" className="mt-5 flex rounded-xl bg-black/[.045] p-1 dark:bg-white/[.06]">
          {availableModes.map((value) => <ToggleGroup.Item key={value} value={value} className={`mode-tab relative isolate ${mode === value ? "active" : ""}`}><AnimatePresence initial={false}>{mode === value && <motion.span layoutId="media-mode" transition={interactionSpring} className="absolute inset-0 -z-10 rounded-lg bg-white shadow-sm dark:bg-white/10"/>}</AnimatePresence>{value === "video" ? <Video size={15}/> : <Music2 size={15}/>} {value === "video" ? "Video" : "Audio"}</ToggleGroup.Item>)}
        </ToggleGroup.Root>}
        <AnimatePresence mode="wait" initial={false}>
          <motion.div layout key={mode} initial={{ opacity: 0, y: 3 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -2 }} transition={{ duration: 0.14 }} className={`mt-4 grid gap-3 ${mode === "video" ? "grid-cols-3" : "grid-cols-2"}`}>
            {mode === "video" ? <><label className="field-label">Quality<Select ariaLabel="Video quality" value={quality} options={videoOptions} onChange={changeQuality}/></label><label className="field-label">Frame rate<Select ariaLabel="Video frame rate" value={frameRate} options={frameRateOptions} onChange={changeFrameRate}/></label></> : <label className="field-label">Audio quality<Select ariaLabel="Audio quality" value="best" options={[{ value: "best", label: "Best" }]} onChange={() => undefined} disabled/></label>}
            <label className="field-label">Format<Select ariaLabel="Output format" value={outputFormat} options={outputOptions} onChange={setOutputFormat}/></label>
          </motion.div>
        </AnimatePresence>
        <AnimatePresence initial={false}>{mode === "audio" && ["flac", "alac", "wav"].includes(outputFormat) && <motion.p initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }} className="mt-3 overflow-hidden text-[11px] leading-relaxed text-black/45 dark:text-white/40">Lossless output avoids further quality loss, but it cannot restore detail missing from a lossy source.</motion.p>}</AnimatePresence>
        <div className="mt-4">
          <button type="button" aria-expanded={advancedOpen} aria-controls={advancedId} onClick={() => setAdvancedOpen((open) => !open)} className="flex items-center gap-2 text-xs font-semibold text-black/50 outline-none transition-colors hover:text-black focus-visible:rounded-sm focus-visible:ring-2 focus-visible:ring-lime/50 dark:text-white/45 dark:hover:text-white">
            Advanced <motion.span animate={{ rotate: advancedOpen ? 180 : 0 }} transition={reduceMotion ? { duration: 0 } : { duration: 0.2, ease: "easeInOut" }}><ChevronDown size={14}/></motion.span>
          </button>
          <AnimatePresence initial={false}>
            {advancedOpen && <motion.div id={advancedId} initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0 }} transition={reduceMotion ? { duration: 0 } : { duration: 0.22, ease: [0.22, 1, 0.36, 1] }} className="overflow-hidden">
              <div className="grid gap-2 pt-3 text-sm"><Checkbox checked={metadata} onChange={setMetadata}>Embed metadata</Checkbox><Checkbox checked={thumbnail && thumbnailSupported} disabled={!thumbnailSupported} onChange={setThumbnail}>Embed thumbnail</Checkbox>{!thumbnailSupported && <p className="text-xs text-black/40 dark:text-white/40">Thumbnail embedding is unavailable for this output format.</p>}{media.subtitles.length > 0 && <Checkbox checked={subtitles} onChange={setSubtitles}><Captions size={15}/> Download subtitles ({media.subtitles.length})</Checkbox>}</div>
            </motion.div>}
          </AnimatePresence>
        </div>
        <motion.button whileHover={busy ? undefined : { y: -1 }} whileTap={busy ? undefined : { scale: 0.985 }} transition={interactionSpring} disabled={busy} onClick={submit} className="mt-5 flex w-full items-center justify-center gap-2 rounded-xl bg-lime px-5 py-3.5 text-sm font-bold text-accentForeground transition-colors hover:brightness-95 disabled:opacity-50"><Download size={17}/>{media.isPlaylist ? "Download playlist" : "Download"}</motion.button>
      </div>
    </div>
  </motion.section>;
}
