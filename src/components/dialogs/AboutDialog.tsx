import { Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { t } from "../../i18n";

interface AboutDialogProps {
  onClose: () => void;
}

export default function AboutDialog({ onClose }: AboutDialogProps) {
  return (
    <Dialog title={t("about.title")} onClose={onClose}>
      <div style={{ padding: 16, textAlign: "center" }}>
        <Text weight="semibold" size="large" style={{ display: "block", marginBottom: 8 }}>
          ProxyDownloadManager
        </Text>
        <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", display: "block" }}>
          Version 0.1.0
        </Text>
        <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", display: "block", marginTop: 4 }}>
          Multi-threaded download manager with per-download proxy support
        </Text>
        <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", display: "block", marginTop: 12 }}>
          Rust + Tauri 2 + React 19
        </Text>
      </div>
    </Dialog>
  );
}
