import { useState, useRef, useEffect } from "react";
import { Text, Checkbox } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { invoke } from "@tauri-apps/api/core";
import { useDownloads } from "../query/downloadQueries";
import { useDownloadSpeed, computeETA } from "../hooks/useDownloadSpeed";
import { useFileIcons, iconFor } from "../hooks/useFileIcons";
import { formatBytes } from "../types";
import { t } from "../i18n";
import type { DownloadItem } from "../types";

interface DownloadTableProps {
  selectedIds: Set<number>;
  onSelectChange: (ids: Set<number>) => void;
  filter: "all" | "completed" | "incomplete";
  onStop: (id: number) => void;
  onDelete: (ids: number[]) => void;
  onProperties: (id: number) => void;
  onRedownload: (item: DownloadItem) => void;
}

function applyFilter(items: DownloadItem[], f: "all" | "completed" | "incomplete") {
  if (f === "all") return items;
  if (f === "completed") return items.filter((d) => d.status === "completed");
  return items.filter((d) => d.status === "downloading" || d.status === "paused" || d.status === "queued");
}

function formatTimestamp(ts: string): string {
  if (!ts) return "—";
  const secs = Number(ts);
  if (!Number.isFinite(secs) || secs <= 0) return ts;
  try {
    const d = new Date(secs * 1000);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  } catch {
    return ts;
  }
}

export default function DownloadTable({
  selectedIds, onSelectChange, filter,
  onStop, onDelete, onProperties, onRedownload,
}: DownloadTableProps) {
  const { data: downloads = [], isLoading } = useDownloads();
  const filtered = applyFilter(downloads, filter);
  const speeds = useDownloadSpeed(filtered);
  const icons = useFileIcons(filtered);
  const [menuState, setMenuState] = useState<{ id: number; x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  // Close context menu on click outside
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuState(null);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const selectAllChecked = filtered.length > 0 && filtered.every((d) => selectedIds.has(d.id));
  const selectAllIndeterminate = !selectAllChecked && filtered.some((d) => selectedIds.has(d.id));

  const toggleSelectAll = () => {
    if (selectAllChecked) {
      onSelectChange(new Set());
    } else {
      onSelectChange(new Set(filtered.map((d) => d.id)));
    }
  };

  const toggleSelect = (id: number) => {
    const next = new Set(selectedIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    onSelectChange(next);
  };

  const handleContext = (e: React.MouseEvent, id: number) => {
    e.preventDefault();
    setMenuState({ id, x: e.clientX, y: e.clientY });
  };

  const closeMenu = () => setMenuState(null);

  const handleOpen = async (path: string) => {
    closeMenu();
    try { await invoke("open_file", { path }); }
    catch (e) { console.error("open failed:", e); }
  };

  const handleOpenFolder = async (path: string) => {
    closeMenu();
    try {
      const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
      await revealItemInDir(path);
    }
    catch {
      try { await invoke("open_file", { path: path.replace(/[/\\][^/\\]*$/, "") || "." }); }
      catch (e) { console.error("open folder failed:", e); }
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

  const menuItemFor = (id: number) => {
    const item = filtered.find((d) => d.id === id);
    if (!item) return null;
    return (
      <>
        {item.status === "downloading" && menuItem(t("toolbar.stop"), () => { closeMenu(); onStop(id); })}
        {menuItem(t("toolbar.delete"), () => { closeMenu(); onDelete([id]); }, true)}
        <div style={{ borderTop: "1px solid var(--borderColor-muted, #d8dee4)", margin: "4px 0" }} />
        {menuItem(t("toolbar.redownload"), () => { closeMenu(); onRedownload(item); })}
        {menuItem(t("downloadRow.open"), () => handleOpen(item.save_path))}
        {menuItem(t("downloadRow.openFolder"), () => handleOpenFolder(item.save_path))}
        {menuItem(t("downloadRow.details"), () => { closeMenu(); onProperties(id); })}
      </>
    );
  };

  const skeletonColumns = [
    { header: "", width: "auto" as const },
    { header: t("downloadTable.fileName") },
    { header: t("downloadTable.size"), width: "auto" as const },
    { header: t("downloadTable.status"), width: "auto" as const },
    { header: t("downloadTable.speed"), width: "auto" as const },
    { header: t("downloadTable.remain"), width: "auto" as const },
    { header: t("downloadTable.threads"), width: "auto" as const },
    { header: t("downloadTable.proxy"), width: "auto" as const },
    { header: t("downloadTable.resume"), width: "auto" as const },
    { header: t("downloadTable.lastTry"), width: "auto" as const },
  ];

  const dataColumns = [
    {
      id: "select",
      header: () => (
        <Checkbox
          checked={selectAllChecked}
          onChange={toggleSelectAll}
          ref={(el) => {
            if (el) el.indeterminate = selectAllIndeterminate;
          }}
        />
      ),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => (
        <Checkbox checked={selectedIds.has(row.id)} onChange={() => toggleSelect(row.id)} />
      ),
    },
    {
      id: "file_name",
      header: t("downloadTable.fileName"),
      field: "file_name" as const,
      rowHeader: true,
      sortBy: "alphanumeric" as const,
      width: "grow" as const,
      renderCell: (row: DownloadItem) => (
        <div onContextMenu={(e) => handleContext(e, row.id)} style={{ cursor: "context-menu", display: "flex", alignItems: "center", gap: 4 }}>
          <img
            src={iconFor(icons, row.file_name)}
            alt=""
            width={18}
            height={18}
            style={{ flexShrink: 0 }}
          />
          <Text weight="semibold" size="medium" style={{ display: "block", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", lineHeight: 1.4 }}>
            {row.file_name}
          </Text>
        </div>
      ),
    },
    {
      id: "size",
      header: t("downloadTable.size"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => (
        <Text size="small" style={{ whiteSpace: "nowrap" }}>
          {row.status === "completed" ? formatBytes(row.total_size) : `${formatBytes(row.downloaded)} / ${formatBytes(row.total_size)}`}
        </Text>
      ),
    },
    {
      id: "status",
      header: t("downloadTable.status"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => {
        const pct = row.total_size > 0 ? Math.round((row.downloaded / row.total_size) * 100) : 0;
        if (row.status === "downloading") {
          return <Text size="small">{pct}{t("progress.percent")}</Text>;
        }
        if (row.status === "paused") {
          return <Text size="small">Paused ({pct}{t("progress.percent")})</Text>;
        }
        if (row.status === "completed") {
          return <Text size="small">Completed</Text>;
        }
        return <Text size="small">{row.status}</Text>;
      },
    },
    {
      id: "speed",
      header: t("downloadTable.speed"),
      width: "auto" as const,
      sortBy: (a: DownloadItem, b: DownloadItem) => {
        const si_a = speeds.get(a.id);
        const si_b = speeds.get(b.id);
        return (si_a?.bps ?? 0) - (si_b?.bps ?? 0);
      },
      renderCell: (row: DownloadItem) => {
        const info = speeds.get(row.id);
        return (
          <Text size="small" style={{ whiteSpace: "nowrap" }}>
            {info?.display ?? "—"}
          </Text>
        );
      },
    },
    {
      id: "remain",
      header: t("downloadTable.remain"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => {
        if (row.status !== "downloading") return <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>—</Text>;
        const info = speeds.get(row.id);
        return (
          <Text size="small">
            {info ? computeETA(row, info.bps) : "—"}
          </Text>
        );
      },
    },
    {
      id: "threads",
      header: t("downloadTable.threads"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => (
        <Text size="small">{row.connections}</Text>
      ),
    },
    {
      id: "proxy",
      header: t("downloadTable.proxy"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => (
        <Text size="small">{row.proxy_name || "—"}</Text>
      ),
    },
    {
      id: "resume",
      header: t("downloadTable.resume"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => (
        <Text size="small">
          {row.resumable === true ? t("properties.yes") : row.resumable === false ? t("properties.no") : t("properties.unknown")}
        </Text>
      ),
    },
    {
      id: "last_try",
      header: t("downloadTable.lastTry"),
      width: "auto" as const,
      minWidth: 140,
      renderCell: (row: DownloadItem) => (
        <Text size="small" style={{ whiteSpace: "nowrap" }}>{formatTimestamp(row.last_try)}</Text>
      ),
    },
  ];

  if (isLoading) {
    return (
      <Table.Container>
        <Table.Skeleton columns={skeletonColumns} rows={10} />
      </Table.Container>
    );
  }

  if (filtered.length === 0) {
    return (
      <div style={{ display: "flex", justifyContent: "center", alignItems: "center", height: "100%", padding: 24 }}>
        <Text size="medium" style={{ color: "var(--fgColor-muted, #656d76)" }}>
          {downloads.length === 0 ? t("downloadTable.empty") : t("downloadTable.noMatch")}
        </Text>
      </div>
    );
  }

  return (
    <>
      <Table.Container>
        <DataTable
          data={filtered}
          columns={dataColumns}
          cellPadding="condensed"
          initialSortColumn="file_name"
          initialSortDirection="ASC"
        />
      </Table.Container>

      {menuState && (() => {
        const item = filtered.find((d) => d.id === menuState.id);
        if (!item) return null;
        return (
          <div
            ref={menuRef}
            style={{
              position: "fixed",
              left: menuState.x,
              top: menuState.y,
              background: "var(--bgColor-default, #ffffff)",
              border: "1px solid var(--borderColor-default, #d0d7de)",
              borderRadius: 6,
              boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
              zIndex: 9999,
              padding: "4px 0",
              minWidth: 160,
            }}
          >
            {menuItemFor(menuState.id)}
          </div>
        );
      })()}
    </>
  );
}
