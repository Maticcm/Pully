import { ArrowLeft, ArrowRight, Maximize2, Minimize2, Minus, RefreshCw, SlidersHorizontal, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { motion } from "motion/react";
import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { interactionSpring } from "../lib/animation";

type Props = {
  canGoBack: boolean;
  canGoForward: boolean;
  reloadDisabled: boolean;
  settingsActive: boolean;
  onBack(): void;
  onForward(): void;
  onSettings(): void;
};

const appWindow = getCurrentWindow();

export function TitleBar({ canGoBack, canGoForward, reloadDisabled, settingsActive, onBack, onForward, onSettings }: Props) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    void appWindow.isMaximized().then(setMaximized).catch(() => undefined);
    const unlisten = appWindow.onResized(() => { void appWindow.isMaximized().then(setMaximized); });
    return () => { void unlisten.then((dispose) => dispose()); };
  }, []);

  const toggleMaximize = async () => {
    await appWindow.toggleMaximize();
    setMaximized(await appWindow.isMaximized());
  };

  return <div data-tauri-drag-region onDoubleClick={() => void toggleMaximize()} className="fixed inset-x-0 top-0 z-50 flex h-10 select-none items-center justify-between border-b border-black/[.08] bg-canvas/95 pl-2 backdrop-blur dark:border-white/[.08]">
    <div className="flex h-full items-center gap-0.5">
      <TitleButton label="Back" disabled={!canGoBack} onClick={onBack}><ArrowLeft size={15}/></TitleButton>
      <TitleButton label="Forward" disabled={!canGoForward} onClick={onForward}><ArrowRight size={15}/></TitleButton>
      <TitleButton label={reloadDisabled ? "Refresh unavailable during downloads" : "Refresh"} disabled={reloadDisabled} onClick={() => location.reload()}><RefreshCw size={14}/></TitleButton>
    </div>
    <div className="flex h-full items-center">
      <TitleButton label="Settings" active={settingsActive} onClick={onSettings}><SlidersHorizontal size={15}/></TitleButton>
      <WindowButton label="Minimize" onClick={() => void appWindow.minimize()}><Minus size={16}/></WindowButton>
      <WindowButton label={maximized ? "Restore" : "Maximize"} onClick={() => void toggleMaximize()}>{maximized ? <Minimize2 size={14}/> : <Maximize2 size={14}/>}</WindowButton>
      <WindowButton label="Close" close onClick={() => void appWindow.close()}><X size={16}/></WindowButton>
    </div>
  </div>;
}

function TitleButton({ label, disabled, active, onClick, children }: { label: string; disabled?: boolean; active?: boolean; onClick(): void; children: ReactNode }) {
  return <motion.button whileTap={disabled ? undefined : { scale: 0.9 }} transition={interactionSpring} aria-label={label} title={label} disabled={disabled} onDoubleClick={(event) => event.stopPropagation()} onClick={onClick} className={`title-button w-9 ${active ? "bg-black/[.07] text-black dark:bg-white/10 dark:text-white" : ""}`}>{children}</motion.button>;
}

function WindowButton({ label, close, onClick, children }: { label: string; close?: boolean; onClick(): void; children: ReactNode }) {
  return <motion.button whileTap={{ scale: 0.92 }} transition={interactionSpring} aria-label={label} title={label} onDoubleClick={(event) => event.stopPropagation()} onClick={onClick} className={`title-button w-11 ${close ? "hover:!bg-red-600 hover:!text-white" : ""}`}>{children}</motion.button>;
}
