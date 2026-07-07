import { useState, useEffect } from "react";
import { Text, Label, Button, ProgressBar } from "@primer/react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "./types";
import { t } from "./i18n";
import type { DownloadItem } from "./types";

const sectionCard: React.CSSProperties = {
  border: "1px solid var(--borderColor-muted, #d8dee4)",
  borderRadius: 6,
  overflow: "hidden",
};

const sectionHeader: React.CSSProperties = {
  padding: "8px 12px",
  fontSize: 12,
  fontWeight: 600,
  color: "var(--fgColor-muted, #656d76)",
  borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
  background: "var(--bgColor-subtle, #f6f8fa)",
  textTransform: "uppercase",
  letterSpacing: "0.05em",
};

const sectionBody: React.CSSProperties = {
  padding: "12px 16px",
  display: "flex",
  flexDirection: "column",
  gap: 8,
};

const infoRow: React.CSSProperties = {
  display: "flex",
  fontSize: 13,
  lineHeight: 1.6,
};

const infoLabel: React.CSSProperties = {
  width: 120,
  flexShrink: 0,
  color: "var(--fgColor-muted, #656d76)",
  fontWeight: 600,
};

const infoValue: React.CSSProperties = {
  flex: 1,
  wordBreak: "break-all",
  color: "var(--fgColor-default, #1f2328)",
};

function statusColor(status: string): "success" | "danger" | "attention" | "accent" | "default" {
  switch (status) {
    case "completed": return "success";
    case "failed": return "danger";
    case "paused": return "attention";
    case "downloading": return "accent";
    default: return "default";
  }
}

export default function DownloadDetailsWindow() {
  const [item, setItem] = useState<DownloadItem | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const id = params.get("id");
    if (id) {
      invoke<DownloadItem[]>("list_downloads").then((items) => {
        const found = items.find((d) => d.id === Number(id));
        setItem(found ?? null);
        setLoading(false);
      }).catch(() => setLoading(false));
    } else {
      setLoading(false);
    }
  }, []);

  const handleClose = () => { getCurrentWebviewWindow().close(); };

  const handleOpenFile = async () => {
    if (!item) return;
    try { await invoke("plugin:opener|open_path", { path: item.save_path }); }
    catch (e) { console.error("open failed:", e); }
  };

  const handleOpenFolder = async () => {
    if (!item) return;
    try { await invoke("plugin:opener|reveal_item_in_dir", { path: item.save_path }); }
    catch {
      try {
        const parent = item.save_path.replace(/[/\\][^/\\]*$/, "");
        await invoke("plugin:opener|open_path", { path: parent || "." });
      } catch (e) { console.error("open folder failed:", e); }
    }
  };

  if (loading) {
    return (
      <div style={{ display: "flex", justifyContent: "center", alignItems: "center", height: "100vh" }}>
        <Text>Loading...</Text>
      </div>
    );
  }

  if (!item) {
    return (
      <div style={{ display: "flex", justifyContent: "center", alignItems: "center", height: "100vh" }}>
        <Text>Download not found.</Text>
      </div>
    );
  }

  const progress = item.total_size > 0 ? Math.round((item.downloaded / item.total_size) * 100) : 0;
  const resumable = item.resumable === true ? t("properties.yes") : item.resumable === false ? t("properties.no") : t("properties.unknown");

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh", overflow: "auto" }}>
      {/* Title bar */}
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "10px 16px", borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
        background: "var(--bgColor-subtle, #f6f8fa)",
      }}>
        <Text weight="semibold">{t("properties.title")}</Text>
        <div style={{ display: "flex", gap: 4 }}>
          {item.status === "completed" && (
            <>
              <Button size="small" onClick={handleOpenFile}>{t("downloadRow.open")}</Button>
              <Button size="small" onClick={handleOpenFolder}>{t("downloadRow.openFolder")}</Button>
            </>
          )}
          <Button size="small" onClick={handleClose}>{t("newDownload.cancel")}</Button>
        </div>
      </div>

      {/* Content */}
      <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 12 }}>

        {/* File info header */}
        <div style={{ padding: "12px 16px", border: "1px solid var(--borderColor-muted, #d8dee4)", borderRadius: 6 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}>
            <Text weight="semibold" size="medium" style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {item.file_name}
            </Text>
            <Label variant={statusColor(item.status)}>{item.status}</Label>
          </div>
          <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", wordBreak: "break-all", display: "block", marginTop: 4 }}>
            {item.url}
          </Text>
        </div>

        {/* Progress */}
        {progress > 0 && (
          <div style={{ textAlign: "center", padding: "8px 0" }}>
            <Text weight="semibold" size="large">{progress}%</Text>
            <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", display: "block" }}>
              {formatBytes(item.downloaded)} / {formatBytes(item.total_size)}
            </Text>
          </div>
        )}

        {/* File section */}
        <div style={sectionCard}>
          <div style={sectionHeader}>{t("properties.file")}</div>
          <div style={sectionBody}>
            <div style={infoRow}>
              <span style={infoLabel}>{t("properties.size")}</span>
              <span style={infoValue}>{formatBytes(item.total_size)}</span>
            </div>
            <div style={infoRow}>
              <span style={infoLabel}>{t("properties.savePath")}</span>
              <span style={infoValue}>{item.save_path || "—"}</span>
            </div>
            <div style={infoRow}>
              <span style={infoLabel}>{t("properties.created")}</span>
              <span style={infoValue}>{item.created_at}</span>
            </div>
          </div>
        </div>

        {/* Download section */}
        <div style={sectionCard}>
          <div style={sectionHeader}>{t("properties.download")}</div>
          <div style={sectionBody}>
            <div style={infoRow}>
              <span style={infoLabel}>{t("properties.status")}</span>
              <span style={infoValue}>{item.status}</span>
            </div>
            <div style={infoRow}>
              <span style={infoLabel}>{t("properties.resumable")}</span>
              <span style={infoValue}>{resumable}</span>
            </div>
            <div style={infoRow}>
              <span style={infoLabel}>{t("properties.lastTry")}</span>
              <span style={infoValue}>{item.last_try || "—"}</span>
            </div>
          </div>
        </div>

        {/* Network section */}
        <div style={sectionCard}>
          <div style={sectionHeader}>{t("properties.network")}</div>
          <div style={sectionBody}>
            <div style={infoRow}>
              <span style={infoLabel}>{t("properties.connections")}</span>
              <span style={infoValue}>{String(item.connections)}</span>
            </div>
            <div style={infoRow}>
              <span style={infoLabel}>{t("properties.proxy")}</span>
              <span style={infoValue}>{item.proxy_name || t("properties.none")}</span>
            </div>
          </div>
        </div>

        {/* Threads section */}
        {item.parts.length > 0 && (
          <div style={sectionCard}>
            <div style={sectionHeader}>Threads ({item.parts.length})</div>
            <div style={sectionBody}>
              {item.parts.map((part) => {
                const partSize = part.end - part.start;
                const partPct = partSize > 0 ? Math.round((part.downloaded / partSize) * 100) : 0;
                const color = part.status === "completed" ? "var(--fgColor-success, #1a7f37)"
                  : part.status === "downloading" ? "var(--fgColor-accent, #0969da)"
                  : part.status === "failed" ? "var(--fgColor-danger, #cf222e)"
                  : "var(--fgColor-muted, #656d76)";
                return (
                  <div key={part.index} style={{ display: "flex", flexDirection: "column", gap: 2, padding: "4px 0" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12 }}>
                      <span>
                        <span style={{ fontWeight: 600, color }}>#{part.index + 1}</span>
                        <span style={{ color: "var(--fgColor-muted, #656d76)", marginLeft: 6 }}>
                          {formatBytes(part.start)} – {formatBytes(part.end)}
                        </span>
                      </span>
                      <span style={{ color: "var(--fgColor-muted, #656d76)" }}>
                        {formatBytes(part.downloaded)} / {formatBytes(partSize)} · {partPct}%
                      </span>
                    </div>
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <div style={{ flex: 1 }}>
                        <ProgressBar progress={Math.min(partPct, 100)} />
                      </div>
                      <Label
                        variant={part.status === "completed" ? "success" : part.status === "downloading" ? "accent" : part.status === "failed" ? "danger" : "default"}
                        style={{ fontSize: 10, lineHeight: "14px" }}
                      >
                        {part.status}{part.retries > 0 ? ` (${part.retries})` : ""}
                      </Label>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

      </div>
    </div>
  );
}
