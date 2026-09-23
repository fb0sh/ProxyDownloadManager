import { useState, useEffect } from "react";
import { useSettings } from "../query/downloadQueries";
import { tauriClient } from "../tauriClient";
import { open } from "@tauri-apps/plugin-dialog";
import { setLanguage } from "../i18n";
import type { ProxyProtocol, Settings } from "../types";

export interface ProxyForm {
  name: string;
  protocol: ProxyProtocol;
  host: string;
  port: number;
  username: string;
  password: string;
}

export function emptyProxy(): ProxyForm {
  return { name: "", protocol: "socks5", host: "127.0.0.1", port: 1080, username: "", password: "" };
}

export function useSettingsForm(onClose: () => void) {
  const { settings: initialSettings, saveSettings } = useSettings();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [newProxy, setNewProxy] = useState<ProxyForm>(emptyProxy());
  const [showProxyForm, setShowProxyForm] = useState(false);
  const [editingProxy, setEditingProxy] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, { ok: boolean; latency_ms: number; error?: string } | null>>({});

  useEffect(() => {
    if (initialSettings) setSettings(initialSettings);
  }, [initialSettings]);

  const handleSave = async () => {
    if (settings) {
      setLanguage(settings.language);
      await saveSettings(settings);
      onClose();
    }
  };

  const browseFolder = async (field: "download_dir" | "home_dir") => {
    if (!settings) return;
    try {
      const dir = await open({ directory: true, multiple: false, title: "Select Folder" });
      if (dir) setSettings({ ...settings, [field]: dir as string });
    } catch (e) {
      console.error("Browse failed:", e);
    }
  };

  const handleTestProxy = async (name: string) => {
    setTestResults((prev) => ({ ...prev, [name]: null }));
    try {
      const result = await tauriClient.testProxy(name);
      setTestResults((prev) => ({ ...prev, [name]: result }));
    } catch (e) {
      setTestResults((prev) => ({ ...prev, [name]: { ok: false, latency_ms: 0, error: String(e) } }));
    }
  };

  const saveProxy = () => {
    if (!settings || !newProxy.name.trim()) return;
    const cfg = {
      protocol: newProxy.protocol,
      host: newProxy.host,
      port: newProxy.port,
      username: newProxy.username,
      password: newProxy.password,
    };
    if (editingProxy) {
      const { [editingProxy]: _, ...rest } = settings.proxies;
      const newProxies = { ...rest, [newProxy.name]: cfg };
      const newDefault = settings.default_proxy === editingProxy ? newProxy.name : settings.default_proxy;
      setSettings({ ...settings, proxies: newProxies, default_proxy: newDefault });
    } else {
      setSettings({ ...settings, proxies: { ...settings.proxies, [newProxy.name]: cfg } });
    }
    setNewProxy(emptyProxy());
    setShowProxyForm(false);
    setEditingProxy(null);
  };

  const startEditProxy = (name: string) => {
    if (!settings) return;
    const p = settings.proxies[name];
    if (!p) return;
    setNewProxy({
      name,
      protocol: p.protocol,
      host: p.host,
      port: p.port,
      username: p.username || "",
      password: p.password || "",
    });
    setEditingProxy(name);
    setShowProxyForm(true);
  };

  const removeProxy = (name: string) => {
    if (!settings) return;
    const { [name]: _, ...rest } = settings.proxies;
    const newDefault = settings.default_proxy === name ? "" : settings.default_proxy;
    setSettings({ ...settings, proxies: rest, default_proxy: newDefault });
  };

  return {
    settings,
    setSettings,
    newProxy,
    setNewProxy,
    showProxyForm,
    setShowProxyForm,
    editingProxy,
    testResults,
    handleSave,
    browseFolder,
    handleTestProxy,
    saveProxy,
    startEditProxy,
    removeProxy,
  };
}
