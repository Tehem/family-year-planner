// Shared application state and the category vocabulary of the prototype.
import type { ApiEvent, Config, Me } from "./api";

/** Category names (CSS classes `cat-*`): the prototype's names for the built-in types, the type
 *  code itself (`custom_1`…) for the types the family added (SPEC §19.3). */
export type Cat = string;
export type View = "week" | "month";

export const CODE_TO_CAT: Record<string, Cat> = {
  birthday: "birthday",
  celebration: "celebration",
  family_gathering: "gathering",
  family_home: "stay",
  trip: "trip",
  school_holiday: "school",
  daycare_closure: "daycare",
  public_holiday: "holiday",
  unclassified: "unclassified",
};
export const CAT_TO_CODE: Record<string, string> = Object.fromEntries(
  Object.entries(CODE_TO_CAT).map(([code, cat]) => [cat, code]),
);
export const catOf = (code: string): Cat => CODE_TO_CAT[code] ?? code;
export const codeOf = (cat: Cat): string => CAT_TO_CODE[cat] ?? cat;

/** Legend order of the prototype (built-in types). */
const BUILTIN_LEGEND: Cat[] = ["birthday", "celebration", "gathering", "stay", "trip", "school", "daycare", "holiday"];

/** The categories of the legend: the prototype's order, with the family's own categories after the
 *  built-in bars (before the tints). `unclassified` is appended by the callers when needed. */
export function legendCats(): Cat[] {
  const custom = (state.config?.types ?? []).filter((t) => t.custom).map((t) => t.code);
  const at = BUILTIN_LEGEND.indexOf("school");
  return [...BUILTIN_LEGEND.slice(0, at), ...custom, ...BUILTIN_LEGEND.slice(at)];
}
/** Categories drawn as cell tints instead of bars. */
export const TINTS: Cat[] = ["school", "daycare", "birthday", "holiday"];
/** Categories that ignore the person filter (SPEC §5.6). */
export const ALWAYS_SHOWN: Cat[] = ["school", "holiday", "birthday"];
/** Order of the category select in the event dialog (prototype's EDITABLE); added types come last. */
export const EDITABLE_CODES = ["birthday", "celebration", "family_gathering", "family_home", "trip", "daycare_closure"];
export const editableRank = (code: string): number => {
  const i = EDITABLE_CODES.indexOf(code);
  return i < 0 ? EDITABLE_CODES.length + +code.replace(/\D/g, "") : i;
};

export interface AppEvent extends ApiEvent {
  cat: Cat;
}
export interface DayItem {
  label: string;
  cat: Cat;
  id: string;
}

export const state = {
  year: new Date().getFullYear(),
  view: "week" as View,
  hidden: new Set<Cat>(),
  member: "all",
  events: [] as AppEvent[],
  byDay: {} as Record<string, DayItem[]>,
  config: null as Config | null,
  me: null as Me | null,
  W: 1,
  H: 1,
};

const VIEW_KEY = "annee.view";
export function loadView(): View {
  try {
    const v = localStorage.getItem(VIEW_KEY);
    if (v === "week" || v === "month") return v;
  } catch {
    /* storage unavailable */
  }
  return "week";
}
export function saveView(v: View): void {
  try {
    localStorage.setItem(VIEW_KEY, v);
  } catch {
    /* storage unavailable */
  }
}

export const today = (): string => state.config?.today ?? "1970-01-01";
export const members = (): string[] => state.config?.members ?? [];

export function typeLabel(code: string): string {
  return state.config?.types.find((t) => t.code === code)?.label ?? code;
}
export function typeLabelPlural(cat: Cat): string {
  return state.config?.types.find((t) => t.code === codeOf(cat))?.label_plural ?? cat;
}

export function toAppEvent(e: ApiEvent): AppEvent {
  return { ...e, cat: catOf(e.type) };
}

/** Category and person filters (SPEC §5.6). An event without members concerns everyone. */
export function visible(e: AppEvent): boolean {
  if (state.hidden.has(e.cat)) return false;
  if (state.member === "all" || ALWAYS_SHOWN.includes(e.cat)) return true;
  return e.members.length === 0 || e.members.includes(state.member);
}

export const findEvent = (id: string): AppEvent | undefined => state.events.find((e) => e.id === id);
export const hasUnclassified = (): boolean => state.events.some((e) => e.cat === "unclassified");
