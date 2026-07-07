import { useState, useEffect } from "react";
import { Button, TextInput, FormControl, Select, Checkbox, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { useSettings } from "../../query/downloadQueries";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { t, setLanguage } from "../../i18n";
import type { Settings } from "../../types";

const THREAD_OPTIONS = [0, 4, 8, 16, 32, 64];
const THREAD_LABELS: Record<number, string> = { 0: "Auto", 4: "4", 8: "8", 16: "16", 32: "32", 64: "64" };
const RETRY_OPTIONS = [3, 5, 10, 20, 50];

interface SettingsDialogProps {
  onClose: () => void;
}

interface ProxyForm {
  name: string;
  protocol: "http" | "socks5";
  host: string;
  port: number;
}

function emptyProxy(): ProxyForm {
  return { name: "", protocol: "socks5", host: "127.0.0.1", port: 1080 };
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

const fieldRow: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 8,
};

const fieldLabel: React.CSSProperties = {
  flexShrink: 0,
  fontSize: 13,
  fontWeight: 600,
  color: "var(--fgColor-default, #1f2328)",
};

const fieldControl: React.CSSProperties = {
  flex: 1,
};

export default function SettingsDialog({ onClose }: SettingsDialogProps) {
  const { settings: initialSettings, saveSettings } = useSettings();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [newProxy, setNewProxy] = useState<ProxyForm>(emptyProxy);
  const [showProxyForm, setShowProxyForm] = useState(false);
  const [editingProxy, setEditingProxy] = useState<string | null>(null); // null = add mode, name = edit mode
  const [testResults, setTestResults] = useState<Record<string, {ok: boolean; latency_ms: number; error?: string} | null>>({});

  useEffect(() => {
    if (initialSettings) setSettings(initialSettings);
  }, [initialSettings]);

  if (!settings) return null;

  const handleTestProxy = async (name: string) => {
    setTestResults(prev => ({ ...prev, [name]: null })); // null = testing
    try {
      const result = await invoke<{ok: boolean; latency_ms: number; status?: number; error?: string}>("test_proxy", { proxyName: name });
      setTestResults(prev => ({ ...prev, [name]: result }));
    } catch (e) {
      setTestResults(prev => ({ ...prev, [name]: { ok: false, latency_ms: 0, error: String(e) } }));
    }
  };

  const handleSave = async () => {
    if (settings) {
      setLanguage(settings.language);
      await saveSettings(settings);
      onClose();
    }
  };

  const browseFolder = async (field: "download_dir" | "home_dir") => {
    try {
      const dir = await open({ directory: true, multiple: false, title: "Select Folder" });
      if (dir) setSettings({ ...settings, [field]: dir as string });
    } catch (e) {
      console.error("Browse failed:", e);
    }
  };

  const saveProxy = () => {
    if (!newProxy.name.trim()) return;
    if (editingProxy) {
      // Editing existing proxy — remove old name, add new one
      const { [editingProxy]: _, ...rest } = settings.proxies;
      const newProxies = { ...rest, [newProxy.name]: { protocol: newProxy.protocol, host: newProxy.host, port: newProxy.port } };
      const newDefault = settings.default_proxy === editingProxy ? newProxy.name : settings.default_proxy;
      setSettings({ ...settings, proxies: newProxies, default_proxy: newDefault });
    } else {
      // Adding new proxy
      setSettings({ ...settings, proxies: { ...settings.proxies, [newProxy.name]: { protocol: newProxy.protocol, host: newProxy.host, port: newProxy.port } } });
    }
    setNewProxy(emptyProxy()); setShowProxyForm(false); setEditingProxy(null);
  };

  const startEditProxy = (name: string) => {
    const p = settings.proxies[name];
    if (!p) return;
    setNewProxy({ name, protocol: p.protocol, host: p.host, port: p.port });
    setEditingProxy(name);
    setShowProxyForm(true);
  };

  const removeProxy = (name: string) => {
    const { [name]: _, ...rest } = settings.proxies;
    const newDefault = settings.default_proxy === name ? "" : settings.default_proxy;
    setSettings({ ...settings, proxies: rest, default_proxy: newDefault });
  };

  return (
    <Dialog title={t("settings.title")} onClose={onClose} width="960px">
      <div style={{ display: "flex", flexDirection: "column", gap: 16, padding: 16, overflowY: "auto" }}>
        {/* HBox: Download + VBox(Storage, Startup, Language) */}
        <div style={{ display: "flex", gap: 16 }}>
          {/* Download card — left */}
          <div style={{ ...sectionCard, flex: 1 }}>
            <div style={sectionHeader}>{t("settings.download")}</div>
            <div style={sectionBody}>
              <FormControl>
                <FormControl.Label>{t("settings.downloadDir")}</FormControl.Label>
                <div style={{ display: "flex", gap: 8 }}>
                  <TextInput value={settings.download_dir} onChange={(e) => setSettings({ ...settings, download_dir: e.target.value })} block />
                  <Button size="small" onClick={() => browseFolder("download_dir")}>{t("settings.browse")}</Button>
                </div>
              </FormControl>
              <div style={fieldRow}>
                <span style={fieldLabel}>{t("settings.maxThreads")}</span>
                <div style={{ ...fieldControl, display: "flex", gap: 12, alignItems: "center" }}>
                  <div style={{ flex: 1 }}>
                    <Select value={String(settings.max_connections)} onChange={(e) => setSettings({ ...settings, max_connections: Number(e.target.value) })}>
                      {THREAD_OPTIONS.map((n) => (<Select.Option key={n} value={String(n)}>{THREAD_LABELS[n]}</Select.Option>))}
                    </Select>
                  </div>
                  <span style={{ fontSize: 13, fontWeight: 600, color: "var(--fgColor-muted, #656d76)" }}>{t("settings.retries")}</span>
                  <div style={{ flex: 1 }}>
                    <Select value={String(settings.max_retries)} onChange={(e) => setSettings({ ...settings, max_retries: Number(e.target.value) })}>
                      {RETRY_OPTIONS.map((n) => (<Select.Option key={n} value={String(n)}>{n}</Select.Option>))}
                    </Select>
                  </div>
                </div>
              </div>
              <FormControl>
                <FormControl.Label>{t("settings.userAgent")}</FormControl.Label>
                <TextInput value={settings.user_agent} onChange={(e) => setSettings({ ...settings, user_agent: e.target.value })} block />
              </FormControl>
            </div>
          </div>

          {/* VBox: Storage, Startup, Language — right */}
          <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 16 }}>
            <div style={sectionCard}>
              <div style={sectionHeader}>{t("settings.storage")}</div>
              <div style={sectionBody}>
                <FormControl>
                  <FormControl.Label>{t("settings.homeDir")}</FormControl.Label>
                  <div style={{ display: "flex", gap: 8 }}>
                    <TextInput value={settings.home_dir} onChange={(e) => setSettings({ ...settings, home_dir: e.target.value })} block />
                    <Button size="small" onClick={() => browseFolder("home_dir")}>{t("settings.browse")}</Button>
                  </div>
                </FormControl>
              </div>
            </div>
            <div style={sectionCard}>
              <div style={sectionHeader}>{t("settings.startup")}</div>
              <div style={sectionBody}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <Checkbox checked={settings.launch_at_startup} onChange={() => setSettings({ ...settings, launch_at_startup: !settings.launch_at_startup })} />
                  <Text size="small">{t("settings.launchStartup")}</Text>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <Checkbox checked={settings.silent_startup} disabled={!settings.launch_at_startup} onChange={() => setSettings({ ...settings, silent_startup: !settings.silent_startup })} />
                  <Text size="small">{t("settings.silentStartup")}</Text>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <Checkbox checked={settings.danger_accept_invalid_certs} onChange={() => setSettings({ ...settings, danger_accept_invalid_certs: !settings.danger_accept_invalid_certs })} />
                  <Text size="small">{t("settings.tlsSkip")}</Text>
                </div>
              </div>
            </div>
            <div style={sectionCard}>
              <div style={sectionHeader}>{t("settings.language")}</div>
              <div style={sectionBody}>
                <Select value={settings.language || "en"} onChange={(e) => setSettings({ ...settings, language: e.target.value })}>
                  <Select.Option value="en">{t("settings.english")}</Select.Option>
                  <Select.Option value="zh">{t("settings.chinese")}</Select.Option>
                </Select>
              </div>
            </div>
          </div>
        </div>

        {/* Proxy card — full width */}
        <div style={sectionCard}>
          <div style={sectionHeader}>{t("settings.proxy")}</div>
          <div style={sectionBody}>
            <FormControl>
              <FormControl.Label>{t("settings.defaultProxy")}</FormControl.Label>
              <Select value={settings.default_proxy} onChange={(e) => setSettings({ ...settings, default_proxy: e.target.value })}>
                <Select.Option value="">{t("settings.none")}</Select.Option>
                {Object.keys(settings.proxies).map((name) => (<Select.Option key={name} value={name}>{name}</Select.Option>))}
              </Select>
            </FormControl>
            <div style={{ border: "1px solid var(--borderColor-muted, #d8dee4)", borderRadius: 6, overflow: "hidden" }}>
              <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
                <thead>
                  <tr style={{ borderBottom: "2px solid var(--borderColor-muted, #d8dee4)", background: "var(--bgColor-subtle, #f6f8fa)" }}>
                    <th style={{ textAlign: "left", padding: "6px 12px", fontWeight: 600, fontSize: 12, color: "var(--fgColor-muted, #656d76)" }}>{t("settings.name")}</th>
                    <th style={{ textAlign: "left", padding: "6px 12px", fontWeight: 600, fontSize: 12, color: "var(--fgColor-muted, #656d76)" }}>{t("settings.type")}</th>
                    <th style={{ textAlign: "left", padding: "6px 12px", fontWeight: 600, fontSize: 12, color: "var(--fgColor-muted, #656d76)" }}>{t("settings.hostPort")}</th>
                    <th style={{ width: 200 }} />
                  </tr>
                </thead>
                <tbody>
                  {Object.entries(settings.proxies).map(([name, proxy]) => {
                    const tr = testResults[name];
                    return (
                    <tr key={name} style={{ borderBottom: "1px solid var(--borderColor-muted, #d8dee4)" }}>
                      <td style={{ padding: "8px 12px", fontWeight: 600, fontSize: 13 }}>{name}</td>
                      <td style={{ padding: "8px 12px", fontSize: 13, color: "var(--fgColor-muted, #656d76)" }}>{proxy.protocol.toUpperCase()}</td>
                      <td style={{ padding: "8px 12px", fontSize: 13, fontFamily: "ui-monospace, SFMono-Regular, monospace" }}>{proxy.host}:{proxy.port}</td>
                      <td style={{ padding: "6px 8px", display: "flex", gap: 6, alignItems: "center" }}>
                        <Button size="small" onClick={() => startEditProxy(name)}>{t("settings.edit")}</Button>
                        <Button size="small" onClick={() => handleTestProxy(name)} disabled={tr === null}>
                          {tr === null ? "..." : t("settings.test")}
                        </Button>
                        <Button size="small" onClick={() => removeProxy(name)}>{t("settings.remove")}</Button>
                        {tr && tr !== null && (
                          <span style={{
                            fontSize: 11,
                            color: tr.ok ? "var(--fgColor-success, #1a7f37)" : "var(--fgColor-danger, #cf222e)",
                            whiteSpace: "nowrap",
                          }}>
                            {tr.ok ? `${tr.latency_ms}ms` : (tr.error ? tr.error.slice(0, 30) : "FAIL")}
                          </span>
                        )}
                      </td>
                    </tr>
                    );
                  })}
                  {Object.keys(settings.proxies).length === 0 && (
                    <tr><td colSpan={4} style={{ padding: 16, textAlign: "center", color: "var(--fgColor-muted, #656d76)", fontSize: 13 }}>{t("settings.noProxy")}</td></tr>
                  )}
                </tbody>
              </table>
            </div>
            {showProxyForm ? (
              <div style={{ display: "flex", flexDirection: "column", gap: 8, padding: 12, border: "1px solid var(--borderColor-default, #d0d7de)", borderRadius: 6, background: "var(--bgColor-subtle, #f6f8fa)" }}>
                <div style={{ display: "flex", gap: 8, alignItems: "flex-start" }}>
                  <div style={{ flex: "0 0 130px" }}>
                    <FormControl><FormControl.Label>{t("settings.name")}</FormControl.Label><TextInput value={newProxy.name} onChange={(e) => setNewProxy({ ...newProxy, name: e.target.value })} placeholder="my-proxy" block /></FormControl>
                  </div>
                  <div style={{ flex: "0 0 110px" }}>
                    <FormControl><FormControl.Label>{t("settings.type")}</FormControl.Label><Select value={newProxy.protocol} onChange={(e) => setNewProxy({ ...newProxy, protocol: e.target.value as "http" | "socks5" })}><Select.Option value="socks5">SOCKS5</Select.Option><Select.Option value="http">HTTP</Select.Option></Select></FormControl>
                  </div>
                  <div style={{ flex: 1 }}>
                    <FormControl><FormControl.Label>Host</FormControl.Label><TextInput value={newProxy.host} onChange={(e) => setNewProxy({ ...newProxy, host: e.target.value })} placeholder="127.0.0.1" block /></FormControl>
                  </div>
                  <div style={{ flex: "0 0 90px" }}>
                    <FormControl><FormControl.Label>Port</FormControl.Label><TextInput type="number" value={String(newProxy.port)} onChange={(e) => setNewProxy({ ...newProxy, port: Number(e.target.value) })} min={1} max={65535} block /></FormControl>
                  </div>
                </div>
                <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
                  <Button size="small" onClick={() => { setShowProxyForm(false); setNewProxy(emptyProxy()); setEditingProxy(null); }}>{t("settings.cancel")}</Button>
                  <Button size="small" variant="primary" onClick={saveProxy} disabled={!newProxy.name.trim()}>
                    {editingProxy ? t("settings.updateProxy") : t("settings.addProxy")}
                  </Button>
                </div>
              </div>
            ) : (
              <Button onClick={() => setShowProxyForm(true)}>{t("settings.addProxy")}</Button>
            )}
          </div>
        </div>

        {/* Actions */}
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, paddingTop: 8 }}>
          <Button onClick={onClose}>{t("settings.cancel")}</Button>
          <Button variant="primary" onClick={handleSave}>{t("settings.save")}</Button>
        </div>
      </div>
    </Dialog>
  );
}
