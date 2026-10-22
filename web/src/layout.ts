// Geometry of the two views, in units of `--cs` (SPEC §8.1).
import type { View } from "./state";
import { addDays, diff, dow, iso } from "./dates";
import { getLang } from "./i18n";

export interface Pos {
  /** Bars are split on track changes (a month row, or a week column). */
  track: number;
  /** Position along the track; consecutive days differ by 1. */
  p: number;
  x: number;
  y: number;
}
export interface HeaderCell {
  cls: string;
  x: number;
  y: number;
  w: number;
  h: number;
  text: string;
}
export interface Layout {
  W: number;
  H: number;
  pos(d: string): Pos;
  hd: HeaderCell[];
  /** Months view: bars run horizontally. */
  horiz: boolean;
}

const MONTHS_FR = ["Janv.", "Févr.", "Mars", "Avr.", "Mai", "Juin", "Juil.", "Août", "Sept.", "Oct.", "Nov.", "Déc."];
const MONTHS_EN = ["Jan.", "Feb.", "Mar.", "Apr.", "May", "June", "July", "Aug.", "Sept.", "Oct.", "Nov.", "Dec."];
export const months = (): string[] => (getLang() === "en" ? MONTHS_EN : MONTHS_FR);
const weekdayLetters = (): string => (getLang() === "en" ? "MTWTFSS" : "LMMJVSD");

export function layout(Y: number, view: View): Layout {
  const MONTHS = months();
  const WD = weekdayLetters();
  if (view === "month") {
    const L = 1.35;
    const T = 0.7;
    const first = (m: number) => dow(iso(Y, m, 1));
    const pos = (d: string): Pos => {
      const m = +d.slice(5, 7);
      const col = first(m) + +d.slice(8) - 1;
      return { track: m, p: col, x: L + col, y: T + m - 1 };
    };
    const hd: HeaderCell[] = [];
    for (let c = 0; c < 37; c++)
      hd.push({ cls: `hd wd${c % 7 >= 5 ? " we" : ""}`, x: L + c, y: 0, w: 1, h: T, text: WD[c % 7] ?? "" });
    for (let m = 1; m <= 12; m++) hd.push({ cls: "hd mo", x: 0, y: T + m - 1, w: L, h: 1, text: MONTHS[m - 1] ?? "" });
    return { W: L + 37, H: T + 12, pos, hd, horiz: true };
  }
  const L = 0.9;
  const HD = 0.75;
  const GAP = 0.55;
  const MG = 0.45;
  const start = addDays(iso(Y, 1, 1), -dow(iso(Y, 1, 1)));
  const weekOf = (d: string) => Math.floor(diff(start, d) / 7);
  const tops = [HD, HD + 7 + GAP + HD] as const;
  const mx: Record<number, { x: number; w0: number; h: 0 | 1 }> = {};
  const wide = [L, L];
  for (let m = 1; m <= 12; m++) {
    const h = m > 6 ? 1 : 0;
    const last = m === 12 ? iso(Y, 12, 31) : addDays(iso(Y, m + 1, 1), -1);
    const w0 = weekOf(iso(Y, m, 1));
    const w1 = weekOf(last);
    if (m !== 1 && m !== 7) wide[h] = (wide[h] ?? L) + MG;
    mx[m] = { x: wide[h] ?? L, w0, h };
    wide[h] = (wide[h] ?? L) + (w1 - w0 + 1);
  }
  const pos = (d: string): Pos => {
    const m = +d.slice(5, 7);
    const o = mx[m]!;
    const w = weekOf(d);
    const r = dow(d);
    return { track: m * 100 + w, p: r, x: o.x + w - o.w0, y: tops[o.h] + r };
  };
  const hd: HeaderCell[] = [];
  for (const t of tops)
    for (let r = 0; r < 7; r++)
      hd.push({ cls: `hd wd${r >= 5 ? " we" : ""}`, x: 0, y: t + r, w: L * 0.8, h: 1, text: WD[r] ?? "" });
  for (let m = 1; m <= 12; m++) {
    const o = mx[m]!;
    hd.push({ cls: "hd mo", x: o.x + 0.08, y: tops[o.h] - HD, w: 3, h: HD, text: MONTHS[m - 1] ?? "" });
  }
  return { W: Math.max(...wide), H: tops[1] + 7, pos, hd, horiz: false };
}
