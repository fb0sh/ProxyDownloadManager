import { t } from "../../i18n";
import type { Settings } from "../../types";
import type { ProxyForm } from "../../hooks/useSettingsForm";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Select } from "../ui/select";
import { Label } from "../ui/label";

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
    <div className="overflow-hidden rounded-md border border-border">
      <table className="w-full text-[13px]">
        <thead className="bg-muted text-left text-[11px] text-muted-foreground">
          <tr>
            <th className="px-3 py-1.5">{t("settings.name")}</th>
            <th className="px-3 py-1.5">{t("settings.type")}</th>
            <th className="px-3 py-1.5">{t("settings.hostPort")}</th>
            <th className="px-3 py-1.5">{t("settings.username")}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {Object.entries(settings.proxies).map(([name, proxy]) => {
            const tr = testResults[name];
            return (
              <tr key={name} className="border-t border-border">
                <td className="px-3 py-1.5 font-medium">{name}</td>
                <td className="px-3 py-1.5 uppercase text-muted-foreground">{proxy.protocol}</td>
                <td className="px-3 py-1.5 font-mono">{proxy.host}:{proxy.port}</td>
                <td className="px-3 py-1.5">{proxy.username ? "••••" : "—"}</td>
                <td className="px-2 py-1">
                  <div className="flex items-center gap-1">
                    <Button size="sm" onClick={() => onStartEdit(name)}>{t("settings.edit")}</Button>
                    <Button size="sm" onClick={() => onTestProxy(name)}>{tr === null ? "…" : t("settings.test")}</Button>
                    <Button size="sm" onClick={() => onRemove(name)}>{t("settings.remove")}</Button>
                    {tr && (
                      <span className={tr.ok ? "text-[11px] text-success" : "text-[11px] text-destructive"}>
                        {tr.ok ? `${tr.latency_ms}ms` : (tr.error ? tr.error.slice(0, 24) : "FAIL")}
                      </span>
                    )}
                  </div>
                </td>
              </tr>
            );
          })}
          {Object.keys(settings.proxies).length === 0 && (
            <tr><td colSpan={5} className="p-4 text-center text-muted-foreground">{t("settings.noProxy")}</td></tr>
          )}
        </tbody>
      </table>
      {showProxyForm ? (
        <div className="grid grid-cols-2 gap-2 border-t border-border bg-muted p-3">
          <div><Label>{t("settings.name")}</Label><Input value={newProxy.name} onChange={(e) => setNewProxy({ ...newProxy, name: e.target.value })} /></div>
          <div>
            <Label>{t("settings.type")}</Label>
            <Select value={newProxy.protocol} onChange={(e) => setNewProxy({ ...newProxy, protocol: e.target.value as ProxyForm["protocol"] })}>
              <option value="socks5">SOCKS5</option>
              <option value="http">HTTP</option>
              <option value="https">HTTPS</option>
            </Select>
          </div>
          <div><Label>Host</Label><Input value={newProxy.host} onChange={(e) => setNewProxy({ ...newProxy, host: e.target.value })} /></div>
          <div><Label>Port</Label><Input type="number" value={String(newProxy.port)} onChange={(e) => setNewProxy({ ...newProxy, port: Number(e.target.value) })} /></div>
          <div><Label>{t("settings.username")}</Label><Input value={newProxy.username} onChange={(e) => setNewProxy({ ...newProxy, username: e.target.value })} /></div>
          <div><Label>{t("settings.password")}</Label><Input type="password" value={newProxy.password} onChange={(e) => setNewProxy({ ...newProxy, password: e.target.value })} /></div>
          <div className="col-span-2 flex justify-end gap-2">
            <Button onClick={() => setShowProxyForm(false)}>{t("settings.cancel")}</Button>
            <Button variant="default" onClick={onSaveProxy}>{editingProxy ? t("settings.updateProxy") : t("settings.addProxy")}</Button>
          </div>
        </div>
      ) : (
        <div className="border-t border-border p-2">
          <Button size="sm" onClick={() => setShowProxyForm(true)}>{t("settings.addProxy")}</Button>
        </div>
      )}
    </div>
  );
}
