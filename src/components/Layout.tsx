import { SegmentedControl } from "@primer/react";
import { DownloadIcon, CheckIcon, PauseIcon } from "@primer/octicons-react";
import Toolbar from "./Toolbar";
import DownloadTable from "./DownloadTable";
import { useDownloads } from "../query/downloadQueries";
import { isFailed } from "../utils/download";
import { t } from "../i18n";
import { useAppContext } from "../contexts/AppContext";

interface LayoutProps {
  onRedownloadItem?: import("../types").DownloadItem;
}

export default function Layout({ onRedownloadItem }: LayoutProps) {
  const { selectedIds, filter, setFilter } = useAppContext();
  const { data: downloads = [] } = useDownloads();

  const selectedDownloadStatuses = downloads
    .filter((d) => selectedIds.has(d.id))
    .map((d) => d.status);
  const hasDownloadingSelected = selectedDownloadStatuses.some((s) => s === "downloading");
  const hasPausedSelected = selectedDownloadStatuses.some((s) => s === "paused");
  const hasCompletedSelected = selectedDownloadStatuses.some((s) => s === "completed");
  const hasFailedSelected = selectedDownloadStatuses.some((s) => isFailed(s));

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
        hasDownloadingSelected={hasDownloadingSelected}
        hasPausedSelected={hasPausedSelected}
        hasCompletedSelected={hasCompletedSelected}
        hasFailedSelected={hasFailedSelected}
        hasRedownloadable={!!onRedownloadItem}
        onRedownloadItem={onRedownloadItem}
      />
      <div style={{ display: "flex", padding: "6px 8px", borderBottom: "1px solid var(--borderColor-muted, #d8dee4)" }}>
      <SegmentedControl
        aria-label={t("sidebar.filters")}
        size="small"
        onChange={(idx) => {
          const map = ["all", "completed", "incomplete"];
          setFilter(map[idx ?? 0] as "all" | "completed" | "incomplete");
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
        <DownloadTable filter={filter} />
      </div>
    </div>
  );
}
