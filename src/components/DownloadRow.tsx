import { useState, useRef, useEffect } from "react";
import { Text, Checkbox } from "@primer/react";
import { FileIcon, DownloadIcon, CheckIcon, PauseIcon, AlertIcon } from "@primer/octicons-react";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "../types";
import { t } from "../i18n";
import type { DownloadItem } from "../types";

function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return "";
  return formatBytes(bytesPerSec) + t("downloadRow.speed");
}

interface DownloadRowProps {
  item: DownloadItem;
  selected: boolean;
  onToggleSelect: () => void;
  onStop: (id: number) => void;
  onDelete: (ids: number[]) => void;
  onProperties: (id: number) => void;
  onRedownload: (item: DownloadItem) => void;
}

export default function DownloadRow({ item, selected, onToggleSelect, onStop, onDelete, onProperties, onRedownload }: DownloadRowProps) {
  const [menuPos, setMenuPos] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const prevRef = useRef({ downloaded: 0, time: 0 });
  const [speed, setSpeed] = useState("");

  useEffect(() => {
    if (item.status !== "downloading") { setSpeed(""); return; }
    const now = Date.now();
    const prev = prevRef.current;
    if (prev.time > 0 && prev.downloaded > 0) {
      const dt = (now - prev.time) / 1000;
      if (dt > 0) {
        const bps = Math.round((item.downloaded - prev.downloaded) / dt);
        setSpeed(formatSpeed(bps));
      }
    }
    prevRef.current = { downloaded: item.downloaded, time: now };
  }, [item.downloaded, item.status]);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuPos(null);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const handleContext = (e: React.MouseEvent) => {
    e.preventDefault();
    setMenuPos({ x: e.clientX, y: e.clientY });
  };

  const handleOpen = async () => {
    setMenuPos(null);
    try { await invoke("plugin:opener|open_path", { path: item.save_path }); }
    catch (e) { console.error("open failed:", e); }
  };

  const handleOpenFolder = async () => {
    setMenuPos(null);
    try {
      await invoke("plugin:opener|reveal_item_in_dir", { path: item.save_path });
    } catch {
      try {
        const parent = item.save_path.replace(/[/\\][^/\\]*$/, "");
        await invoke("plugin:opener|open_path", { path: parent || "." });
      } catch (e) {
        console.error("open folder failed:", e);
      }
    }
  };

  const progress = item.total_size > 0 ? (item.downloaded / item.total_size) * 100 : 0;

  const statusIcon = () => {
    switch (item.status) {
      case "downloading": return <DownloadIcon />;
      case "completed": return <CheckIcon />;
      case "paused": return <PauseIcon />;
      case "failed": return <AlertIcon />;
      default: return <FileIcon />;
    }
  };

  const statusColor = () => {
    switch (item.status) {
      case "completed": return "var(--fgColor-success, #1a7f37)";
      case "failed": return "var(--fgColor-danger, #cf222e)";
      case "paused": return "var(--fgColor-attention, #9a6700)";
      default: return "var(--fgColor-default, #1f2328)";
    }
  };

  const menuItem = (label: string, onClick: () => void, danger?: boolean) => (
    <div
      onClick={onClick}
      style={{
        padding: "6px 16px",
        cursor: "pointer",
        fontSize: 13,
        color: danger ? "var(--fgColor-danger, #cf222e)" : "var(--fgColor-default, #1f2328)",
      }}
      onMouseEnter={(e) => (e.currentTarget.style.background = "var(--bgColor-subtle, #f6f8fa)")}
      onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
    >
      {label}
    </div>
  );

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        padding: 8,
        gap: 8,
        borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
        cursor: "pointer",
      }}
      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.backgroundColor = "var(--bgColor-subtle, #f6f8fa)"; }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.backgroundColor = ""; }}
      onContextMenu={handleContext}
    >
      <Checkbox checked={selected} onChange={onToggleSelect} />
      <div style={{ color: statusColor() }}>{statusIcon()}</div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <Text weight="semibold" size="small" style={{ display: "block", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {item.file_name}
        </Text>
        <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>
          {formatBytes(item.downloaded)} / {formatBytes(item.total_size)}
        </Text>
        {(item.status === "downloading" || item.status === "paused") && (
          <div style={{ marginTop: 4, position: "relative" }}>
            <div style={{
              width: "100%",
              height: 10,
              background: "var(--bgColor-muted, #f6f8fa)",
              borderRadius: 5,
              overflow: "hidden",
              position: "relative",
            }}>
              <div style={{
                width: `${Math.round(progress)}%`,
                height: "100%",
                background: "var(--fgColor-accent, #0969da)",
                borderRadius: 5,
                transition: "width 0.4s ease",
              }} />
            </div>
            {item.status === "downloading" && (
              <span style={{
                position: "absolute",
                right: 4,
                top: -1,
                fontSize: 10,
                color: "var(--fgColor-muted, #656d76)",
              }}>
                {Math.round(progress)}{t("progress.percent")}
              </span>
            )}
          </div>
        )}
      </div>
      <div style={{ textAlign: "right", flexShrink: 0, fontSize: 12, lineHeight: 1.4 }}>
        {speed && (
          <Text size="small" style={{ display: "block", color: "var(--fgColor-muted, #656d76)" }}>
            {speed}
          </Text>
        )}
        <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", display: "block" }}>
          {item.status}{item.connections > 0 ? ` (${item.connections}t)` : ""}
        </Text>
        {item.proxy_name && (
          <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>
            {item.proxy_name}
          </Text>
        )}
      </div>

      {/* Context menu */}
      {menuPos && (
        <div
          ref={menuRef}
          style={{
            position: "fixed",
            left: menuPos.x,
            top: menuPos.y,
            background: "var(--bgColor-default, #ffffff)",
            border: "1px solid var(--borderColor-default, #d0d7de)",
            borderRadius: 6,
            boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
            zIndex: 9999,
            padding: "4px 0",
            minWidth: 160,
          }}
        >
          {item.status === "downloading" && menuItem(t("toolbar.stop"), () => { setMenuPos(null); onStop(item.id); })}
          <div
            onClick={() => { setMenuPos(null); onDelete([item.id]); }}
            style={{
              padding: "6px 16px",
              cursor: "pointer",
              fontSize: 13,
              color: "var(--fgColor-danger, #cf222e)",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = "var(--bgColor-subtle, #f6f8fa)")}
            onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
          >
            {t("toolbar.delete")}
          </div>
          <div style={{ borderTop: "1px solid var(--borderColor-muted, #d8dee4)", margin: "4px 0" }} />
          <div onClick={() => { setMenuPos(null); onRedownload(item); }} style={{ padding: "6px 16px", cursor: "pointer", fontSize: 13 }}
            onMouseEnter={(e) => (e.currentTarget.style.background = "var(--bgColor-subtle, #f6f8fa)")}
            onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}>
            {t("toolbar.redownload")}
          </div>
          <div onClick={handleOpen} style={{ padding: "6px 16px", cursor: "pointer", fontSize: 13 }}
            onMouseEnter={(e) => (e.currentTarget.style.background = "var(--bgColor-subtle, #f6f8fa)")}
            onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}>
            {t("downloadRow.open")}
          </div>
          <div onClick={handleOpenFolder} style={{ padding: "6px 16px", cursor: "pointer", fontSize: 13 }}
            onMouseEnter={(e) => (e.currentTarget.style.background = "var(--bgColor-subtle, #f6f8fa)")}
            onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}>
            {t("downloadRow.openFolder")}
          </div>
          <div onClick={() => { setMenuPos(null); onProperties(item.id); }} style={{ padding: "6px 16px", cursor: "pointer", fontSize: 13 }}
            onMouseEnter={(e) => (e.currentTarget.style.background = "var(--bgColor-subtle, #f6f8fa)")}
            onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}>
            {t("downloadRow.details")}
          </div>
        </div>
      )}
    </div>
  );
}
