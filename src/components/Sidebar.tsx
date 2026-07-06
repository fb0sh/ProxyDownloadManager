import { Text, NavList } from "@primer/react";
import { useDownloads } from "../query/downloadQueries";
import { DownloadIcon, CheckIcon, PauseIcon } from "@primer/octicons-react";
import { t } from "../i18n";

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
    <div style={{ display: "flex", flexDirection: "column", height: "100%", padding: "12px 0 0 0" }}>
      <Text weight="semibold" size="small" style={{ padding: "0 12px", marginBottom: 8, display: "block" }}>
        {t("sidebar.filters")}
      </Text>
      <div style={{ paddingLeft: 12 }}>
        <NavList>
          <NavList.Item
            aria-current={filter === "all" ? "page" : undefined}
            onClick={() => onFilterChange("all")}
          >
            <NavList.LeadingVisual><DownloadIcon /></NavList.LeadingVisual>
            {t("sidebar.all")} ({counts.all})
          </NavList.Item>
          <NavList.Item
            aria-current={filter === "completed" ? "page" : undefined}
            onClick={() => onFilterChange("completed")}
          >
            <NavList.LeadingVisual><CheckIcon /></NavList.LeadingVisual>
            {t("sidebar.completed")} ({counts.completed})
          </NavList.Item>
          <NavList.Item
            aria-current={filter === "incomplete" ? "page" : undefined}
            onClick={() => onFilterChange("incomplete")}
          >
            <NavList.LeadingVisual><PauseIcon /></NavList.LeadingVisual>
            {t("sidebar.incomplete")} ({counts.incomplete})
          </NavList.Item>
        </NavList>
      </div>
      <div style={{ marginTop: "auto", padding: "8px 12px", borderTop: "1px solid var(--borderColor-muted, #d8dee4)" }}>
        <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>
          {t("sidebar.total")}: {counts.all}
        </Text>
      </div>
    </div>
  );
}
