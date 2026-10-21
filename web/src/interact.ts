// Pointer interactions on the board: hover tooltip, click, drag-to-create (port of the prototype).
//
// State machine: `drag === null` (idle) -> pointerdown on a cell -> `drag` set (a = b = cell)
// -> pointermove over other cells sets `moved` and extends the selection -> pointerup either opens
// the dialog (moved over >= 2 days), the event panel (started on a bar) or the day panel.
// pointercancel (e.g. a touch scroll) just clears the selection, so scrolling is never blocked.
import { byId } from "./dom";
import { openCreate } from "./dialog";
import { openDay, openEvent } from "./panel";
import { hideTip, showTip } from "./tooltip";

interface Drag {
  /** Start cell; null when the press landed on a bar in the gap between two cells. */
  a: string | null;
  b: string | null;
  bar: string | null;
  moved: boolean;
}
let drag: Drag | null = null;

function cellAt(x: number, y: number): HTMLElement | null {
  for (const el of document.elementsFromPoint(x, y)) if (el instanceof HTMLElement && el.classList.contains("cell")) return el;
  return null;
}
function paintSel(): void {
  if (!drag) return;
  if (!drag.a || !drag.b) return;
  const [a, b] = [drag.a, drag.b].sort() as [string, string];
  document.querySelectorAll<HTMLElement>(".cell").forEach((c) => {
    const d = c.dataset["d"] ?? "";
    c.classList.toggle("sel", d >= a && d <= b);
  });
}
const clearSel = (): void => document.querySelectorAll(".cell.sel").forEach((c) => c.classList.remove("sel"));

export function initInteractions(): void {
  const board = byId("board");
  board.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    const c = cellAt(e.clientX, e.clientY);
    const bar = (e.target as Element).closest<HTMLElement>(".bar");
    if (!c?.dataset["d"] && !bar) return;
    const d = c?.dataset["d"] ?? null;
    drag = { a: d, b: d, bar: bar?.dataset["id"] ?? null, moved: false };
  });
  window.addEventListener("pointermove", (e) => {
    if (e.pointerType !== "mouse") return;
    const c = cellAt(e.clientX, e.clientY);
    if (drag) {
      if (drag.a && c?.dataset["d"] && c.dataset["d"] !== drag.b) {
        drag.b = c.dataset["d"];
        drag.moved = true;
        paintSel();
      }
      hideTip();
      return;
    }
    if (c?.dataset["d"] && !byId("panel").contains(e.target as Node)) showTip(c.dataset["d"], e);
    else hideTip();
  });
  window.addEventListener("pointerup", () => {
    if (!drag) return;
    const d = drag;
    drag = null;
    clearSel();
    if (d.moved && d.a && d.b && d.a !== d.b) {
      const [s, t] = [d.a, d.b].sort() as [string, string];
      openCreate(s, t);
    } else if (d.bar) openEvent(d.bar);
    else if (d.a) openDay(d.a);
  });
  window.addEventListener("pointercancel", () => {
    drag = null;
    clearSel();
  });
  board.addEventListener("keydown", (e) => {
    const b = (e.target as Element).closest<HTMLElement>(".bar");
    if (b?.dataset["id"] && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      openEvent(b.dataset["id"]);
    }
  });
  board.addEventListener("pointerleave", hideTip);
}
