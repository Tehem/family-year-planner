import { expect, test } from "./helpers";

test.use({ storageState: { cookies: [], origins: [] } });

test.describe("login page", () => {
  test("protected pages redirect to the login page, the demo button signs in", async ({ page }) => {
    await page.goto("/");
    await expect(page).toHaveURL(/\/connexion\?next=%2F$/);
    await expect(page.getByRole("heading", { name: "Année en famille" })).toBeVisible();
    await expect(page.getByText("Connectez-vous pour voir l’année de la famille.")).toBeVisible();
    await expect(page.getByRole("link", { name: "Se connecter avec Google" })).toHaveCount(0);
    await page.getByRole("button", { name: "Entrer en mode démo" }).click();
    await expect(page).toHaveURL(/\/$/);
    await expect(page.locator(".bar").first()).toBeVisible();
  });

  test("the demo form keeps a safe next", async ({ page }) => {
    await page.goto("/connexion?next=/reglages");
    await expect(page.locator("form")).toHaveAttribute("action", "/auth/demo?next=%2Freglages");
    await page.goto("/connexion?next=//evil.example");
    await expect(page.locator("form")).toHaveAttribute("action", "/auth/demo?next=%2F");
  });

  test("production mode shows the Google button", async ({ page }) => {
    await page.route("**/healthz", (route) => route.fulfill({ json: { status: "ok", version: "0", mode: "production" } }));
    await page.goto("/connexion?next=/reglages");
    const link = page.getByRole("link", { name: "Se connecter avec Google" });
    await expect(link).toHaveAttribute("href", "/auth/google/start?next=%2Freglages");
    await expect(page.getByRole("button", { name: "Entrer en mode démo" })).toHaveCount(0);
  });

  test("a refused account is explained", async ({ page }) => {
    await page.goto("/connexion?error=not_allowed&email=intrus%40example.com");
    const alert = page.getByRole("alert");
    await expect(alert).toContainText("Ce compte Google n’est pas autorisé.");
    await expect(alert).toContainText("intrus@example.com");
  });

  for (const [code, text] of [
    ["invalid", "Votre demande de connexion a expiré. Réessayez."],
    ["denied", "La connexion a été annulée."],
    ["failed", "La connexion avec Google a échoué. Réessayez."],
    ["kiosk", "Ce lien kiosque n’est plus valide."],
  ] as const) {
    test(`error code « ${code} » is explained`, async ({ page }) => {
      await page.goto(`/connexion?error=${code}`);
      await expect(page.getByRole("alert")).toHaveText(text);
    });
  }

  test("an OAuth authorize path in next is kept exactly, for both sign-in methods", async ({ page }) => {
    const path = "/oauth/authorize?response_type=code&client_id=c1&redirect_uri=https%3A%2F%2Fa.example%2Fcb&state=x%20y";
    const url = `/connexion?next=${encodeURIComponent(path)}`;
    await page.goto(url);
    await expect(page.locator("form")).toHaveAttribute("action", `/auth/demo?next=${encodeURIComponent(path)}`);
    await page.route("**/healthz", (route) => route.fulfill({ json: { status: "ok", version: "0", mode: "production" } }));
    await page.goto(url);
    await expect(page.getByRole("link", { name: "Se connecter avec Google" })).toHaveAttribute(
      "href",
      `/auth/google/start?next=${encodeURIComponent(path)}`,
    );
  });

  test("the login page is dark in dark mode", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "dark" });
    await page.goto("/connexion");
    await expect(page.locator(".login")).toBeVisible();
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe("rgb(17, 20, 26)");
  });
});

test("no third-party request and no CSP violation on the main page", async ({ page }) => {
  await page.goto("/connexion");
  await page.getByRole("button", { name: "Entrer en mode démo" }).click();
  await expect(page.locator(".bar").first()).toBeVisible();
  await page.locator(".bar").first().click();
  await page.getByRole("button", { name: "Modifier" }).click();
  await expect(page.locator("#dlg")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Imprimer", exact: true }).click();
  await expect(page.locator("#printPop")).toBeVisible();
  // the `guard` fixture asserts on every request and console message when the test ends
  const fonts = await page.evaluate(() => [...document.fonts].map((f) => `${f.family}:${f.status}`));
  expect(fonts.some((f) => f.includes("Bricolage Grotesque:loaded"))).toBeTruthy();
});
