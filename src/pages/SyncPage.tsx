import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, ArrowDownToLine, Check, Clock3, FolderOpen, Link2, ListMusic, LoaderCircle, Pause, Play, Plus, RefreshCw, Trash2, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { Select } from "../components/Select";
import { Toggle } from "../components/Toggle";
import { useEffect, useState } from "react";
import { api } from "../lib/tauri";
import type { AppSettings } from "../types/settings";
import type { DownloadRequest, SyncedPlaylist } from "../types/media";

export function SyncPage({ settings }: { settings: AppSettings }) {
  const [items, setItems] = useState<SyncedPlaylist[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [url, setUrl] = useState("");
  const [directory, setDirectory] = useState(settings.downloadDirectory);
  const [interval, setInterval] = useState("60");
  const [customInterval, setCustomInterval] = useState("120");
  const [mode, setMode] = useState<"audio" | "video">("audio");
  const [busy, setBusy] = useState(false);
  const [writeM3u, setWriteM3u] = useState(false);
  const [playlistFolder, setPlaylistFolder] = useState(true);
  const [error, setError] = useState<string>();
  const spotifyLink = /^https:\/\/open\.spotify\.com\/(?:intl-[a-z-]+\/)?playlist\//i.test(url.trim());
  useEffect(() => {
    let mounted = true;
    const subscription = listen<SyncedPlaylist[]>("sync-playlists-changed", ({ payload }) => { if (mounted) setItems(payload); });
    void api.listSyncedPlaylists().then((value) => { if (mounted) setItems(value); }).catch((reason) => { if (mounted) setError(String(reason)); }).finally(() => { if (mounted) setLoading(false); });
    return () => { mounted = false; void subscription.then((stop) => stop()); };
  }, []);
  const add = async () => {
    setBusy(true); setError(undefined);
    try {
      if (!directory.trim()) throw new Error("Choose a destination folder.");
      const media = spotifyLink
        ? await api.analyze(url.trim()).catch(() => ({ url: url.trim(), title: `Spotify playlist ${url.trim().split("?")[0].split("/").pop() ?? ""}`, source: "Spotify", isPlaylist: true, creator: undefined, thumbnail: undefined }))
        : await api.analyze(url.trim());
      if (!media.isPlaylist) throw new Error("Enter a playlist or collection link.");

      const request: DownloadRequest = { url: media.url, title: media.title, creator: media.creator, thumbnail: media.thumbnail, mode: spotifyLink ? "audio" : mode, outputFormat: spotifyLink ? "flac" : mode === "audio" ? settings.defaultAudioFormat : settings.defaultVideoFormat, outputDirectory: directory, filenameTemplate: settings.filenameTemplate, isPlaylist: true, playlistFolder, existingFileBehavior: "skip", embedMetadata: settings.embedMetadata, embedThumbnail: settings.embedThumbnail, saveThumbnail: settings.saveThumbnail, downloadSubtitles: settings.downloadSubtitles, embedSubtitles: settings.embedSubtitles };
      const minutes = Number(interval === "custom" ? customInterval : interval);
      if (!Number.isInteger(minutes) || minutes < 15 || minutes > 10080) throw new Error("Choose an interval between 15 and 10080 minutes.");
      await api.addSyncedPlaylist(request, minutes, writeM3u); setUrl(""); setShowForm(false);
    } catch (cause) { setError(String(cause)); } finally { setBusy(false); }
  };
  const action = async (id: string, value: "pause" | "resume" | "sync" | "remove" | "folder" | "root") => {
    if (value === "remove" && !window.confirm("Remove this playlist from sync? Downloaded files will stay on your computer.")) return;
    try { setError(undefined); await api.changeSyncedPlaylist(id, value); } catch (cause) { setError(String(cause)); }
  };
  const running = items.filter((item) => item.jobId).length;
  const enabled = items.filter((item) => !item.paused).length;
  return <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} className="sync-screen">
    <div className="sync-content">
      <header className="sync-header">
        <div><h1>Synced playlists</h1><p>Your collections, kept up to date on this device.</p></div>
        <button type="button" className="primary-button sync-add-button" onClick={() => setShowForm((value) => !value)} aria-expanded={showForm} aria-controls="sync-add-form">
          {showForm ? <X size={16}/> : <Plus size={16}/>} {showForm ? "Close" : "Add playlist"}
        </button>
      </header>

      <AnimatePresence initial={false}>
        {showForm && <motion.section id="sync-add-form" className="sync-form" initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }} transition={{ duration: 0.18 }}>
          <div className="sync-form-inner">
            <div className="sync-form-heading"><div><h2>Set up a playlist</h2><p>Choose a collection and how often Pully should check it.</p></div><span className="sync-form-icon"><ListMusic size={19}/></span></div>
            <div className="sync-form-grid">
              <label className="sync-field sync-field--wide"><span>Playlist link</span><div className="sync-input-wrap"><Link2 size={16}/><input value={url} onChange={(event) => setUrl(event.target.value)} placeholder="Paste a playlist URL" autoFocus/></div></label>
              <label className="sync-field sync-field--wide"><span>Save to</span><div className="sync-destination"><div className="sync-input-wrap"><FolderOpen size={16}/><input value={directory} onChange={(event) => setDirectory(event.target.value)} placeholder="Choose a destination folder"/></div><button type="button" className="secondary-button" onClick={() => void open({ directory: true, multiple: false }).then((value) => { if (typeof value === "string") setDirectory(value); }).catch((cause) => setError(String(cause)))}>Browse</button></div></label>
              <div className="sync-field"><span>Check for changes</span><Select ariaLabel="Synchronization interval" value={interval} onChange={setInterval} options={[{ value: "15", label: "Every 15 minutes" }, { value: "30", label: "Every 30 minutes" }, { value: "60", label: "Every hour" }, { value: "1440", label: "Every day" }, { value: "custom", label: "Custom interval" }]}/>{interval === "custom" && <input className="settings-input mt-2" aria-label="Custom interval in minutes" type="number" min="15" max="10080" step="1" value={customInterval} onChange={(event) => setCustomInterval(event.target.value)}/>}</div>
              {!spotifyLink && <div className="sync-field"><span>Save as</span><Select ariaLabel="Media type" value={mode} onChange={(value) => setMode(value as "audio" | "video")} options={[{ value: "audio", label: "Audio" }, { value: "video", label: "Video" }]}/></div>}
            </div>
            <div className="mt-4"><Toggle checked={playlistFolder} onChange={setPlaylistFolder} label="Save in a playlist subfolder" description={playlistFolder ? "Keep this playlist in its own folder." : "Save tracks directly in the selected folder."}/><Toggle checked={writeM3u} onChange={setWriteM3u} label="Create an M3U8 playlist" description="Keep a playable list in the destination folder."/></div>
            {spotifyLink && <p className="sync-support-note">Spotify playlists use SpotiFLAC and save FLAC files. Install or update SpotiFLAC in Settings if needed.</p>}
            <div className="sync-form-footer"><p>{spotifyLink ? "Spotify audio ? FLAC" : `Uses your default ${mode} format and naming settings.`}</p><button type="button" className="primary-button" disabled={busy || !url.trim()} onClick={() => void add()}>{busy ? <LoaderCircle className="animate-spin" size={16}/> : <Plus size={16}/>} {busy ? "Checking playlist?" : "Start syncing"}</button></div>
            <p className="sync-support-note">Spotify playlists require a compatible SpotiFLAC installation.</p>
          </div>
        </motion.section>}
      </AnimatePresence>

      {error && <div role="alert" className="sync-alert"><AlertCircle size={17}/><span>{error}</span></div>}
      <div className="sync-section-heading"><div><h2>Your playlists</h2><p>{loading ? "Loading your collections?" : items.length ? `${items.length} ${items.length === 1 ? "playlist" : "playlists"} ? ${running ? `${running} syncing` : `${enabled} active`}` : "Everything you sync will appear here."}</p></div></div>
      {loading ? <div className="sync-loading"><span className="skeleton"/><span className="skeleton"/></div> : items.length === 0 ? <div className="sync-empty-state"><div className="sync-empty-art" aria-hidden="true"><span className="sync-empty-disc"><ListMusic size={26}/></span><span className="sync-empty-badge"><ArrowDownToLine size={15}/></span></div><h3>Keep your favorites close</h3><p>Add a playlist to check for new items automatically and save them to a folder you choose.</p>{!showForm && <button type="button" className="secondary-button" onClick={() => setShowForm(true)}><Plus size={15}/> Add your first playlist</button>}</div> : <div className="sync-list">{items.map((item) => <article key={item.id} className="sync-card">
        <div className="sync-card-main"><div className="sync-artwork">{item.request.thumbnail ? <img src={item.request.thumbnail} alt=""/> : <ListMusic size={23}/>}</div><div className="sync-card-info"><div className="sync-card-title"><h3 title={item.request.title}>{item.request.title}</h3><span className={`sync-status ${item.error ? "is-error" : item.jobId ? "is-syncing" : item.paused ? "is-paused" : "is-ready"}`}><i/>{item.error ? "Needs attention" : item.jobId ? "Syncing" : item.paused ? "Paused" : "Active"}</span></div><p className="sync-card-folder" title={item.request.outputDirectory}><FolderOpen size={13}/>{item.request.url.includes("open.spotify.com/") ? (item.spotifyDownload ? "Spotify ? FLAC" : "Spotify ? needs a folder") : item.request.outputDirectory}</p><div className="sync-card-times"><span><Clock3 size={13}/>{item.jobId ? "Sync in progress" : item.paused ? "Schedule paused" : `Next: ${new Date(item.nextRun * 1000).toLocaleString()}`}</span><span>{item.lastSuccess ? `Last synced: ${new Date(item.lastSuccess * 1000).toLocaleString()}` : "Not synced yet"}</span></div></div></div>
        {item.changes && <p className="sync-changes"><Check size={14}/>{item.changes}</p>}
        {item.error && <p className="sync-card-error"><AlertCircle size={14}/>{item.error}</p>}
        <div className="mt-3"><Toggle checked={item.request.playlistFolder} disabled={Boolean(item.jobId)} onChange={(value) => void action(item.id, value ? "folder" : "root")} label="Save in a playlist subfolder" description="Changing this applies to future syncs; existing files stay where they are."/></div>
        <div className="sync-card-actions"><button type="button" className="sync-text-action" disabled={Boolean(item.jobId)} onClick={() => void action(item.id, "sync")}><RefreshCw size={14}/> Sync now</button><button type="button" className="sync-text-action" onClick={() => void action(item.id, item.paused ? "resume" : "pause")}>{item.paused ? <Play size={14}/> : <Pause size={14}/>} {item.paused ? "Resume" : "Pause"}</button><button type="button" className="sync-text-action sync-remove" disabled={Boolean(item.jobId)} onClick={() => void action(item.id, "remove")}><Trash2 size={14}/> Remove</button></div>
      </article>)}</div>}
    </div>
  </motion.div>;
}
