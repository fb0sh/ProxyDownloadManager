import { useState, useEffect } from "react";
import { tauriClient } from "../../tauriClient";
import { t } from "../../i18n";
import { AppDialog } from "../ui/dialog";
import { Button } from "../ui/button";

interface LogDialogProps {
  onClose: () => void;
}

export default function LogDialog({ onClose }: LogDialogProps) {
  const [logs, setLogs] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  const loadLogs = async () => {
    setLoading(true);
    try {
      const data = await tauriClient.readLogs(50);
      setLogs(data);
    } catch { setLogs(["[ERROR] Failed to read logs"]); }
    setLoading(false);
  };

  useEffect(() => { loadLogs(); }, []);

  return (
    <AppDialog title={t("log.title")} onClose={onClose} width="max-w-3xl">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-[12px] text-muted-foreground">{logs.length} {t("log.entries")}</span>
        <Button size="sm" onClick={loadLogs} disabled={loading}>
          {loading ? t("log.loading") : t("log.refresh")}
        </Button>
      </div>
      <pre className="max-h-[420px] overflow-auto p-3 font-mono text-[12px] leading-5">
        {logs.length === 0 ? t("log.noEntries") : logs.join("\n")}
      </pre>
    </AppDialog>
  );
}
