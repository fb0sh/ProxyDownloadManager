import { useState } from "react";
import Toolbar from "./Toolbar";
import DownloadTable from "./DownloadTable";
import { useDownloads } from "../query/downloadQueries";
import { isFailed } from "../utils/download";
import { t } from "../i18n";
import { useAppContext } from "../contexts/AppContext";
import { Input } from "./ui/input";
import { Select } from "./ui/select";
import type { TypeFilter } from "../utils/url";
import { cn } from "../lib/utils";

interface LayoutProps {
  onRedownloadItem?: import("../types").DownloadItem;
  className?: string;
}

export default function Layout({ onRedownloadItem, className }: LayoutProps) {
  const { selectedIds, filter, setFilter } = useAppContext();
  const { data: downloads = [] } = useDownloads();
  const [query, setQuery] = useState("");
  const [type, setType] = useState<TypeFilter>("all");

  const selectedDownloadStatuses = downloads
    .filter((d) => selectedIds.has(d.id))
    .map((d) => d.status);
  const hasDownloadingSelected = selectedDownloadStatuses.some((s) => s === "downloading" || s === "connecting" || s === "retrying");
  const hasPausedSelected = selectedDownloadStatuses.some((s) => s === "paused");
  const hasCompletedSelected = selectedDownloadStatuses.some((s) => s === "completed");
  const hasFailedSelected = selectedDownloadStatuses.some((s) => isFailed(s));

  const counts = {
    all: downloads.length,
    downloading: downloads.filter((d) => d.status === "downloading" || d.status === "connecting" || d.status === "retrying" || d.status === "merging").length,
    completed: downloads.filter((d) => d.status === "completed").length,
    incomplete: downloads.filter((d) => d.status !== "completed").length,
  };

  const filters: Array<{ id: typeof filter; label: string; n: number }> = [
    { id: "all", label: t("sidebar.all"), n: counts.all },
    { id: "downloading", label: t("sidebar.downloading"), n: counts.downloading },
    { id: "completed", label: t("sidebar.completed"), n: counts.completed },
    { id: "incomplete", label: t("sidebar.incomplete"), n: counts.incomplete },
  ];

  return (
    <div className={cn("flex h-screen flex-col", className)}>
      <Toolbar
        hasDownloadingSelected={hasDownloadingSelected}
        hasPausedSelected={hasPausedSelected}
        hasCompletedSelected={hasCompletedSelected}
        hasFailedSelected={hasFailedSelected}
        hasRedownloadable={!!onRedownloadItem}
        onRedownloadItem={onRedownloadItem}
      />
      <div className="flex items-center gap-2 border-b border-border px-2 py-1.5">
        {filters.map((f) => (
          <button
            key={f.id}
            type="button"
            onClick={() => setFilter(f.id)}
            className={`h-8 rounded-md px-3 text-[13px] ${filter === f.id ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-muted"}`}
          >
            {f.label} {f.n}
          </button>
        ))}
        <div className="flex-1" />
        <Select className="w-[120px]" value={type} onChange={(e) => setType(e.target.value as TypeFilter)}>
          <option value="all">{t("sidebar.typeAll")}</option>
          <option value="archive">{t("sidebar.typeArchive")}</option>
          <option value="video">{t("sidebar.typeVideo")}</option>
          <option value="audio">{t("sidebar.typeAudio")}</option>
          <option value="document">{t("sidebar.typeDocument")}</option>
          <option value="other">{t("sidebar.typeOther")}</option>
        </Select>
        <Input
          className="w-[180px]"
          placeholder={t("sidebar.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <DownloadTable filter={filter} query={query} typeFilter={type} />
      </div>
    </div>
  );
}
