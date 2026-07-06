import Sidebar from "./Sidebar";
import Toolbar from "./Toolbar";
import DownloadTable from "./DownloadTable";

interface LayoutProps {
  onNewDownload: () => void;
  onSettings: () => void;
  onAbout: () => void;
  onQuit: () => void;
  onLog: () => void;
  onResumeSelected: () => void;
  onPauseSelected: () => void;
  onDeleteSelected: () => void;
  onStop: (id: number) => void;
  onDelete: (ids: number[]) => void;
  onProperties: (id: number) => void;
  onRedownload: (item: import("../types").DownloadItem) => void;
  onRedownloadItem?: import("../types").DownloadItem;
  selectedIds: Set<number>;
  onSelectChange: (ids: Set<number>) => void;
  hasSelection: boolean;
  filter: "all" | "completed" | "incomplete";
  onFilterChange: (f: "all" | "completed" | "incomplete") => void;
}

export default function Layout({
  onNewDownload, onSettings, onAbout, onQuit, onLog,
  onResumeSelected, onPauseSelected, onDeleteSelected,
  onStop, onDelete, onProperties, onRedownload, onRedownloadItem,
  selectedIds, onSelectChange, hasSelection,
  filter, onFilterChange,
}: LayoutProps) {
  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh" }}>
      <Toolbar
        onNewDownload={onNewDownload}
        onSettings={onSettings}
        onAbout={onAbout}
        onQuit={onQuit}
        onLog={onLog}
        onResumeSelected={onResumeSelected}
        onPauseSelected={onPauseSelected}
        onDeleteSelected={onDeleteSelected}
        onRedownloadSelected={onRedownloadItem ? () => onRedownload(onRedownloadItem) : undefined}
        hasSelection={hasSelection}
        hasRedownloadable={!!onRedownloadItem}
      />
      <div style={{ display: "flex", flex: 1, overflow: "hidden" }}>
        <div
          style={{
            width: 180,
            flexShrink: 0,
            borderRight: "1px solid var(--borderColor-muted, #d8dee4)",
          }}
        >
          <Sidebar filter={filter} onFilterChange={onFilterChange} />
        </div>
        <div style={{ flex: 1, overflow: "auto" }}>
          <DownloadTable selectedIds={selectedIds} onSelectChange={onSelectChange} filter={filter} onStop={onStop} onDelete={onDelete} onProperties={onProperties} onRedownload={onRedownload} />
        </div>
      </div>
    </div>
  );
}
