import en from "./en";
import zh from "./zh";
import type { Translations } from "./en";

const locales: Record<string, Translations> = { en, zh };

function resolve(obj: Record<string, unknown>, path: string): string {
  const keys = path.split(".");
  let val: unknown = obj;
  for (const key of keys) {
    if (val && typeof val === "object" && key in val) {
      val = (val as Record<string, unknown>)[key];
    } else {
      return path;
    }
  }
  return typeof val === "string" ? val : path;
}

let current: Translations = en;

export function setLanguage(lang: string) {
  current = locales[lang] || en;
}

export function t(path: string): string {
  return resolve(current as unknown as Record<string, unknown>, path);
}

export { type Translations };
