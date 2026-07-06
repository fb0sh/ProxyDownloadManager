import { SegmentedControl } from "@primer/react";
import { DownloadIcon, CheckIcon, PauseIcon } from "@primer/octicons-react";
import Toolbar from "./Toolbar";
import DownloadTable from "./DownloadTable";
import { useDownloads } from "../query/downloadQueries";
import { t } from "../i18n";

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
  const { data: downloads = [] } = useDownloads();

  const counts = {
    all: downloads.length,
    completed: downloads.filter((d) => d.status === "completed").length,
    incomplete: downloads.filter(
      (d) => d.status === "downloading" || d.status === "paused" || d.status === "queued"
    ).length,
  };

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
      <div style={{ display: "flex", padding: "6px 8px", borderBottom: "1px solid var(--borderColor-muted, #d8dee4)" }}>
      <SegmentedControl
        aria-label={t("sidebar.filters")}
        size="small"
        onChange={(idx) => {
          const map = ["all", "completed", "incomplete"];
          onFilterChange(map[idx ?? 0] as "all" | "completed" | "incomplete");
        }}
      >
        <SegmentedControl.Button
          leadingIcon={<DownloadIcon />}
          selected={filter === "all"}
        >
          {`${t("sidebar.all")} ${counts.all}`}
        </SegmentedControl.Button>
        <SegmentedControl.Button
          leadingIcon={<CheckIcon />}
          selected={filter === "completed"}
        >
          {`${t("sidebar.completed")} ${counts.completed}`}
        </SegmentedControl.Button>
        <SegmentedControl.Button
          leadingIcon={<PauseIcon />}
          selected={filter === "incomplete"}
        >
          {`${t("sidebar.incomplete")} ${counts.incomplete}`}
        </SegmentedControl.Button>
      </SegmentedControl>
      </div>
      <div style={{ flex: 1, overflow: "auto" }}>
        <DownloadTable selectedIds={selectedIds} onSelectChange={onSelectChange} filter={filter} onStop={onStop} onDelete={onDelete} onProperties={onProperties} onRedownload={onRedownload} />
      </div>
    </div>
  );
}
