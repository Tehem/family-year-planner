// Side panel: day view and event view (details, edit, delete with confirmation).
import { ApiError, deleteEvent } from "./api";
import { loadYear } from "./app";
import { openCreate, openEdit } from "./dialog";
import { byId, h, swatch } from "./dom";
import { hideTip } from "./tooltip";
import { cap, fmtLong, fmtRange, span } from "./dates";
import { t } from "./i18n";
import { findEvent, members, state, typeLabel, type AppEvent } from "./state";

let lastFocus: Element | null = null;

function show(nodes: Node[]): void {
  const panel = byId("panel");
  byId("panelBody").replaceChildren(...nodes);
  if (!panel.classList.contains("open")) lastFocus = document.activeElement;
  panel.classList.add("open");
  panel.inert = false;
  hideTip();
  byId("panelBody").querySelector<HTMLElement>("h2")?.focus({ preventScroll: true });
}

export function closePanel(): void {
  const panel = byId("panel");
  if (!panel.classList.contains("open")) return;
  const hadFocus = panel.contains(document.activeElement);
  panel.classList.remove("open");
  panel.inert = true;
  if (hadFocus && lastFocus instanceof HTMLElement && lastFocus.isConnected) lastFocus.focus({ preventScroll: true });
  lastFocus = null;
}

function heading(text: string): HTMLElement {
  const el = h("h2", undefined, text);
  el.tabIndex = -1;
  return el;
}

/** Day view. `readOnly` (kiosk): plain list, no way into the event view and no `Ajouter`. */
export function openDay(d: string, readOnly = false): void {
  const items = state.byDay[d] ?? [];
  const nodes: Node[] = [heading(cap(fmtLong(d)))];
  if (items.length) {
    const ul = h("ul", "items");
    for (const it of items) {
      const li = h("li");
      if (readOnly) {
        const row = h("div");
        row.append(swatch(it.cat), h("span", undefined, it.label));
        li.append(row);
      } else {
        const b = h("button");
        b.type = "button";
        b.dataset["open"] = it.id;
        b.append(swatch(it.cat), h("span", undefined, it.label));
        li.append(b);
      }
      ul.append(li);
    }
    nodes.push(ul);
  } else nodes.push(h("p", "nothing", t("day.nothing")));
  if (!readOnly) {
    const add = h("button", "btn", t("day.add"));
    add.type = "button";
    add.dataset["add"] = d;
    nodes.push(add);
  }
  show(nodes);
}

function titleWithAge(e: AppEvent): string {
  if (e.cat !== "birthday" || e.age === null) return e.title;
  return `${e.title} (${t(e.age > 1 ? "age.many" : "age.one", { n: e.age })})`;
}

function metaRow(dl: HTMLElement, term: string, value: string | Node, cls?: string): void {
  const dd = h("dd", cls);
  dd.append(value);
  dl.append(h("dt", undefined, term), dd);
}

export function openEvent(id: string, confirming = false): void {
  const e = findEvent(id);
  if (!e) return;
  const n = span(e.start, e.end);
  const nodes: Node[] = [heading(titleWithAge(e)), h("p", "sub", cap(fmtRange(e.start, e.end)))];
  if (e.start_time) nodes.push(h("p", "sub time", e.end_time ? t("time.range", { a: e.start_time, b: e.end_time }) : t("time.at", { a: e.start_time })));

  const dl = h("dl", "meta");
  const catDd = h("dd", "cat");
  catDd.append(swatch(e.cat), document.createTextNode(typeLabel(e.type)));
  dl.append(h("dt", undefined, t("meta.cat")), catDd);
  metaRow(dl, t("meta.duration"), t(n > 1 ? "meta.days" : "meta.day", { n }));
  const all = e.members.length === 0 || e.members.length === members().length;
  metaRow(dl, t("meta.who"), all ? t("meta.everyone") : e.members.join(", "));
  if (e.location) metaRow(dl, t("meta.loc"), e.location);
  if (e.notes) metaRow(dl, t("meta.notes"), e.notes, "notes");
  if (e.created_by) metaRow(dl, t("meta.addedBy"), e.created_by);
  if (e.updated_by && e.updated_by !== e.created_by) metaRow(dl, t("meta.modifiedBy"), e.updated_by);
  nodes.push(dl);

  if (e.google_url) {
    const a = h("a", "link", t("link.gcal"));
    a.href = e.google_url;
    a.target = "_blank";
    a.rel = "noopener";
    nodes.push(a);
  }

  if (e.read_only) nodes.push(h("p", "imported", t("ev.imported")));
  else if (confirming) nodes.push(confirmBox(e));
  else {
    const row = h("div", "btns");
    const edit = h("button", "btn", t("ev.edit"));
    edit.type = "button";
    edit.addEventListener("click", () => openEdit(e));
    const del = h("button", "btn danger", t("ev.delete"));
    del.type = "button";
    del.addEventListener("click", () => openEvent(e.id, true));
    row.append(edit, del);
    nodes.push(row);
  }
  show(nodes);
  if (confirming) byId("panelBody").querySelector<HTMLElement>(".confirm .btn.danger")?.focus();
}

function confirmBox(e: AppEvent): HTMLElement {
  const box = h("div", "confirm");
  box.setAttribute("role", "group");
  box.setAttribute("aria-label", t("confirm.aria"));
  box.append(h("p", undefined, t("confirm.q", { title: e.title })));
  if (e.series_id) box.append(h("p", undefined, t("confirm.series")));
  const err = h("p", "err");
  err.setAttribute("role", "alert");
  const ok = h("button", "btn danger", t("ev.delete"));
  ok.type = "button";
  const no = h("button", "btn ghost", t("btn.cancel"));
  no.type = "button";
  no.addEventListener("click", () => openEvent(e.id));
  ok.addEventListener("click", async () => {
    ok.disabled = true;
    no.disabled = true;
    err.textContent = "";
    try {
      await deleteEvent(e.id);
      closePanel();
      await loadYear();
    } catch (ex) {
      err.textContent = ex instanceof ApiError ? ex.message : t("err.short");
      ok.disabled = false;
      no.disabled = false;
    }
  });
  const row = h("div", "btns");
  row.append(ok, no);
  box.append(err, row);
  return box;
}

export function initPanel(): void {
  const panel = byId("panel");
  panel.inert = true;
  byId("closePanel").addEventListener("click", closePanel);
  byId("panelBody").addEventListener("click", (ev) => {
    const t = ev.target as Element;
    const o = t.closest<HTMLElement>("[data-open]");
    const a = t.closest<HTMLElement>("[data-add]");
    if (o?.dataset["open"]) openEvent(o.dataset["open"]);
    else if (a?.dataset["add"]) openCreate(a.dataset["add"], a.dataset["add"]);
  });
  document.addEventListener("keydown", (ev) => {
    if (ev.key === "Escape" && !(byId("dlg") as HTMLDialogElement).open) closePanel();
  });
}
