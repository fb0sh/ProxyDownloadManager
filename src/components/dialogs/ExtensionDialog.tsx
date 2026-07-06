import { Text, Button } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../../i18n";

interface ExtensionDialogProps {
  onClose: () => void;
}

const EXT_URL = "https://github.com/fb0sh/ProxyDownloadManager/tree/main/browsers-extension";

export default function ExtensionDialog({ onClose }: ExtensionDialogProps) {
  const handleOpen = async () => {
    try {
      await invoke("plugin:opener|open_url", { url: EXT_URL });
    } catch {
      window.open(EXT_URL, "_blank");
    }
    onClose();
  };

  return (
    <Dialog title={t("toolbar.extension")} onClose={onClose}>
      <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 16, textAlign: "center" }}>
        <Text>
          Browser extension not yet available on the Chrome Web Store or Firefox Add-ons.
          Please download it from GitHub:
        </Text>
        <Text weight="semibold" size="small" style={{ wordBreak: "break-all" }}>
          {EXT_URL}
        </Text>
        <div style={{ display: "flex", justifyContent: "center", gap: 8 }}>
          <Button onClick={onClose}>{t("newDownload.cancel")}</Button>
          <Button variant="primary" onClick={handleOpen}>
            Open in Browser
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
