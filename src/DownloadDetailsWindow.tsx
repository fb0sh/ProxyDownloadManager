import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { formatBytes, formatRateLimit, statusColor, statusString } from "./utils/format";
import { useDownloadDetail, useDownloadIdFromUrl } from "./hooks/useDownloadDetail";
import { useDownloadSpeed } from "./hooks/useDownloadSpeed";
import { useSettings } from "./query/downloadQueries";
import ProgressMap from "./components/ProgressMap";
import { overallPercent } from "./utils/progressMap";
import { setLanguage, t } from "./i18n";
import { Button } from "./components/ui/button";
import { Badge } from "./components/ui/badge";
import { Progress } from "./components/ui/progress";
import { Select } from "./components/ui/select";
import { Input } from "./components/ui/input";
import { Label } from "./components/ui/label";
import { tauriClient } from "./tauriClient";

export default function DownloadDetailsWindow() {
  const idParam = new URLSearchParams(window.location.search).get("id");
  const id = useDownloadIdFromUrl();
  const { settings: loadedSettings } = useSettings();
  const [, bumpLang] = useState(0);
  const [conns, setConns] = useState<number | null>(null);
  const [rate, setRate] = useState("0");
  const [newUrl, setNewUrl] = useState("");

  useEffect(() => {
    if (loadedSettings) {
      setLanguage(loadedSettings.language || "en");
      bumpLang((n) => n + 1);
    }
  }, [loadedSettings]);

  const {
    item, urlCopied, controls, handleCopyUrl, handleOpenFile, handleOpenFolder, handlePause, handleResume,
  } = useDownloadDetail(id);

  const speedInputs = item && (item.status === "downloading" || item.status === "connecting") ? [item] : [];
  const speeds = useDownloadSpeed(speedInputs);

  useEffect(() => {
    if (item) {
      setConns(item.connections);
      setRate(String(item.rate_limit_bps || 0));
    }
  }, [item?.id]);

  if (!idParam) return <div className="grid h-screen place-items-center">No download ID provided</div>;
  if (!item) return <div className="grid h-screen place-items-center">Loading...</div>;

  const progress = overallPercent(item.downloaded, item.total_size, item.status);
  const speed = speeds.get(item.id);
  const variant = statusColor(item.status);
  const badge = variant === "attention" ? "warning" : variant === "default" ? "default" : variant;

  const applyRuntime = async () => {
    if (conns != null) await tauriClient.setDownloadConnections(item.id, conns);
    await tauriClient.setDownloadRateLimit(item.id, Number(rate) || 0);
  };

  const refresh = async () => {
    if (!newUrl.trim()) return;
    await tauriClient.refreshDownloadUrl(item.id, newUrl.trim());
    setNewUrl("");
  };

  return (
    <div className="flex h-full flex-col gap-2 overflow-auto p-3">
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1 truncate font-semibold">{item.file_name}</div>
        <Badge variant={badge}>{statusString(item.status)}</Badge>
      </div>
      <Progress value={progress} />
      <div className="flex gap-3 text-[12px] text-muted-foreground">
        <span>{formatBytes(item.downloaded)} / {item.total_size ? formatBytes(item.total_size) : "—"}</span>
        <span>{speed?.display ?? "—"}</span>
      </div>
      <div className="flex gap-1">
        {controls.showPause && <Button size="sm" disabled={controls.busy} onClick={handlePause}>{t("toolbar.stop")}</Button>}
        {controls.showResume && <Button size="sm" disabled={controls.busy} onClick={handleResume}>{t("toolbar.resume")}</Button>}
        {controls.showOpen && <Button size="sm" onClick={async () => { if (await handleOpenFile()) getCurrentWebviewWindow().close(); }}>{t("downloadRow.open")}</Button>}
        <Button size="sm" onClick={async () => { if (await handleOpenFolder()) getCurrentWebviewWindow().close(); }}>{t("downloadRow.openFolder")}</Button>
        <Button size="sm" onClick={handleCopyUrl}>{urlCopied ? "OK" : t("downloadRow.copyUrl")}</Button>
      </div>
      <div className="grid grid-cols-2 gap-2">
        <div>
          <Label>{t("properties.connections")}</Label>
          <Select value={String(conns ?? item.connections)} onChange={(e) => setConns(Number(e.target.value))}>
            {[1, 4, 8, 16, 32, 64].map((n) => <option key={n} value={n}>{n}</option>)}
          </Select>
        </div>
        <div>
          <Label>{t("properties.speedLimit")}</Label>
          <Select value={rate} onChange={(e) => setRate(e.target.value)}>
            <option value="0">{formatRateLimit(0)}</option>
            <option value={String(256 * 1024)}>256 KB/s</option>
            <option value={String(1024 * 1024)}>1 MB/s</option>
            <option value={String(5 * 1024 * 1024)}>5 MB/s</option>
          </Select>
        </div>
      </div>
      <Button size="sm" onClick={applyRuntime}>{t("properties.apply")}</Button>
      <Label>{t("properties.refreshUrl")}</Label>
      <div className="flex gap-1">
        <Input value={newUrl} onChange={(e) => setNewUrl(e.target.value)} placeholder={t("properties.newUrl")} />
        <Button size="sm" onClick={refresh}>{t("properties.apply")}</Button>
      </div>
      <p className="text-[11px] text-muted-foreground">{t("properties.authHidden")}</p>
      <ProgressMap parts={item.parts} status={item.status} />
    </div>
  );
}
