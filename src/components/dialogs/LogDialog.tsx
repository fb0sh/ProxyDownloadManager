import { useState, useEffect } from "react";
import { Button, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { invoke } from "@tauri-apps/api/core";

interface LogDialogProps {
  onClose: () => void;
}

export default function LogDialog({ onClose }: LogDialogProps) {
  const [logs, setLogs] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  const loadLogs = async () => {
    setLoading(true);
    try {
      const data = await invoke<string[]>("read_logs", { maxLines: 50 });
      setLogs(data);
    } catch {
      setLogs(["[ERROR] Failed to read logs"]);
    }
    setLoading(false);
  };

  useEffect(() => { loadLogs(); }, []);

  return (
    <Dialog title="Log" onClose={onClose} width="xlarge">
      <div style={{ display: "flex", flexDirection: "column", gap: 0 }}>
        {/* Toolbar */}
        <div style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          padding: "12px 16px",
          borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
        }}>
          <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>
            {logs.length} entries
          </Text>
          <Button size="small" onClick={loadLogs} disabled={loading}>
            {loading ? "Loading..." : "Refresh"}
          </Button>
        </div>

        {/* Log area */}
        <div style={{
          background: "var(--bgColor-inset, #f0f2f5)",
          padding: 16,
          maxHeight: 520,
          overflowY: "auto",
        }}>
          <div style={{
            fontFamily: "ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, monospace",
            fontSize: 11,
            lineHeight: 1.6,
            whiteSpace: "pre-wrap",
            wordBreak: "break-all",
          }}>
            {logs.length === 0 ? (
              <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>
                No log entries.
              </Text>
            ) : (
              logs.map((line, i) => {
                let color = "var(--fgColor-default, #1f2328)";
                let bg = "transparent";
                if (line.includes("[ERROR]")) {
                  color = "var(--fgColor-danger, #cf222e)";
                  bg = "var(--bgColor-danger-muted, #ffebe9)";
                } else if (line.includes("[WARN]")) {
                  color = "var(--fgColor-attention, #9a6700)";
                  bg = "var(--bgColor-attention-muted, #fff8c5)";
                } else if (line.includes("[INFO]")) {
                  color = "var(--fgColor-accent, #0969da)";
                }
                return (
                  <div key={i} style={{
                    padding: "1px 8px",
                    borderRadius: 3,
                    color,
                    background: bg,
                  }}>
                    {line}
                  </div>
                );
              })
            )}
          </div>
        </div>
      </div>
    </Dialog>
  );
}
