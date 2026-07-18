import type { CSSProperties } from "react";

/** Card container for dialog sections. */
export const sectionCard: CSSProperties = {
  border: "1px solid var(--borderColor-muted, #d8dee4)",
  borderRadius: 6,
  overflow: "hidden",
};

/** Uppercase header bar inside a section card. */
export const sectionHeader: CSSProperties = {
  padding: "8px 12px",
  fontSize: 12,
  fontWeight: 600,
  color: "var(--fgColor-muted, #656d76)",
  borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
  background: "var(--bgColor-subtle, #f6f8fa)",
  textTransform: "uppercase",
  letterSpacing: "0.05em",
};

/** Padding container inside a section card, below the header. */
export const sectionBody: CSSProperties = {
  padding: "12px 16px",
  display: "flex",
  flexDirection: "column",
  gap: 12,
};

/** A labeled field row (label + control side by side). */
export const fieldRow: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
};

/** Label in a field row. */
export const fieldLabel: CSSProperties = {
  fontSize: 13,
  color: "var(--fgColor-default, #1f2328)",
};

/** Control area in a field row. */
export const fieldControl: CSSProperties = {
  display: "flex",
  alignItems: "center",
};
