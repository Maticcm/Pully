import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useCallback, useEffect, useRef, useState } from "react";

export type UpdateStatus = {
  phase: "idle" | "checking" | "current" | "waiting" | "downloading" | "installing" | "error";
  version?: string;
  progress?: number;
  message?: string;
};

export function useAppUpdater(enabled: boolean, idle: boolean) {
  const [status, setStatus] = useState<UpdateStatus>({ phase: "idle" });
  const updateRef = useRef<Update | null>(null);
  const checkingRef = useRef(false);
  const installingRef = useRef(false);
  const downloadedRef = useRef(false);
  const idleRef = useRef(idle);
  const enabledRef = useRef(enabled);
  const manualInstallRef = useRef(false);
  idleRef.current = idle;
  enabledRef.current = enabled;

  const checkForUpdates = useCallback(async (manual = false) => {
    if (import.meta.env.DEV || checkingRef.current || installingRef.current) return;
    if (updateRef.current) {
      if (manual) {
        manualInstallRef.current = true;
        setStatus({ phase: "waiting", version: updateRef.current.version });
      }
      return;
    }
    checkingRef.current = true;
    if (manual) setStatus({ phase: "checking" });
    try {
      const update = await check();
      if (update) {
        manualInstallRef.current = manual;
        updateRef.current = update;
        setStatus({ phase: "waiting", version: update.version });
      } else if (manual) {
        setStatus({ phase: "current" });
      }
    } catch (error) {
      if (manual) setStatus({ phase: "error", message: String(error) });
    } finally {
      checkingRef.current = false;
    }
  }, []);

  useEffect(() => {
    if (!enabled || import.meta.env.DEV) return;
    void checkForUpdates();
    const timer = window.setInterval(() => void checkForUpdates(), 24 * 60 * 60 * 1000);
    return () => window.clearInterval(timer);
  }, [enabled, checkForUpdates]);

  useEffect(() => {
    if ((!enabled && !manualInstallRef.current) || !idle || !updateRef.current || installingRef.current) return;
    const update = updateRef.current;
    installingRef.current = true;
    let received = 0;
    let total = 0;
    if (!downloadedRef.current) setStatus({ phase: "downloading", version: update.version });
    void (async () => {
      if (!downloadedRef.current) {
        await update.download((event) => {
          if (event.event === "Started") total = event.data.contentLength ?? 0;
          if (event.event === "Progress") received += event.data.chunkLength;
          if (total > 0) setStatus({ phase: "downloading", version: update.version, progress: Math.min(100, Math.round(received / total * 100)) });
        });
        downloadedRef.current = true;
      }
      // Download activity can begin while the package is arriving. Recheck
      // the live queue before launching the installer.
      if (!idleRef.current || (!enabledRef.current && !manualInstallRef.current)) {
        setStatus({ phase: "waiting", version: update.version });
        installingRef.current = false;
        return;
      }
      setStatus({ phase: "installing", version: update.version });
      await update.install();
      // The Windows installer exits and restarts the app itself.
      if (!navigator.userAgent.includes("Windows")) await relaunch();
    })().catch((error) => {
      setStatus({ phase: "error", message: String(error) });
      installingRef.current = false;
      downloadedRef.current = false;
      updateRef.current = null;
      void update.close();
    });
  }, [enabled, idle, status.phase]);

  return { status, checkForUpdates };
}
