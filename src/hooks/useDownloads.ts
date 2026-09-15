import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/tauri";
import type { DownloadProgress, DownloadRequest } from "../types/media";

export function useDownloads() {
  const [downloads, setDownloads] = useState<DownloadProgress[]>([]);

  useEffect(() => {
    const unlisten = listen<DownloadProgress>("download-progress", ({ payload }) => {
      setDownloads((items) => {
        const index = items.findIndex((item) => item.id === payload.id);
        if (index < 0) return [payload, ...items];
        return items
          .filter((item, itemIndex) => item.id !== payload.id || itemIndex === index)
          .map((item) => item.id === payload.id ? payload : item);
      });
    });
    return () => { void unlisten.then((fn) => fn()); };
  }, []);

  const start = useCallback(async (request: DownloadRequest) => {
    const id = await api.startDownload(request);
    setDownloads((items) => items.some((item) => item.id === id)
      ? items
      : [{ id, title: request.title, thumbnail: request.thumbnail, status: "waiting", percent: 0 }, ...items]);
  }, []);

  const remove = useCallback(async (id: string) => {
    await api.removeDownload(id);
    setDownloads((items) => items.filter((item) => item.id !== id));
  }, []);

  return { downloads, start, cancel: api.cancelDownload, retry: api.retryDownload, remove, openFile: api.openFile, revealFile: api.revealFile };
}
