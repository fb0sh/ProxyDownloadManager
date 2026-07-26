import type { DownloadPart, DownloadStatus } from "../types";
import { cellPercents } from "../utils/progressMap";
import { t } from "../i18n";

const COLS = 8;
const CELL = 36;

interface ProgressMapProps {
  parts: DownloadPart[];
  /** Status drives the map rules (Completed → all cells 100%). */
  status: DownloadStatus;
}

/**
 * Progress Map: one cell per fixed Part, 8 columns LTR then top→bottom.
 * Green fills bottom-up by part %; percent label centered. The status rules
 * live in cellPercents — this component only renders.
 */
export default function ProgressMap({ parts, status }: ProgressMapProps) {
  if (!parts.length) {
    return (
      <div style={{ fontSize: 12, color: "var(--fgColor-muted, #656d76)" }}>
        {t("properties.progressMapEmpty")}
      </div>
    );
  }

  const percents = cellPercents(parts, status);

  return (
    <div
      role="img"
      aria-label={t("properties.progressMap")}
      style={{
        display: "grid",
        gridTemplateColumns: `repeat(${COLS}, ${CELL}px)`,
        gap: 4,
        width: "fit-content",
        maxWidth: "100%",
      }}
    >
      {parts.map((part, i) => {
        const pct = percents[i] ?? 0;
        return (
          <div
            key={part.index}
            title={`${pct}% · ${part.start}–${part.end}`}
            style={{
              width: CELL,
              height: CELL,
              position: "relative",
              border: "1px solid var(--borderColor-muted, #d8dee4)",
              borderRadius: 3,
              overflow: "hidden",
              background: "var(--bgColor-muted, #eaeef2)",
              flexShrink: 0,
            }}
          >
            <div
              style={{
                position: "absolute",
                left: 0,
                right: 0,
                bottom: 0,
                height: `${pct}%`,
                background: "var(--bgColor-success-emphasis, #1a7f37)",
                transition: "height 0.2s ease-out",
              }}
            />
            <span
              style={{
                position: "absolute",
                inset: 0,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                fontSize: 8,
                fontWeight: 600,
                lineHeight: 1,
                color: pct > 55 ? "#fff" : "var(--fgColor-default, #1f2328)",
                textShadow: pct > 55 ? "0 0 2px rgba(0,0,0,0.35)" : undefined,
                userSelect: "none",
              }}
            >
              {pct}%
            </span>
          </div>
        );
      })}
    </div>
  );
}
