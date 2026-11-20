// Dates and sizes shown on the download page, in the language of the page. Pure (no `astro:`), so the build and the browser script share them.
import type { Locale } from "./i18n";

/** "2026-11-20" as a long date: "November 20, 2026" / "20 de novembro de 2026". */
export const fmtDate = (iso: string, locale: Locale): string => new Date(`${iso}T00:00:00Z`).toLocaleDateString(locale, { year: "numeric", month: "long", day: "numeric", timeZone: "UTC" });

/** A file size in MB with one decimal, the decimal mark of the language: "12.3 MB" / "12,3 MB". */
export const fmtSize = (bytes: number, locale: Locale): string => `${new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(bytes / 1_048_576)} MB`;
