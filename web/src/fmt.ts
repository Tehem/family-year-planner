// Date and time formatting in the household's time zone.
import { locale, t } from "./i18n";

function parts(d: Date, tz: string, opts: Intl.DateTimeFormatOptions): (type: string) => string {
  const ps = new Intl.DateTimeFormat(locale(), { timeZone: tz, hourCycle: "h23", ...opts }).formatToParts(d);
  return (t) => ps.find((p) => p.type === t)?.value ?? "";
}

/** "16 oct. 2026 à 14:32" for an RFC 3339 timestamp, in `tz`. */
export function fmtDateTime(iso: string, tz: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const g = parts(d, tz, { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" });
  return `${g("day")} ${g("month")} ${g("year")} ${t("fmt.at")} ${g("hour")}:${g("minute")}`;
}

/** "14:32" in `tz`. */
export function fmtTime(d: Date, tz: string): string {
  const g = parts(d, tz, { hour: "2-digit", minute: "2-digit" });
  return `${g("hour")}:${g("minute")}`;
}

/** The calendar day ("YYYY-MM-DD") and seconds since local midnight of `d` in `tz`. */
export function localDay(d: Date, tz: string): { day: string; secs: number } {
  const g = parts(d, tz, { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit" });
  return {
    day: `${g("year")}-${g("month")}-${g("day")}`,
    secs: +g("hour") * 3600 + +g("minute") * 60 + +g("second"),
  };
}
