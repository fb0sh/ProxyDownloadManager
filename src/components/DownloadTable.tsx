import { Text } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useDownloads } from "../query/downloadQueries";
import { useDownloadSpeed } from "../hooks/useDownloadSpeed";
import { useFileIcons } from "../hooks/useFileIcons";
import { t } from "../i18n";
import { applyFilter, openFile, openFolder } from "../utils/download";
import { useAppContext } from "../contexts/AppContext";
import { useContextMenu } from "../hooks/useContextMenu";
import { buildColumns } from "./columns";

interface DownloadTableProps {
  filter: "all" | "completed" | "incomplete";
}

export default function DownloadTable({ filter }: DownloadTableProps) {
  const { selectedIds, selectionActions, actions } = useAppContext();
  const { select, clearSelection } = selectionActions;
  const { onStop, onDelete, onProperties, onRedownload } = actions;
  const { data: downloads = [], isLoading } = useDownloads();
  console.debug('[ProxyDM FE] DownloadTable render filter=', filter, 'count=', downloads.length);
  const filtered = applyFilter(downloads, filter);
  const speeds = useDownloadSpeed(filtered);
  const icons = useFileIcons(filtered);
  const { menuState, menuRef, handleContext, closeMenu } = useContextMenu();

  const selectAllChecked = filtered.length > 0 && filtered.every((d) => selectedIds.has(d.id));
  const selectAllIndeterminate = !selectAllChecked && filtered.some((d) => selectedIds.has(d.id));

  const toggleSelectAll = () => {
    if (selectAllChecked) {
      clearSelection();
    } else {
      select(new Set(filtered.map((d) => d.id)));
    }
  };

  const toggleSelect = (id: number) => {
    selectionActions.toggle(id);
  };

  const handleOpen = async (path: string) => {
    closeMenu();
    await openFile(path);
  };

  const handleOpenFolder = async (path: string) => {
    closeMenu();
    await openFolder(path);
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

  const columns = buildColumns({
    selectedIds, toggleSelect, selectAllChecked, selectAllIndeterminate,
    toggleSelectAll, speeds, icons, onContextMenu: handleContext,
    onStop, onDelete, onProperties, onRedownload,
  });

  if (isLoading) {
    return (
      <Table.Container>
        <Table.Skeleton columns={columns.map((c) => ({ header: c.header, width: c.width }))} rows={10} />
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
          columns={columns}
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
