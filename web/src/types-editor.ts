// Catégories section of Réglages (SPEC §19.9): rename types, pick their Google color, add types.
import {
  ApiError,
  getConfig,
  getTypes,
  putTypes,
  resumeTypes,
  type EditableType,
  type PaletteEntry,
  type TypeChange,
  type TypesEditor,
} from "./api";
import { applyTypeColors } from "./colors";
import { byId, h } from "./dom";
import { getLang, t, tIn, type Lang } from "./i18n";

let server: TypesEditor;
let draft: EditableType[] = [];
let errText: (e: unknown) => string = (e) => (e instanceof ApiError ? e.message : t("err.generic"));

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
const LANGS: Lang[] = ["fr", "en"];

const colorName = (p: PaletteEntry | undefined): string => (p ? (getLang() === "en" ? p.name_en : p.name_fr) : "");
const palette = (id: string | null): PaletteEntry | undefined => server.palette.find((p) => p.color_id === id);
const nameOf = (ty: EditableType): string => ty.labels[getLang()].singular || ty.code;

function swatchFor(color: string | null): HTMLElement {
  const s = h("span", "sw tint");
  s.setAttribute("aria-hidden", "true");
  s.style.setProperty("--cc", color ? `var(--p${color}-bar)` : "var(--c-unclassified)");
  return s;
}

/** Colors used twice in the draft: `code → name of the other type`. */
function duplicates(): Map<string, string> {
  const seen = new Map<string, EditableType>();
  const dup = new Map<string, string>();
  for (const ty of draft) {
    if (!ty.color_id) continue;
    const other = seen.get(ty.color_id);
    if (other) {
      dup.set(ty.code, nameOf(other));
      dup.set(other.code, nameOf(ty));
    } else seen.set(ty.color_id, ty);
  }
  return dup;
}

function namesOk(ty: EditableType): boolean {
  return LANGS.every((l) => [ty.labels[l].singular, ty.labels[l].plural].every((v) => v.trim() !== "" && v.length <= server.max_name));
}

/** A slot (`custom_1`…) is reusable only when neither the draft nor the saved catalog holds it: a
 *  deleted type and a new one sharing a code would read as one renamed type (SPEC §19.5). */
function freeSlot(): number | undefined {
  const taken = new Set([...draft, ...server.types].map((d) => d.code));
  return [1, 2, 3].find((n) => !taken.has(`custom_${n}`));
}

function freeColors(): string[] {
  const used = new Set(draft.map((d) => d.color_id));
  return server.palette.map((p) => p.color_id).filter((c) => !used.has(c));
}

const plain = ({ code, color_id, labels }: EditableType): EditableType => ({ code, color_id, labels });

function dirty(): boolean {
  return JSON.stringify(draft.map(plain)) !== JSON.stringify(server.types.map(plain));
}

function valid(): boolean {
  return draft.every(namesOk) && duplicates().size === 0;
}

// ---------- rendering ----------

function nameField(ty: EditableType, lang: Lang, kind: "singular" | "plural"): HTMLElement {
  const label = h("label", "f");
  label.append(h("span", undefined, `${lang.toUpperCase()} · ${t(kind === "singular" ? "types.singular" : "types.plural")}`));
  const input = h("input");
  input.maxLength = server.max_name;
  input.value = ty.labels[lang][kind];
  input.dataset["code"] = ty.code;
  input.dataset["lang"] = lang;
  input.dataset["kind"] = kind;
  input.lang = lang;
  input.addEventListener("input", () => {
    ty.labels[lang][kind] = input.value;
    refresh();
  });
  label.append(input);
  return label;
}

function card(ty: EditableType): HTMLElement {
  const box = h("fieldset", "type-card");
  box.dataset["code"] = ty.code;
  const legend = h("legend");
  const sw = swatchFor(ty.color_id);
  legend.append(sw, h("span", "tname", nameOf(ty)), h("span", "badge soft", t(ty.custom ? "types.custom" : "types.builtin")));
  box.append(legend);

  const grid = h("div", "type-names");
  for (const lang of LANGS) {
    grid.append(nameField(ty, lang, "singular"), nameField(ty, lang, "plural"));
  }
  box.append(grid);

  if (ty.code !== "unclassified") {
    const wrap = h("label", "f type-color");
    wrap.append(h("span", undefined, t("types.color")));
    const select = h("select");
    select.dataset["code"] = ty.code;
    for (const p of server.palette) {
      const o = h("option", undefined, colorName(p));
      o.value = p.color_id;
      select.append(o);
    }
    select.value = ty.color_id ?? "";
    select.addEventListener("change", () => {
      ty.color_id = select.value;
      sw.style.setProperty("--cc", `var(--p${select.value}-bar)`);
      refresh();
    });
    wrap.append(select);
    const msg = h("span", "err type-dup");
    msg.dataset["code"] = ty.code;
    msg.setAttribute("role", "alert");
    wrap.append(msg);
    box.append(wrap);
  } else {
    box.append(h("p", "muted", t("types.noColor")));
  }

  if (ty.custom) {
    const del = h("button", "btn ghost sm", t("types.delete"));
    del.type = "button";
    del.setAttribute("aria-label", `${t("types.delete")} : ${nameOf(ty)}`);
    del.addEventListener("click", () => {
      draft = draft.filter((d) => d.code !== ty.code);
      render();
    });
    box.append(del);
  }
  return box;
}

function render(): void {
  const host = byId("typesEditor");
  const customs = draft.filter((d) => d.custom).length;
  const cards = draft.map(card);

  const add = h("button", "btn ghost", t("types.add"));
  add.type = "button";
  add.id = "typesAdd";
  const free = freeColors();
  const full = customs >= server.max_custom;
  add.disabled = full || free.length === 0 || freeSlot() === undefined;
  add.addEventListener("click", addType);
  const note = h(
    "p",
    "muted",
    full
      ? t("types.add.max")
      : free.length === 0
        ? t("types.add.none")
        : freeSlot() === undefined
          ? t("types.add.saveFirst")
          : t("types.add.free", { n: free.length }),
  );

  const save = h("button", "btn", t("types.save"));
  save.type = "button";
  save.id = "typesSave";
  save.addEventListener("click", () => void save_());
  const reset = h("button", "btn ghost", t("types.reset"));
  reset.type = "button";
  reset.id = "typesReset";
  reset.addEventListener("click", () => {
    draft = clone(server.types.map(({ code, custom, color_id, labels }) => ({ code, custom, color_id, labels })));
    status();
    render();
  });
  const err = h("p", "err");
  err.id = "typesErr";
  err.setAttribute("role", "alert");
  const ok = h("p", "ok");
  ok.id = "typesOk";
  ok.setAttribute("role", "status");
  const actions = h("div", "type-actions");
  actions.append(save, reset);

  host.replaceChildren(...cards, add, note, actions, err, ok);
  refresh();
}

/** Brings validity messages and the buttons in line with the draft (no DOM rebuild: focus survives). */
function refresh(): void {
  const dup = duplicates();
  document.querySelectorAll<HTMLElement>(".type-dup").forEach((el) => {
    const other = dup.get(el.dataset["code"] ?? "");
    el.textContent = other ? t("types.dup", { name: other }) : "";
    const select = el.parentElement?.querySelector("select");
    if (select) select.setAttribute("aria-invalid", String(Boolean(other)));
  });
  document.querySelectorAll<HTMLElement>(".type-card").forEach((box) => {
    const ty = draft.find((d) => d.code === box.dataset["code"]);
    const name = box.querySelector<HTMLElement>(".tname");
    if (ty && name) name.textContent = nameOf(ty);
    box.querySelectorAll<HTMLElement>("input").forEach((i) => i.setAttribute("aria-invalid", String(i instanceof HTMLInputElement && i.value.trim() === "")));
  });
  const save = byId<HTMLButtonElement>("typesSave");
  const reset = byId<HTMLButtonElement>("typesReset");
  save.disabled = !dirty();
  reset.disabled = !dirty();
  const err = byId("typesErr");
  if (dirty() && !valid()) err.textContent = draft.every(namesOk) ? t("types.invalid") : t("types.nameRequired");
  else if (err.dataset["server"] !== "1") err.textContent = "";
}

function status(error?: string, ok?: string): void {
  const err = document.getElementById("typesErr");
  const okEl = document.getElementById("typesOk");
  if (err) {
    err.textContent = error ?? "";
    err.dataset["server"] = error ? "1" : "";
  }
  if (okEl) okEl.textContent = ok ?? "";
}

function addType(): void {
  const slot = freeSlot();
  const color = freeColors()[0];
  if (!slot || !color) return;
  draft.push({
    code: `custom_${slot}`,
    custom: true,
    color_id: color,
    labels: {
      fr: { singular: tIn("fr", "types.newName"), plural: tIn("fr", "types.newName") },
      en: { singular: tIn("en", "types.newName"), plural: tIn("en", "types.newName") },
    },
  });
  render();
  document.querySelector<HTMLInputElement>(`.type-card[data-code="custom_${slot}"] input`)?.focus();
}

// ---------- saving ----------

function describe(changes: TypeChange[], orphans: number): string[] {
  const lines = [t("types.confirm.changes", { n: changes.reduce((n, c) => n + c.events, 0) })];
  for (const c of changes) {
    const ty = server.types.find((s) => s.code === c.code);
    const name = ty ? nameOf(ty) : c.code;
    if (c.to_color_id === null) lines.push(t("types.confirm.delete", { name, n: c.events }));
    else
      lines.push(
        t("types.confirm.change", {
          name,
          n: c.events,
          from: colorName(palette(c.from_color_id)) || t("types.none"),
          to: colorName(palette(c.to_color_id)) || t("types.none"),
        }),
      );
  }
  if (orphans > 0) lines.push(t("types.confirm.orphans", { n: orphans }));
  return lines;
}

async function reload(message?: string): Promise<void> {
  server = await getTypes();
  draft = clone(server.types.map(({ code, custom, color_id, labels }) => ({ code, custom, color_id, labels })));
  try {
    const cfg = await getConfig();
    applyTypeColors(cfg.types);
  } catch {
    /* the colors refresh on the next page load */
  }
  renderMigration();
  render();
  status(undefined, message);
}

/** Refreshes the banner of an unfinished migration without touching the draft. */
async function refreshMigration(): Promise<void> {
  try {
    server.migration = (await getTypes()).migration;
    renderMigration();
  } catch {
    /* the banner refreshes on the next load */
  }
}

async function save_(): Promise<void> {
  if (!dirty() || !valid()) return;
  const btn = byId<HTMLButtonElement>("typesSave");
  btn.disabled = true;
  status();
  try {
    const preview = await putTypes(draft, true, server.revision);
    // Nothing to confirm unless an existing event is concerned (a new or unused type has none).
    const concerned = preview.changes.reduce((n, c) => n + c.events, 0) + preview.orphans;
    if (concerned === 0) {
      await putTypes(draft, false, server.revision);
      await reload(t("types.saved"));
      return;
    }
    confirmApply(preview.changes, preview.orphans);
  } catch (e) {
    status(errText(e));
  } finally {
    btn.disabled = !dirty();
  }
}

function confirmApply(changes: TypeChange[], orphans: number): void {
  const dlg = byId<HTMLDialogElement>("typesDlg");
  byId("typesDlgList").replaceChildren(...describe(changes, orphans).map((line) => h("li", undefined, line)));
  byId("typesDlgErr").textContent = "";
  dlg.showModal();
  byId("typesDlgNo").focus();
}

function initDialog(): void {
  const dlg = byId<HTMLDialogElement>("typesDlg");
  byId("typesDlgNo").addEventListener("click", () => dlg.close());
  byId("typesForm").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const yes = byId<HTMLButtonElement>("typesDlgYes");
    yes.disabled = true;
    try {
      await putTypes(draft, false, server.revision);
      dlg.close();
      await reload(t("types.saved"));
    } catch (e) {
      byId("typesDlgErr").textContent = errText(e);
      // A failed migration blocks editing until resumed: show it, but keep the unsaved edits.
      void refreshMigration();
    } finally {
      yes.disabled = false;
    }
  });
}

function renderMigration(): void {
  const host = byId("typesMigration");
  const skipped = server.skipped
    ? h("p", "muted", t("types.skipped", { n: String(server.skipped.count) }))
    : undefined;
  if (skipped) {
    skipped.id = "typesSkipped";
    skipped.setAttribute("role", "status");
    skipped.title = server.skipped?.ids.join(", ") ?? "";
  }
  if (!server.migration) {
    host.replaceChildren(...(skipped ? [skipped] : []));
    return;
  }
  const box = h("div", "banner");
  box.setAttribute("role", "alert");
  const text = h("span", undefined, t("types.migration"));
  box.append(text);
  if (server.migration.error) box.append(h("span", "muted", t("types.migration.error", { error: server.migration.error })));
  const resume = h("button", "btn sm", t("types.migration.resume"));
  resume.type = "button";
  resume.id = "typesResume";
  resume.addEventListener("click", async () => {
    resume.disabled = true;
    try {
      await resumeTypes();
      await reload(t("types.saved"));
    } catch (e) {
      status(errText(e));
      resume.disabled = false;
    }
  });
  box.append(resume);
  host.replaceChildren(box, ...(skipped ? [skipped] : []));
}

export async function initTypesEditor(errorText: (e: unknown) => string): Promise<void> {
  errText = errorText;
  initDialog();
  try {
    await reload();
  } catch (e) {
    byId("typesEditor").replaceChildren(h("p", "err", errText(e)));
  }
}
