import { useState, useEffect } from "react";
import { Text, Label, Button } from "@primer/react";
import { CopyIcon } from "@primer/octicons-react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { formatBytes } from "./types";
import { formatTimestamp, statusColor, openFile, openFolder } from "./utils/download";
import type { DownloadItem } from "./types";

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
  console.log('[ProxyDM FE] DownloadDetailsWindow mount');
  const [item, setItem] = useState<DownloadItem | null>(null);
  const [loading, setLoading] = useState(true);
  const [urlCopied, setUrlCopied] = useState(false);

  const handleCopyUrl = async () => {
    try {
      await navigator.clipboard.writeText(item?.url ?? "");
      setUrlCopied(true);
      setTimeout(() => setUrlCopied(false), 2000);
    } catch {} // clipboard not available
  };

  useEffect(() => {
    const p = new URLSearchParams(window.location.search);
    const id = p.get("id");
    console.log('[ProxyDM FE] DownloadDetailsWindow id=', id);
    if (id) invoke<DownloadItem[]>("list_downloads")
      .then((items) => {
        const found = items.find((d) => d.id === Number(id)) ?? null;
        console.log('[ProxyDM FE] DownloadDetailsWindow found:', found?.file_name, found?.status);
        setItem(found); setLoading(false);
      })
      .catch(() => setLoading(false));
    else setLoading(false);
  }, []);

  const closeWindow = () => { getCurrentWebviewWindow().close(); };

  const handleOpenFile = async () => {
    if (!item) return;
    await openFile(item.save_path);
    closeWindow();
  };
  const handleOpenFolder = async () => {
    if (!item) return;
    await openFolder(item.save_path);
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
            <Button size="small" onClick={handleOpenFile}>Open</Button>
            <Button size="small" onClick={handleOpenFolder}>Folder</Button>
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
          <div style={hdr}>File</div>
          <div style={bd}>
            <div style={r}><span style={l}>Size</span><span style={v}>{formatBytes(item.total_size)}</span></div>
            <div style={r}><span style={l}>Saved</span><span style={v}>{item.save_path || "—"}</span></div>
            <div style={r}><span style={l}>Created</span><span style={v}>{formatTimestamp(item.created_at)}</span></div>
          </div>
        </div>

        {/* Download */}
        <div style={card}>
          <div style={hdr}>Download</div>
          <div style={bd}>
            <div style={r}><span style={l}>Status</span><span style={v}>{item.status}</span></div>
            <div style={r}><span style={l}>Resume</span><span style={v}>{resumable}</span></div>
            <div style={r}><span style={l}>Last try</span><span style={v}>{formatTimestamp(item.last_try)}</span></div>
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
