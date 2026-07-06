import { useState, useCallback, useEffect } from "react";
import { TextInput, Button, FormControl, Select } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { useStartDownload } from "../../query/downloadQueries";
import { useSettingsStore } from "../../stores/settingsStore";
import { t } from "../../i18n";

function extractFilename(url: string): string {
  try {
    const u = new URL(url);
    const path = u.pathname;
    const segments = path.split("/").filter(Boolean);
    if (segments.length === 0) return "";
    const last = segments[segments.length - 1];
    if (last.includes(".") && !last.endsWith(".")) return decodeURIComponent(last);
    return "";
  } catch {
    return "";
  }
}

interface NewDownloadDialogProps {
  initialUrl?: string;
  onClose: () => void;
}

const sectionCard: React.CSSProperties = {
  border: "1px solid var(--borderColor-muted, #d8dee4)",
  borderRadius: 6,
  overflow: "hidden",
};

const sectionHeader: React.CSSProperties = {
  padding: "8px 12px",
  fontSize: 12,
  fontWeight: 600,
  color: "var(--fgColor-muted, #656d76)",
  borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
  background: "var(--bgColor-subtle, #f6f8fa)",
  textTransform: "uppercase",
  letterSpacing: "0.05em",
};

const sectionBody: React.CSSProperties = {
  padding: "12px 16px",
  display: "flex",
  flexDirection: "column",
  gap: 12,
};

export default function NewDownloadDialog({ initialUrl = "", onClose }: NewDownloadDialogProps) {
  const settings = useSettingsStore((s) => s.settings);
  const proxies = settings.proxies;
  const [url, setUrl] = useState(initialUrl);
  const [filename, setFilename] = useState("");
  const [autoFilled, setAutoFilled] = useState(false);
  const [proxyName, setProxyName] = useState(settings.default_proxy);
  const [connections, setConnections] = useState(settings.max_connections);
  const [savePath, setSavePath] = useState(settings.download_dir);
  const startDownload = useStartDownload();

  const handleUrlChange = useCallback((value: string) => {
    setUrl(value);
    if (autoFilled) {
      // Keep auto-filling — filename follows URL
      const fn = extractFilename(value);
      if (fn) setFilename(fn);
    } else {
      // User hasn't manually set filename yet — auto-fill once
      const fn = extractFilename(value);
      if (fn) {
        setFilename(fn);
        setAutoFilled(true);
      }
    }
  }, [autoFilled]);

  const handleFilenameChange = useCallback((value: string) => {
    setFilename(value);
    setAutoFilled(false);
  }, []);

  useEffect(() => {
    if (initialUrl && extractFilename(initialUrl)) {
      setFilename(extractFilename(initialUrl));
      setAutoFilled(true);
    }
  }, []);

  const handleSubmit = async () => {
    if (!url) return;

    try {
      await startDownload.mutateAsync({ url, filename, proxyName, connections, savePath });
      onClose();
    } catch (err) {
      console.error("Download failed:", err);
      alert(t("downloadError.failed") + ": " + (err instanceof Error ? err.message : String(err)));
    }
  };

  return (
    <Dialog title={t("newDownload.title")} onClose={onClose} width="large">
      <div style={{ display: "flex", flexDirection: "column", gap: 16, padding: 16 }}>
        <FormControl required>
          <FormControl.Label>{t("newDownload.url")}</FormControl.Label>
          <TextInput
            value={url}
            onChange={(e) => handleUrlChange(e.target.value)}
            placeholder="https://example.com/file.zip"
            block
          />
        </FormControl>

        <div style={sectionCard}>
          <div style={sectionHeader}>{t("newDownload.file")}</div>
          <div style={sectionBody}>
            <FormControl>
              <FormControl.Label>{t("newDownload.filename")}</FormControl.Label>
              <TextInput
                value={filename}
                onChange={(e) => handleFilenameChange(e.target.value)}
                placeholder={t("newDownload.autoDetect")}
                block
              />
            </FormControl>
            <FormControl>
              <FormControl.Label>{t("newDownload.saveTo")}</FormControl.Label>
              <TextInput
                value={savePath}
                onChange={(e) => setSavePath(e.target.value)}
                block
              />
            </FormControl>
          </div>
        </div>

        <div style={sectionCard}>
          <div style={sectionHeader}>{t("newDownload.network")}</div>
          <div style={sectionBody}>
            <FormControl>
              <FormControl.Label>{t("newDownload.proxy")}</FormControl.Label>
              <Select value={proxyName} onChange={(e) => setProxyName(e.target.value)}>
                <Select.Option value="">{t("newDownload.noProxy")}</Select.Option>
                {Object.keys(proxies).map((name) => (
                  <Select.Option key={name} value={name}>{name}</Select.Option>
                ))}
              </Select>
            </FormControl>
            <FormControl>
              <FormControl.Label>{t("newDownload.connections")}</FormControl.Label>
              <TextInput
                type="number"
                value={String(connections)}
                onChange={(e) => setConnections(Number(e.target.value))}
                min={1}
                max={32}
                block
              />
            </FormControl>
          </div>
        </div>

        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, paddingTop: 8 }}>
          <Button onClick={onClose}>{t("newDownload.cancel")}</Button>
          <Button variant="primary" onClick={handleSubmit} disabled={!url || startDownload.isPending}>
            {startDownload.isPending ? t("newDownload.starting") : t("newDownload.download")}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
