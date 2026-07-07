import { useState, useEffect } from "react";
import { Text, Label, Button } from "@primer/react";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "./types";
import { t } from "./i18n";
import type { DownloadItem } from "./types";

function formatTimestamp(ts: string): string {
  if (!ts) return "—";
  const secs = Number(ts);
  if (!Number.isFinite(secs) || secs <= 0) return ts;
  try {
    const d = new Date(secs * 1000);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  } catch {
    return ts;
  }
}

function statusColor(status: string): "success" | "danger" | "attention" | "accent" | "default" {
  switch (status) {
    case "completed": return "success";
    case "failed": return "danger";
    case "paused": return "attention";
    case "downloading": return "accent";
    default: return "default";
  }
}

const row: React.CSSProperties = {
  display: "flex", fontSize: 12, lineHeight: 1.5, padding: "3px 0",
};
const label: React.CSSProperties = {
  width: 90, flexShrink: 0, color: "var(--fgColor-muted, #656d76)", fontWeight: 600,
};
const value: React.CSSProperties = {
  flex: 1, wordBreak: "break-all", color: "var(--fgColor-default, #1f2328)",
};

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

  if (loading) return <div style={{ display: "flex", justifyContent: "center", alignItems: "center", height: "100vh" }}><Text>Loading...</Text></div>;
  if (!item) return <div style={{ display: "flex", justifyContent: "center", alignItems: "center", height: "100vh" }}><Text>Download not found.</Text></div>;

  const resumable = item.resumable === true ? t("properties.yes") : item.resumable === false ? t("properties.no") : t("properties.unknown");

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh", fontSize: 12 }}>
      {/* Title bar */}
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "6px 12px", borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
        background: "var(--bgColor-subtle, #f6f8fa)", minHeight: 32,
      }}>
        <Text weight="semibold" size="small" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flex: 1 }}>
          {item.file_name}
        </Text>
        <Label variant={statusColor(item.status)} style={{ fontSize: 10, marginLeft: 8 }}>{item.status}</Label>
        {item.status === "completed" && (
          <div style={{ display: "flex", gap: 3, marginLeft: 6 }}>
            <Button size="small" onClick={handleOpenFile}>{t("downloadRow.open")}</Button>
            <Button size="small" onClick={handleOpenFolder}>{t("downloadRow.openFolder")}</Button>
          </div>
        )}
      </div>

      {/* Compact details */}
      <div style={{ padding: "8px 12px", flex: 1, overflow: "auto" }}>
        <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", wordBreak: "break-all", display: "block", marginBottom: 8, lineHeight: 1.4 }}>
          {item.url}
        </Text>

        {/* Single card with all info */}
        <div style={{ border: "1px solid var(--borderColor-muted, #d8dee4)", borderRadius: 6, overflow: "hidden" }}>
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
            <tbody>
              <tr style={row as React.CSSProperties}><td style={label}>Size</td><td style={value}>{formatBytes(item.total_size)}</td></tr>
              <tr style={{ ...row, borderTop: "1px solid var(--borderColor-muted, #d8dee4)" } as React.CSSProperties}><td style={label}>Saved</td><td style={value}>{item.save_path || "—"}</td></tr>
              <tr style={{ ...row, borderTop: "1px solid var(--borderColor-muted, #d8dee4)" } as React.CSSProperties}><td style={label}>Created</td><td style={value}>{formatTimestamp(item.created_at)}</td></tr>
              <tr style={{ ...row, borderTop: "1px solid var(--borderColor-muted, #d8dee4)" } as React.CSSProperties}><td style={label}>Status</td><td style={value}>{item.status}</td></tr>
              <tr style={{ ...row, borderTop: "1px solid var(--borderColor-muted, #d8dee4)" } as React.CSSProperties}><td style={label}>Resumable</td><td style={value}>{resumable}</td></tr>
              <tr style={{ ...row, borderTop: "1px solid var(--borderColor-muted, #d8dee4)" } as React.CSSProperties}><td style={label}>Last try</td><td style={value}>{formatTimestamp(item.last_try)}</td></tr>
              <tr style={{ ...row, borderTop: "1px solid var(--borderColor-muted, #d8dee4)" } as React.CSSProperties}><td style={label}>Threads</td><td style={value}>{String(item.connections)}</td></tr>
              <tr style={{ ...row, borderTop: "1px solid var(--borderColor-muted, #d8dee4)" } as React.CSSProperties}><td style={label}>Proxy</td><td style={value}>{item.proxy_name || "—"}</td></tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
