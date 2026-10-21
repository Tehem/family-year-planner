// Date helpers on ISO "YYYY-MM-DD" strings (local calendar days, no time zones involved).

export const pad = (n: number): string => String(n).padStart(2, "0");
export const iso = (y: number, m: number, d: number): string => `${y}-${pad(m)}-${pad(d)}`;

export function parse(s: string): Date {
  const [y = 1970, m = 1, d = 1] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}
export const fromDate = (dt: Date): string => iso(dt.getFullYear(), dt.getMonth() + 1, dt.getDate());
export function addDays(s: string, n: number): string {
  const d = parse(s);
  d.setDate(d.getDate() + n);
  return fromDate(d);
}
export const diff = (a: string, b: string): number => Math.round((parse(b).getTime() - parse(a).getTime()) / 864e5);
export const span = (s: string, e: string): number => diff(s, e) + 1;
/** Day of week, Monday = 0. */
export const dow = (s: string): number => (parse(s).getDay() + 6) % 7;

export const fmtLong = (s: string): string =>
  parse(s).toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
export const fmtShort = (s: string): string =>
  parse(s).toLocaleDateString("fr-FR", { weekday: "short", day: "numeric", month: "short" });
export const fmtRange = (s: string, e: string): string =>
  s === e ? fmtLong(s) : `du ${fmtShort(s)} au ${fmtShort(e)} ${parse(e).getFullYear()}`;
/** "16 octobre 2026" (used by the print footer). */
export const fmtDayMonthYear = (s: string): string =>
  parse(s).toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" });
export const cap = (t: string): string => t.charAt(0).toUpperCase() + t.slice(1);

/** Calls `fn` for every day of [s, e] clipped to the year `Y`. */
export function eachDay(s: string, e: string, Y: number, fn: (d: string) => void): void {
  const a = s < `${Y}-01-01` ? `${Y}-01-01` : s;
  const b = e > `${Y}-12-31` ? `${Y}-12-31` : e;
  for (let d = a; d <= b; d = addDays(d, 1)) fn(d);
}
