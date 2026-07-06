import { Text, Label, ProgressBar } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { useDownload } from "../../query/downloadQueries";
import { formatBytes } from "../../types";
import { t } from "../../i18n";

interface PropertiesDialogProps {
  id: number;
  onClose: () => void;
}

const sectionStyle: React.CSSProperties = {
  border: "1px solid var(--borderColor-muted, #d8dee4)",
  borderRadius: 6,
  overflow: "hidden",
};

const sectionHeader: React.CSSProperties = {
  padding: "8px 12px",
  fontSize: 12,
  fontWeight: 600,
  color: "var(--fgColor-muted, #656d76)",
  borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
  background: "var(--bgColor-subtle, #f6f8fa)",
  textTransform: "uppercase",
  letterSpacing: "0.05em",
};

const gridRow: React.CSSProperties = {
  display: "flex",
  borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
};

const gridRowLast: React.CSSProperties = { ...gridRow, borderBottom: "none" };

const cellLabel: React.CSSProperties = {
  width: 140,
  flexShrink: 0,
  padding: "8px 12px",
  fontSize: 12,
  fontWeight: 600,
  color: "var(--fgColor-muted, #656d76)",
  background: "var(--bgColor-default, #ffffff)",
  borderRight: "1px solid var(--borderColor-muted, #d8dee4)",
};

const cellValue: React.CSSProperties = {
  flex: 1,
  padding: "8px 12px",
  fontSize: 13,
  color: "var(--fgColor-default, #1f2328)",
  wordBreak: "break-all",
  background: "var(--bgColor-default, #ffffff)",
};

function InfoRow({ label, value, last }: { label: string; value: string; last?: boolean }) {
  return (
    <div style={last ? gridRowLast : gridRow}>
      <div style={cellLabel}>{label}</div>
      <div style={cellValue}>{value || "—"}</div>
    </div>
  );
}

function statusColor(status: string): "success" | "danger" | "attention" | "default" | "accent" {
  switch (status) {
    case "completed": return "success";
    case "failed": return "danger";
    case "paused": return "attention";
    case "downloading": return "accent";
    default: return "default";
  }
}

export default function PropertiesDialog({ id, onClose }: PropertiesDialogProps) {
  const item = useDownload(id);

  if (!item) return null;

  const progress = item.total_size > 0 ? (item.downloaded / item.total_size) * 100 : 0;

  const resumable = item.resumable === true ? t("properties.yes") : item.resumable === false ? t("properties.no") : t("properties.unknown");

  return (
    <Dialog title={t("properties.title")} onClose={onClose} width="large">
      <div style={{ display: "flex", flexDirection: "column", gap: 0 }}>
        <div style={{ padding: "16px 20px", borderBottom: "1px solid var(--borderColor-muted, #d8dee4)" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}>
            <Text weight="semibold" size="medium" style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {item.file_name}
            </Text>
            <Label variant={statusColor(item.status)}>{item.status}</Label>
          </div>
          <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", wordBreak: "break-all", display: "block", marginTop: 4 }}>
            {item.url}
          </Text>
          {(item.status === "downloading" || item.status === "paused") && (
            <div style={{ marginTop: 12, display: "flex", alignItems: "center", gap: 8 }}>
              <div style={{ flex: 1 }}>
                <ProgressBar progress={Math.round(progress)} />
              </div>
              <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)", flexShrink: 0 }}>
                {Math.round(progress)}{t("progress.percent")}
              </Text>
            </div>
          )}
        </div>

        <div style={{ padding: "12px 20px" }}>
          <div style={sectionStyle}>
            <div style={sectionHeader}>{t("properties.file")}</div>
            <InfoRow label={t("properties.size")} value={formatBytes(item.total_size)} />
            <InfoRow label={t("properties.downloaded")} value={`${formatBytes(item.downloaded)} (${progress > 0 ? Math.round(progress) + "%" : "0%"})`} />
            <InfoRow label={t("properties.savePath")} value={item.save_path || "—"} />
            <InfoRow label={t("properties.created")} value={item.created_at} last />
          </div>
        </div>

        <div style={{ padding: "0 20px 12px" }}>
          <div style={sectionStyle}>
            <div style={sectionHeader}>{t("properties.download")}</div>
            <InfoRow label={t("properties.status")} value={item.status} />
            <InfoRow label={t("properties.resumable")} value={resumable} />
            <InfoRow label={t("properties.lastTry")} value={item.last_try || "—"} last />
          </div>
        </div>

        <div style={{ padding: "0 20px 16px" }}>
          <div style={sectionStyle}>
            <div style={sectionHeader}>{t("properties.network")}</div>
            <InfoRow label={t("properties.connections")} value={String(item.connections)} />
            <InfoRow label={t("properties.proxy")} value={item.proxy_name || t("properties.none")} last />
          </div>
        </div>
      </div>
    </Dialog>
  );
}
