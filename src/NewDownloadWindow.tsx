import { useState, useCallback, useEffect } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import { useStartDownload, useSettings } from "./query/downloadQueries";
import { setLanguage, t } from "./i18n";
import { extractFilename } from "./utils/download";
import { formatBytes } from "./utils/format";
import { tauriClient } from "./tauriClient";
import type { PendingDownloadRequest, ProbeInfo } from "./types";
import { Button } from "./components/ui/button";
import { Input } from "./components/ui/input";
import { Label } from "./components/ui/label";
import { Select } from "./components/ui/select";

export default function NewDownloadWindow() {
  const { settings: loadedSettings } = useSettings();
  const proxies = loadedSettings?.proxies ?? {};
  const startDownload = useStartDownload();
  const [url, setUrl] = useState("");
  const [filename, setFilename] = useState("");
  const [autoFilled, setAutoFilled] = useState(false);
  const [proxyName, setProxyName] = useState(loadedSettings?.default_proxy ?? "");
  const [connections, setConnections] = useState(0);
  const [savePath, setSavePath] = useState(loadedSettings?.download_dir ?? "");
  const [headers, setHeaders] = useState<Record<string, string>>({});
  const [probe, setProbe] = useState<ProbeInfo | null>(null);
  const [probing, setProbing] = useState(false);

  useEffect(() => {
    if (loadedSettings) {
      setLanguage(loadedSettings.language || "en");
      setProxyName(loadedSettings.default_proxy);
      setConnections(loadedSettings.max_connections);
      setSavePath(loadedSettings.download_dir);
    }
  }, [loadedSettings]);

  const applyRequest = (req: PendingDownloadRequest | string) => {
    if (typeof req === "string") {
      setUrl(req);
      const fn = extractFilename(req);
      if (fn) { setFilename(fn); setAutoFilled(true); }
      return;
    }
    const u = req.final_url || req.url;
    setUrl(u);
    if (req.filename) { setFilename(req.filename); setAutoFilled(true); }
    else {
      const fn = extractFilename(u);
      if (fn) { setFilename(fn); setAutoFilled(true); }
    }
    if (req.connections) setConnections(req.connections);
    const nextHeaders = { ...(req.headers || {}) };
    if (req.cookies && !nextHeaders.Cookie) nextHeaders.Cookie = req.cookies;
    if (req.referrer && !nextHeaders.Referer) nextHeaders.Referer = req.referrer;
    if (req.user_agent && !nextHeaders["User-Agent"]) nextHeaders["User-Agent"] = req.user_agent;
    setHeaders(nextHeaders);
  };

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const initial = params.get("url") ?? "";
    if (initial) applyRequest(initial);

    let cancelled = false;
    let unlistenFn: (() => void) | null = null;
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const unlisten = await listen<PendingDownloadRequest | string>("new-download-request", (event) => {
        if (!cancelled) applyRequest(event.payload);
      });
      const unlistenOld = await listen<string>("new-download-url", (event) => {
        if (!cancelled) applyRequest(event.payload);
      });
      if (cancelled) { unlisten(); unlistenOld(); }
      else unlistenFn = () => { unlisten(); unlistenOld(); };
    })();

    return () => {
      cancelled = true;
      if (unlistenFn) unlistenFn();
    };
  }, []);

  useEffect(() => {
    if (!url.startsWith("http")) { setProbe(null); return; }
    const handle = window.setTimeout(async () => {
      setProbing(true);
      try {
        const info = await tauriClient.probeUrl(url, headers, proxyName);
        setProbe(info);
        if (!filename || autoFilled) {
          setFilename(info.file_name);
          setAutoFilled(true);
        }
        if (connections === 0) setConnections(info.suggested_connections);
      } catch {
        setProbe(null);
      } finally {
        setProbing(false);
      }
    }, 400);
    return () => window.clearTimeout(handle);
  }, [url, proxyName, headers]);

  const handleUrlChange = useCallback((value: string) => {
    setUrl(value);
    const fn = extractFilename(value);
    if (fn) { setFilename(fn); setAutoFilled(true); }
  }, []);

  const browse = async () => {
    const dir = await open({ directory: true, multiple: false, title: t("newDownload.saveTo") });
    if (dir) setSavePath(dir as string);
  };

  const submit = async (paused: boolean) => {
    if (!url) return;
    try {
      const id = await startDownload.mutateAsync({
        url, filename, proxyName, connections, savePath, headers, startPaused: paused,
      });
      const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      const existing = await WebviewWindow.getByLabel("download-details");
      const base = window.location.origin + window.location.pathname.replace(/\/+$/, "");
      if (existing) {
        try { await existing.emit("details-id", id); } catch { /* closed */ }
        await existing.show().catch(() => {});
        await existing.setFocus().catch(() => {});
      } else {
        const win = new WebviewWindow("download-details", {
          url: `${base}?view=download-details&id=${id}`,
          width: 460,
          height: 520,
          title: t("properties.title"),
        });
        win.once("tauri://created", async () => {
          await win.show().catch(() => {});
          await win.setFocus().catch(() => {});
        });
      }
      getCurrentWebviewWindow().close();
    } catch (err) {
      alert(t("downloadError.failed") + ": " + (err instanceof Error ? err.message : String(err)));
    }
  };

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-3">
      <div>
        <Label>{t("newDownload.url")}</Label>
        <Input value={url} onChange={(e) => handleUrlChange(e.target.value)} placeholder="https://example.com/file.zip" />
      </div>
      <div>
        <Label>{t("newDownload.filename")}</Label>
        <Input value={filename} onChange={(e) => { setFilename(e.target.value); setAutoFilled(false); }} placeholder={t("newDownload.autoDetect")} />
      </div>
      <div>
        <Label>{t("newDownload.saveTo")}</Label>
        <div className="flex gap-1">
          <Input value={savePath} onChange={(e) => setSavePath(e.target.value)} />
          <Button onClick={browse}>{t("newDownload.browse")}</Button>
        </div>
      </div>
      <div className="grid grid-cols-2 gap-2">
        <div>
          <Label>{t("newDownload.connections")}</Label>
          <Select value={String(connections)} onChange={(e) => setConnections(Number(e.target.value))}>
            <option value="0">Auto</option>
            {[1, 4, 8, 16, 32, 64].map((n) => <option key={n} value={n}>{n}</option>)}
          </Select>
        </div>
        <div>
          <Label>{t("newDownload.proxy")}</Label>
          <Select value={proxyName} onChange={(e) => setProxyName(e.target.value)}>
            <option value="">{t("newDownload.noProxy")}</option>
            {Object.keys(proxies).map((name) => <option key={name} value={name}>{name}</option>)}
          </Select>
        </div>
      </div>
      <div className="rounded-md border border-border bg-muted p-2 text-[12px]">
        {probing && <div>{t("newDownload.probing")}</div>}
        {probe && (
          <div className="grid grid-cols-2 gap-x-3 gap-y-1">
            <span className="text-muted-foreground">{t("newDownload.size")}</span><span>{probe.file_size ? formatBytes(probe.file_size) : "—"}</span>
            <span className="text-muted-foreground">{t("newDownload.type")}</span><span>{probe.content_type || "—"}</span>
            <span className="text-muted-foreground">{t("newDownload.range")}</span><span>{probe.supports_range ? t("properties.yes") : t("properties.no")}</span>
            <span className="text-muted-foreground">{t("newDownload.suggested")}</span><span>{probe.suggested_connections}</span>
            <span className="text-muted-foreground">{t("newDownload.finalUrl")}</span><span className="truncate">{probe.final_url || url}</span>
          </div>
        )}
        {Object.keys(headers).length > 0 && (
          <p className="mt-1 text-muted-foreground">{t("properties.authHidden")}</p>
        )}
      </div>
      <div className="mt-auto flex justify-end gap-2">
        <Button onClick={() => submit(true)} disabled={!url || startDownload.isPending}>{t("newDownload.later")}</Button>
        <Button variant="default" onClick={() => submit(false)} disabled={!url || startDownload.isPending}>
          {startDownload.isPending ? t("newDownload.starting") : t("newDownload.download")}
        </Button>
      </div>
    </div>
  );
}
