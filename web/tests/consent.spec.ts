import { authorizePath, expect, LOOPBACK, pkce, REDIRECT, registerClient, interceptConsent, test } from "./helpers";

let clientId = "";

test.beforeAll(async ({ request }) => {
  clientId = await registerClient(request, "Agent <b>de</b> test");
});

let consent: Awaited<ReturnType<typeof interceptConsent>>;
test.beforeEach(async ({ page }) => {
  consent = await interceptConsent(page);
});

const BOTH = "calendar:read calendar:write";

test.describe("OAuth consent screen", () => {
  test("shows who asks, what for, where to, and escapes the client name", async ({ page }) => {
    await page.goto(authorizePath(clientId, pkce().challenge, { scope: BOTH }));
    await expect(page).toHaveTitle("Autorisation – Année en famille");
    await expect(page.getByRole("heading", { name: "Année en famille" })).toBeVisible();
    await expect(page.locator(".lead")).toHaveText("Agent <b>de</b> test demande l’accès à l’agenda familial.");
    await expect(page.locator(".lead strong")).toHaveText("Agent <b>de</b> test");
    const perms = page.locator("ul.perms li");
    await expect(perms).toHaveText(["Voir les événements", "Ajouter, modifier et supprimer des événements"]);
    await expect(page.locator(".redirect")).toHaveText("Vous serez redirigé vers agent.invalid");
    await expect(page.locator(".redirect strong")).toHaveText("agent.invalid");
    await expect(page.locator(".who")).toContainText("Connecté en tant que");
    await expect(page.locator(".warn")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Autoriser" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Refuser" })).toBeVisible();
    // The card is styled: the stylesheet is linked and the list has check marks.
    const bg = await page.evaluate(() => getComputedStyle(document.querySelector("main.card")!).boxShadow);
    expect(bg).not.toBe("none");
    expect(await page.evaluate(() => getComputedStyle(document.querySelector("ul.perms li")!, "::before").content)).toContain("✓");
  });

  test("read-only requests do not list the write permission", async ({ page }) => {
    await page.goto(authorizePath(clientId, pkce().challenge, { scope: "calendar:read" }));
    await expect(page.locator("ul.perms li")).toHaveText(["Voir les événements"]);
  });

  test("a local redirect address shows a warning", async ({ page }) => {
    await page.goto(authorizePath(clientId, pkce().challenge, { redirect: LOOPBACK }));
    const warn = page.locator(".warn");
    await expect(warn).toContainText("Cette application tourne sur un ordinateur");
    await expect(warn).toBeVisible();
    expect(await warn.evaluate((el) => getComputedStyle(el).borderLeftWidth)).toBe("4px");
  });

  test("« Refuser » redirects with access_denied", async ({ page }) => {
    await page.goto(authorizePath(clientId, pkce().challenge, { state: "abc" }));
    await page.getByRole("button", { name: "Refuser" }).click();
    const u = await consent.redirectedTo();
    expect(`${u.origin}${u.pathname}`).toBe(REDIRECT);
    expect(u.searchParams.get("error")).toBe("access_denied");
    expect(u.searchParams.get("state")).toBe("abc");
    expect(u.searchParams.get("code")).toBeNull();
  });

  test("« Autoriser » redirects with a code and the issuer", async ({ page }) => {
    await page.goto(authorizePath(clientId, pkce().challenge, { state: "xyz", scope: BOTH }));
    await page.getByRole("button", { name: "Autoriser" }).click();
    const u = await consent.redirectedTo();
    expect(u.searchParams.get("code")).toMatch(/\S{10,}/);
    expect(u.searchParams.get("iss")).toMatch(/^https?:\/\//);
    expect(u.searchParams.get("state")).toBe("xyz");
    expect(u.searchParams.get("error")).toBeNull();
  });

  test("an unknown client gets a plain error page, never a redirect", async ({ page }) => {
    await page.goto(authorizePath("fy_client_unknown", pkce().challenge));
    await expect(page.getByRole("heading", { name: "Année en famille" })).toBeVisible();
    await expect(page.locator(".lead")).not.toHaveText("");
    await expect(page.locator(".hint")).toHaveText("Fermez cette page et recommencez depuis l’application.");
    await expect(page).toHaveURL(/\/oauth\/authorize/);
  });

  test("dark theme", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "dark" });
    await page.goto(authorizePath(clientId, pkce().challenge));
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe("rgb(17, 20, 26)");
  });
});

test.describe("signed out", () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test("sign in through the login page, then land on the consent screen", async ({ page }) => {
    const path = authorizePath(clientId, pkce().challenge, { scope: BOTH, state: "s p/a&ce" });
    await page.goto(path);
    await expect(page).toHaveURL(/\/connexion\?next=%2Foauth%2Fauthorize%3F/);
    // `next` is decoded once and re-encoded once for the demo form
    const action = await page.locator("form").getAttribute("action");
    expect(action).toBe(`/auth/demo?next=${encodeURIComponent(path)}`);
    await page.getByRole("button", { name: "Entrer en mode démo" }).click();
    await expect(page.locator(".lead")).toContainText("demande l’accès à l’agenda familial.");
    await page.getByRole("button", { name: "Autoriser" }).click();
    expect((await consent.redirectedTo()).searchParams.get("state")).toBe("s p/a&ce");
  });
});
