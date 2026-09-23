import { t } from "../../i18n";
import { useSettingsForm } from "../../hooks/useSettingsForm";
import ProxyTable from "./ProxyTable";
import { AppDialog } from "../ui/dialog";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Select } from "../ui/select";
import { Label } from "../ui/label";
import { Checkbox } from "../ui/checkbox";
import type { FileConflictPolicy } from "../../types";

const THREAD_OPTIONS = [0, 4, 8, 16, 32, 64];
const THREAD_LABELS: Record<number, string> = { 0: "Auto", 4: "4", 8: "8", 16: "16", 32: "32", 64: "64" };
const RETRY_OPTIONS = [3, 5, 10, 20, 50];

interface SettingsDialogProps {
  onClose: () => void;
}

export default function SettingsDialog({ onClose }: SettingsDialogProps) {
  const form = useSettingsForm(onClose);
  const { settings, setSettings } = form;
  if (!settings) return null;

  return (
    <AppDialog title={t("settings.title")} onClose={onClose} width="max-w-4xl">
      <div className="grid max-h-[70vh] grid-cols-2 gap-3 overflow-auto p-3">
        <div className="flex flex-col gap-2">
          <div className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">{t("settings.download")}</div>
          <Label>{t("settings.downloadDir")}</Label>
          <div className="flex gap-1">
            <Input value={settings.download_dir} onChange={(e) => setSettings({ ...settings, download_dir: e.target.value })} />
            <Button size="sm" onClick={() => form.browseFolder("download_dir")}>{t("settings.browse")}</Button>
          </div>
          <div className="grid grid-cols-2 gap-2">
            <div>
              <Label>{t("settings.maxThreads")}</Label>
              <Select value={String(settings.max_connections)} onChange={(e) => setSettings({ ...settings, max_connections: Number(e.target.value) })}>
                {THREAD_OPTIONS.map((n) => <option key={n} value={n}>{THREAD_LABELS[n]}</option>)}
              </Select>
            </div>
            <div>
              <Label>{t("settings.maxRetries")}</Label>
              <Select value={String(settings.max_retries)} onChange={(e) => setSettings({ ...settings, max_retries: Number(e.target.value) })}>
                {RETRY_OPTIONS.map((n) => <option key={n} value={n}>{n}</option>)}
              </Select>
            </div>
          </div>
          <Label>{t("settings.userAgent")}</Label>
          <Input value={settings.user_agent} onChange={(e) => setSettings({ ...settings, user_agent: e.target.value })} />
          <Label>{t("settings.fileConflict")}</Label>
          <Select value={settings.file_conflict || "rename"} onChange={(e) => setSettings({ ...settings, file_conflict: e.target.value as FileConflictPolicy })}>
            <option value="rename">{t("settings.conflictRename")}</option>
            <option value="overwrite">{t("settings.conflictOverwrite")}</option>
            <option value="ask">{t("settings.conflictAsk")}</option>
            <option value="skip">{t("settings.conflictSkip")}</option>
          </Select>
          <Label>{t("settings.globalRate")}</Label>
          <Input type="number" value={String(settings.global_rate_limit)} onChange={(e) => setSettings({ ...settings, global_rate_limit: Number(e.target.value) })} />
        </div>
        <div className="flex flex-col gap-2">
          <div className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">{t("settings.storage")}</div>
          <Label>{t("settings.homeDir")}</Label>
          <div className="flex gap-1">
            <Input value={settings.home_dir} onChange={(e) => setSettings({ ...settings, home_dir: e.target.value })} />
            <Button size="sm" onClick={() => form.browseFolder("home_dir")}>{t("settings.browse")}</Button>
          </div>
          <label className="flex items-center gap-2 text-[13px]"><Checkbox checked={settings.launch_at_startup} onCheckedChange={(v) => setSettings({ ...settings, launch_at_startup: v === true })} /> {t("settings.launchStartup")}</label>
          <label className="flex items-center gap-2 text-[13px]"><Checkbox checked={settings.silent_startup} disabled={!settings.launch_at_startup} onCheckedChange={(v) => setSettings({ ...settings, silent_startup: v === true })} /> {t("settings.silentStartup")}</label>
          <label className="flex items-center gap-2 text-[13px]"><Checkbox checked={settings.danger_accept_invalid_certs} onCheckedChange={(v) => setSettings({ ...settings, danger_accept_invalid_certs: v === true })} /> {t("settings.tlsSkip")}</label>
          <Label>{t("settings.language")}</Label>
          <Select value={settings.language} onChange={(e) => setSettings({ ...settings, language: e.target.value })}>
            <option value="en">{t("settings.english")}</option>
            <option value="zh">{t("settings.chinese")}</option>
          </Select>
          <Label>{t("settings.shortcutLabel")}</Label>
          <Input value={settings.global_shortcut} onChange={(e) => setSettings({ ...settings, global_shortcut: e.target.value })} />
          <p className="text-[11px] text-muted-foreground">{t("settings.shortcutCaption")}</p>
        </div>
        <div className="col-span-2">
          <div className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">{t("settings.proxy")}</div>
          <div className="mb-2 max-w-xs">
            <Label>{t("settings.defaultProxy")}</Label>
            <Select
              value={settings.default_proxy}
              onChange={(e) => setSettings({ ...settings, default_proxy: e.target.value })}
            >
              <option value="">{t("settings.none")}</option>
              {Object.keys(settings.proxies).map((name) => (
                <option key={name} value={name}>{name}</option>
              ))}
            </Select>
          </div>
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
      <div className="flex justify-end gap-2 border-t border-border p-3">
        <Button onClick={onClose}>{t("settings.cancel")}</Button>
        <Button variant="default" onClick={form.handleSave}>{t("settings.save")}</Button>
      </div>
    </AppDialog>
  );
}
