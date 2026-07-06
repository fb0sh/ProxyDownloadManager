import { useState, useEffect } from "react";
import { Button, TextInput, FormControl, Select, Checkbox, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { useSettings } from "../../query/downloadQueries";
import { open } from "@tauri-apps/plugin-dialog";
import type { Settings } from "../../types";

const THREAD_OPTIONS = [4, 8, 16, 32, 64];
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

  useEffect(() => {
    if (initialSettings) setSettings(initialSettings);
  }, [initialSettings]);

  if (!settings) return null;

  const handleSave = async () => {
    if (settings) { await saveSettings(settings); onClose(); }
  };

  const browseFolder = async (field: "download_dir" | "home_dir") => {
    try {
      const dir = await open({ directory: true, multiple: false, title: "Select Folder" });
      if (dir) setSettings({ ...settings, [field]: dir as string });
    } catch (e) {
      console.error("Browse failed:", e);
    }
  };

  const addProxy = () => {
    if (!newProxy.name.trim()) return;
    setSettings({ ...settings, proxies: { ...settings.proxies, [newProxy.name]: { protocol: newProxy.protocol, host: newProxy.host, port: newProxy.port } } });
    setNewProxy(emptyProxy()); setShowProxyForm(false);
  };

  const removeProxy = (name: string) => {
    const { [name]: _, ...rest } = settings.proxies;
    const newDefault = settings.default_proxy === name ? "" : settings.default_proxy;
    setSettings({ ...settings, proxies: rest, default_proxy: newDefault });
  };

  return (
    <Dialog title="Settings" onClose={onClose} width="960px">
      <div style={{ display: "flex", flexDirection: "column", gap: 16, padding: 16, overflowY: "auto" }}>
        {/* HBox: Download + VBox(Storage, Startup) */}
        <div style={{ display: "flex", gap: 16 }}>
          {/* Download card — left */}
          <div style={{ ...sectionCard, flex: 1 }}>
            <div style={sectionHeader}>Download</div>
            <div style={sectionBody}>
              <FormControl>
                <FormControl.Label>Download Directory</FormControl.Label>
                <div style={{ display: "flex", gap: 8 }}>
                  <TextInput value={settings.download_dir} onChange={(e) => setSettings({ ...settings, download_dir: e.target.value })} block />
                  <Button size="small" onClick={() => browseFolder("download_dir")}>Browse</Button>
                </div>
              </FormControl>
              <div style={fieldRow}>
                <span style={fieldLabel}>Max Threads</span>
                <div style={{ ...fieldControl, display: "flex", gap: 12, alignItems: "center" }}>
                  <div style={{ flex: 1 }}>
                    <Select value={String(settings.max_connections)} onChange={(e) => setSettings({ ...settings, max_connections: Number(e.target.value) })}>
                      {THREAD_OPTIONS.map((n) => (<Select.Option key={n} value={String(n)}>{n}</Select.Option>))}
                    </Select>
                  </div>
                  <span style={{ fontSize: 13, fontWeight: 600, color: "var(--fgColor-muted, #656d76)" }}>Retries</span>
                  <div style={{ flex: 1 }}>
                    <Select value={String(settings.max_retries)} onChange={(e) => setSettings({ ...settings, max_retries: Number(e.target.value) })} block>
                      {RETRY_OPTIONS.map((n) => (<Select.Option key={n} value={String(n)}>{n}</Select.Option>))}
                    </Select>
                  </div>
                </div>
              </div>
              <FormControl>
                <FormControl.Label>User-Agent</FormControl.Label>
                <TextInput value={settings.user_agent} onChange={(e) => setSettings({ ...settings, user_agent: e.target.value })} block />
              </FormControl>
            </div>
          </div>

          {/* VBox: Storage + Startup — right */}
          <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 16 }}>
            <div style={sectionCard}>
              <div style={sectionHeader}>Storage</div>
              <div style={sectionBody}>
                <FormControl>
                  <FormControl.Label>Home Directory</FormControl.Label>
                  <div style={{ display: "flex", gap: 8 }}>
                    <TextInput value={settings.home_dir} onChange={(e) => setSettings({ ...settings, home_dir: e.target.value })} block />
                    <Button size="small" onClick={() => browseFolder("home_dir")}>Browse</Button>
                  </div>
                </FormControl>
              </div>
            </div>
            <div style={sectionCard}>
              <div style={sectionHeader}>Startup</div>
              <div style={sectionBody}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <Checkbox checked={settings.launch_at_startup} onChange={() => setSettings({ ...settings, launch_at_startup: !settings.launch_at_startup })} />
                  <Text size="small">Launch ProxyDM on sign in</Text>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Proxy card — full width */}
        <div style={sectionCard}>
          <div style={sectionHeader}>Proxy</div>
          <div style={sectionBody}>
            <FormControl>
              <FormControl.Label>Default Proxy</FormControl.Label>
              <Select value={settings.default_proxy} onChange={(e) => setSettings({ ...settings, default_proxy: e.target.value })}>
                <Select.Option value="">None</Select.Option>
                {Object.keys(settings.proxies).map((name) => (<Select.Option key={name} value={name}>{name}</Select.Option>))}
              </Select>
            </FormControl>
            <div style={{ border: "1px solid var(--borderColor-muted, #d8dee4)", borderRadius: 6, overflow: "hidden" }}>
              <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
                <thead>
                  <tr style={{ borderBottom: "2px solid var(--borderColor-muted, #d8dee4)", background: "var(--bgColor-subtle, #f6f8fa)" }}>
                    <th style={{ textAlign: "left", padding: "6px 12px", fontWeight: 600, fontSize: 12, color: "var(--fgColor-muted, #656d76)" }}>Name</th>
                    <th style={{ textAlign: "left", padding: "6px 12px", fontWeight: 600, fontSize: 12, color: "var(--fgColor-muted, #656d76)" }}>Type</th>
                    <th style={{ textAlign: "left", padding: "6px 12px", fontWeight: 600, fontSize: 12, color: "var(--fgColor-muted, #656d76)" }}>Host:Port</th>
                    <th style={{ width: 80 }} />
                  </tr>
                </thead>
                <tbody>
                  {Object.entries(settings.proxies).map(([name, proxy]) => (
                    <tr key={name} style={{ borderBottom: "1px solid var(--borderColor-muted, #d8dee4)" }}>
                      <td style={{ padding: "8px 12px", fontWeight: 600, fontSize: 13 }}>{name}</td>
                      <td style={{ padding: "8px 12px", fontSize: 13, color: "var(--fgColor-muted, #656d76)" }}>{proxy.protocol.toUpperCase()}</td>
                      <td style={{ padding: "8px 12px", fontSize: 13, fontFamily: "ui-monospace, SFMono-Regular, monospace" }}>{proxy.host}:{proxy.port}</td>
                      <td style={{ padding: "6px 12px" }}><Button size="small" onClick={() => removeProxy(name)}>Remove</Button></td>
                    </tr>
                  ))}
                  {Object.keys(settings.proxies).length === 0 && (
                    <tr><td colSpan={4} style={{ padding: 16, textAlign: "center", color: "var(--fgColor-muted, #656d76)", fontSize: 13 }}>No proxies configured</td></tr>
                  )}
                </tbody>
              </table>
            </div>
            {showProxyForm ? (
              <div style={{ display: "flex", flexDirection: "column", gap: 8, padding: 12, border: "1px solid var(--borderColor-default, #d0d7de)", borderRadius: 6, background: "var(--bgColor-subtle, #f6f8fa)" }}>
                <div style={{ display: "flex", gap: 8, alignItems: "flex-start" }}>
                  <div style={{ flex: "0 0 130px" }}>
                    <FormControl><FormControl.Label>Name</FormControl.Label><TextInput value={newProxy.name} onChange={(e) => setNewProxy({ ...newProxy, name: e.target.value })} placeholder="my-proxy" block /></FormControl>
                  </div>
                  <div style={{ flex: "0 0 110px" }}>
                    <FormControl><FormControl.Label>Protocol</FormControl.Label><Select value={newProxy.protocol} onChange={(e) => setNewProxy({ ...newProxy, protocol: e.target.value as "http" | "socks5" })}><Select.Option value="socks5">SOCKS5</Select.Option><Select.Option value="http">HTTP</Select.Option></Select></FormControl>
                  </div>
                  <div style={{ flex: 1 }}>
                    <FormControl><FormControl.Label>Host</FormControl.Label><TextInput value={newProxy.host} onChange={(e) => setNewProxy({ ...newProxy, host: e.target.value })} placeholder="127.0.0.1" block /></FormControl>
                  </div>
                  <div style={{ flex: "0 0 90px" }}>
                    <FormControl><FormControl.Label>Port</FormControl.Label><TextInput type="number" value={String(newProxy.port)} onChange={(e) => setNewProxy({ ...newProxy, port: Number(e.target.value) })} min={1} max={65535} block /></FormControl>
                  </div>
                </div>
                <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
                  <Button size="small" onClick={() => { setShowProxyForm(false); setNewProxy(emptyProxy()); }}>Cancel</Button>
                  <Button size="small" variant="primary" onClick={addProxy} disabled={!newProxy.name.trim()}>Add</Button>
                </div>
              </div>
            ) : (
              <Button onClick={() => setShowProxyForm(true)}>+ Add Proxy</Button>
            )}
          </div>
        </div>

        {/* Actions */}
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, paddingTop: 8 }}>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={handleSave}>Save</Button>
        </div>
      </div>
    </Dialog>
  );
}
