import { memo, useCallback } from "react";
import { useDownloads } from "../query/downloadQueries";
import { useDownloadSpeed } from "../hooks/useDownloadSpeed";
import { useFileIcons, iconFor } from "../hooks/useFileIcons";
import { t } from "../i18n";
import { applyFilter, openFile, openFolder } from "../utils/download";
import { useAppContext } from "../contexts/AppContext";
import { useContextMenu } from "../hooks/useContextMenu";
import { Checkbox } from "./ui/checkbox";
import { overallPercent } from "../utils/progressMap";
import { formatBytes, statusString, isFailed, isActiveStatus } from "../utils/format";
import { computeETA } from "../hooks/useDownloadSpeed";
import type { DownloadItem } from "../types";
import type { StatusFilter, TypeFilter } from "../utils/url";
import { Progress } from "./ui/progress";

interface DownloadTableProps {
  filter: StatusFilter;
  query?: string;
  typeFilter?: TypeFilter;
}

export default function DownloadTable({ filter, query = "", typeFilter = "all" }: DownloadTableProps) {
  const { selectedIds, selectionActions, actions } = useAppContext();
  const { onStop, onDelete, onProperties, onRedownload } = actions;
  const { data: downloads = [], isLoading } = useDownloads();
  const filtered = applyFilter(downloads, filter, query, typeFilter);
  const speeds = useDownloadSpeed(filtered);
  const icons = useFileIcons(filtered);
  const { menuState, menuRef, handleContext, closeMenu } = useContextMenu();

  const selectAllChecked = filtered.length > 0 && filtered.every((d) => selectedIds.has(d.id));
  const selectAllIndeterminate = !selectAllChecked && filtered.some((d) => selectedIds.has(d.id));

  const toggleSelectAll = () => {
    if (selectAllChecked) selectionActions.clearSelection();
    else selectionActions.select(new Set(filtered.map((d) => d.id)));
  };

  const onDoubleClick = useCallback(async (item: DownloadItem) => {
    if (item.status === "completed") await openFile(item.save_path);
    else actions.onProperties(item.id);
  }, [actions]);

  if (isLoading) {
    return <div className="p-6 text-muted-foreground">{t("downloadTable.loading")}</div>;
  }
  if (filtered.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-muted-foreground">
        {downloads.length === 0 ? t("downloadTable.empty") : t("downloadTable.noMatch")}
      </div>
    );
  }

  const menuItemFor = (id: number) => {
    const item = filtered.find((d) => d.id === id);
    if (!item) return null;
    const s = statusString(item.status);
    return (
      <>
        {isActiveStatus(item.status) && <MenuItem label={t("downloadRow.pause")} onClick={() => { closeMenu(); onStop(id); }} />}
        {s === "paused" && <MenuItem label={t("downloadRow.resume")} onClick={() => { closeMenu(); actions.onResumeSelected(); }} />}
        <MenuItem label={t("downloadRow.restart")} onClick={() => { closeMenu(); onRedownload(item); }} />
        <MenuItem label={t("downloadRow.open")} onClick={() => { closeMenu(); openFile(item.save_path); }} />
        <MenuItem label={t("downloadRow.openFolder")} onClick={() => { closeMenu(); openFolder(item.save_path); }} />
        <MenuItem label={t("downloadRow.copyUrl")} onClick={() => { closeMenu(); navigator.clipboard.writeText(item.url); }} />
        <MenuItem label={t("downloadRow.refreshUrl")} onClick={() => { closeMenu(); onProperties(id); }} />
        <MenuItem label={t("downloadRow.properties")} onClick={() => { closeMenu(); onProperties(id); }} />
        <MenuItem label={t("toolbar.delete")} danger onClick={() => { closeMenu(); onDelete([id]); }} />
      </>
    );
  };

  return (
    <div className="relative">
      <table className="w-full border-collapse text-[13px]">
        <thead className="sticky top-0 z-10 bg-muted text-left text-[11px] uppercase tracking-wide text-muted-foreground">
          <tr>
            <th className="w-8 px-2 py-1">
              <Checkbox checked={selectAllChecked} onCheckedChange={toggleSelectAll} {...(selectAllIndeterminate ? { "data-state": "indeterminate" } : {})} />
            </th>
            <th className="px-2 py-1">{t("downloadTable.fileName")}</th>
            <th className="w-[140px] px-2 py-1">{t("downloadTable.size")}</th>
            <th className="w-[160px] px-2 py-1">{t("downloadTable.status")}</th>
            <th className="w-[90px] px-2 py-1">{t("downloadTable.speed")}</th>
            <th className="w-[90px] px-2 py-1">{t("downloadTable.remain")}</th>
            <th className="w-[70px] px-2 py-1">{t("downloadTable.threads")}</th>
            <th className="w-[90px] px-2 py-1">{t("downloadTable.proxy")}</th>
          </tr>
        </thead>
        <tbody>
          {filtered.map((row) => (
            <DownloadRow
              key={row.id}
              item={row}
              selected={selectedIds.has(row.id)}
              icon={iconFor(icons, row.file_name)}
              speed={speeds.get(row.id)?.display ?? "—"}
              bps={speeds.get(row.id)?.bps ?? 0}
              onToggle={() => selectionActions.toggle(row.id)}
              onContext={(e) => handleContext(e, row.id)}
              onDoubleClick={() => onDoubleClick(row)}
            />
          ))}
        </tbody>
      </table>
      {menuState && (
        <div
          ref={menuRef}
          className="fixed z-20 min-w-[160px] rounded-md border border-border bg-card py-1 shadow-sm"
          style={{ left: menuState.x, top: menuState.y }}
        >
          {menuItemFor(menuState.id)}
        </div>
      )}
    </div>
  );
}

const DownloadRow = memo(function DownloadRow({
  item, selected, icon, speed, bps, onToggle, onContext, onDoubleClick,
}: {
  item: DownloadItem;
  selected: boolean;
  icon: string;
  speed: string;
  bps: number;
  onToggle: () => void;
  onContext: (e: React.MouseEvent) => void;
  onDoubleClick: () => void;
}) {
  const pct = overallPercent(item.downloaded, item.total_size, item.status);
  const live = isActiveStatus(item.status);
  const size = item.total_size === 0
    ? "—"
    : item.status === "completed"
      ? formatBytes(item.total_size)
      : `${formatBytes(item.downloaded)} / ${formatBytes(item.total_size)}`;
  return (
    <tr
      className={`border-b border-border hover:bg-muted/70 ${selected ? "bg-muted" : ""} ${live ? "row-live" : ""}`}
      onContextMenu={onContext}
      onDoubleClick={onDoubleClick}
    >
      <td className="px-2 py-1"><Checkbox checked={selected} onCheckedChange={onToggle} /></td>
      <td className="max-w-0 px-2 py-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <img src={icon} alt="" width={16} height={16} className="shrink-0" />
          <span className="truncate font-medium">{item.file_name}</span>
        </div>
      </td>
      <td className="tabular px-2 py-1 whitespace-nowrap">{size}</td>
      <td className="px-2 py-1">
        {item.total_size > 0 && live ? (
          <div className="flex items-center gap-2">
            <Progress className="w-20" value={pct} />
            <span className="tabular text-[12px]">{pct}%</span>
          </div>
        ) : (
          <span>{isFailed(item.status) ? item.error_message || "failed" : statusString(item.status)}</span>
        )}
      </td>
      <td className="tabular px-2 py-1 whitespace-nowrap">{live ? speed : "—"}</td>
      <td className="tabular px-2 py-1 whitespace-nowrap">{live ? computeETA(item, bps) : "—"}</td>
      <td className="tabular px-2 py-1">{item.connections || "A"}</td>
      <td className="truncate px-2 py-1 text-muted-foreground">{item.proxy_name || "—"}</td>
    </tr>
  );
});

function MenuItem({ label, onClick, danger }: { label: string; onClick: () => void; danger?: boolean }) {
  return (
    <div
      onClick={onClick}
      className={`cursor-pointer px-3 py-1.5 text-[13px] hover:bg-muted ${danger ? "text-destructive" : ""}`}
    >
      {label}
    </div>
  );
}
