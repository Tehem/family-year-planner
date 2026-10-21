import "./styles.css";
import { ApiError, getConfig, getMe, resetDemo } from "./api";
import { gotoYear, loadYear, renderAll } from "./app";
import { applyTypeColors } from "./colors";
import { initDialog, openAddToday } from "./dialog";
import { byId } from "./dom";
import { initFilters } from "./filters";
import { adoptLang, applyStatic, mountLangSwitch } from "./i18n";
import { initInteractions } from "./interact";
import { closePanel, initPanel } from "./panel";
import { initPrint } from "./print";
import { fit } from "./render";
import { loadView, state } from "./state";

async function boot(): Promise<void> {
  try {
    const [me, config] = await Promise.all([getMe(), getConfig()]);
    state.me = me;
    state.config = config;
    adoptLang(config.lang);
    applyTypeColors(config.types);
    // The server clock (fixed in tests) decides which year is "now".
    state.year = +config.today.slice(0, 4);
  } catch (e) {
    if (!(e instanceof ApiError && e.status === 401)) byId("banner").hidden = false;
    fit();
    return;
  }
  byId("reset").hidden = !state.me?.demo;
  byId("banner").hidden = true;
  renderAll();
  await loadYear();
}

function init(): void {
  applyStatic();
  mountLangSwitch(byId("langSwitch"));
  state.view = loadView();
  initFilters(renderAll);
  initPanel();
  initDialog();
  initInteractions();
  initPrint();

  byId("prev").addEventListener("click", () => {
    closePanel();
    void gotoYear(state.year - 1);
  });
  byId("next").addEventListener("click", () => {
    closePanel();
    void gotoYear(state.year + 1);
  });
  byId("addBtn").addEventListener("click", openAddToday);
  byId("reset").addEventListener("click", async () => {
    try {
      await resetDemo();
    } catch {
      /* the reload below shows the banner if the server is down */
    }
    closePanel();
    await loadYear();
  });
  byId("retry").addEventListener("click", () => void (state.config ? loadYear() : boot()));
  window.addEventListener("resize", fit);
  void document.fonts?.ready.then(fit);

  // Header and empty grid immediately; events fill in when loaded.
  renderAll();
  void boot();
}

init();
