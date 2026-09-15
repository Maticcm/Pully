import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/tauri";
import { defaultSettings, type AppSettings } from "../types/settings";

const storageKey = "pully.settings.v1";

function loadSettings(): AppSettings {
  try {
    const stored = JSON.parse(localStorage.getItem(storageKey) ?? "{}");
    return { ...defaultSettings, ...stored };
  } catch {
    return defaultSettings;
  }
}

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings>(loadSettings);

  useEffect(() => {
    void api.setConcurrency(settings.concurrentDownloads).catch(() => undefined);
  }, [settings.concurrentDownloads]);
  useEffect(() => {
    void api.setBrowserIntegrationEnabled(settings.browserIntegrationEnabled).catch(() => undefined);
  }, [settings.browserIntegrationEnabled]);
  useEffect(() => {
    void api.setRunInTray(settings.runInTray).catch(() => undefined);
  }, [settings.runInTray]);

  const save = useCallback((next: AppSettings) => {
    const normalized = {
      ...next,
      concurrentDownloads: Math.max(1, Math.min(6, Math.round(next.concurrentDownloads))),
      filenameTemplate: next.filenameTemplate.trim() || defaultSettings.filenameTemplate,
    };
    localStorage.setItem(storageKey, JSON.stringify(normalized));
    setSettings(normalized);
  }, []);

  const reset = useCallback(() => {
    localStorage.removeItem(storageKey);
    setSettings(defaultSettings);
  }, []);

  return { settings, save, reset };
}
