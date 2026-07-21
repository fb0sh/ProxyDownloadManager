import { useState, useCallback } from "react";
import type { DownloadItem } from "../types";

export interface SelectionActions {
  /** Select specific IDs. */
  select: (ids: Set<number>) => void;
  /** Clear all selections. */
  clearSelection: () => void;
  /** Remove specific IDs (e.g. after delete) without clearing the rest. */
  removeIds: (ids: number[]) => void;
  /** Toggle a single ID in/out of selection. */
  toggle: (id: number) => void;
  /** Select all items matching a predicate. */
  selectWhere: (items: DownloadItem[], predicate: (item: DownloadItem) => boolean) => void;
}

export function useSelection(): { selectedIds: Set<number> } & SelectionActions {
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  const select = useCallback((ids: Set<number>) => setSelectedIds(ids), []);
  const clearSelection = useCallback(() => setSelectedIds(new Set()), []);
  const removeIds = useCallback((ids: number[]) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      for (const id of ids) next.delete(id);
      return next;
    });
  }, []);
  const toggle = useCallback((id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);
  const selectWhere = useCallback(
    (items: DownloadItem[], predicate: (item: DownloadItem) => boolean) => {
      setSelectedIds(new Set(items.filter(predicate).map((d) => d.id)));
    },
    [],
  );

  return { selectedIds, select, clearSelection, removeIds, toggle, selectWhere };
}
