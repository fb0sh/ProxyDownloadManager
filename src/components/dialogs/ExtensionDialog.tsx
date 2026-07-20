import { useEffect, useState } from "react";
import { Text, Button, Spinner } from "@primer/react";
import { FileDirectoryIcon, CopyIcon } from "@primer/octicons-react";
import { Dialog } from "@primer/react/experimental";
import { tauriClient } from "../../tauriClient";
import { t } from "../../i18n";

interface ExtensionDialogProps {
  onClose: () => void;
}

type LoadState = "loading" | "loaded" | "error";

export default function ExtensionDialog({ onClose }: ExtensionDialogProps) {
  console.debug('[ProxyDM FE] ExtensionDialog mount');
  const [extDir, setExtDir] = useState<string>("");
  const [loadState, setLoadState] = useState<LoadState>("loading");
  const [copyLabel, setCopyLabel] = useState<string>(t("extension.copy"));

  useEffect(() => {
    tauriClient.getExtensionsDir()
      .then((dir) => {
        setExtDir(dir);
        setLoadState("loaded");
      })
      .catch(() => setLoadState("error"));
  }, []);

  const handleOpenDir = async () => {
    try {
      await tauriClient.openExtensionsFolder();
    } catch (e) {
      console.error("Failed to open extensions folder:", e);
    }
    onClose();
  };

  const handleCopyPath = async () => {
    try {
      await navigator.clipboard.writeText(extDir);
      setCopyLabel(t("extension.copied"));
      setTimeout(() => setCopyLabel(t("extension.copy")), 2000);
    } catch {
      // clipboard API unavailable — fall back to Tauri clipboard
      const ta = await import("@tauri-apps/plugin-clipboard-manager");
      await ta.writeText(extDir);
      setCopyLabel(t("extension.copied"));
      setTimeout(() => setCopyLabel(t("extension.copy")), 2000);
    }
  };

  return (
    <Dialog title={t("toolbar.extension")} onClose={onClose}>
      <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 12 }}>
        {loadState === "loading" && (
          <div style={{ display: "flex", justifyContent: "center", padding: 24 }}>
            <Spinner />
          </div>
        )}

        {loadState === "error" && (
          <Text>
            {t("extension.notFound")}{" "}
            <a
              href="https://github.com/fb0sh/ProxyDownloadManager/tree/main/browsers-extension"
              target="_blank"
              rel="noreferrer"
            >
              GitHub
            </a>
            .
          </Text>
        )}

        {loadState === "loaded" && (
          <>
            <Text>
              {t("extension.intro")}
            </Text>

            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "8px 10px",
                borderRadius: 6,
                backgroundColor: "var(--bgColor-muted, #f6f8fa)",
                border: "1px solid var(--borderColor-default, #d0d7de)",
                fontFamily: "monospace",
                fontSize: 12,
                wordBreak: "break-all",
              }}
            >
              <FileDirectoryIcon size={16} />
              <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {extDir}
              </span>
            </div>

            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <Button
                leadingVisual={FileDirectoryIcon}
                variant="primary"
                onClick={handleOpenDir}
              >
                {t("extension.openFolder")}
              </Button>
              <Button
                leadingVisual={CopyIcon}
                onClick={handleCopyPath}
              >
                {copyLabel}
              </Button>
            </div>

            <div
              style={{
                padding: "8px 12px",
                borderRadius: 6,
                backgroundColor: "var(--attention-bgColor, #fff8c5)",
                border: "1px solid var(--attention-borderColor, #d4a72c)",
                fontSize: 12,
                lineHeight: 1.5,
              }}
            >
              <Text weight="semibold" size="small">
                {t("extension.instructions")}
              </Text>
              <ol style={{ margin: "4px 0 0 0", paddingLeft: 20 }}>
                <li>{t("extension.step1")}</li>
                <li>{t("extension.step2")}</li>
                <li>{t("extension.step3")}</li>
                <li>{t("extension.step4")}</li>
              </ol>
            </div>

            <div
              style={{
                padding: "8px 12px",
                borderRadius: 6,
                backgroundColor: "var(--bgColor-muted, #f6f8fa)",
                border: "1px solid var(--borderColor-default, #d0d7de)",
                fontSize: 12,
                lineHeight: 1.5,
              }}
            >
              <Text weight="semibold" size="small">
                {t("extension.firefox")}
              </Text>
              <ol style={{ margin: "4px 0 0 0", paddingLeft: 20 }}>
                <li>{t("extension.ffStep1")}</li>
                <li>{t("extension.ffStep2")}</li>
                <li>{t("extension.ffStep3")}</li>
                <li style={{ color: "var(--fgColor-muted, #656d76)" }}>
                  {t("extension.ffNote")}
                </li>
              </ol>
            </div>
          </>
        )}

        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <Button onClick={onClose}>{t("newDownload.cancel")}</Button>
        </div>
      </div>
    </Dialog>
  );
}
