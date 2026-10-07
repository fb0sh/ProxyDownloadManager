import { useState } from "react";
import { t } from "../../i18n";
import { useStartDownload } from "../../query/downloadQueries";
import { AppDialog } from "../ui/dialog";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { HeadersEditor, newHeaderRow, rowsToHeaders, type HeaderRow } from "../HeadersEditor";

interface NewDownloadDialogProps {
  onClose: () => void;
  initialUrl?: string;
}

export default function NewDownloadDialog({ onClose, initialUrl = "" }: NewDownloadDialogProps) {
  const [url, setUrl] = useState(initialUrl);
  const [rows, setRows] = useState<HeaderRow[]>(() => [newHeaderRow()]);
  const [advanced, setAdvanced] = useState(false);
  const startDownload = useStartDownload();

  const handleStart = async () => {
    if (!url.trim()) return;
    await startDownload.mutateAsync({
      url: url.trim(),
      filename: "",
      proxyName: "",
      connections: 0,
      savePath: "",
      headers: rowsToHeaders(rows),
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
        <div>
          <Button className="h-7 px-2 text-[12px]" onClick={() => setAdvanced((v) => !v)}>
            {advanced ? t("headers.hide") : t("headers.show")}
          </Button>
        </div>
        {advanced && (
          <div className="flex max-h-64 min-h-0 flex-col rounded-md border border-border p-2">
            <HeadersEditor rows={rows} onChange={setRows} disabled={startDownload.isPending} />
          </div>
        )}
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>{t("newDownload.cancel")}</Button>
          <Button variant="default" onClick={handleStart} disabled={!url.trim()}>{t("newDownload.download")}</Button>
        </div>
      </div>
    </AppDialog>
  );
}
