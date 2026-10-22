// Kiosk page (SPEC §9): a wall tablet showing the year, read-only, always fresh.
import "./styles.css";
import { ApiError, getConfig, getEvents, setLoginUrl, type ApiEvent, type Config } from "./api";
import { applyTypeColors } from "./colors";
import { byId, h, swatch } from "./dom";
import { adoptLang, applyStatic, t } from "./i18n";
import { localDay, fmtTime } from "./fmt";
import { closePanel, openDay } from "./panel";
import { fit, render } from "./render";
import { hasUnclassified, legendCats, state, toAppEvent, typeLabelPlural } from "./state";

const PANEL_MS = 20_000;
const CURSOR_MS = 5_000;
const OFFLINE_RETRY_MS = 30_000;
const TICK_MS = 30_000;
const RELOAD_HOUR = 3;

let tz = "Europe/Paris";
let offlineSince: Date | null = null;
let shownSig = "";
let inflight = false;
let refreshTimer = 0;
let panelTimer = 0;

// ---------- painting ----------

function buildLegend(): void {
  const cats = hasUnclassified() ? [...legendCats(), "unclassified"] : legendCats();
  const chips = cats.map((k) => {
    const c = h("span", "chip static");
    c.setAttribute("role", "listitem");
    c.append(swatch(k), document.createTextNode(typeLabelPlural(k)));
    return c;
  });
  const both = h("span", "chip static");
  both.setAttribute("role", "listitem");
  both.append(swatch("both"), document.createTextNode(t("both.name")));
  byId("legend").replaceChildren(...chips, both);
}

/** Re-renders the legend and the grid in one go (the canvas is replaced atomically: no blank frame). */
function paint(): void {
  buildLegend();
  render();
  // Bars are not controls here: no focus stop, no button role.
  document.querySelectorAll<HTMLElement>(".bar").forEach((b) => {
    b.removeAttribute("tabindex");
    b.removeAttribute("role");
  });
}

function setOffline(since: Date | null): void {
  offlineSince = since;
  const el = byId("offline");
  el.hidden = since === null;
  el.textContent = since ? t("kiosk.offline", { time: fmtTime(since, tz) }) : "";
}

// ---------- data ----------

function applyConfig(cfg: Config): void {
  state.config = cfg;
  adoptLang(cfg.lang);
  applyTypeColors(cfg.types);
  tz = cfg.timezone;
  state.year = +cfg.today.slice(0, 4);
}

/** Fetches config (today) then the events of today's year. Keeps the screen as is on failure. */
async function refresh(): Promise<boolean> {
  if (inflight) return offlineSince === null;
  inflight = true;
  window.clearTimeout(refreshTimer);
  let ok = false;
  try {
    const cfg = await getConfig();
    const year = +cfg.today.slice(0, 4);
    const r = await getEvents(year);
    applyConfig(cfg);
    const events: ApiEvent[] = r.events;
    const sig = JSON.stringify([cfg.today, state.view, events, cfg.types]);
    if (sig !== shownSig) {
      shownSig = sig;
      state.events = events.map(toAppEvent);
      paint();
    }
    setOffline(null);
    ok = true;
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) return false;
    setOffline(offlineSince ?? new Date());
  } finally {
    inflight = false;
    const minutes = state.config?.kiosk.refresh_minutes ?? 5;
    refreshTimer = window.setTimeout(() => void refresh(), ok ? minutes * 60_000 : OFFLINE_RETRY_MS);
  }
  return ok;
}

// ---------- time: midnight, 03:00 reload ----------

let lastDay = "";
let reloadAt = 0;

function nextReload(now: Date): number {
  const { secs } = localDay(now, tz);
  let wait = RELOAD_HOUR * 3600 - secs;
  if (wait <= 0) wait += 86400;
  return now.getTime() + wait * 1000;
}

async function tick(): Promise<void> {
  const now = new Date();
  if (reloadAt && now.getTime() >= reloadAt) {
    window.location.reload();
    return;
  }
  const { day, secs } = localDay(now, tz);
  if (day !== lastDay) {
    lastDay = day;
    const ok = await refresh();
    if (!ok && state.config && state.config.today < day) {
      // Offline at midnight: the ring still moves, by the tablet's own clock.
      applyConfig({ ...state.config, today: day });
      shownSig = "";
      paint();
    }
  }
  // Check again at the next midnight at the latest, and every 30 s otherwise.
  const toMidnight = (86400 - secs) * 1000 + 500;
  window.setTimeout(() => void tick(), Math.min(TICK_MS, toMidnight));
}

// ---------- day panel (read-only) ----------

function cellAt(x: number, y: number): HTMLElement | null {
  for (const el of document.elementsFromPoint(x, y)) if (el instanceof HTMLElement && el.classList.contains("cell")) return el;
  return null;
}

function closeDay(): void {
  window.clearTimeout(panelTimer);
  closePanel();
}

function initTaps(): void {
  byId("panel").inert = true;
  document.addEventListener("click", (e) => {
    const inPanel = (e.target as Element).closest("#panel");
    if (inPanel) return;
    const d = cellAt(e.clientX, e.clientY)?.dataset["d"];
    if (!d) {
      closeDay();
      return;
    }
    openDay(d, true);
    window.clearTimeout(panelTimer);
    panelTimer = window.setTimeout(closeDay, PANEL_MS);
  });
}

// ---------- device care ----------

let sentinel: WakeLockSentinel | null = null;

async function keepAwake(): Promise<void> {
  if (!("wakeLock" in navigator) || document.visibilityState !== "visible" || sentinel) return;
  try {
    const s = await navigator.wakeLock.request("screen");
    sentinel = s;
    s.addEventListener("release", () => {
      if (sentinel === s) sentinel = null;
    });
  } catch {
    /* refused (battery saver, permissions): the screen may dim, nothing else to do */
  }
}

function initCursor(): void {
  let timer = 0;
  const arm = (): void => {
    document.body.classList.remove("idle");
    window.clearTimeout(timer);
    timer = window.setTimeout(() => document.body.classList.add("idle"), CURSOR_MS);
  };
  window.addEventListener("pointermove", arm);
  arm();
}

// ---------- boot ----------

function pickView(cfg: Config): "week" | "month" {
  const v = new URLSearchParams(window.location.search).get("view");
  if (v === "week" || v === "month") return v;
  return cfg.kiosk.default_view === "month" ? "month" : "week";
}

async function boot(): Promise<void> {
  applyStatic();
  setLoginUrl("/connexion?error=kiosk");
  initTaps();
  initCursor();
  void keepAwake();
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState !== "visible") sentinel = null;
    else void keepAwake();
  });
  window.addEventListener("resize", fit);
  window.addEventListener("online", () => void refresh());
  void document.fonts?.ready.then(fit);

  // The view needs the config's default; the first refresh also paints.
  for (;;) {
    try {
      const cfg = await getConfig();
      applyConfig(cfg);
      state.view = pickView(cfg);
      break;
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) return;
      setOffline(offlineSince ?? new Date());
      await new Promise((r) => window.setTimeout(r, OFFLINE_RETRY_MS));
    }
  }
  await refresh();
  const now = new Date();
  lastDay = localDay(now, tz).day;
  reloadAt = nextReload(now);
  void tick();
}

void boot();
