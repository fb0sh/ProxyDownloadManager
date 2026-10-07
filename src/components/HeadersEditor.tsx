import { useState } from "react";
import { t } from "../i18n";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

export interface HeaderRow {
  id: number;
  name: string;
  value: string;
}

let nextId = 1;

export function newHeaderRow(name = "", value = ""): HeaderRow {
  nextId += 1;
  return { id: nextId, name, value };
}

export function headersToRows(headers?: Record<string, string>): HeaderRow[] {
  const rows = Object.entries(headers ?? {}).map(([name, value]) => newHeaderRow(name, String(value)));
  return [...rows, newHeaderRow()];
}

/** Rows → the map the backend takes; nameless or valueless rows are skipped. */
export function rowsToHeaders(rows: HeaderRow[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const row of rows) {
    const name = row.name.trim();
    if (!name || !row.value.trim()) continue;
    out[name] = row.value;
  }
  return out;
}

/** RFC 7230 token: what a header name is allowed to be, and nothing else. */
const HEADER_NAME = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;

/**
 * Parse what people actually paste: DevTools' "Copy request headers", or a
 * `curl` command with its `-H 'name: value'` lines. A name that is not a token
 * is skipped, which is what keeps the `curl 'https://…' \` line out of the
 * result — its `https:` colon is not a header separator.
 */
export function parseHeaderLines(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of String(text ?? "").split(/\r?\n/)) {
    let line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    line = line.replace(/^-H\s+/i, "").replace(/[,\\]\s*$/, "");
    const at = line.indexOf(":");
    if (at <= 0) continue;
    const name = line.slice(0, at).trim().replace(/^['"]|['"]$/g, "");
    const value = line.slice(at + 1).trim().replace(/^['"]|['"]$/g, "");
    if (!HEADER_NAME.test(name)) continue;
    out[name] = value;
  }
  return out;
}

interface HeadersEditorProps {
  rows: HeaderRow[];
  onChange: (rows: HeaderRow[]) => void;
  disabled?: boolean;
}

export function HeadersEditor({ rows, onChange, disabled = false }: HeadersEditorProps) {
  const [paste, setPaste] = useState("");
  const [pasting, setPasting] = useState(false);

  const patch = (id: number, next: Partial<HeaderRow>) =>
    onChange(rows.map((row) => (row.id === id ? { ...row, ...next } : row)));

  const remove = (id: number) => {
    const kept = rows.filter((row) => row.id !== id);
    onChange(kept.length ? kept : [newHeaderRow()]);
  };

  const add = () => onChange([...rows, newHeaderRow()]);

  const importPasted = () => {
    const parsed = parseHeaderLines(paste);
    const replaced = new Set(Object.keys(parsed).map((name) => name.toLowerCase()));
    const kept = rows.filter((row) => row.name.trim() && !replaced.has(row.name.trim().toLowerCase()));
    const added = Object.entries(parsed).map(([name, value]) => newHeaderRow(name, value));
    onChange([...kept, ...added, newHeaderRow()]);
    setPaste("");
    setPasting(false);
  };

  return (
    <div className="flex min-h-0 flex-col gap-2">
      <div className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto pr-1">
        {rows.map((row) => (
          <div key={row.id} className="flex items-center gap-1.5">
            <Input
              className="h-7 flex-[0_0_36%] text-[12px]"
              placeholder={t("headers.name")}
              value={row.name}
              disabled={disabled}
              onChange={(e) => patch(row.id, { name: e.target.value })}
            />
            <Input
              className="h-7 min-w-0 flex-1 text-[12px]"
              placeholder={t("headers.value")}
              value={row.value}
              disabled={disabled}
              onChange={(e) => patch(row.id, { value: e.target.value })}
            />
            <Button
              className="h-7 shrink-0 px-2 text-[12px]"
              disabled={disabled}
              title={t("headers.remove")}
              onClick={() => remove(row.id)}
            >
              ×
            </Button>
          </div>
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button className="h-7 px-2 text-[12px]" disabled={disabled} onClick={add}>
          {t("headers.add")}
        </Button>
        <Button
          className="h-7 px-2 text-[12px]"
          disabled={disabled}
          onClick={() => setPasting((v) => !v)}
        >
          {t("headers.paste")}
        </Button>
        <span className="text-[11px] text-muted-foreground">{t("headers.hint")}</span>
      </div>
      {pasting && (
        <div className="flex flex-col gap-1.5">
          <textarea
            className="h-16 w-full resize-none rounded-md border border-border bg-card p-2 text-[12px] outline-none placeholder:text-muted-foreground focus:border-primary"
            placeholder={t("headers.pastePlaceholder")}
            value={paste}
            disabled={disabled}
            onChange={(e) => setPaste(e.target.value)}
          />
          <div className="flex justify-end">
            <Button
              className="h-7 px-2 text-[12px]"
              variant="default"
              disabled={disabled || !paste.trim()}
              onClick={importPasted}
            >
              {t("headers.import")}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

export default HeadersEditor;
