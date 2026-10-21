// Header chips: category legend, person filter, view switch.
import { byId, h, swatch } from "./dom";
import { t } from "./i18n";
import { hasUnclassified, legendCats as baseLegend, members, saveView, state, typeLabelPlural, type Cat, type View } from "./state";

let legendSig = "";
let whoSig = "";

function legendCats(): Cat[] {
  return hasUnclassified() ? [...baseLegend(), "unclassified"] : baseLegend();
}

/** Brings the chips in line with the state. Rebuilds the DOM only when the set of chips changed,
 *  so that keyboard focus survives a toggle. */
export function buildChips(): void {
  const legend = byId("legend");
  const cats = legendCats();
  const sig = cats.join(",") + (state.config ? "+cfg" : "") + cats.map(typeLabelPlural).join("|");
  if (sig !== legendSig) {
    legendSig = sig;
    const nodes: Node[] = cats.map((k) => {
      const b = h("button", "chip");
      b.type = "button";
      b.dataset["cat"] = k;
      b.append(swatch(k), document.createTextNode(typeLabelPlural(k)));
      return b;
    });
    const both = h("span", "chip static");
    both.title = t("both.title");
    both.append(swatch("both"), document.createTextNode(t("both.name")));
    legend.replaceChildren(...nodes, both);
  }
  legend.querySelectorAll<HTMLElement>("[data-cat]").forEach((b) => {
    b.setAttribute("aria-pressed", String(!state.hidden.has(b.dataset["cat"] as Cat)));
  });

  const who = byId("who");
  const people = ["all", ...members()];
  const wsig = people.join(",");
  if (wsig !== whoSig) {
    whoSig = wsig;
    who.replaceChildren(
      ...people.map((p) => {
        const b = h("button", "chip", p === "all" ? t("who.all") : p);
        b.type = "button";
        b.dataset["who"] = p;
        return b;
      }),
    );
  }
  who.querySelectorAll<HTMLElement>("[data-who]").forEach((b) => {
    b.setAttribute("aria-pressed", String(state.member === b.dataset["who"]));
  });

  byId("view")
    .querySelectorAll<HTMLElement>("button")
    .forEach((b) => b.setAttribute("aria-pressed", String(b.dataset["view"] === state.view)));
}

export function initFilters(onChange: () => void): void {
  byId("legend").addEventListener("click", (e) => {
    const b = (e.target as Element).closest<HTMLElement>("[data-cat]");
    if (!b) return;
    const k = b.dataset["cat"] as Cat;
    if (state.hidden.has(k)) state.hidden.delete(k);
    else state.hidden.add(k);
    onChange();
  });
  byId("who").addEventListener("click", (e) => {
    const b = (e.target as Element).closest<HTMLElement>("[data-who]");
    if (!b) return;
    state.member = b.dataset["who"] ?? "all";
    onChange();
  });
  byId("view").addEventListener("click", (e) => {
    const b = (e.target as Element).closest<HTMLElement>("[data-view]");
    if (!b) return;
    state.view = b.dataset["view"] as View;
    saveView(state.view);
    onChange();
  });
}
