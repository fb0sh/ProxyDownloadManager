import { useState, useCallback } from "react";
import type { SharedDialog } from "../components/DialogRenderer";

export interface DialogActions {
  /** Open a settings dialog. */
  openSettings: () => void;
  /** Open the delete confirmation for specific IDs. */
  openDelete: (ids: number[]) => void;
  /** Open the about dialog. */
  openAbout: () => void;
  /** Open the browser extension instructions. */
  openExtension: () => void;
  /** Open the log viewer. */
  openLog: () => void;
  /** Close the current dialog. */
  closeDialog: () => void;
}

export function useDialog(): { dialog: SharedDialog } & DialogActions {
  const [dialog, setDialog] = useState<SharedDialog>(null);

  const openSettings = useCallback(() => setDialog({ type: "settings" }), []);
  const openDelete = useCallback((ids: number[]) => setDialog({ type: "delete", ids }), []);
  const openAbout = useCallback(() => setDialog({ type: "about" }), []);
  const openExtension = useCallback(() => setDialog({ type: "extension" }), []);
  const openLog = useCallback(() => setDialog({ type: "log" }), []);
  const closeDialog = useCallback(() => setDialog(null), []);

  return { dialog, openSettings, openDelete, openAbout, openExtension, openLog, closeDialog };
}
