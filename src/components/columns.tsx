import { Text, Checkbox } from "@primer/react";
import { t } from "../i18n";
import { formatBytes, formatTimestamp, statusString } from "../utils/format";
import type { DownloadItem } from "../types";
import type { IconMap } from "../hooks/useFileIcons";
import { iconFor } from "../hooks/useFileIcons";
import type { SpeedInfo } from "../hooks/useDownloadSpeed";
import { computeETA } from "../hooks/useDownloadSpeed";

interface ColumnContext {
  selectedIds: Set<number>;
  toggleSelect: (id: number) => void;
  selectAllChecked: boolean;
  selectAllIndeterminate: boolean;
  toggleSelectAll: () => void;
  speeds: Map<number, SpeedInfo>;
  icons: IconMap;
  onContextMenu: (e: React.MouseEvent, id: number) => void;
  onStop: (id: number) => void;
  onDelete: (ids: number[]) => void;
  onProperties: (id: number) => void;
  onRedownload: (item: DownloadItem) => void;
}

export function buildColumns(ctx: ColumnContext) {
  const {
    selectedIds, toggleSelect, selectAllChecked, selectAllIndeterminate,
    toggleSelectAll, speeds, icons, onContextMenu,
  } = ctx;

  const ctxWrap = (row: DownloadItem, content: React.ReactNode) => (
    <div onContextMenu={(e) => onContextMenu(e, row.id)} style={{ cursor: "context-menu" }}>
      {content}
    </div>
  );

  return [
    {
      id: "select",
      header: () => (
        <Checkbox
          checked={selectAllChecked}
          onChange={toggleSelectAll}
          ref={(el) => { if (el) el.indeterminate = selectAllIndeterminate; }}
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
        <div onContextMenu={(e) => onContextMenu(e, row.id)} style={{ cursor: "context-menu", display: "flex", alignItems: "center", gap: 4 }}>
          <img src={iconFor(icons, row.file_name)} alt="" width={18} height={18} style={{ flexShrink: 0 }} />
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
      renderCell: (row: DownloadItem) => ctxWrap(row,
        <Text size="small" style={{ whiteSpace: "nowrap" }}>
          {row.total_size === 0 ? "—" : row.status === "completed" ? formatBytes(row.total_size) : `${formatBytes(row.downloaded)} / ${formatBytes(row.total_size)}`}
        </Text>
      ),
    },
    {
      id: "status",
      header: t("downloadTable.status"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => {
        const pct = row.total_size > 0 ? Math.round((row.downloaded / row.total_size) * 100) : 0;
        if (row.total_size === 0) {
          return ctxWrap(row, <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>—</Text>);
        }
        if (row.status === "downloading") {
          return ctxWrap(row, <Text size="small">{pct}{t("progress.percent")}</Text>);
        }
        if (row.status === "paused") {
          return ctxWrap(row, <Text size="small">Paused ({pct}{t("progress.percent")})</Text>);
        }
        if (row.status === "completed") {
          return ctxWrap(row, <Text size="small">Completed</Text>);
        }
        return ctxWrap(row, <Text size="small">{statusString(row.status)}</Text>);
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
        return ctxWrap(row,
          <Text size="small" style={{ whiteSpace: "nowrap" }}>{info?.display ?? "—"}</Text>
        );
      },
    },
    {
      id: "remain",
      header: t("downloadTable.remain"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => {
        if (row.status !== "downloading") return ctxWrap(row, <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>—</Text>);
        const info = speeds.get(row.id);
        return ctxWrap(row,
          <Text size="small">{info ? computeETA(row, info.bps) : "—"}</Text>
        );
      },
    },
    {
      id: "threads",
      header: t("downloadTable.threads"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => ctxWrap(row, <Text size="small">{row.connections}</Text>),
    },
    {
      id: "proxy",
      header: t("downloadTable.proxy"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => ctxWrap(row, <Text size="small">{row.proxy_name || "—"}</Text>),
    },
    {
      id: "resume",
      header: t("downloadTable.resume"),
      width: "auto" as const,
      renderCell: (row: DownloadItem) => ctxWrap(row,
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
      renderCell: (row: DownloadItem) => ctxWrap(row,
        <Text size="small" style={{ whiteSpace: "nowrap" }}>{formatTimestamp(row.last_try)}</Text>
      ),
    },
  ];
}
