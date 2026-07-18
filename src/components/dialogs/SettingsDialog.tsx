import { Button, TextInput, FormControl, Select, Checkbox, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { t } from "../../i18n";
import { sectionCard, sectionHeader, sectionBody } from "../../utils/styles";
import { useSettingsForm } from "../../hooks/useSettingsForm";
import ProxyTable from "./ProxyTable";

const THREAD_OPTIONS = [0, 4, 8, 16, 32];
const THREAD_LABELS: Record<number, string> = { 0: "Auto", 4: "4", 8: "8", 16: "16", 32: "32", 64: "64" };
const RETRY_OPTIONS = [3, 5, 10, 20, 50];

interface SettingsDialogProps {
  onClose: () => void;
}

const fieldRow: React.CSSProperties = { display: "flex", alignItems: "center", gap: 8 };
const fieldLabel: React.CSSProperties = { flexShrink: 0, fontSize: 13, fontWeight: 600, color: "var(--fgColor-default, #1f2328)" };
const fieldControl: React.CSSProperties = { flex: 1 };

export default function SettingsDialog({ onClose }: SettingsDialogProps) {
  const form = useSettingsForm(onClose);
  const { settings, setSettings } = form;

  if (!settings) return null;

  return (
    <Dialog title={t("settings.title")} onClose={onClose} width="960px">
      <div style={{ display: "flex", flexDirection: "column", gap: 16, padding: 16, overflowY: "auto" }}>
        <div style={{ display: "flex", gap: 16 }}>
          {/* Download card */}
          <div style={{ ...sectionCard, flex: 1 }}>
            <div style={sectionHeader}>{t("settings.download")}</div>
            <div style={sectionBody}>
              <FormControl>
                <FormControl.Label>{t("settings.downloadDir")}</FormControl.Label>
                <div style={{ display: "flex", gap: 8 }}>
                  <TextInput value={settings.download_dir} onChange={(e) => setSettings({ ...settings, download_dir: e.target.value })} block />
                  <Button size="small" onClick={() => form.browseFolder("download_dir")}>{t("settings.browse")}</Button>
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

          {/* Right column: Storage, Startup, Language, Shortcut */}
          <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 16 }}>
            <div style={sectionCard}>
              <div style={sectionHeader}>{t("settings.storage")}</div>
              <div style={sectionBody}>
                <FormControl>
                  <FormControl.Label>{t("settings.homeDir")}</FormControl.Label>
                  <div style={{ display: "flex", gap: 8 }}>
                    <TextInput value={settings.home_dir} onChange={(e) => setSettings({ ...settings, home_dir: e.target.value })} block />
                    <Button size="small" onClick={() => form.browseFolder("home_dir")}>{t("settings.browse")}</Button>
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
              <div style={sectionHeader}>{t("settings.globalShortcut")}</div>
              <div style={sectionBody}>
                <FormControl>
                  <FormControl.Label>{t("settings.shortcutLabel")}</FormControl.Label>
                  <TextInput value={settings.global_shortcut} onChange={(e) => setSettings({ ...settings, global_shortcut: e.target.value })} placeholder="Ctrl+Super+J" monospace block />
                  <FormControl.Caption>{t("settings.shortcutCaption")}</FormControl.Caption>
                </FormControl>
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

        {/* Proxy card */}
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
            <ProxyTable
              settings={settings}
              newProxy={form.newProxy}
              setNewProxy={form.setNewProxy}
              showProxyForm={form.showProxyForm}
              setShowProxyForm={form.setShowProxyForm}
              editingProxy={form.editingProxy}
              testResults={form.testResults}
              onTestProxy={form.handleTestProxy}
              onSaveProxy={form.saveProxy}
              onStartEdit={form.startEditProxy}
              onRemove={form.removeProxy}
            />
          </div>
        </div>

        {/* Actions */}
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, paddingTop: 8 }}>
          <Button onClick={onClose}>{t("settings.cancel")}</Button>
          <Button variant="primary" onClick={form.handleSave}>{t("settings.save")}</Button>
        </div>
      </div>
    </Dialog>
  );
}
