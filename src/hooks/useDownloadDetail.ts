import { useState } from "react";
import { useDownload } from "../query/downloadQueries";
import { openFile, openFolder } from "../utils/download";

export function useDownloadDetail(id: number | undefined) {
  const item = useDownload(id);
  const [urlCopied, setUrlCopied] = useState(false);

  const handleCopyUrl = async () => {
    try {
      await navigator.clipboard.writeText(item?.url ?? "");
      setUrlCopied(true);
      setTimeout(() => setUrlCopied(false), 2000);
    } catch { /* clipboard not available */ }
  };

  const handleOpenFile = async () => {
    if (!item) return;
    await openFile(item.save_path);
  };

  const handleOpenFolder = async () => {
    if (!item) return;
    await openFolder(item.save_path);
  };

  return { item, urlCopied, handleCopyUrl, handleOpenFile, handleOpenFolder };
}

export function useDownloadIdFromUrl(): number | undefined {
  const p = new URLSearchParams(window.location.search);
  const idParam = p.get("id");
  return idParam ? Number(idParam) : undefined;
}
