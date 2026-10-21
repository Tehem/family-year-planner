// Login page (SPEC §8.8).
import "./styles.css";
import { byId, h } from "./dom";

/** Only same-site relative paths are accepted as `next` (no open redirect). */
export function safeNext(raw: string | null): string {
  if (!raw || !raw.startsWith("/") || raw.startsWith("//") || raw.includes("\\")) return "/";
  return raw;
}

/** Error codes of `/connexion?error=` set by the server (not_allowed is handled separately). */
const ERRORS: Record<string, string> = {
  invalid: "Votre demande de connexion a expiré. Réessayez.",
  denied: "La connexion a été annulée.",
  failed: "La connexion avec Google a échoué. Réessayez.",
  kiosk: "Ce lien kiosque n’est plus valide.",
};

async function isDemo(): Promise<boolean> {
  try {
    const res = await fetch("/healthz", { headers: { Accept: "application/json" } });
    const j = (await res.json()) as { mode?: string };
    return j.mode === "demo";
  } catch {
    return false;
  }
}

async function main(): Promise<void> {
  const q = new URLSearchParams(window.location.search);
  const next = safeNext(q.get("next"));
  const nextParam = `next=${encodeURIComponent(next)}`;

  if (q.get("error")) {
    const err = byId("loginErr");
    if (q.get("error") === "not_allowed") {
      err.append(document.createTextNode("Ce compte Google n’est pas autorisé."));
      const email = q.get("email");
      if (email) err.append(document.createElement("br"), h("strong", undefined, email));
    } else err.textContent = ERRORS[q.get("error") ?? ""] ?? "La connexion a échoué. Réessayez.";
    err.hidden = false;
  }

  const slot = byId("loginAction");
  if (await isDemo()) {
    const form = h("form");
    form.method = "post";
    form.action = `/auth/demo?${nextParam}`;
    const btn = h("button", "btn", "Entrer en mode démo");
    btn.type = "submit";
    form.append(btn);
    slot.replaceChildren(form);
  } else {
    const a = h("a", "btn", "Se connecter avec Google");
    a.href = `/auth/google/start?${nextParam}`;
    slot.replaceChildren(a);
  }
}

void main();
