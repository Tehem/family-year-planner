// Réglages page (SPEC §8.9): agent tokens, OAuth connections, kiosk link, imports.
import "./styles.css";
import {
  ApiError,
  createToken,
  getConfig,
  getImports,
  listGrants,
  listTokens,
  revokeGrant,
  revokeToken,
  startImport,
  type Grant,
  type ImportRun,
  type Scope,
  type Token,
} from "./api";
import { copyText } from "./clipboard";
import { applyTypeColors } from "./colors";
import { byId, h } from "./dom";
import { fmtDateTime } from "./fmt";
import { adoptLang, applyStatic, mountLangSwitch, t } from "./i18n";
import { initTypesEditor } from "./types-editor";

let tz = "Europe/Paris";

const errText = (e: unknown): string =>
  e instanceof ApiError ? e.message : t("err.generic");

function showError(id: string, e: unknown | null): void {
  const el = byId(id);
  el.textContent = e === null ? "" : errText(e);
  el.hidden = e === null;
}

// ---------- small builders ----------

function table(label: string, headers: string[], rows: Node[][], empty: string): HTMLElement {
  if (rows.length === 0) return h("p", "muted", empty);
  const tbl = h("table", "tbl");
  tbl.setAttribute("aria-label", label);
  const head = h("tr");
  for (const text of headers) {
    const th = h("th");
    th.scope = "col";
    if (text) th.textContent = text;
    else th.append(h("span", "sr", t("set.actions")));
    head.append(th);
  }
  const thead = h("thead");
  thead.append(head);
  const tbody = h("tbody");
  for (const cells of rows) {
    const tr = h("tr");
    for (const c of cells) {
      const td = h("td");
      td.append(c);
      tr.append(td);
    }
    tbody.append(tr);
  }
  tbl.append(thead, tbody);
  return tbl;
}

const text = (s: string): Text => document.createTextNode(s);
const when = (iso: string | null): Text => text(iso ? fmtDateTime(iso, tz) : t("fmt.never"));

function revokeButton(name: string, action: () => Promise<void>): HTMLButtonElement {
  const b = h("button", "btn ghost sm", t("set.revoke"));
  b.type = "button";
  b.setAttribute("aria-label", t("set.revokeAria", { name }));
  b.addEventListener("click", () => confirmRevoke(name, action));
  return b;
}

/** A read-only field with a `Copier` button. The input doubles as the copy fallback (selected on copy). */
function copyRow(label: string, value: string, cls = ""): HTMLElement {
  const row = h("div", `copyrow ${cls}`.trim());
  const input = h("input", "copyval");
  input.readOnly = true;
  input.value = value;
  input.setAttribute("aria-label", label);
  input.addEventListener("focus", () => input.select());
  const btn = h("button", "btn", t("btn.copy"));
  btn.type = "button";
  let timer = 0;
  btn.addEventListener("click", async () => {
    const ok = await copyText(input);
    btn.textContent = ok ? t("btn.copied") : t("btn.copyFailed");
    window.clearTimeout(timer);
    timer = window.setTimeout(() => (btn.textContent = t("btn.copy")), 2500);
  });
  row.append(input, btn);
  return row;
}

function secretBox(label: string, value: string, warning: string): HTMLElement {
  const box = h("div", "secret");
  box.setAttribute("role", "group");
  box.setAttribute("aria-label", t("set.newSecret"));
  box.append(h("p", "secret-warn", warning), copyRow(label, value));
  return box;
}

// ---------- revoke confirmation ----------

let pendingRevoke: (() => Promise<void>) | null = null;

function confirmRevoke(name: string, action: () => Promise<void>): void {
  pendingRevoke = action;
  byId("confirmText").textContent = t("set.revokeQ", { name });
  byId("confirmErr").textContent = "";
  (byId("confirmDlg") as HTMLDialogElement).showModal();
  byId("confirmNo").focus();
}

function initConfirm(): void {
  const dlg = byId("confirmDlg") as HTMLDialogElement;
  byId("confirmNo").addEventListener("click", () => dlg.close());
  dlg.addEventListener("close", () => (pendingRevoke = null));
  byId("confirmForm").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const action = pendingRevoke;
    if (!action) return;
    const yes = byId("confirmYes") as HTMLButtonElement;
    yes.disabled = true;
    try {
      await action();
      dlg.close();
    } catch (e) {
      byId("confirmErr").textContent = errText(e);
    } finally {
      yes.disabled = false;
    }
  });
}

// ---------- tokens ----------

const scopeLabel = (s: Scope): string => t(s === "read" ? "set.tokens.read" : "set.tokens.readWrite");

function renderTokens(tokens: Token[]): void {
  const rows = tokens.map((tk) => {
    const name = h("span", "tname");
    name.append(text(tk.name));
    if (tk.kiosk) name.append(h("span", "badge", t("set.kioskBadge")));
    return [
      name,
      text(scopeLabel(tk.scope)),
      when(tk.created_at),
      when(tk.last_used_at),
      revokeButton(tk.name, async () => {
        await revokeToken(tk.id);
        await reloadTokens();
      }),
    ];
  });
  byId("tokens").replaceChildren(
    table(
      t("set.tokens.table"),
      [t("set.tokens.name"), t("set.tokens.rights"), t("set.tokens.created"), t("set.tokens.lastUsed"), ""],
      rows,
      t("set.tokens.empty"),
    ),
  );
}

async function reloadTokens(): Promise<void> {
  renderTokens(await listTokens());
}

function initTokenForm(): void {
  const form = byId("tokenForm") as HTMLFormElement;
  const input = byId("tokenName") as HTMLInputElement;
  form.addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const name = input.value.trim();
    if (!name) {
      showError("tokenErr", new ApiError(400, "validation_failed", t("set.tokens.nameRequired")));
      input.focus();
      return;
    }
    const scope = (form.elements.namedItem("scope") as RadioNodeList).value as Scope;
    const btn = byId("tokenSubmit") as HTMLButtonElement;
    btn.disabled = true;
    showError("tokenErr", null);
    try {
      const created = await createToken({ name, scope });
      byId("newToken").replaceChildren(
        secretBox(t("set.tokens.secretLabel"), created.token, t("set.tokens.once")),
      );
      input.value = "";
      await reloadTokens();
    } catch (e) {
      showError("tokenErr", e);
    } finally {
      btn.disabled = false;
    }
  });
}

function initKiosk(): void {
  byId("kioskCreate").addEventListener("click", async () => {
    const btn = byId("kioskCreate") as HTMLButtonElement;
    btn.disabled = true;
    showError("kioskErr", null);
    try {
      const created = await createToken({ name: "", kiosk: true });
      const url = created.kiosk_url ?? "";
      byId("newKiosk").replaceChildren(
        secretBox(t("set.kiosk.label"), url, t("set.kiosk.once")),
      );
      await reloadTokens();
    } catch (e) {
      showError("kioskErr", e);
    } finally {
      btn.disabled = false;
    }
  });
}

// ---------- OAuth grants ----------

function renderGrants(grants: Grant[]): void {
  const rows = grants.map((g) => [
    text(g.client_name),
    text(g.redirect_host),
    when(g.created_at),
    when(g.last_used_at),
    revokeButton(g.client_name, async () => {
      await revokeGrant(g.id);
      await reloadGrants();
    }),
  ]);
  byId("grants").replaceChildren(
    table(
      t("set.grants"),
      [t("set.grants.app"), t("set.grants.redirect"), t("set.grants.authorized"), t("set.tokens.lastUsed"), ""],
      rows,
      t("set.grants.empty"),
    ),
  );
}

async function reloadGrants(): Promise<void> {
  renderGrants(await listGrants());
}

// ---------- imports ----------

let pollTimer = 0;

/** A run that never started (a category migration was running) is stored as `skipped:<reason>`. */
const isSkipped = (r: ImportRun): boolean => r.status === "failed" && (r.error ?? "").startsWith("skipped:");
const runLabel = (r: ImportRun): string =>
  isSkipped(r)
    ? t("set.imports.skipped")
    : t(r.status === "success" ? "set.imports.success" : r.status === "failed" ? "set.imports.failed" : "set.imports.runningStatus");
const runError = (r: ImportRun): string =>
  isSkipped(r) ? t("set.imports.skipped.migrating") : (r.error ?? "");

function renderRuns(runs: ImportRun[]): void {
  const rows = runs.map((r) => {
    const status = h("span", `run-${isSkipped(r) ? "skipped" : r.status}`, runLabel(r));
    return [
      text(fmtDateTime(r.started_at, tz)),
      status,
      text(String(r.created)),
      text(String(r.updated)),
      text(String(r.deleted)),
      text(String(r.unchanged)),
      text(runError(r)),
    ];
  });
  byId("runs").replaceChildren(
    table(
      t("set.imports.table"),
      [
        t("set.imports.date"),
        t("set.imports.result"),
        t("set.imports.created"),
        t("set.imports.updated"),
        t("set.imports.deleted"),
        t("set.imports.unchanged"),
        t("set.imports.error"),
      ],
      rows,
      t("set.imports.none"),
    ),
  );
}

/** Reads the import status, repaints, and keeps polling every second while an import runs. */
async function refreshImports(): Promise<void> {
  window.clearTimeout(pollTimer);
  try {
    const s = await getImports();
    renderRuns(s.runs);
    const last = s.runs[0];
    const status = byId("importStatus");
    if (s.running) status.textContent = t("set.imports.running");
    else if (last) status.textContent = t("set.imports.last", { date: fmtDateTime(last.started_at, tz) });
    else status.textContent = t("set.imports.none");
    (byId("importNow") as HTMLButtonElement).disabled = s.running;
    showError("importErr", null);
    if (s.running) pollTimer = window.setTimeout(() => void refreshImports(), 1000);
  } catch (e) {
    showError("importErr", e);
    pollTimer = window.setTimeout(() => void refreshImports(), 3000);
  }
}

function initImports(): void {
  byId("importNow").addEventListener("click", async () => {
    const btn = byId("importNow") as HTMLButtonElement;
    btn.disabled = true;
    byId("importStatus").textContent = t("set.imports.running");
    showError("importErr", null);
    try {
      await startImport();
    } catch (e) {
      showError("importErr", e);
    }
    await refreshImports();
  });
}

// ---------- boot ----------

async function boot(): Promise<void> {
  applyStatic();
  mountLangSwitch(byId("langSwitch"));
  initConfirm();
  initTokenForm();
  initKiosk();
  initImports();
  try {
    const cfg = await getConfig();
    adoptLang(cfg.lang);
    applyTypeColors(cfg.types);
    tz = cfg.timezone;
    byId("mcp").replaceChildren(copyRow(t("set.mcp"), cfg.mcp_url));
    void initTypesEditor(errText);
    const [tokens, grants] = await Promise.all([listTokens(), listGrants()]);
    renderTokens(tokens);
    renderGrants(grants);
  } catch (e) {
    if (!(e instanceof ApiError && e.status === 401)) {
      const el = byId("pageErr");
      el.textContent = errText(e);
      el.hidden = false;
    }
  }
  await refreshImports();
}

void boot();
