import { useState, useEffect } from "react";
import { Text, Label, Button } from "@primer/react";
import { invoke } from "@tauri-apps/api/core";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { formatBytes } from "./types";
import type { DownloadItem } from "./types";

function fmt(ts: string): string {
  if (!ts) return "—";
  const s = Number(ts);
  if (!Number.isFinite(s) || s <= 0) return ts;
  try {
    const d = new Date(s * 1000);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  } catch { return ts; }
}

const card: React.CSSProperties = {
  border: "1px solid var(--borderColor-muted, #d8dee4)", borderRadius: 6, overflow: "hidden",
};
const hdr: React.CSSProperties = {
  padding: "5px 10px", fontSize: 11, fontWeight: 600,
  color: "var(--fgColor-muted, #656d76)",
  borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
  background: "var(--bgColor-subtle, #f6f8fa)",
  textTransform: "uppercase", letterSpacing: "0.05em",
};
const bd: React.CSSProperties = {
  padding: "8px 10px", display: "flex", flexDirection: "column", gap: 4,
};
const r: React.CSSProperties = {
  display: "flex", fontSize: 12, lineHeight: 1.5,
};
const l: React.CSSProperties = {
  width: 80, flexShrink: 0, color: "var(--fgColor-muted, #656d76)", fontWeight: 600,
};
const v: React.CSSProperties = {
  flex: 1, wordBreak: "break-all", color: "var(--fgColor-default, #1f2328)",
};

function statusColor(s: string): "success" | "danger" | "attention" | "accent" | "default" {
  switch (s) {
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
    const p = new URLSearchParams(window.location.search);
    const id = p.get("id");
    if (id) invoke<DownloadItem[]>("list_downloads")
      .then((items) => { setItem(items.find((d) => d.id === Number(id)) ?? null); setLoading(false); })
      .catch(() => setLoading(false));
    else setLoading(false);
  }, []);

  const closeWindow = () => { getCurrentWebviewWindow().close(); };

  const openFile = async () => {
    if (!item) return;
    try {
      await openUrl("file://" + encodeURI(item.save_path));
    } catch (e) {
      console.error("[ProxyDM] openUrl failed:", e);
      try { await openPath(item.save_path); }
      catch (e2) { console.error(e2); }
    }
    closeWindow();
  };
  const openFolder = async () => {
    if (!item) return;
    try {
      const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
      await revealItemInDir(item.save_path);
    }
    catch {
      try { await openPath(item.save_path.replace(/[/\\][^/\\]*$/, "") || "."); }
      catch (e) { console.error(e); }
    }
    closeWindow();
  };

  if (loading) return <div style={{ display: "flex", justifyContent: "center", alignItems: "center", height: "100vh" }}><Text>Loading...</Text></div>;
  if (!item) return <div style={{ display: "flex", justifyContent: "center", alignItems: "center", height: "100vh" }}><Text>Not found</Text></div>;

  const yes = "Yes", no = "No", uk = "Unknown";
  const resumable = item.resumable === true ? yes : item.resumable === false ? no : uk;

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
        <Label variant={statusColor(item.status)} style={{ fontSize: 11 }}>{item.status}</Label>
        {item.status === "completed" && (
          <>
            <Button size="small" onClick={openFile}>Open</Button>
            <Button size="small" onClick={openFolder}>Folder</Button>
          </>
        )}
      </div>

      {/* URL */}
      <div style={{ padding: "8px 14px", borderBottom: "1px solid var(--borderColor-muted, #d8dee4)" }}>
        <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", wordBreak: "break-all", lineHeight: 1.4 }}>
          {item.url}
        </Text>
      </div>

      {/* Content */}
      <div style={{ padding: "10px 14px", flex: 1, overflow: "auto", display: "flex", flexDirection: "column", gap: 10 }}>

        {/* File */}
        <div style={card}>
          <div style={hdr}>File</div>
          <div style={bd}>
            <div style={r}><span style={l}>Size</span><span style={v}>{formatBytes(item.total_size)}</span></div>
            <div style={r}><span style={l}>Saved</span><span style={v}>{item.save_path || "—"}</span></div>
            <div style={r}><span style={l}>Created</span><span style={v}>{fmt(item.created_at)}</span></div>
          </div>
        </div>

        {/* Download */}
        <div style={card}>
          <div style={hdr}>Download</div>
          <div style={bd}>
            <div style={r}><span style={l}>Status</span><span style={v}>{item.status}</span></div>
            <div style={r}><span style={l}>Resume</span><span style={v}>{resumable}</span></div>
            <div style={r}><span style={l}>Last try</span><span style={v}>{fmt(item.last_try)}</span></div>
          </div>
        </div>

        {/* Network */}
        <div style={card}>
          <div style={hdr}>Network</div>
          <div style={bd}>
            <div style={r}><span style={l}>Threads</span><span style={v}>{String(item.connections)}</span></div>
            <div style={r}><span style={l}>Proxy</span><span style={v}>{item.proxy_name || "—"}</span></div>
          </div>
        </div>

      </div>
    </div>
  );
}
