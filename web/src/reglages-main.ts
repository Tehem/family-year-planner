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
import { byId, h } from "./dom";
import { fmtDateTime } from "./fmt";

let tz = "Europe/Paris";

const errText = (e: unknown): string =>
  e instanceof ApiError ? e.message : "Une erreur est survenue. Réessayez dans un instant.";

function showError(id: string, e: unknown | null): void {
  const el = byId(id);
  el.textContent = e === null ? "" : errText(e);
  el.hidden = e === null;
}

// ---------- small builders ----------

function table(label: string, headers: string[], rows: Node[][], empty: string): HTMLElement {
  if (rows.length === 0) return h("p", "muted", empty);
  const t = h("table", "tbl");
  t.setAttribute("aria-label", label);
  const head = h("tr");
  for (const text of headers) {
    const th = h("th");
    th.scope = "col";
    if (text) th.textContent = text;
    else th.append(h("span", "sr", "Actions"));
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
  t.append(thead, tbody);
  return t;
}

const text = (s: string): Text => document.createTextNode(s);
const when = (iso: string | null): Text => text(iso ? fmtDateTime(iso, tz) : "Jamais");

function revokeButton(name: string, action: () => Promise<void>): HTMLButtonElement {
  const b = h("button", "btn ghost sm", "Révoquer");
  b.type = "button";
  b.setAttribute("aria-label", `Révoquer ${name}`);
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
  const btn = h("button", "btn", "Copier");
  btn.type = "button";
  let timer = 0;
  btn.addEventListener("click", async () => {
    const ok = await copyText(input);
    btn.textContent = ok ? "Copié" : "Copie impossible";
    window.clearTimeout(timer);
    timer = window.setTimeout(() => (btn.textContent = "Copier"), 2500);
  });
  row.append(input, btn);
  return row;
}

function secretBox(label: string, value: string, warning: string): HTMLElement {
  const box = h("div", "secret");
  box.setAttribute("role", "group");
  box.setAttribute("aria-label", "Nouveau secret");
  box.append(h("p", "secret-warn", warning), copyRow(label, value));
  return box;
}

// ---------- revoke confirmation ----------

let pendingRevoke: (() => Promise<void>) | null = null;

function confirmRevoke(name: string, action: () => Promise<void>): void {
  pendingRevoke = action;
  byId("confirmText").textContent = `Révoquer l’accès de « ${name} » ?`;
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

const SCOPE_LABEL: Record<Scope, string> = { read: "Lecture seule", read_write: "Lecture et écriture" };

function renderTokens(tokens: Token[]): void {
  const rows = tokens.map((t) => {
    const name = h("span", "tname");
    name.append(text(t.name));
    if (t.kiosk) name.append(h("span", "badge", "Kiosque"));
    return [
      name,
      text(SCOPE_LABEL[t.scope] ?? t.scope),
      when(t.created_at),
      when(t.last_used_at),
      revokeButton(t.name, async () => {
        await revokeToken(t.id);
        await reloadTokens();
      }),
    ];
  });
  byId("tokens").replaceChildren(
    table("Jetons personnels", ["Nom", "Droits", "Créé le", "Dernière utilisation", ""], rows, "Aucun jeton pour l’instant."),
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
      showError("tokenErr", new ApiError(400, "validation_failed", "Donnez un nom au jeton."));
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
        secretBox("Jeton", created.token, "Copiez ce jeton maintenant : il ne sera plus affiché."),
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
        secretBox("Lien kiosque", url, "Copiez ce lien maintenant : il ne sera plus affiché."),
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
    table("Connexions OAuth", ["Application", "Redirection", "Autorisé le", "Dernière utilisation", ""], rows, "Aucune application connectée."),
  );
}

async function reloadGrants(): Promise<void> {
  renderGrants(await listGrants());
}

// ---------- imports ----------

let pollTimer = 0;

const RUN_LABEL: Record<ImportRun["status"], string> = { success: "Réussi", failed: "Échec", running: "En cours" };

function renderRuns(runs: ImportRun[]): void {
  const rows = runs.map((r) => {
    const status = h("span", `run-${r.status}`, RUN_LABEL[r.status] ?? r.status);
    return [
      text(fmtDateTime(r.started_at, tz)),
      status,
      text(String(r.created)),
      text(String(r.updated)),
      text(String(r.deleted)),
      text(String(r.unchanged)),
      text(r.error ?? ""),
    ];
  });
  byId("runs").replaceChildren(
    table(
      "Derniers imports",
      ["Date", "Résultat", "Créés", "Mis à jour", "Supprimés", "Inchangés", "Erreur"],
      rows,
      "Aucun import pour l’instant.",
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
    if (s.running) status.textContent = "Import en cours…";
    else if (last) status.textContent = `Dernier import : ${fmtDateTime(last.started_at, tz)}.`;
    else status.textContent = "Aucun import pour l’instant.";
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
    byId("importStatus").textContent = "Import en cours…";
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
  initConfirm();
  initTokenForm();
  initKiosk();
  initImports();
  try {
    const cfg = await getConfig();
    tz = cfg.timezone;
    byId("mcp").replaceChildren(copyRow("Adresse du serveur MCP", cfg.mcp_url));
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
