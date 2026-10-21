import { authorizePath, clearAccess, expect, pkce, registerClient, REDIRECT, interceptConsent, test } from "./helpers";

let clientId = "";

test.beforeAll(async ({ request }) => {
  clientId = await registerClient(request, "Agent de test");
});

test.beforeEach(async ({ request }) => {
  await clearAccess(request);
});

const row = (page: import("@playwright/test").Page, name: string) => page.getByRole("row").filter({ hasText: name });

test.describe("Réglages", () => {
  test("header, sections and empty states", async ({ page }) => {
    await page.goto("/reglages");
    await expect(page.getByRole("heading", { name: "Réglages" })).toBeVisible();
    await expect(page.getByRole("link", { name: "← Retour à l’année" })).toHaveAttribute("href", "/");
    for (const h of ["Accès des agents", "Tablette (mode kiosque)", "Imports", "Jetons personnels", "Connexions OAuth", "Adresse du serveur MCP"]) {
      await expect(page.getByRole("heading", { name: h })).toBeVisible();
    }
    await expect(page.getByText("Aucun jeton pour l’instant.")).toBeVisible();
    await expect(page.getByText("Aucune application connectée.")).toBeVisible();
    await expect(page.getByRole("radio", { name: "Lecture seule" })).toBeChecked();
  });

  test("a new token is shown once, then listed with « Jamais »", async ({ page }) => {
    await page.goto("/reglages");
    await page.getByLabel("Nom", { exact: true }).fill("Grok API");
    await page.getByRole("button", { name: "Créer le jeton" }).click();
    const box = page.getByRole("group", { name: "Nouveau secret" });
    await expect(box).toContainText("Copiez ce jeton maintenant : il ne sera plus affiché.");
    await expect(box.getByLabel("Jeton")).toHaveValue(/^fy_pat_[A-Za-z0-9_-]+$/);
    const r = row(page, "Grok API");
    await expect(r).toContainText("Lecture seule");
    await expect(r).toContainText("Jamais");
    await expect(r).toContainText(/\d{1,2} \S+ 2026 à \d\d:\d\d/);
    await expect(r.getByText("Kiosque")).toHaveCount(0);
    await page.reload();
    await expect(page.getByText("Copiez ce jeton maintenant")).toHaveCount(0);
    await expect(page.locator("input.copyval")).toHaveCount(1); // only the MCP address remains
    await expect(row(page, "Grok API")).toBeVisible();
  });

  test("read and write rights are labelled", async ({ page }) => {
    await page.goto("/reglages");
    await page.getByLabel("Nom", { exact: true }).fill("Agent écriture");
    await page.getByLabel("Lecture et écriture").check();
    await page.getByRole("button", { name: "Créer le jeton" }).click();
    await expect(row(page, "Agent écriture")).toContainText("Lecture et écriture");
  });

  test("form errors are shown: empty name and duplicate name", async ({ page }) => {
    await page.goto("/reglages");
    await page.getByRole("button", { name: "Créer le jeton" }).click();
    await expect(page.getByRole("alert").filter({ hasText: "Donnez un nom au jeton." })).toBeVisible();
    await page.getByLabel("Nom", { exact: true }).fill("Doublon");
    await page.getByRole("button", { name: "Créer le jeton" }).click();
    await expect(row(page, "Doublon")).toBeVisible();
    await page.getByLabel("Nom", { exact: true }).fill("Doublon");
    await page.getByRole("button", { name: "Créer le jeton" }).click();
    const alert = page.locator("#tokenErr");
    await expect(alert).toBeVisible();
    await expect(alert).not.toHaveText("");
    await expect(page.getByRole("row").filter({ hasText: "Doublon" })).toHaveCount(1);
  });

  test("revoking asks for confirmation; « Annuler » keeps the token", async ({ page, request }) => {
    await page.goto("/reglages");
    await page.getByLabel("Nom", { exact: true }).fill("Éphémère");
    await page.getByRole("button", { name: "Créer le jeton" }).click();
    const clear = await page.getByRole("group", { name: "Nouveau secret" }).getByLabel("Jeton").inputValue();

    await row(page, "Éphémère").getByRole("button", { name: "Révoquer" }).click();
    const dlg = page.getByRole("dialog");
    await expect(dlg).toContainText("Révoquer l’accès de « Éphémère » ?");
    await dlg.getByRole("button", { name: "Annuler" }).click();
    await expect(dlg).toBeHidden();
    await expect(row(page, "Éphémère")).toBeVisible();

    await row(page, "Éphémère").getByRole("button", { name: "Révoquer" }).click();
    await expect(dlg).toBeVisible();
    await dlg.getByRole("button", { name: "Révoquer" }).click();
    await expect(dlg).toBeHidden();
    await expect(row(page, "Éphémère")).toHaveCount(0);
    // the token really stopped working
    const mcp = await request.post("/mcp", {
      headers: { Authorization: `Bearer ${clear}`, Accept: "application/json, text/event-stream" },
      data: { jsonrpc: "2.0", id: 1, method: "tools/list" },
    });
    expect(mcp.status()).toBe(401);
  });

  test("kiosk link: shown once with the kiosk URL, badge in the table", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.goto("/reglages");
    await page.getByRole("button", { name: "Créer un lien kiosque" }).click();
    const box = page.getByRole("group", { name: "Nouveau secret" });
    const url = box.getByLabel("Lien kiosque");
    await expect(url).toHaveValue(/\/kiosk\?token=fy_pat_[A-Za-z0-9_-]+$/);
    await expect(box).toContainText("il ne sera plus affiché");
    const r = page.getByRole("row").filter({ hasText: /Kiosque – / });
    await expect(r).toContainText("Lecture seule");
    await expect(r.locator(".badge")).toHaveText("Kiosque");

    const copy = box.locator("button");
    await copy.click();
    await expect(copy).toHaveText("Copié");
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(await url.inputValue());
    await page.reload();
    await expect(page.getByLabel("Lien kiosque")).toHaveCount(0);
    await expect(r.locator(".badge")).toHaveText("Kiosque");
  });

  test("copy falls back to execCommand when the Clipboard API is unavailable", async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
      (window as unknown as { __copied: string }).__copied = "";
      document.execCommand = (cmd: string): boolean => {
        const sel = document.activeElement as HTMLInputElement;
        (window as unknown as { __copied: string }).__copied = cmd + ":" + sel.value.slice(sel.selectionStart ?? 0, sel.selectionEnd ?? 0);
        return true;
      };
    });
    await page.goto("/reglages");
    const mcp = page.locator("#mcp");
    await mcp.getByRole("button", { name: "Copier" }).click();
    await expect(mcp.getByRole("button")).toHaveText("Copié");
    const value = await mcp.getByLabel("Adresse du serveur MCP").inputValue();
    expect(await page.evaluate(() => (window as unknown as { __copied: string }).__copied)).toBe(`copy:${value}`);
  });

  test("the MCP address is shown and can be copied", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.goto("/reglages");
    const field = page.locator("#mcp").getByLabel("Adresse du serveur MCP");
    await expect(field).toHaveValue(/^https?:\/\/.+\/mcp$/);
    const copy = page.locator("#mcp button");
    await copy.click();
    await expect(copy).toHaveText("Copié");
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(await field.inputValue());
  });

  test("imports: « Mettre à jour maintenant » adds a successful run", async ({ page }) => {
    await page.goto("/reglages");
    await expect(page.getByRole("button", { name: "Mettre à jour maintenant" })).toBeEnabled();
    const before = await page.locator("#runs tbody tr").count();
    await page.getByRole("button", { name: "Mettre à jour maintenant" }).click();
    await expect(page.locator("#runs tbody tr")).toHaveCount(before + 1);
    const first = page.locator("#runs tbody tr").first();
    await expect(first).toContainText("Réussi");
    await expect(first).toContainText(/\d{1,2} \S+ 2026 à \d\d:\d\d/);
    await expect(page.getByRole("button", { name: "Mettre à jour maintenant" })).toBeEnabled();
    await expect(page.locator("#importStatus")).toContainText("Dernier import");
    await expect(page.locator("#runs th")).toHaveText(["Date", "Résultat", "Créés", "Mis à jour", "Supprimés", "Inchangés", "Erreur"]);
  });

  test("imports: running state disables the button and polls; failures show the error", async ({ page }) => {
    let polls = 0;
    await page.route("**/api/admin/import", async (route) => {
      if (route.request().method() === "POST") return route.fulfill({ status: 202, json: { running: true } });
      polls++;
      const base = { id: 1, started_at: "2026-10-16T12:32:00Z", finished_at: "2026-10-16T12:32:05Z", created: 0, updated: 0, deleted: 0, unchanged: 0, trigger: "manual" };
      if (polls < 3) return route.fulfill({ json: { running: polls > 1, runs: [{ ...base, status: "failed", error: "Le service est indisponible", finished_at: null }] } });
      return route.fulfill({ json: { running: false, runs: [{ ...base, status: "failed", error: "Le service est indisponible" }] } });
    });
    await page.goto("/reglages");
    await page.getByRole("button", { name: "Mettre à jour maintenant" }).click();
    await expect(page.locator("#importStatus")).toContainText("Import en cours");
    await expect(page.getByRole("button", { name: "Mettre à jour maintenant" })).toBeDisabled();
    await expect(page.getByRole("button", { name: "Mettre à jour maintenant" })).toBeEnabled({ timeout: 10_000 });
    const first = page.locator("#runs tbody tr").first();
    await expect(first).toContainText("Échec");
    await expect(first).toContainText("Le service est indisponible");
    await expect(first).toContainText("14:32"); // 12:32Z in Europe/Paris
  });

  test("API errors are announced", async ({ page }) => {
    await page.route("**/api/admin/tokens", (route) =>
      route.request().method() === "GET" ? route.fulfill({ status: 500, json: { error: { code: "internal", message: "Erreur interne." } } }) : route.continue(),
    );
    await page.goto("/reglages");
    await expect(page.locator("#pageErr")).toHaveText("Erreur interne.");
  });

  test("OAuth connections are listed and can be revoked", async ({ page, request }) => {
    // Run the real consent flow, then exchange the code: that creates the grant.
    const { verifier, challenge } = pkce();
    const consent = await interceptConsent(page);
    await page.goto(authorizePath(clientId, challenge));
    await page.getByRole("button", { name: "Autoriser" }).click();
    const code = (await consent.redirectedTo()).searchParams.get("code");
    expect(code).toBeTruthy();
    const tok = await request.post("/oauth/token", {
      form: { grant_type: "authorization_code", code: code!, redirect_uri: REDIRECT, client_id: clientId, code_verifier: verifier },
    });
    expect(tok.status(), await tok.text()).toBe(200);

    await page.goto("/reglages");
    const r = row(page, "Agent de test");
    await expect(r).toContainText("agent.invalid");
    await expect(r).toContainText("Jamais");
    await expect(page.locator("#grants th")).toHaveText(["Application", "Redirection", "Autorisé le", "Dernière utilisation", "Actions"]);
    await r.getByRole("button", { name: "Révoquer" }).click();
    await expect(page.getByRole("dialog")).toContainText("Révoquer l’accès de « Agent de test » ?");
    await page.getByRole("dialog").getByRole("button", { name: "Révoquer" }).click();
    await expect(page.getByText("Aucune application connectée.")).toBeVisible();
    // the access token no longer works
    const { access_token } = (await tok.json()) as { access_token: string };
    const mcp = await request.post("/mcp", {
      headers: { Authorization: `Bearer ${access_token}`, Accept: "application/json, text/event-stream" },
      data: { jsonrpc: "2.0", id: 1, method: "tools/list" },
    });
    expect(mcp.status()).toBe(401);
  });

  test("the page is usable in dark mode and on a phone", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "dark" });
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/reglages");
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe("rgb(17, 20, 26)");
    await expect(page.getByRole("button", { name: "Créer le jeton" })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
});
