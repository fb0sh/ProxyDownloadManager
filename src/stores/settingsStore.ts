import { create } from "zustand";
import type { Settings } from "../types";

interface SettingsStore {
  settings: Settings;
  setSettings: (settings: Settings) => void;
  updateProxy: (name: string, protocol: string, host: string, port: number) => void;
  removeProxy: (name: string) => void;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  settings: {
    download_dir: "",
    max_connections: 0, // 0 = auto
    max_retries: 10,
    user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36 Edg/150.0.0.0",
    launch_at_startup: false,
    silent_startup: true,
    proxies: {},
    global_rate_limit: 0,
    default_proxy: "",
    home_dir: "",
    language: "en",
    danger_accept_invalid_certs: true,
  },
  setSettings: (settings) => set({ settings }),
  updateProxy: (name, protocol, host, port) =>
    set((state) => ({
      settings: {
        ...state.settings,
        proxies: {
          ...state.settings.proxies,
          [name]: { protocol: protocol as "http" | "socks5", host, port },
        },
      },
    })),
  removeProxy: (name) =>
    set((state) => {
      const { [name]: _, ...rest } = state.settings.proxies;
      return { settings: { ...state.settings, proxies: rest } };
    }),
}));
