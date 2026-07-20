import { createContext, useContext, type ReactNode } from "react";
import type { DownloadItem } from "../types";
import type { SharedDialog } from "../components/DialogRenderer";
import type { DialogActions } from "../hooks/useDialog";
import type { SelectionActions } from "../hooks/useSelection";

export type { SharedDialog as Dialog };

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

/** App-wide state with behavior-rich hooks instead of raw setters. */
interface AppCtx {
  dialog: SharedDialog;
  dialogActions: DialogActions;
  selectedIds: Set<number>;
  selectionActions: SelectionActions;
  filter: "all" | "completed" | "incomplete";
  setFilter: (f: "all" | "completed" | "incomplete") => void;
  actions: AppActions;
  onRedownloadItem?: DownloadItem;
}

const AppContext = createContext<AppCtx | null>(null);

interface AppProviderProps {
  dialog: SharedDialog;
  dialogActions: DialogActions;
  selectedIds: Set<number>;
  selectionActions: SelectionActions;
  filter: "all" | "completed" | "incomplete";
  setFilter: (f: "all" | "completed" | "incomplete") => void;
  actions: AppActions;
  onRedownloadItem?: DownloadItem;
  children: ReactNode;
}

export function AppProvider({
  dialog, dialogActions,
  selectedIds, selectionActions,
  filter, setFilter,
  actions, onRedownloadItem,
  children,
}: AppProviderProps) {
  return (
    <AppContext.Provider value={{
      dialog, dialogActions,
      selectedIds, selectionActions,
      filter, setFilter,
      actions, onRedownloadItem,
    }}>
      {children}
    </AppContext.Provider>
  );
}

export function useAppContext() {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useAppContext must be used within AppProvider");
  return ctx;
}
