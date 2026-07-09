import { describe, it, expect, beforeEach } from "vitest";
import { useSettingsStore } from "../stores/settingsStore";

describe("settingsStore", () => {
  beforeEach(() => {
    // Reset store to defaults
    useSettingsStore.setState({
      settings: {
        download_dir: "",
        max_connections: 8,
        max_retries: 10,
        user_agent: "ProxyDM/0.1.0",
        launch_at_startup: false,
        silent_startup: true,
        proxies: {},
        global_rate_limit: 0,
        default_proxy: "",
        home_dir: "",
        language: "en",
        danger_accept_invalid_certs: true,
    global_shortcut: "Ctrl+Super+J",
      },
    });
  });

  it("has default values", () => {
    const state = useSettingsStore.getState();
    expect(state.settings.max_connections).toBe(8);
    expect(state.settings.max_retries).toBe(10);
  });

  it("setSettings replaces settings", () => {
    useSettingsStore.getState().setSettings({
      download_dir: "/tmp/dl",
      max_connections: 16,
      max_retries: 5,
      user_agent: "test",
      launch_at_startup: true,
      silent_startup: false,
      proxies: {},
      global_rate_limit: 0,
      default_proxy: "",
      home_dir: "",
      language: "en",
      danger_accept_invalid_certs: true,
    global_shortcut: "Ctrl+Super+J",
    });
    const s = useSettingsStore.getState().settings;
    expect(s.max_connections).toBe(16);
    expect(s.max_retries).toBe(5);
    expect(s.download_dir).toBe("/tmp/dl");
    expect(s.silent_startup).toBe(false);
  });

  it("updateProxy adds a proxy", () => {
    useSettingsStore.getState().updateProxy("p1", "socks5", "127.0.0.1", 1080);
    const proxies = useSettingsStore.getState().settings.proxies;
    expect(proxies["p1"]).toBeDefined();
    expect(proxies["p1"].protocol).toBe("socks5");
    expect(proxies["p1"].host).toBe("127.0.0.1");
    expect(proxies["p1"].port).toBe(1080);
  });

  it("updateProxy preserves existing proxies", () => {
    useSettingsStore.getState().updateProxy("p1", "http", "10.0.0.1", 3128);
    useSettingsStore.getState().updateProxy("p2", "socks5", "10.0.0.2", 1080);
    const proxies = useSettingsStore.getState().settings.proxies;
    expect(Object.keys(proxies).length).toBe(2);
  });

  it("removeProxy deletes a proxy", () => {
    useSettingsStore.getState().updateProxy("p1", "http", "10.0.0.1", 3128);
    useSettingsStore.getState().updateProxy("p2", "socks5", "10.0.0.2", 1080);
    useSettingsStore.getState().removeProxy("p1");
    const proxies = useSettingsStore.getState().settings.proxies;
    expect(Object.keys(proxies).length).toBe(1);
    expect(proxies["p1"]).toBeUndefined();
  });

  it("removeProxy does nothing for missing key", () => {
    useSettingsStore.getState().updateProxy("p1", "http", "10.0.0.1", 3128);
    useSettingsStore.getState().removeProxy("nonexistent");
    const proxies = useSettingsStore.getState().settings.proxies;
    expect(Object.keys(proxies).length).toBe(1);
  });

  it("updateProxy replaces existing proxy with same name", () => {
    useSettingsStore.getState().updateProxy("p1", "http", "10.0.0.1", 3128);
    useSettingsStore.getState().updateProxy("p1", "socks5", "10.0.0.2", 1080);
    const p = useSettingsStore.getState().settings.proxies["p1"];
    expect(p.protocol).toBe("socks5");
    expect(p.host).toBe("10.0.0.2");
    expect(p.port).toBe(1080);
  });
});
