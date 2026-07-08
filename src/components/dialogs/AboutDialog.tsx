import { useState, useEffect } from "react";
import { Text, Button, Select, Spinner } from "@primer/react";
import {
  CheckIcon,
  DownloadIcon,
  SyncIcon,
  LinkExternalIcon,
  ArrowRightIcon,
} from "@primer/octicons-react";
import { Dialog } from "@primer/react/experimental";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { useSettingsStore } from "../../stores/settingsStore";
import { t } from "../../i18n";
import type { UpdateInfo } from "../../types";

interface AboutDialogProps {
  onClose: () => void;
  onDownloadUpdate: (url: string) => void;
}

type CheckState = "idle" | "checking" | "done" | "error";

export default function AboutDialog({ onClose, onDownloadUpdate }: AboutDialogProps) {
  console.debug('[ProxyDM FE] AboutDialog mount');
  const [version, setVersion] = useState("");
  const [proxyName, setProxyName] = useState("");
  const [checkState, setCheckState] = useState<CheckState>("idle");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [errorMsg, setErrorMsg] = useState("");

  const proxies = useSettingsStore((s) => s.settings.proxies);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion("?"));
  }, []);

  const handleCheck = async () => {
    setCheckState("checking");
    setErrorMsg("");
    try {
      const info = await invoke<UpdateInfo>("check_update", {
        proxyName,
      });
      setUpdateInfo(info);
      setCheckState("done");
    } catch (e) {
      setErrorMsg(String(e));
      setCheckState("error");
    }
  };

  const handleDownload = (url: string) => {
    onDownloadUpdate(url);
    onClose();
  };

  const recommendedAsset = updateInfo?.assets.find((a) => a.recommended);
  const otherAssets = updateInfo?.assets.filter((a) => !a.recommended) ?? [];

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
            <div
              style={{
                padding: "8px 12px",
                borderRadius: 6,
                backgroundColor: updateInfo.has_update
                  ? "var(--attention-bgColor, #fff8c5)"
                  : "var(--success-bgColor, #dafbe1)",
                border: `1px solid ${
                  updateInfo.has_update
                    ? "var(--attention-borderColor, #d4a72c)"
                    : "var(--success-borderColor, #a6d3a0)"
                }`,
                fontSize: 13,
              }}
            >
              {updateInfo.has_update ? (
                <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                  <Text>
                    {t("about.newVersion")}: <strong>{updateInfo.latest_version}</strong>
                    <span style={{ margin: "0 4px", display: "inline-flex", verticalAlign: "middle" }}><ArrowRightIcon size={12} /></span>
                    <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>
                      {t("about.version")} {updateInfo.current_version}
                    </Text>
                  </Text>

                  {/* Recommended asset */}
                  {recommendedAsset && (
                    <Button
                      size="small"
                      variant="primary"
                      leadingVisual={DownloadIcon}
                      onClick={() => handleDownload(recommendedAsset.url)}
                    >
                      {t("about.downloadUpdate")} ({recommendedAsset.name})
                    </Button>
                  )}

                  {/* Other platform assets */}
                  {otherAssets.length > 0 && (
                    <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                      {otherAssets.map((asset) => (
                        <Button
                          key={asset.name}
                          size="small"
                          leadingVisual={DownloadIcon}
                          onClick={() => handleDownload(asset.url)}
                        >
                          {asset.name}
                        </Button>
                      ))}
                    </div>
                  )}

                  <a
                    href={updateInfo.release_url}
                    target="_blank"
                    rel="noreferrer"
                    style={{ fontSize: 12, display: "inline-flex", alignItems: "center", gap: 4 }}
                  >
                    {t("about.releasePage")} <LinkExternalIcon size={12} />
                  </a>
                </div>
              ) : (
                <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                  <CheckIcon size={16} />
                  <Text>{t("about.upToDate")}</Text>
                </div>
              )}
            </div>
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
