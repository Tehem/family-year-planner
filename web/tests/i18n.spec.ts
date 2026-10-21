import { cell, expect, openApp, resetDemo, test } from "./helpers";

const anonymous = { cookies: [], origins: [] };

test.describe("languages (SPEC §19.2)", () => {
  test.beforeEach(async ({ request }) => {
    await resetDemo(request);
  });

  test("French is the default and the switch shows it", async ({ page }) => {
    await openApp(page);
    await expect(page.locator("html")).toHaveAttribute("lang", "fr");
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Année en famille");
    const sw = page.getByRole("group", { name: "Langue" });
    await expect(sw.getByRole("button", { name: "FR" })).toHaveAttribute("aria-pressed", "true");
    await expect(sw.getByRole("button", { name: "EN" })).toHaveAttribute("aria-pressed", "false");
  });

  test("switching to English translates the screen, the dialog, the panel and the dates", async ({ page, context }) => {
    await openApp(page);
    await page.getByRole("group", { name: "Langue" }).getByRole("button", { name: "EN" }).click();
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    expect((await context.cookies()).find((c) => c.name === "annee_lang")?.value).toBe("en");

    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Family year");
    await expect(page.getByRole("button", { name: "Weeks" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Months" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Add an event" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Everyone", exact: true })).toBeVisible();
    await expect(page.locator('[data-cat="trip"]')).toHaveText("Trips & vacations");
    await expect(page.locator('[data-cat="birthday"]')).toHaveText("Birthdays");
    await expect(page.getByRole("link", { name: "Settings" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Sign out" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Next year" })).toBeVisible();
    await expect(page.locator(".hd.mo").first()).toHaveText("Jan.");

    // Imported holidays are translated, user events are not.
    await cell(page, "2026-07-14").click();
    await expect(page.locator("#panel h2")).toHaveText(/^Tuesday,? 14 July 2026$/);
    await expect(page.locator("#panel")).toContainText("Bastille Day");
    await page.locator("#closePanel").click();
    await page.locator(".bar", { hasText: "Semaine de ski aux Arcs" }).first().click();
    await expect(page.locator("#panel")).toContainText("Semaine de ski aux Arcs");
    await expect(page.locator("#panel")).toContainText("Category");
    await expect(page.locator("#panel")).toContainText("Trip or vacation");
    await expect(page.locator("#panel")).toContainText("Open in Google Calendar");
    await expect(page.locator("#panel .sub").first()).toContainText(/^Sat,? 21 Feb – Sat,? 28 Feb 2026$/);
    await page.getByRole("button", { name: "Edit" }).click();
    await expect(page.locator("#dlgTitle")).toHaveText("Edit the event");
    await expect(page.getByLabel("Title")).toBeVisible();
    await expect(page.locator("#fCat")).toBeVisible();
    await expect(page.getByRole("button", { name: "Save" })).toBeVisible();
    await page.getByLabel("Title").fill("");
    await page.getByRole("button", { name: "Save" }).click();
    await expect(page.locator("#fErr")).toHaveText("Give the event a title.");
  });

  test("the choice survives a reload and applies to Réglages, then French comes back", async ({ page }) => {
    await page.goto("/reglages?lang=en");
    await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Categories" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Agent access" })).toBeVisible();
    await expect(page.getByRole("link", { name: "← Back to the year" })).toBeVisible();
    await expect(page.getByText("No tokens yet.")).toBeVisible();
    await page.reload();
    await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
    await page.getByRole("group", { name: "Language" }).getByRole("button", { name: "FR" }).click();
    await expect(page.getByRole("heading", { name: "Réglages", level: 1 })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Accès des agents" })).toBeVisible();
  });

  test("user text is inserted verbatim in translated messages", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator(".bar").first()).toBeVisible();
    await page.getByRole("button", { name: "Ajouter un événement", exact: true }).click();
    await page.getByLabel("Titre").fill("Cost $& more {title}");
    await page.getByLabel("Premier jour").fill("2026-11-10");
    await page.getByLabel("Dernier jour").fill("2026-11-10");
    await page.getByRole("button", { name: "Ajouter", exact: true }).click();
    await page.locator(".bar", { hasText: "Cost $& more" }).click();
    await page.getByRole("button", { name: "Supprimer" }).click();
    await expect(page.locator("#panel .confirm")).toContainText("Supprimer « Cost $& more {title} » ?");
  });

  test("API errors follow the language", async ({ page }) => {
    await page.goto("/?lang=en");
    await expect(page.locator(".bar").first()).toBeVisible();
    await page.getByRole("button", { name: "Add an event" }).click();
    await page.getByRole("button", { name: "Add", exact: true }).click();
    await expect(page.locator("#fErr")).toHaveText("Give the event a title.");
    await page.getByLabel("Title").fill("Pique-nique");
    await page.getByLabel("Last day").fill("2026-01-01");
    await page.getByLabel("First day").fill("2026-02-01");
    await page.getByRole("button", { name: "Add", exact: true }).click();
    await expect(page.locator("#fErr")).toHaveText("The last day cannot be before the first day.");
  });
});

test.describe("login and kiosk pages", () => {
  test.use({ storageState: anonymous });

  test("the login page is French by default and English with ?lang=en", async ({ page }) => {
    await page.goto("/connexion");
    await expect(page.getByText("Connectez-vous pour voir l’année de la famille.")).toBeVisible();
    await expect(page.getByRole("button", { name: "Entrer en mode démo" })).toBeVisible();
    await page.goto("/connexion?lang=en");
    await expect(page.getByText("Sign in to see the family year.")).toBeVisible();
    await expect(page.getByRole("button", { name: "Enter demo mode" })).toBeVisible();
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    await page.goto("/connexion?error=denied");
    await expect(page.getByRole("alert")).toHaveText("Sign-in was cancelled.");
    await page.getByRole("group", { name: "Language" }).getByRole("button", { name: "FR" }).click();
    await expect(page.getByText("Connectez-vous pour voir l’année de la famille.")).toBeVisible();
  });
});
