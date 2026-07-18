import { useState } from "react";
import { Button, TextInput } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { t } from "../../i18n";
import { useStartDownload } from "../../query/downloadQueries";

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
    <Dialog onClose={onClose} title={t("newDownload.title")}>
      <Dialog.Body>
        <TextInput
          block
          placeholder="https://example.com/file.zip"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") handleStart(); }}
        />
      </Dialog.Body>
      <Dialog.Footer>
        <Button onClick={onClose}>Cancel</Button>
        <Button variant="primary" onClick={handleStart} disabled={!url.trim()}>
          {t("newDownload.start")}
        </Button>
      </Dialog.Footer>
    </Dialog>
  );
}
