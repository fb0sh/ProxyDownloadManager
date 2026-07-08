import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";
import { fileURLToPath } from "url";

const __dirname = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  plugins: [react()],
  root: __dirname,
  base: "./",
  build: {
    outDir: resolve(__dirname, "..", "dist-present"),
    emptyOutDir: true,
  },
});
