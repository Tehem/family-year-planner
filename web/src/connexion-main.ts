// Login page (SPEC §8.8).
import "./styles.css";
import { byId, h } from "./dom";
import { adoptDefaultLang, applyStatic, mountLangSwitch, t, type Key } from "./i18n";

/** Only same-site relative paths are accepted as `next` (no open redirect). */
export function safeNext(raw: string | null): string {
  if (!raw || !raw.startsWith("/") || raw.startsWith("//") || raw.includes("\\")) return "/";
  return raw;
}

/** Error codes of `/connexion?error=` set by the server (not_allowed is handled separately). */
const ERRORS: Record<string, Key> = {
  invalid: "login.invalid",
  denied: "login.denied",
  failed: "login.failed",
  kiosk: "login.kiosk",
};

async function isDemo(): Promise<boolean> {
  try {
    const res = await fetch("/healthz", { headers: { Accept: "application/json" } });
    const j = (await res.json()) as { mode?: string; default_language?: string };
    adoptDefaultLang(j.default_language);
    return j.mode === "demo";
  } catch {
    return false;
  }
}

async function main(): Promise<void> {
  applyStatic();
  mountLangSwitch(byId("langSwitch"));
  const demo = await isDemo();
  const q = new URLSearchParams(window.location.search);
  const next = safeNext(q.get("next"));
  const nextParam = `next=${encodeURIComponent(next)}`;

  if (q.get("error")) {
    const err = byId("loginErr");
    if (q.get("error") === "not_allowed") {
      err.append(document.createTextNode(t("login.notAllowed")));
      const email = q.get("email");
      if (email) err.append(document.createElement("br"), h("strong", undefined, email));
    } else err.textContent = t(ERRORS[q.get("error") ?? ""] ?? "login.generic");
    err.hidden = false;
  }

  const slot = byId("loginAction");
  if (demo) {
    const form = h("form");
    form.method = "post";
    form.action = `/auth/demo?${nextParam}`;
    const btn = h("button", "btn", t("login.demo"));
    btn.type = "submit";
    form.append(btn);
    slot.replaceChildren(form);
  } else {
    const a = h("a", "btn", t("login.google"));
    a.href = `/auth/google/start?${nextParam}`;
    slot.replaceChildren(a);
  }
}

void main();
