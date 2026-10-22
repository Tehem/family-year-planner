// Day tooltip (mouse only).
import { byId, h, swatch } from "./dom";
import { cap, fmtShort } from "./dates";
import { t } from "./i18n";
import { state } from "./state";

export function showTip(d: string, e: PointerEvent): void {
  const tip = byId("tip");
  const items = state.byDay[d] ?? [];
  const nodes: Node[] = [h("b", undefined, cap(fmtShort(d)))];
  if (items.length) {
    for (const it of items) {
      const row = h("div", "row");
      row.append(swatch(it.cat), h("span", undefined, it.label));
      nodes.push(row);
    }
  } else nodes.push(h("div", "empty-note", t("tip.nothing")));
  tip.replaceChildren(...nodes);
  tip.classList.add("on");
  const r = tip.getBoundingClientRect();
  let x = e.clientX + 14;
  let y = e.clientY + 14;
  if (x + r.width > innerWidth - 8) x = e.clientX - r.width - 14;
  if (y + r.height > innerHeight - 8) y = e.clientY - r.height - 14;
  tip.style.left = x + "px";
  tip.style.top = y + "px";
}

export const hideTip = (): void => byId("tip").classList.remove("on");
