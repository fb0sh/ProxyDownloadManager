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
    await tauriClient.setGlobalRateLimit(bps);
    if (settings) {
      await saveSettings({ ...settings, global_rate_limit: bps });
    }
  };

  return (
    <div className="flex items-center gap-1.5 border-b border-border bg-muted px-2 py-1.5">
      <Button variant="default" onClick={onNewDownload}>
        <Plus className="h-4 w-4" /> {t("toolbar.new")}
      </Button>
      {hasPausedSelected && (
        <Button onClick={onResumeSelected}><Play className="h-4 w-4" /> {t("toolbar.resume")}</Button>
      )}
      {hasDownloadingSelected && (
        <Button onClick={onPauseSelected}><Square className="h-4 w-4" /> {t("toolbar.stop")}</Button>
      )}
      {(hasPausedSelected || hasDownloadingSelected || hasCompletedSelected || hasFailedSelected) && (
        <Button variant="destructive" onClick={onDeleteSelected}><Trash2 className="h-4 w-4" /> {t("toolbar.delete")}</Button>
      )}
      {hasRedownloadable && handleRedownloadSelected && (
        <Button onClick={handleRedownloadSelected}><RotateCcw className="h-4 w-4" /> {t("toolbar.redownload")}</Button>
      )}
      <div className="ml-2 flex items-center gap-1.5 text-[13px] text-muted-foreground">
        <Gauge className="h-4 w-4" />
        <Select
          className="w-[120px]"
          value={String(settings?.global_rate_limit ?? 0)}
          onChange={(e) => onGlobalRate(e.target.value)}
        >
          {RATE_OPTIONS.map((o) => (
            <option key={o.v} value={o.v}>{o.label}</option>
          ))}
        </Select>
      </div>
      <div className="flex-1" />
      <Button variant="ghost" onClick={onLog}><ScrollText className="h-4 w-4" /> {t("toolbar.log")}</Button>
      <Button variant="ghost" onClick={onSettings}><Settings className="h-4 w-4" /> {t("toolbar.settings")}</Button>
      <Button variant="ghost" onClick={onExtension}><Globe className="h-4 w-4" /> {t("toolbar.extension")}</Button>
      <Button variant="ghost" onClick={onAbout}><Info className="h-4 w-4" /> {t("toolbar.about")}</Button>
      <Button variant="ghost" onClick={onQuit}><LogOut className="h-4 w-4" /> {t("toolbar.quit")}</Button>
    </div>
  );
}
