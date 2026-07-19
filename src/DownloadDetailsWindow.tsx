import { Text, Label, Button } from "@primer/react";
import { CopyIcon } from "@primer/octicons-react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { formatBytes } from "./utils/format";
import { formatTimestamp, statusColor, statusString } from "./utils/download";
import { useDownloadDetail, useDownloadIdFromUrl } from "./hooks/useDownloadDetail";
import { t } from "./i18n";

const card: React.CSSProperties = {
  border: "1px solid var(--borderColor-muted, #d8dee4)", borderRadius: 6,
};
const hdr: React.CSSProperties = {
  padding: "5px 10px", fontSize: 11, fontWeight: 600,
  color: "var(--fgColor-muted, #656d76)",
  borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
  background: "var(--bgColor-subtle, #f6f8fa)",
  textTransform: "uppercase", letterSpacing: "0.05em",
};
const bd: React.CSSProperties = {
  padding: "8px 10px",
};
const r: React.CSSProperties = {
  display: "flex", fontSize: 12, lineHeight: 1.5,
};
const l: React.CSSProperties = {
  width: 80, flexShrink: 0, color: "var(--fgColor-muted, #656d76)", fontWeight: 600,
};
const v: React.CSSProperties = {
  flex: 1, minWidth: 0, wordBreak: "break-all", color: "var(--fgColor-default, #1f2328)",
};

export default function DownloadDetailsWindow() {
  const idParam = new URLSearchParams(window.location.search).get("id");
  const id = useDownloadIdFromUrl();
  const { item, urlCopied, handleCopyUrl, handleOpenFile, handleOpenFolder } = useDownloadDetail(id);

  const closeWindow = () => { getCurrentWebviewWindow().close(); };

  const handleOpenFileAndClose = async () => {
    await handleOpenFile();
    closeWindow();
  };
  const handleOpenFolderAndClose = async () => {
    await handleOpenFolder();
    closeWindow();
  };

  if (!idParam) return <div style={{ display: "flex", justifyContent: "center", alignItems: "center", height: "100vh" }}><Text>No download ID provided</Text></div>;
  if (!item) return <div style={{ display: "flex", justifyContent: "center", alignItems: "center", height: "100vh" }}><Text>Loading...</Text></div>;

  const resumable = item.resumable === true ? t("properties.yes") : item.resumable === false ? t("properties.no") : t("properties.unknown");

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh", fontSize: 12, background: "var(--bgColor-default, #fff)" }}>
      {/* Header */}
      <div style={{
        display: "flex", alignItems: "center", gap: 8,
        padding: "10px 14px", borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
        background: "var(--bgColor-subtle, #f6f8fa)",
      }}>
        <Text weight="semibold" size="small" style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {item.file_name}
        </Text>
        <Label variant={statusColor(item.status)} style={{ fontSize: 11 }}>{statusString(item.status)}</Label>
        {item.status === "completed" && (
          <>
            <Button size="small" onClick={handleOpenFileAndClose}>{t("downloadRow.open")}</Button>
            <Button size="small" onClick={handleOpenFolderAndClose}>{t("downloadRow.openFolder")}</Button>
          </>
        )}
      </div>

      {/* URL */}
      <div style={{ padding: "8px 14px", borderBottom: "1px solid var(--borderColor-muted, #d8dee4)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <Text size="small" style={{
            color: "var(--fgColor-muted, #656d76)",
            flex: 1, minWidth: 0,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            lineHeight: 1.4,
          }}>
            {item.url}
          </Text>
          <Button size="small" onClick={handleCopyUrl}
            leadingVisual={CopyIcon}
            style={{ flexShrink: 0 }}
          >
            {urlCopied ? "✓" : ""}
          </Button>
        </div>
      </div>

      {/* Content */}
      <div style={{ padding: "10px 14px", flex: 1, overflow: "auto", display: "flex", flexDirection: "column", gap: 10 }}>

        {/* File */}
        <div style={card}>
          <div style={hdr}>{t("properties.file")}</div>
          <div style={bd}>
            <div style={r}><span style={l}>{t("properties.size")}</span><span style={v}>{formatBytes(item.total_size)}</span></div>
            <div style={r}><span style={l}>{t("properties.savePath")}</span><span style={v}>{item.save_path || "—"}</span></div>
            <div style={r}><span style={l}>{t("properties.created")}</span><span style={v}>{formatTimestamp(item.created_at)}</span></div>
          </div>
        </div>

        {/* Download */}
        <div style={card}>
          <div style={hdr}>{t("properties.download")}</div>
          <div style={bd}>
            <div style={r}><span style={l}>{t("properties.status")}</span><span style={v}>{statusString(item.status)}</span></div>
            <div style={r}><span style={l}>{t("properties.resumable")}</span><span style={v}>{resumable}</span></div>
            <div style={r}><span style={l}>{t("properties.lastTry")}</span><span style={v}>{formatTimestamp(item.last_try)}</span></div>
          </div>
        </div>

        {/* Network */}
        <div style={card}>
          <div style={hdr}>{t("properties.network")}</div>
          <div style={bd}>
            <div style={r}><span style={l}>{t("properties.connections")}</span><span style={v}>{String(item.connections)}</span></div>
            <div style={r}><span style={l}>{t("properties.proxy")}</span><span style={v}>{item.proxy_name || "—"}</span></div>
          </div>
        </div>

      </div>
    </div>
  );
}
