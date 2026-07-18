import { createContext, useContext, useState, type ReactNode } from "react";

export type Dialog =
  | { type: "delete"; ids: number[] }
  | { type: "settings" }
  | { type: "about" }
  | { type: "extension" }
  | { type: "log" }
  | null;

interface AppState {
  dialog: Dialog;
  setDialog: (d: Dialog) => void;
  selectedIds: Set<number>;
  setSelectedIds: (ids: Set<number>) => void;
  filter: "all" | "completed" | "incomplete";
  setFilter: (f: "all" | "completed" | "incomplete") => void;
}

const AppContext = createContext<AppState | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
  const [dialog, setDialog] = useState<Dialog>(null);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [filter, setFilter] = useState<"all" | "completed" | "incomplete">("all");

  return (
    <AppContext.Provider value={{ dialog, setDialog, selectedIds, setSelectedIds, filter, setFilter }}>
      {children}
    </AppContext.Provider>
  );
}

export function useAppContext() {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useAppContext must be used within AppProvider");
  return ctx;
}
