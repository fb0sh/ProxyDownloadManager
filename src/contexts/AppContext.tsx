import { createContext, useContext, useState, type ReactNode } from "react";
import type { DownloadItem } from "../types";

export type Dialog =
  | { type: "delete"; ids: number[] }
  | { type: "settings" }
  | { type: "about" }
  | { type: "extension" }
  | { type: "log" }
  | null;

export interface AppActions {
  onNewDownload: () => void;
  onExtension: () => void;
  onLog: () => void;
  onSettings: () => void;
  onAbout: () => void;
  onQuit: () => void;
  onResumeSelected: () => void;
  onPauseSelected: () => void;
  onDeleteSelected: () => void;
  onStop: (id: number) => void;
  onDelete: (ids: number[]) => void;
  onProperties: (id: number) => void;
  onRedownload: (item: DownloadItem) => void;
}

interface AppState {
  dialog: Dialog;
  setDialog: (d: Dialog) => void;
  selectedIds: Set<number>;
  setSelectedIds: (ids: Set<number>) => void;
  filter: "all" | "completed" | "incomplete";
  setFilter: (f: "all" | "completed" | "incomplete") => void;
  actions: AppActions;
  onRedownloadItem?: DownloadItem;
}

const AppContext = createContext<AppState | null>(null);

interface AppProviderProps {
  actions: AppActions;
  onRedownloadItem?: DownloadItem;
  children: ReactNode;
}

export function AppProvider({ actions, onRedownloadItem, children }: AppProviderProps) {
  const [dialog, setDialog] = useState<Dialog>(null);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [filter, setFilter] = useState<"all" | "completed" | "incomplete">("all");

  return (
    <AppContext.Provider value={{ dialog, setDialog, selectedIds, setSelectedIds, filter, setFilter, actions, onRedownloadItem }}>
      {children}
    </AppContext.Provider>
  );
}

export function useAppContext() {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useAppContext must be used within AppProvider");
  return ctx;
}
