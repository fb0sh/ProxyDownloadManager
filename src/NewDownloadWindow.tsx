import { useState, useCallback, useEffect } from "react";
import { TextInput, Button, FormControl, Select } from "@primer/react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useStartDownload, useSettings } from "./query/downloadQueries";
import { useSettingsStore } from "./stores/settingsStore";
import { setLanguage, t } from "./i18n";

const DOWNLOAD_EXTENSIONS = [
  ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar", ".iso",
  ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
  ".mp3", ".mp4", ".avi", ".mkv", ".mov", ".wmv", ".flv",
  ".exe", ".msi", ".dmg", ".pkg", ".deb", ".rpm",
  ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp",
  ".dll", ".so", ".dylib", ".bin", ".dat",
  ".csv", ".json", ".xml", ".sql", ".db",
  ".apk", ".ipa", ".appimage", ".flatpak", ".snap",
];

function looksLikeDownloadUrl(text: string): boolean {
  try {
    const url = new URL(text);
    const path = url.pathname.toLowerCase();
    return DOWNLOAD_EXTENSIONS.some((ext) => path.endsWith(ext));
  } catch {
    return false;
  }
}

async function readClipboardUrl(): Promise<string | null> {
  try {
    const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
    const text = await readText();
    if (text && (text.startsWith("http://") || text.startsWith("https://") || text.startsWith("ftp://")) && looksLikeDownloadUrl(text)) {
      return text;
    }
  } catch {}
  return null;
}

function extractFilename(url: string): string {
  try {
    const u = new URL(url);
    const path = u.pathname;
    const segments = path.split("/").filter(Boolean);
    if (segments.length === 0) return "";
    const last = segments[segments.length - 1];
    if (last.includes(".") && !last.endsWith(".")) return decodeURIComponent(last);
    return "";
  } catch { return ""; }
}

const sectionCard: React.CSSProperties = {
  border: "1px solid var(--borderColor-muted, #d8dee4)", borderRadius: 6, overflow: "hidden",
};
const sectionHeader: React.CSSProperties = {
  padding: "8px 12px", fontSize: 12, fontWeight: 600, color: "var(--fgColor-muted, #656d76)",
  borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
  background: "var(--bgColor-subtle, #f6f8fa)", textTransform: "uppercase", letterSpacing: "0.05em",
};
const sectionBody: React.CSSProperties = {
  padding: "12px 16px", display: "flex", flexDirection: "column", gap: 12,
};

export default function NewDownloadWindow() {
  const settings = useSettingsStore((s) => s.settings);
  const proxies = settings.proxies;
  const startDownload = useStartDownload();
  const { settings: loadedSettings } = useSettings();
  const setSettings = useSettingsStore((s) => s.setSettings);
  const [url, setUrl] = useState("");
  const [filename, setFilename] = useState("");
  const [autoFilled, setAutoFilled] = useState(false);
  const [proxyName, setProxyName] = useState(settings.default_proxy);
  const [connections, setConnections] = useState(settings.max_connections);
  const [savePath, setSavePath] = useState(settings.download_dir);

  // Sync backend settings
  useEffect(() => {
    if (loadedSettings) {
      setSettings(loadedSettings);
      setLanguage(loadedSettings.language || "en");
      setProxyName(loadedSettings.default_proxy);
      setConnections(loadedSettings.max_connections);
      setSavePath(loadedSettings.download_dir);
    }
  }, [loadedSettings, setSettings]);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const initial = params.get("url") ?? "";
    if (initial) {
      setUrl(initial);
      if (extractFilename(initial)) { setFilename(extractFilename(initial)); setAutoFilled(true); }
    } else {
      readClipboardUrl().then((clipUrl) => {
        if (clipUrl) {
          setUrl(clipUrl);
          if (extractFilename(clipUrl)) { setFilename(extractFilename(clipUrl)); setAutoFilled(true); }
        }
      });
    }
  }, []);

  const handleUrlChange = useCallback((value: string) => {
    setUrl(value);
    if (autoFilled) { const fn = extractFilename(value); if (fn) setFilename(fn); }
    else { const fn = extractFilename(value); if (fn) { setFilename(fn); setAutoFilled(true); } }
  }, [autoFilled]);

  const handleFilenameChange = useCallback((value: string) => {
    setFilename(value); setAutoFilled(false);
  }, []);

  const handleSubmit = async () => {
    if (!url) return;
    try {
      await startDownload.mutateAsync({ url, filename, proxyName, connections, savePath });
      try {
        const { emit } = await import("@tauri-apps/api/event");
        await emit("download-created");
      } catch {}
      getCurrentWebviewWindow().close();
    } catch (err) {
      console.error("Download failed:", err);
      alert(t("downloadError.failed") + ": " + (err instanceof Error ? err.message : String(err)));
    }
  };

// ─── Form view ─────────────────────────────────────────────────────────
  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "auto" }}>
      <div style={{ display: "flex", flexDirection: "column", gap: 10, padding: 12 }}>
        <FormControl required>
          <FormControl.Label>{t("newDownload.url")}</FormControl.Label>
          <TextInput value={url} onChange={(e) => handleUrlChange(e.target.value)}
            placeholder="https://example.com/file.zip" block />
        </FormControl>

        <div style={sectionCard}>
          <div style={sectionHeader}>{t("newDownload.file")}</div>
          <div style={sectionBody}>
            <FormControl>
              <FormControl.Label>{t("newDownload.filename")}</FormControl.Label>
              <TextInput value={filename} onChange={(e) => handleFilenameChange(e.target.value)}
                placeholder={t("newDownload.autoDetect")} block />
            </FormControl>
            <FormControl>
              <FormControl.Label>{t("newDownload.saveTo")}</FormControl.Label>
              <TextInput value={savePath} onChange={(e) => setSavePath(e.target.value)} block />
            </FormControl>
          </div>
        </div>

        <div style={sectionCard}>
          <div style={sectionHeader}>{t("newDownload.network")}</div>
          <div style={{ ...sectionBody, flexDirection: "row", gap: 12 }}>
            <FormControl style={{ flex: 1 }}>
              <FormControl.Label>{t("newDownload.connections")}</FormControl.Label>
              <TextInput type="number" value={String(connections)}
                onChange={(e) => setConnections(Number(e.target.value))} min={1} max={32} block />
            </FormControl>
            <FormControl style={{ flex: 1 }}>
              <FormControl.Label>{t("newDownload.proxy")}</FormControl.Label>
              <Select value={proxyName} onChange={(e) => setProxyName(e.target.value)}>
                <Select.Option value="">{t("newDownload.noProxy")}</Select.Option>
                {Object.keys(proxies).map((name) => (
                  <Select.Option key={name} value={name}>{name}</Select.Option>
                ))}
              </Select>
            </FormControl>
          </div>
        </div>

        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, paddingTop: 8 }}>
          <Button variant="primary" onClick={handleSubmit} disabled={!url || startDownload.isPending}>
            {startDownload.isPending ? t("newDownload.starting") : t("newDownload.download")}
          </Button>
        </div>
      </div>
    </div>
  );
}
