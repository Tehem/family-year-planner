// Event dialog: create and edit (SPEC §8.6).
import { ApiError, createEvent, updateEvent, type EventInput } from "./api";
import { gotoYear, loadYear } from "./app";
import { byId, h } from "./dom";
import { closePanel } from "./panel";
import { hideTip } from "./tooltip";
import { t } from "./i18n";
import { editableRank, members, state, today, type AppEvent } from "./state";

let editing: AppEvent | null = null;
let saving = false;

const dlg = () => byId<HTMLDialogElement>("dlg");
const input = (id: string) => byId<HTMLInputElement>(id);
const submitLabel = () => (editing ? t("dlg.save") : t("dlg.add"));

function fillCategories(unclassified: boolean, selected: string): void {
  const sel = byId<HTMLSelectElement>("fCat");
  const types = (state.config?.types ?? [])
    .filter((t) => t.writable)
    .sort((a, b) => editableRank(a.code) - editableRank(b.code));
  const opts: HTMLOptionElement[] = [];
  if (unclassified) {
    const ph = h("option", undefined, t("dlg.pickCat"));
    ph.value = "";
    ph.disabled = true;
    opts.push(ph);
  }
  for (const t of types) {
    const o = h("option", undefined, t.label);
    o.value = t.code;
    opts.push(o);
  }
  sel.replaceChildren(...opts);
  sel.value = selected;
}

function fillMembers(selected: string[]): void {
  const all = selected.length === 0;
  byId("fMem").replaceChildren(
    h("legend", undefined, t("f.who")),
    ...members().map((m) => {
      const label = h("label");
      const cb = h("input");
      cb.type = "checkbox";
      cb.value = m;
      cb.checked = all || selected.includes(m);
      label.append(cb, document.createTextNode(m));
      return label;
    }),
  );
}

/** Birthdays last one day and carry an optional birth year. */
function syncType(): void {
  const birthday = byId<HTMLSelectElement>("fCat").value === "birthday";
  byId("fEndWrap").hidden = birthday;
  byId("fBirthWrap").hidden = !birthday;
  byId("fDates").classList.toggle("one", birthday);
}

function open(title: string, ev: AppEvent | null, s: string, e: string): void {
  editing = ev;
  closePanel();
  hideTip();
  byId("dlgTitle").textContent = title;
  input("fTitle").value = ev?.title ?? "";
  const unclassified = ev?.type === "unclassified";
  fillCategories(unclassified, ev ? (unclassified ? "" : ev.type) : s === e ? "celebration" : "trip");
  input("fStart").value = s;
  input("fEnd").value = e;
  input("fBirth").value = ev?.birth_year ? String(ev.birth_year) : "";
  fillMembers(ev?.members ?? []);
  input("fLoc").value = ev?.location ?? "";
  byId<HTMLTextAreaElement>("fNotes").value = ev?.notes ?? "";
  byId("fErr").textContent = "";
  const submit = byId<HTMLButtonElement>("submit");
  submit.disabled = false;
  submit.textContent = submitLabel();
  saving = false;
  syncType();
  dlg().showModal();
  input("fTitle").focus();
}

export function openCreate(s: string, e: string): void {
  open(t("dlg.new"), null, s, e);
}
export function openAddToday(): void {
  const t = today();
  const s = t.startsWith(String(state.year)) ? t : `${state.year}-01-01`;
  openCreate(s, s);
}
export function openEdit(ev: AppEvent): void {
  open(t("dlg.edit"), ev, ev.start, ev.end);
}

function readForm(): { error: string } | { payload: EventInput } {
  const title = input("fTitle").value.trim();
  const type = byId<HTMLSelectElement>("fCat").value;
  const birthday = type === "birthday";
  const start = input("fStart").value;
  const end = birthday ? start : input("fEnd").value;
  const boxes = [...byId("fMem").querySelectorAll<HTMLInputElement>("input")];
  const checked = boxes.filter((i) => i.checked).map((i) => i.value);
  if (!title) return { error: t("err.title") };
  if (!type) return { error: t("err.cat") };
  if (!start || !end) return { error: t("err.dates") };
  if (end < start) return { error: t("err.order") };
  if (!checked.length) return { error: t("err.person") };
  const payload: EventInput = {
    type,
    title,
    start,
    end,
    // The form sends an empty list when everybody is checked (SPEC §5.2).
    members: checked.length === boxes.length ? [] : checked,
  };
  const loc = input("fLoc").value.trim();
  const notes = byId<HTMLTextAreaElement>("fNotes").value.trim();
  if (loc || editing) payload.location = loc || null;
  if (notes || editing) payload.notes = notes || null;
  if (birthday) {
    const raw = input("fBirth").value.trim();
    if (raw) {
      const y = Number(raw);
      if (!Number.isInteger(y) || y < 1900 || y > new Date().getFullYear()) return { error: t("err.birth") };
      payload.birth_year = y;
    } else if (editing) payload.birth_year = null;
  }
  return { payload };
}

async function submit(): Promise<void> {
  if (saving) return;
  const err = byId("fErr");
  const r = readForm();
  if ("error" in r) {
    err.textContent = r.error;
    return;
  }
  err.textContent = "";
  saving = true;
  const btn = byId<HTMLButtonElement>("submit");
  btn.disabled = true;
  btn.textContent = t("dlg.saving");
  try {
    if (editing) await updateEvent(editing.id, r.payload);
    else await createEvent(r.payload);
  } catch (e) {
    err.textContent = e instanceof ApiError ? e.message : t("err.generic");
    saving = false;
    btn.disabled = false;
    btn.textContent = submitLabel();
    return;
  }
  saving = false;
  dlg().close();
  closePanel();
  const { start = "", end = "" } = r.payload;
  const y = +start.slice(0, 4);
  // Follow the event when it was created or moved outside the displayed year.
  if (y !== state.year && +end.slice(0, 4) !== state.year) await gotoYear(y);
  else await loadYear();
}

export function initDialog(): void {
  byId("cancel").addEventListener("click", () => dlg().close());
  byId("fCat").addEventListener("change", syncType);
  byId("form").addEventListener("submit", (ev) => {
    ev.preventDefault();
    void submit();
  });
}
