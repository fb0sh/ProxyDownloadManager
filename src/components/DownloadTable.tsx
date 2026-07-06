import { Text } from "@primer/react";
import { useDownloads } from "../query/downloadQueries";
import DownloadRow from "./DownloadRow";
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

export default function DownloadTable({ selectedIds, onSelectChange, filter, onStop, onDelete, onProperties, onRedownload }: DownloadTableProps) {
  const { data: downloads = [], isLoading } = useDownloads();
  const filtered = applyFilter(downloads, filter);

  if (isLoading) {
    return (
      <div style={{ display: "flex", justifyContent: "center", padding: 24 }}>
        <Text style={{ color: "var(--fgColor-muted, #656d76)" }}>Loading...</Text>
      </div>
    );
  }

  if (filtered.length === 0) {
    return (
      <div style={{ display: "flex", justifyContent: "center", padding: 24 }}>
        <Text style={{ color: "var(--fgColor-muted, #656d76)" }}>
          {downloads.length === 0 ? 'No downloads yet. Click "New" to start one.' : "No matching downloads."}
        </Text>
      </div>
    );
  }

  const toggleSelect = (id: number) => {
    const next = new Set(selectedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    onSelectChange(next);
  };

  return (
    <div>
      {filtered.map((d) => (
        <DownloadRow
          key={d.id}
          item={d}
          selected={selectedIds.has(d.id)}
          onToggleSelect={() => toggleSelect(d.id)}
          onStop={onStop}
          onDelete={onDelete}
          onProperties={onProperties}
          onRedownload={onRedownload}
        />
      ))}
    </div>
  );
}
