// Tiny DOM helpers. The CSP forbids inline styles, so geometry goes through the CSSOM.
import type { Cat } from "./state";

export const byId = <T extends HTMLElement = HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`Missing #${id}`);
  return el as T;
};

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  cls?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  if (cls) el.className = cls;
  if (text !== undefined) el.textContent = text;
  return el;
}

export function clear(el: Element): void {
  el.replaceChildren();
}

const u = (v: number): string => `calc(var(--cs) * ${+v.toFixed(3)})`;

/** Positions an element on the `--cs` grid. `dx` is an extra pixel offset on both axes. */
export function place(el: HTMLElement, x: number, y: number, w?: number, hgt?: number, dx = 0): void {
  const off = dx ? ` + ${dx}px` : "";
  el.style.setProperty("left", `calc(var(--cs) * ${+x.toFixed(3)}${off})`);
  el.style.setProperty("top", `calc(var(--cs) * ${+y.toFixed(3)}${off})`);
  if (w !== undefined) el.style.setProperty("width", u(w));
  if (hgt !== undefined) el.style.setProperty("height", u(hgt));
}

/** Colour swatch of a category (legend, tooltip, panel). */
export function swatch(cat: Cat | "both"): HTMLElement {
  if (cat === "holiday") return h("span", "sw phs", "14");
  const tint = ["school", "daycare", "birthday", "both"].includes(cat) ? " tint" : "";
  const s = h("span", `sw cat-${cat}${tint}`);
  s.setAttribute("aria-hidden", "true");
  return s;
}
