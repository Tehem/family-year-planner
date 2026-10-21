// Print popover and print layout (SPEC §10).
//
// The page size comes from the named pages `a3` / `a4` of styles.css (`@page a3 { size: A3 landscape }`),
// selected with `html[data-paper]`. The sheet (body) has a fixed size in millimetres, so the layout
// never depends on the printer's real margins: it is one page by construction. Only `--print-cs`
// is computed here, from the grid area left under the header and above the footer.
import { byId, h, swatch } from "./dom";
import { fmtDayMonthYear } from "./dates";
import { LEGEND_CATS, state, today, typeLabelPlural, type Cat } from "./state";

type Paper = "a3" | "a4";
/** Printable area (page minus 10 mm margins) and fixed header/footer heights, all in mm. */
const PAPERS: Record<Paper, { w: number; h: number; head: number; foot: number; fs: number }> = {
  a3: { w: 400, h: 277, head: 20, foot: 6, fs: 15 },
  a4: { w: 277, h: 190, head: 20, foot: 6, fs: 12 },
};
const MM = 96 / 25.4;
let paper: Paper = "a3";

function buildPrintHeader(): void {
  byId("printYear").textContent = String(state.year);
  const cats: Cat[] = [...LEGEND_CATS, ...(state.events.some((e) => e.cat === "unclassified") ? (["unclassified"] as Cat[]) : [])];
  const legend = byId("printLegend");
  const chips = cats
    .filter((c) => !state.hidden.has(c))
    .map((c) => {
      const chip = h("span", "chip");
      chip.append(swatch(c), document.createTextNode(typeLabelPlural(c)));
      return chip;
    });
  const both = h("span", "chip");
  both.append(swatch("both"), document.createTextNode("Les deux"));
  legend.replaceChildren(...chips, both);

  const hidden = cats.filter((c) => state.hidden.has(c)).map((c) => typeLabelPlural(c));
  const parts: string[] = [];
  if (hidden.length) parts.push(`sans ${hidden.join(", ")}`);
  if (state.member !== "all") parts.push(state.member);
  const line = byId("printFilters");
  line.hidden = parts.length === 0;
  line.textContent = parts.length ? `Filtres : ${parts.join(" · ")}` : "";
  byId("printFoot").textContent = `Imprimé le ${fmtDayMonthYear(today())}`;
}

/** Sets the page format and the cell size for the print layout. Idempotent. */
export function preparePrint(p: Paper): void {
  paper = p;
  const spec = PAPERS[p];
  const root = document.documentElement;
  root.dataset["paper"] = p;
  root.style.setProperty("--sheet-w", `${spec.w}mm`);
  root.style.setProperty("--sheet-h", `${spec.h}mm`);
  root.style.setProperty("--sheet-head", `${spec.head}mm`);
  root.style.setProperty("--sheet-foot", `${spec.foot}mm`);
  root.style.setProperty("--sheet-fs", `${spec.fs}px`);
  const areaW = spec.w * MM;
  const areaH = (spec.h - 1 - spec.head - spec.foot) * MM;
  const cs = Math.floor(Math.min((areaW - 4) / state.W, (areaH - 4) / state.H) * 10) / 10;
  root.style.setProperty("--print-cs", `${cs}px`);
  buildPrintHeader();
}

export function initPrint(): void {
  const btn = byId("printBtn");
  const pop = byId("printPop");
  const select = byId<HTMLSelectElement>("printFormat");
  const close = (focusBtn = false) => {
    pop.hidden = true;
    btn.setAttribute("aria-expanded", "false");
    if (focusBtn) btn.focus();
  };
  btn.addEventListener("click", () => {
    const open = pop.hidden;
    pop.hidden = !open;
    btn.setAttribute("aria-expanded", String(open));
    if (open) select.focus();
  });
  byId("printCancel").addEventListener("click", () => close(true));
  pop.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close(true);
    }
  });
  document.addEventListener("pointerdown", (e) => {
    if (!pop.hidden && !pop.contains(e.target as Node) && !btn.contains(e.target as Node)) close();
  });
  pop.addEventListener("submit", (e) => {
    e.preventDefault();
    close();
    preparePrint(select.value === "a4" ? "a4" : "a3");
    window.print();
  });
  // Ctrl+P / browser menu: print with the last chosen format.
  window.addEventListener("beforeprint", () => preparePrint(paper));
}
