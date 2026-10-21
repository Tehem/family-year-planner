// Data loading for the displayed year, error banner.
import { ApiError, getEvents } from "./api";
import { byId } from "./dom";
import { buildChips } from "./filters";
import { fit, render } from "./render";
import { state, toAppEvent } from "./state";

let seq = 0;

/** Re-renders the legend and the grid from the current state. */
export function renderAll(): void {
  buildChips();
  render();
}

function setBanner(on: boolean): void {
  byId("banner").hidden = !on;
  fit();
}

/** Fetches the events of `state.year`. Stale responses (fast year navigation) are ignored. */
export async function loadYear(): Promise<void> {
  const mine = ++seq;
  try {
    const r = await getEvents(state.year);
    if (mine !== seq) return;
    state.events = r.events.map(toAppEvent);
    setBanner(false);
  } catch (e) {
    if (mine !== seq) return;
    if (e instanceof ApiError && e.status === 401) return;
    state.events = [];
    setBanner(true);
  }
  renderAll();
}

/** Switches year: empty grid immediately, events fill in when loaded. */
export function gotoYear(y: number): Promise<void> {
  state.year = y;
  state.events = [];
  renderAll();
  return loadYear();
}
