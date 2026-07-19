import { useState } from "react";
import { Text, Button, Select, Spinner } from "@primer/react";
import { SyncIcon } from "@primer/octicons-react";
import { Dialog } from "@primer/react/experimental";
import { useSettings } from "../../query/downloadQueries";
import { useUpdateChecker } from "../../hooks/useUpdateChecker";
import UpdateResult from "./UpdateResult";
import { t } from "../../i18n";

interface AboutDialogProps {
  onClose: () => void;
  onDownloadUpdate: (url: string) => void;
}

export default function AboutDialog({ onClose, onDownloadUpdate }: AboutDialogProps) {
  console.debug('[ProxyDM FE] AboutDialog mount');
  const [proxyName, setProxyName] = useState("");

  const { settings: loadedSettings } = useSettings();
  const proxies = loadedSettings?.proxies ?? {};

  const { version, checkState, updateInfo, errorMsg, handleCheck } = useUpdateChecker(proxyName);

  const handleDownload = (url: string) => {
    onDownloadUpdate(url);
    onClose();
  };

  return (
    <Dialog title={t("about.title")} onClose={onClose}>
      <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 12 }}>
        {/* App info */}
        <div style={{ textAlign: "center" }}>
          <Text weight="semibold" size="large" style={{ display: "block", marginBottom: 8 }}>
            ProxyDownloadManager
          </Text>
          <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", display: "block" }}>
            {t("about.version")} {version}
          </Text>
          <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", display: "block", marginTop: 4 }}>
            {t("about.description")}
          </Text>
          <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", display: "block" }}>
            {t("about.techStack")}
          </Text>
          <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", display: "block", marginTop: 8 }}>
            {t("about.author")}: fb0sh, DohHoKun
          </Text>
        </div>

        <hr style={{ border: "none", borderTop: "1px solid var(--borderColor-muted, #d8dee4)", margin: "4px 0" }} />

        {/* Update checker */}
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <div style={{ flex: 1 }}>
              <Select
                size="small"
                value={proxyName}
                onChange={(e) => setProxyName(e.target.value)}
                aria-label="Proxy"
              >
                <Select.Option value="">{t("about.noProxy")}</Select.Option>
                {Object.keys(proxies).map((name) => (
                  <Select.Option key={name} value={name}>
                    {name}
                  </Select.Option>
                ))}
              </Select>
            </div>
            <Button
              size="small"
              leadingVisual={checkState === "checking" ? undefined : SyncIcon}
              disabled={checkState === "checking"}
              onClick={handleCheck}
            >
              {checkState === "checking" ? (
                <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                  <Spinner size="small" /> {t("about.checking")}
                </span>
              ) : (
                t("about.checkUpdate")
              )}
            </Button>
          </div>

          {checkState === "done" && updateInfo && (
            <UpdateResult updateInfo={updateInfo} onDownload={handleDownload} />
          )}

          {checkState === "error" && (
            <Text size="small" style={{ color: "var(--fgColor-danger, #cf222e)" }}>
              {t("about.updateCheckFailed")}: {errorMsg}
            </Text>
          )}
        </div>
      </div>
    </Dialog>
  );
}
