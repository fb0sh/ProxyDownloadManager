import { useState, useEffect } from "react";
import { Button, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { tauriClient } from "../../tauriClient";
import { t } from "../../i18n";

interface LogDialogProps {
  onClose: () => void;
}

export default function LogDialog({ onClose }: LogDialogProps) {
  console.debug('[ProxyDM FE] LogDialog mount');
  const [logs, setLogs] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  const loadLogs = async () => {
    setLoading(true);
    try {
      const data = await tauriClient.readLogs(50);
      setLogs(data);
    } catch { setLogs(["[ERROR] Failed to read logs"]); }
    setLoading(false);
  };

  useEffect(() => { loadLogs(); }, []);

  return (
    <Dialog title={t("log.title")} onClose={onClose} width="xlarge">
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
            {logs.length} {t("log.entries")}
          </Text>
          <Button size="small" onClick={loadLogs} disabled={loading}>
            {loading ? t("log.loading") : t("log.refresh")}
          </Button>
        </div>

        {/* Log area */}
        <div style={{
          padding: 16,
          maxHeight: 520,
          overflowY: "auto",
          display: "flex",
          flexDirection: "column",
          gap: 8,
        }}>
          {logs.length === 0 ? (
            <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>
              {t("log.noEntries")}
            </Text>
          ) : (
            logs.map((line, i) => {
              let borderColor = "var(--borderColor-default, #d0d7de)";
              let textColor = "var(--fgColor-default, #1f2328)";
              if (line.includes("[ERROR]")) {
                borderColor = "var(--borderColor-danger-emphasis, #cf222e)";
                textColor = "var(--fgColor-danger, #cf222e)";
              } else if (line.includes("[WARN]")) {
                borderColor = "var(--borderColor-attention-emphasis, #bf8700)";
                textColor = "var(--fgColor-attention, #9a6700)";
              } else if (line.includes("[INFO]")) {
                textColor = "var(--fgColor-accent, #0969da)";
              }
              return (
                <div key={i} style={{
                  border: `1px solid ${borderColor}`,
                  borderRadius: 6,
                  background: "var(--bgColor-muted, #f6f8fa)",
                  padding: "8px 12px",
                }}>
                  <span style={{
                    fontFamily: "ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, monospace",
                    fontSize: 11,
                    lineHeight: 1.6,
                    whiteSpace: "pre-wrap",
                    overflowWrap: "break-word",
                    color: textColor,
                  }}>
                    {line}
                  </span>
                </div>
              );
            })
          )}
        </div>
      </div>
    </Dialog>
  );
}
