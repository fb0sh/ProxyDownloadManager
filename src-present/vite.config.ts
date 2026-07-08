import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";
import { fileURLToPath } from "url";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const rootDir = resolve(__dirname, "..");

export default defineConfig({
  plugins: [react()],
  root: __dirname,
  base: "./",

  resolve: {
    alias: {
      "@tauri-apps/api/core": resolve(__dirname, "tauri-mocks.ts"),
      "@tauri-apps/api/event": resolve(__dirname, "tauri-mocks.ts"),
      "@tauri-apps/api/webviewWindow": resolve(__dirname, "tauri-mocks.ts"),
      "@tauri-apps/api/window": resolve(__dirname, "tauri-mocks.ts"),
      "@tauri-apps/plugin-notification": resolve(__dirname, "tauri-mocks.ts"),
      "@tauri-apps/plugin-clipboard-manager": resolve(__dirname, "tauri-mocks.ts"),
      "@tauri-apps/plugin-autostart": resolve(__dirname, "tauri-mocks.ts"),
      "@tauri-apps/plugin-dialog": resolve(__dirname, "tauri-mocks.ts"),
      "@tauri-apps/plugin-opener": resolve(__dirname, "tauri-mocks.ts"),
    },
  },

  server: {
    fs: {
      allow: [rootDir],
    },
  },

  build: {
    outDir: resolve(rootDir, "dist-present"),
    emptyOutDir: true,
  },
});
