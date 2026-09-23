import { useState } from "react";
import { t } from "../../i18n";
import { useStartDownload } from "../../query/downloadQueries";
import { AppDialog } from "../ui/dialog";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

interface NewDownloadDialogProps {
  onClose: () => void;
  initialUrl?: string;
}

export default function NewDownloadDialog({ onClose, initialUrl = "" }: NewDownloadDialogProps) {
  const [url, setUrl] = useState(initialUrl);
  const startDownload = useStartDownload();

  const handleStart = async () => {
    if (!url.trim()) return;
    await startDownload.mutateAsync({
      url: url.trim(),
      filename: "",
      proxyName: "",
      connections: 0,
      savePath: "",
    });
    onClose();
  };

  return (
    <AppDialog title={t("newDownload.title")} onClose={onClose}>
      <div className="flex flex-col gap-3 p-3">
        <Input
          placeholder="https://example.com/file.zip"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") handleStart(); }}
        />
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>{t("newDownload.cancel")}</Button>
          <Button variant="default" onClick={handleStart} disabled={!url.trim()}>{t("newDownload.download")}</Button>
        </div>
      </div>
    </AppDialog>
  );
}
