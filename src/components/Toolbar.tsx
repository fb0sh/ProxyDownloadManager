import { Plus, Play, Square, Trash2, RotateCcw, ScrollText, Settings, Globe, Info, LogOut, Gauge } from "lucide-react";
import { t } from "../i18n";
import { useAppContext } from "../contexts/AppContext";
import type { DownloadItem } from "../types";
import { Button } from "./ui/button";
import { Select } from "./ui/select";
import { useSettings } from "../query/downloadQueries";
import { tauriClient } from "../tauriClient";

interface ToolbarProps {
  hasDownloadingSelected: boolean;
  hasPausedSelected: boolean;
  hasCompletedSelected: boolean;
  hasFailedSelected: boolean;
  hasRedownloadable: boolean;
  onRedownloadItem?: DownloadItem;
}

const RATE_OPTIONS = [
  { v: 0, label: "∞" },
  { v: 256 * 1024, label: "256 KB/s" },
  { v: 1024 * 1024, label: "1 MB/s" },
  { v: 5 * 1024 * 1024, label: "5 MB/s" },
  { v: 10 * 1024 * 1024, label: "10 MB/s" },
];

export default function Toolbar({
  hasDownloadingSelected, hasPausedSelected, hasCompletedSelected, hasFailedSelected,
  hasRedownloadable, onRedownloadItem,
}: ToolbarProps) {
  const { actions } = useAppContext();
  const { onNewDownload, onExtension, onSettings, onAbout, onQuit, onLog,
    onResumeSelected, onPauseSelected, onDeleteSelected, onRedownload } = actions;
  const { settings, saveSettings } = useSettings();

  const handleRedownloadSelected = onRedownloadItem ? () => onRedownload(onRedownloadItem) : undefined;

  const onGlobalRate = async (value: string) => {
    const bps = Number(value);
    if (!settings) {
      await tauriClient.setGlobalRateLimit(bps);
      return;
    }
    await saveSettings({ ...settings, global_rate_limit: bps });
    await tauriClient.setGlobalRateLimit(bps);
  };

  return (
    <div className="flex items-center gap-1 border-b border-border bg-muted px-1.5 py-1">
      <Button variant="default" size="sm" onClick={onNewDownload}>
        <Plus className="h-3.5 w-3.5" /> {t("toolbar.new")}
      </Button>
      {hasPausedSelected && (
        <Button size="sm" onClick={onResumeSelected}><Play className="h-3.5 w-3.5" /> {t("toolbar.resume")}</Button>
      )}
      {hasDownloadingSelected && (
        <Button size="sm" onClick={onPauseSelected}><Square className="h-3.5 w-3.5" /> {t("toolbar.stop")}</Button>
      )}
      {(hasPausedSelected || hasDownloadingSelected || hasCompletedSelected || hasFailedSelected) && (
        <Button size="sm" variant="destructive" onClick={onDeleteSelected}><Trash2 className="h-3.5 w-3.5" /> {t("toolbar.delete")}</Button>
      )}
      {hasRedownloadable && handleRedownloadSelected && (
        <Button size="sm" onClick={handleRedownloadSelected}><RotateCcw className="h-3.5 w-3.5" /> {t("toolbar.redownload")}</Button>
      )}
      <div className="ml-2 flex items-center gap-1 text-muted-foreground">
        <Gauge className="h-3.5 w-3.5" />
        <Select
          className="w-[110px]"
          value={String(settings?.global_rate_limit ?? 0)}
          onChange={(e) => onGlobalRate(e.target.value)}
        >
          {RATE_OPTIONS.map((o) => (
            <option key={o.v} value={o.v}>{o.label}</option>
          ))}
        </Select>
      </div>
      <div className="flex-1" />
      <Button size="sm" variant="ghost" onClick={onLog}><ScrollText className="h-3.5 w-3.5" /> {t("toolbar.log")}</Button>
      <Button size="sm" variant="ghost" onClick={onSettings}><Settings className="h-3.5 w-3.5" /> {t("toolbar.settings")}</Button>
      <Button size="sm" variant="ghost" onClick={onExtension}><Globe className="h-3.5 w-3.5" /> {t("toolbar.extension")}</Button>
      <Button size="sm" variant="ghost" onClick={onAbout}><Info className="h-3.5 w-3.5" /> {t("toolbar.about")}</Button>
      <Button size="sm" variant="ghost" onClick={onQuit}><LogOut className="h-3.5 w-3.5" /> {t("toolbar.quit")}</Button>
    </div>
  );
}
