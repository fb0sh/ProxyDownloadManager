import { Button, FormControl, Select, TextInput } from "@primer/react";
import { t } from "../../i18n";
import type { Settings } from "../../types";
import type { ProxyForm } from "../../hooks/useSettingsForm";

interface ProxyTableProps {
  settings: Settings;
  newProxy: ProxyForm;
  setNewProxy: (p: ProxyForm) => void;
  showProxyForm: boolean;
  setShowProxyForm: (show: boolean) => void;
  editingProxy: string | null;
  testResults: Record<string, { ok: boolean; latency_ms: number; error?: string } | null>;
  onTestProxy: (name: string) => void;
  onSaveProxy: () => void;
  onStartEdit: (name: string) => void;
  onRemove: (name: string) => void;
}

export default function ProxyTable({
  settings, newProxy, setNewProxy, showProxyForm, setShowProxyForm,
  editingProxy, testResults, onTestProxy, onSaveProxy, onStartEdit, onRemove,
}: ProxyTableProps) {
  return (
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
                  <Button size="small" onClick={() => onStartEdit(name)}>{t("settings.edit")}</Button>
                  <Button size="small" onClick={() => onTestProxy(name)} disabled={tr === null}>
                    {tr === null ? "..." : t("settings.test")}
                  </Button>
                  <Button size="small" onClick={() => onRemove(name)}>{t("settings.remove")}</Button>
                  {tr && tr !== null && (
                    <span style={{ fontSize: 11, color: tr.ok ? "var(--fgColor-success, #1a7f37)" : "var(--fgColor-danger, #cf222e)", whiteSpace: "nowrap" }}>
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
            <Button size="small" onClick={() => { setShowProxyForm(false); }}>{t("settings.cancel")}</Button>
            <Button size="small" variant="primary" onClick={onSaveProxy} disabled={!newProxy.name.trim()}>
              {editingProxy ? t("settings.updateProxy") : t("settings.addProxy")}
            </Button>
          </div>
        </div>
      ) : (
        <Button onClick={() => setShowProxyForm(true)}>{t("settings.addProxy")}</Button>
      )}
    </div>
  );
}
