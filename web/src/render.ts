// Grid rendering: cells, tints, bars, markers (port of the prototype's render()).
import { byId, h, place } from "./dom";
import { addDays, dow, eachDay, fmtRange } from "./dates";
import { layout } from "./layout";
import { state, today, TINTS, visible, type AppEvent, type DayItem } from "./state";

const LANES = 2;

interface Seg {
  ev: AppEvent;
  track: number;
  p0: number;
  p1: number;
  x: number;
  y: number;
  s: string;
  t: string;
  lane: number;
}

export function render(): void {
  const Y = state.year;
  byId("yl").textContent = String(Y);
  const lay = layout(Y, state.view);
  const evs = state.events.filter(visible);

  const byDay: Record<string, DayItem[]> = {};
  const sc = new Set<string>();
  const dc = new Set<string>();
  const bd = new Set<string>();
  const ph = new Set<string>();
  const overflow = new Set<string>();
  const push = (d: string, it: DayItem) => (byDay[d] ??= []).push(it);
  // Day items are listed school holidays first, then public holidays, then the other events.
  for (const cat of ["school", "holiday"] as const)
    evs
      .filter((e) => e.cat === cat)
      .forEach((ev) =>
        eachDay(ev.start, ev.end, Y, (d) => {
          (cat === "school" ? sc : ph).add(d);
          push(d, { label: ev.title, cat, id: ev.id });
        }),
      );
  evs
    .filter((e) => e.cat !== "school" && e.cat !== "holiday")
    .forEach((ev) =>
      eachDay(ev.start, ev.end, Y, (d) => {
        if (ev.cat === "daycare") dc.add(d);
        if (ev.cat === "birthday") bd.add(d);
        push(d, { label: ev.title, cat: ev.cat, id: ev.id });
      }),
    );
  state.byDay = byDay;

  // Bar segments: consecutive days that share a track (month row or week column).
  const segs: Seg[] = [];
  evs
    .filter((e) => !TINTS.includes(e.cat))
    .forEach((ev) => {
      let cur: Seg | null = null;
      eachDay(ev.start, ev.end, Y, (d) => {
        const p = lay.pos(d);
        if (cur && cur.track === p.track && p.p === cur.p1 + 1) {
          cur.p1 = p.p;
          cur.t = d;
        } else {
          cur = { ev, track: p.track, p0: p.p, p1: p.p, x: p.x, y: p.y, s: d, t: d, lane: 0 };
          segs.push(cur);
        }
      });
    });
  const byTrack = new Map<number, Seg[]>();
  for (const s of segs) {
    const list = byTrack.get(s.track) ?? [];
    list.push(s);
    byTrack.set(s.track, list);
  }
  for (const list of byTrack.values()) {
    list.sort((a, b) => a.p0 - b.p0 || b.p1 - b.p0 - (a.p1 - a.p0));
    const ends: number[] = [];
    for (const s of list) {
      let l = ends.findIndex((x) => x < s.p0);
      if (l < 0) {
        l = ends.length;
        ends.push(0);
      }
      ends[l] = s.p1;
      s.lane = l;
      if (l >= LANES) eachDay(s.s, s.t, Y, (d) => overflow.add(d));
    }
  }

  const frag = document.createDocumentFragment();
  for (const c of lay.hd) {
    const el = h("div", c.cls, c.text);
    place(el, c.x, c.y, c.w, c.h);
    frag.append(el);
  }

  const now = today();
  for (let d = `${Y}-01-01`; d <= `${Y}-12-31`; d = addDays(d, 1)) {
    const p = lay.pos(d);
    const w = dow(d);
    const cls = ["cell"];
    if (w >= 5 || ph.has(d)) cls.push("we");
    if (ph.has(d)) cls.push("ph");
    if (sc.has(d)) cls.push("sc");
    if (dc.has(d)) cls.push("dc");
    if (bd.has(d)) cls.push("bd");
    if (d.endsWith("-01")) cls.push("first");
    if (d === now) cls.push("today");
    const el = h("div", cls.join(" "));
    el.dataset["d"] = d;
    place(el, p.x, p.y, undefined, undefined, 1.5);
    el.append(h("span", "dn", String(+d.slice(8))));
    if (bd.has(d) || overflow.has(d)) {
      const mk = h("span", "mk");
      if (bd.has(d)) {
        const cake = h("span", "cake", "🎂");
        cake.setAttribute("aria-hidden", "true");
        mk.append(cake);
      }
      if (overflow.has(d)) mk.append(h("i", "dot more"));
      el.append(mk);
    }
    frag.append(el);
  }

  const T = 0.3;
  const G = 0.03;
  const M = 0.06;
  const E = 0.05;
  for (const s of segs) {
    if (s.lane >= LANES) continue;
    const len = s.p1 - s.p0 + 1;
    const ca = s.s > s.ev.start;
    const cb = s.t < s.ev.end;
    const a = ca ? 0 : E;
    const b = cb ? 0 : E;
    const off = 1 - M - (s.lane + 1) * T - s.lane * G;
    const el = h("div", `bar ${lay.horiz ? "h" : "v"} cat-${s.ev.cat}${ca ? " ca" : ""}${cb ? " cb" : ""}`, s.ev.title);
    el.setAttribute("role", "button");
    el.tabIndex = 0;
    el.dataset["id"] = s.ev.id;
    el.setAttribute("aria-label", `${s.ev.title}, ${fmtRange(s.ev.start, s.ev.end)}`);
    if (lay.horiz) place(el, s.x + a, s.y + off, len - a - b, T);
    else place(el, s.x + off, s.y + a, T, len - a - b);
    frag.append(el);
  }

  const cv = byId("canvas");
  cv.className = "canvas " + (lay.horiz ? "v-month" : "v-week");
  cv.replaceChildren(frag);
  state.W = lay.W;
  state.H = lay.H;
  cv.style.setProperty("--W", String(lay.W));
  cv.style.setProperty("--H", String(lay.H));
  fit();
}

/** Computes the cell size `--cs` (clamped to 22..80 px) that fits the board. */
export function fit(): void {
  const b = byId("board");
  const hint = b.querySelector<HTMLElement>(".hint");
  const hh = hint && getComputedStyle(hint).display !== "none" ? hint.offsetHeight + 8 : 0;
  const cs = Math.floor(Math.min((b.clientWidth - 24) / state.W, (b.clientHeight - 14 - hh) / state.H));
  const v = Math.max(22, Math.min(cs, 80));
  document.documentElement.style.setProperty("--cs", v + "px");
}
