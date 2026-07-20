import { tauriClient } from "../tauriClient";

export async function openFile(path: string): Promise<void> {
  try {
    await tauriClient.openFile(path);
  } catch (e) {
    console.error("[ProxyDM] open file error:", e);
  }
}

export async function openFolder(path: string): Promise<void> {
  try {
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(path);
  } catch {
    // plugin-opener unavailable — fall back to open parent directory
    try {
      await tauriClient.openFile(path.replace(/[/\\][^/\\]*$/, "") || ".");
    } catch (e) {
      console.error("[ProxyDM] open folder error:", e);
    }
  }
}
