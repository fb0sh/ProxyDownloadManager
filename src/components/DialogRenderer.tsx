import type { ReactNode } from "react";
import DeleteDialog from "./dialogs/DeleteDialog";
import SettingsDialog from "./dialogs/SettingsDialog";
import AboutDialog from "./dialogs/AboutDialog";
import ExtensionDialog from "./dialogs/ExtensionDialog";
import LogDialog from "./dialogs/LogDialog";

/** Shared dialog types used by both main app and demo. */
export type SharedDialog =
  | { type: "delete"; ids: number[] }
  | { type: "settings" }
  | { type: "about" }
  | { type: "extension" }
  | { type: "log" }
  | null;

interface DialogRendererProps<T extends SharedDialog> {
  dialog: T;
  onClose: () => void;
  /** Called when user clicks "download update" from About dialog. */
  onDownloadUpdate: (url: string) => void;
  /** Render extra dialog types not in SharedDialog. Receives the full dialog object. */
  renderExtras?: (dialog: T & {}) => ReactNode;
}

/** Maps dialog type → component. Both App.tsx and src-present/App.tsx share this. */
function DialogRenderer<T extends SharedDialog>({ dialog, onClose, onDownloadUpdate, renderExtras }: DialogRendererProps<T>) {
  if (!dialog) return null;

  // Allow each app to inject its own dialog types before the shared ones
  if (renderExtras) {
    const extras = renderExtras(dialog);
    if (extras) return extras;
  }

  switch (dialog.type) {
    case "delete":
      return <DeleteDialog ids={dialog.ids} onClose={onClose} />;
    case "settings":
      return <SettingsDialog onClose={onClose} />;
    case "about":
      return <AboutDialog onClose={onClose} onDownloadUpdate={onDownloadUpdate} />;
    case "extension":
      return <ExtensionDialog onClose={onClose} />;
    case "log":
      return <LogDialog onClose={onClose} />;
  }
}

export default DialogRenderer;
