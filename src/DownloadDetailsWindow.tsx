import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { formatBytes, formatRateLimit, statusString } from "./utils/format";
import { useDownloadDetail, useDownloadIdFromUrl } from "./hooks/useDownloadDetail";
import { useDownloadSpeed, computeETA } from "./hooks/useDownloadSpeed";
import { useSettings } from "./query/downloadQueries";
import ProgressMap from "./components/ProgressMap";
import { overallPercent } from "./utils/progressMap";
import { setLanguage, t } from "./i18n";
import { Button } from "./components/ui/button";
import { Progress } from "./components/ui/progress";
import { Select } from "./components/ui/select";
import { Input } from "./components/ui/input";
import { Label } from "./components/ui/label";
import { tauriClient } from "./tauriClient";

export default function DownloadDetailsWindow() {
  const idParam = new URLSearchParams(window.location.search).get("id");
  const urlId = useDownloadIdFromUrl();
  const [liveId, setLiveId] = useState(urlId);
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

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<number>("details-id", (e) => setLiveId(e.payload));
    })();
    return () => { unlisten?.(); };
  }, []);

  const id = liveId ?? urlId;
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

  if (!idParam && liveId == null) return <div className="grid h-screen place-items-center text-[13px]">No download ID provided</div>;
  if (!item) return <div className="grid h-screen place-items-center text-[13px]">Loading...</div>;

  const progress = overallPercent(item.downloaded, item.total_size, item.status);
  const speed = speeds.get(item.id);

  const applyConnections = async (value: number) => {
    setConns(value);
    await tauriClient.setDownloadConnections(item.id, value);
  };
  const applyRate = async (value: string) => {
    setRate(value);
    await tauriClient.setDownloadRateLimit(item.id, Number(value) || 0);
  };
  const refresh = async () => {
    if (!newUrl.trim()) return;
    await tauriClient.refreshDownloadUrl(item.id, newUrl.trim());
    setNewUrl("");
  };

  return (
    <div className="flex h-full flex-col gap-3 overflow-hidden p-3 text-[13px]">
      <div className="min-w-0 truncate text-[14px] font-semibold">{item.file_name}</div>
      <Progress className="h-2.5" value={progress} />
      <div className="grid grid-cols-[88px_1fr] gap-x-3 gap-y-1">
        <span className="text-muted-foreground">{t("properties.status")}</span>
        <span>{statusString(item.status)} · {progress}%</span>
        <span className="text-muted-foreground">{t("properties.size")}</span>
        <span className="tabular">{item.total_size ? `${formatBytes(item.downloaded)} / ${formatBytes(item.total_size)}` : "—"}</span>
        <span className="text-muted-foreground">{t("downloadTable.speed")}</span>
        <span className="tabular">{speed?.display ?? "—"}</span>
        <span className="text-muted-foreground">{t("downloadTable.remain")}</span>
        <span className="tabular">{speed ? computeETA(item, speed.bps) : "—"}</span>
        <span className="text-muted-foreground">{t("properties.proxy")}</span>
        <span>{item.proxy_name || t("properties.none")}</span>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {controls.showPause && <Button disabled={controls.busy} onClick={handlePause}>{t("toolbar.stop")}</Button>}
        {controls.showResume && <Button disabled={controls.busy} onClick={handleResume}>{t("toolbar.resume")}</Button>}
        {controls.showOpen && <Button onClick={async () => { if (await handleOpenFile()) getCurrentWebviewWindow().close(); }}>{t("downloadRow.open")}</Button>}
        <Button onClick={async () => { if (await handleOpenFolder()) getCurrentWebviewWindow().close(); }}>{t("downloadRow.openFolder")}</Button>
        <Button onClick={handleCopyUrl}>{urlCopied ? "OK" : t("downloadRow.copyUrl")}</Button>
      </div>
      <div className="grid grid-cols-2 gap-2">
        <div>
          <Label>{t("properties.connections")}</Label>
          <Select value={String(conns ?? item.connections)} onChange={(e) => applyConnections(Number(e.target.value))}>
            {[1, 4, 8, 16, 32, 64].map((n) => <option key={n} value={n}>{n}</option>)}
          </Select>
        </div>
        <div>
          <Label>{t("properties.speedLimit")}</Label>
          <Select value={rate} onChange={(e) => applyRate(e.target.value)}>
            <option value="0">{formatRateLimit(0)}</option>
            <option value={String(256 * 1024)}>256 KB/s</option>
            <option value={String(1024 * 1024)}>1 MB/s</option>
            <option value={String(5 * 1024 * 1024)}>5 MB/s</option>
          </Select>
        </div>
      </div>
      <div>
        <Label>{t("properties.url")}</Label>
        <div className="truncate text-[12px] text-muted-foreground" title={item.url}>{item.url}</div>
      </div>
      <div>
        <Label>{t("properties.refreshUrl")}</Label>
        <div className="flex gap-1.5">
          <Input value={newUrl} onChange={(e) => setNewUrl(e.target.value)} placeholder={t("properties.newUrl")} />
          <Button onClick={refresh}>{t("properties.apply")}</Button>
        </div>
      </div>
      <div className="min-h-0 flex-1">
        <Label>{t("properties.connections")}</Label>
        <ProgressMap parts={item.parts} connections={conns ?? item.connections} />
      </div>
    </div>
  );
}
