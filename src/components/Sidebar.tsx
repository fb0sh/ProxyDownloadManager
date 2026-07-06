import { Text, NavList } from "@primer/react";
import { useDownloads } from "../query/downloadQueries";
import { DownloadIcon, CheckIcon, PauseIcon } from "@primer/octicons-react";

interface SidebarProps {
  filter: "all" | "completed" | "incomplete";
  onFilterChange: (f: "all" | "completed" | "incomplete") => void;
}

export default function Sidebar({ filter, onFilterChange }: SidebarProps) {
  const { data: downloads = [] } = useDownloads();

  const counts = {
    all: downloads.length,
    completed: downloads.filter((d) => d.status === "completed").length,
    incomplete: downloads.filter(
      (d) => d.status === "downloading" || d.status === "paused" || d.status === "queued"
    ).length,
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", padding: 16 }}>
      <Text weight="semibold" size="medium" style={{ marginBottom: 8, display: "block" }}>
        Filters
      </Text>
      <NavList>
        <NavList.Item
          aria-current={filter === "all" ? "page" : undefined}
          onClick={() => onFilterChange("all")}
        >
          <NavList.LeadingVisual><DownloadIcon /></NavList.LeadingVisual>
          All ({counts.all})
        </NavList.Item>
        <NavList.Item
          aria-current={filter === "completed" ? "page" : undefined}
          onClick={() => onFilterChange("completed")}
        >
          <NavList.LeadingVisual><CheckIcon /></NavList.LeadingVisual>
          Completed ({counts.completed})
        </NavList.Item>
        <NavList.Item
          aria-current={filter === "incomplete" ? "page" : undefined}
          onClick={() => onFilterChange("incomplete")}
        >
          <NavList.LeadingVisual><PauseIcon /></NavList.LeadingVisual>
          Incomplete ({counts.incomplete})
        </NavList.Item>
      </NavList>
      <div style={{ marginTop: "auto", paddingTop: 16 }}>
        <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>
          Total: {counts.all}
        </Text>
      </div>
    </div>
  );
}
