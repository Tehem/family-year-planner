// Colors of the types (SPEC §19.4). The palette itself is in styles.css (`--p<colorId>-bar` and
// `--p<colorId>-tint`, with their dark and print variants); here each type is pointed at the entry
// of its current Google `colorId`, so themes and the always-light print need no extra logic.
import type { TypeInfo } from "./api";
import { catOf } from "./state";

/** CSS variable holding the tint of the types drawn as cell backgrounds. */
const TINT_VAR: Record<string, string> = { school: "--school", daycare: "--daycare", birthday: "--bday" };

export function applyTypeColors(types: TypeInfo[]): void {
  const root = document.documentElement.style;
  for (const t of types) {
    if (!t.color_id || t.code === "unclassified" || t.code === "public_holiday") continue;
    const cat = catOf(t.code);
    const tint = TINT_VAR[cat];
    if (tint) root.setProperty(tint, `var(--p${t.color_id}-tint)`);
    else root.setProperty(`--b-${cat}`, `var(--p${t.color_id}-bar)`);
  }
}
