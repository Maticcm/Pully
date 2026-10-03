import type { DownloadRequest, MediaFormat, MediaInfo } from "../types/media";
import type { AppSettings } from "../types/settings";

export function canEmbedThumbnail(mode: "video" | "audio", format: string) {
  return mode === "video"
    ? ["mp4", "mkv", "mov"].includes(format)
    : ["mp3", "m4a", "flac", "opus", "vorbis", "alac"].includes(format);
}

export function qualityValue(format: MediaFormat) {
  return format.height ? String(format.height) : "original";
}

export function frameRateValue(format: MediaFormat) {
  return format.fps && format.fps > 0 ? String(Math.round(format.fps)) : undefined;
}

export function videoFormats(formats: MediaFormat[]) {
  return formats.filter((format) => format.hasVideo).sort((a, b) => {
    const resolutionDifference = (b.height ?? 0) - (a.height ?? 0);
    if (resolutionDifference) return resolutionDifference;
    const frameRateDifference = (b.fps ?? 0) - (a.fps ?? 0);
    if (frameRateDifference) return frameRateDifference;
    const directDifference = Number(b.protocol === "https") - Number(a.protocol === "https");
    if (directDifference) return directDifference;
    return (b.fileSize ?? 0) - (a.fileSize ?? 0);
  });
}

export function sourceFormatFor(videos: MediaFormat[], quality: string) {
  const matching = quality === "best" ? videos : videos.filter((format) => qualityValue(format) === quality);
  return matching.find((format) => !format.hasAudio) ?? matching[0];
}

/** Builds a fully-automatic download request from a media analysis using the user's quick-mode preset. */
export function buildQuickDownloadRequest(media: MediaInfo, settings: AppSettings): DownloadRequest {
  const videos = videoFormats(media.formats);
  const hasVideoFormats = videos.length > 0;
  const hasAudioFormats = media.formats.some((format) => format.hasAudio);
  const mode = settings.quickModeType === "video" && hasVideoFormats
    ? "video"
    : hasAudioFormats ? "audio" : "video";

  if (mode === "video") {
    const selected = settings.quickModeVideoQuality === "best" ? undefined : sourceFormatFor(videos, settings.quickModeVideoQuality);
    return {
      url: media.url,
      title: media.title,
      creator: media.creator,
      thumbnail: media.thumbnail,
      mode,
      formatId: selected?.id,
      formatHasAudio: selected?.hasAudio,
      outputFormat: settings.quickModeVideoFormat,
      outputDirectory: settings.downloadDirectory || undefined,
      filenameTemplate: settings.filenameTemplate,
      isPlaylist: media.isPlaylist,
      playlistFolder: settings.playlistFolder,
      existingFileBehavior: settings.existingFileBehavior,
      embedMetadata: settings.quickModeEmbedMetadata,
      embedThumbnail: settings.quickModeEmbedThumbnail && canEmbedThumbnail(mode, settings.quickModeVideoFormat),
      saveThumbnail: settings.saveThumbnail,
      downloadSubtitles: settings.quickModeDownloadSubtitles,
      embedSubtitles: settings.embedSubtitles && settings.quickModeDownloadSubtitles,
    };
  }
  return {
    url: media.url,
    title: media.title,
    creator: media.creator,
    thumbnail: media.thumbnail,
    mode,
    outputFormat: settings.quickModeAudioFormat,
    outputDirectory: settings.downloadDirectory || undefined,
    filenameTemplate: settings.filenameTemplate,
    isPlaylist: media.isPlaylist,
    playlistFolder: settings.playlistFolder,
    existingFileBehavior: settings.existingFileBehavior,
    embedMetadata: settings.quickModeEmbedMetadata,
    embedThumbnail: settings.quickModeEmbedThumbnail && media.source.toLowerCase() !== "spotify" && canEmbedThumbnail(mode, settings.quickModeAudioFormat),
    saveThumbnail: settings.saveThumbnail,
    downloadSubtitles: settings.quickModeDownloadSubtitles,
    embedSubtitles: settings.embedSubtitles && settings.quickModeDownloadSubtitles,
  };
}
