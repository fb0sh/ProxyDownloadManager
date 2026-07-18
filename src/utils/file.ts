import { invoke } from "@tauri-apps/api/core";

export async function openFile(path: string): Promise<void> {
  try {
    await invoke("open_file", { path });
  } catch (e) {
    console.error("[ProxyDM] open file error:", e);
  }
}

export async function openFolder(path: string): Promise<void> {
  try {
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(path);
  } catch {
    try {
      await invoke("open_file", { path: path.replace(/[/\\][^/\\]*$/, "") || "." });
    } catch (e) {
      console.error("[ProxyDM] open folder error:", e);
    }
  }
}
