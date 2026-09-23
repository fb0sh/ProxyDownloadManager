import { t } from "../../i18n";
import { useDownloadDetail } from "../../hooks/useDownloadDetail";
import { formatBytes, formatRateLimit, statusColor, statusString } from "../../utils/format";
import { overallPercent } from "../../utils/progressMap";
import { AppDialog } from "../ui/dialog";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Progress } from "../ui/progress";

interface PropertiesDialogProps {
  id: number;
  onClose: () => void;
}

export default function PropertiesDialog({ id, onClose }: PropertiesDialogProps) {
  const { item, urlCopied, handleCopyUrl } = useDownloadDetail(id, { subscribe: false });
  if (!item) return null;
  const progress = overallPercent(item.downloaded, item.total_size, item.status);
  const variant = statusColor(item.status);
  const badge = variant === "attention" ? "warning" : variant === "default" ? "default" : variant;

  return (
    <AppDialog title={t("properties.title")} onClose={onClose}>
      <div className="flex flex-col gap-3 p-3">
        <div className="flex items-center gap-2">
          <div className="min-w-0 flex-1 truncate font-semibold">{item.file_name}</div>
          <Badge variant={badge}>{statusString(item.status)}</Badge>
        </div>
        <Progress value={progress} />
        <InfoRow label={t("properties.size")} value={item.total_size ? `${formatBytes(item.downloaded)} / ${formatBytes(item.total_size)}` : "—"} />
        <InfoRow label={t("properties.savePath")} value={item.save_path} />
        <InfoRow label={t("properties.connections")} value={String(item.connections)} />
        <InfoRow label={t("properties.proxy")} value={item.proxy_name || t("properties.none")} />
        <InfoRow label={t("properties.speedLimit")} value={formatRateLimit(item.rate_limit_bps || 0)} />
        <div className="flex items-start gap-2 text-[13px]">
          <div className="w-32 shrink-0 text-muted-foreground">{t("properties.url")}</div>
          <div className="min-w-0 flex-1 break-all">{item.url}</div>
          <Button size="sm" onClick={handleCopyUrl}>{urlCopied ? "OK" : "Copy"}</Button>
        </div>
        <p className="text-[11px] text-muted-foreground">{t("properties.authHidden")}</p>
      </div>
    </AppDialog>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex gap-2 text-[13px]">
      <div className="w-32 shrink-0 text-muted-foreground">{label}</div>
      <div className="min-w-0 flex-1 break-all">{value || "—"}</div>
    </div>
  );
}
