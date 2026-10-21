import type { Page } from "@playwright/test";
import { bar, expect, resetDemo, test } from "./helpers";

// SPEC §19: renaming, recoloring and adding types, from Réglages.
test.beforeEach(async ({ request }) => {
  await resetDemo(request);
});

const card = (page: Page, code: string) => page.locator(`.type-card[data-code="${code}"]`);
const field = (page: Page, code: string, lang: "fr" | "en", kind: "singular" | "plural") =>
  page.locator(`input[data-code="${code}"][data-lang="${lang}"][data-kind="${kind}"]`);
const save = (page: Page) => page.getByRole("button", { name: "Enregistrer les catégories" });
const barColor = (page: Page, title: string) => bar(page, title).evaluate((el) => getComputedStyle(el).backgroundColor);

async function openTypes(page: Page, count = 9): Promise<void> {
  await page.goto("/reglages");
  await expect(page.locator(".type-card")).toHaveCount(count);
}

test.describe("Catégories", () => {
  test("lists the nine default types with their names and colors", async ({ page }) => {
    await openTypes(page);
    await expect(field(page, "trip", "fr", "singular")).toHaveValue("Voyage ou vacances");
    await expect(field(page, "trip", "fr", "plural")).toHaveValue("Voyages & vacances");
    await expect(field(page, "trip", "en", "singular")).toHaveValue("Trip or vacation");
    await expect(card(page, "trip").locator("select")).toHaveValue("9");
    await expect(card(page, "trip").locator("select option:checked")).toHaveText("Myrtille");
    await expect(card(page, "unclassified").locator("select")).toHaveCount(0);
    await expect(save(page)).toBeDisabled();
    await expect(page.getByRole("button", { name: "Ajouter une catégorie" })).toBeEnabled();
    await expect(page.getByText("3 couleur(s) libre(s) dans Google Agenda.")).toBeVisible();
  });

  test("renaming a type updates the legend, the dialog and the panel without any recoloring", async ({ page }) => {
    await openTypes(page);
    await field(page, "trip", "fr", "singular").fill("Escapade");
    await field(page, "trip", "fr", "plural").fill("Escapades");
    await save(page).click();
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await page.goto("/");
    await expect(page.locator('[data-cat="trip"]')).toHaveText("Escapades");
    await bar(page, "Semaine de ski aux Arcs").click();
    await expect(page.locator("#panel")).toContainText("Escapade");
    await page.getByRole("button", { name: "Modifier" }).click();
    await expect(page.locator("#fCat option", { hasText: "Escapade" })).toHaveCount(1);
  });

  test("English names are used when the interface is in English", async ({ page }) => {
    await openTypes(page);
    await field(page, "celebration", "en", "plural").fill("Big days");
    await save(page).click();
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
    await page.goto("/?lang=en");
    await expect(page.locator('[data-cat="celebration"]')).toHaveText("Big days");
    await page.goto("/?lang=fr");
    await expect(page.locator('[data-cat="celebration"]')).toHaveText("Fêtes & cérémonies");
  });

  test("a color used twice is refused", async ({ page }) => {
    await openTypes(page);
    await card(page, "trip").locator("select").selectOption("11"); // Tomate: family_gathering's
    await expect(card(page, "trip").locator(".type-dup")).toContainText("déjà utilisée par « Réunion de famille »");
    await expect(card(page, "family_gathering").locator(".type-dup")).toContainText("Voyage ou vacances");
    await expect(save(page)).toBeEnabled();
    await save(page).click();
    await expect(page.getByRole("alert").filter({ hasText: "Corrigez les champs signalés" })).toBeVisible();
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await card(page, "trip").locator("select").selectOption("9");
    await expect(card(page, "trip").locator(".type-dup")).toHaveText("");
  });

  test("an empty name is refused", async ({ page }) => {
    await openTypes(page);
    await field(page, "trip", "en", "plural").fill("   ");
    await save(page).click();
    await expect(page.getByRole("alert").filter({ hasText: "Renseignez les quatre noms." })).toBeVisible();
  });

  test("recoloring asks for confirmation, shows the event count and recolors the bars", async ({ page }) => {
    await page.goto("/");
    const before = await barColor(page, "Semaine de ski aux Arcs");
    await openTypes(page);
    await card(page, "trip").locator("select").selectOption("6"); // Mandarine
    await save(page).click();
    const dlg = page.getByRole("dialog", { name: "Modifier les couleurs dans Google Agenda ?" });
    await expect(dlg).toBeVisible();
    await expect(dlg).toContainText(/\d+ événement\(s\) seront recolorés dans Google Agenda\./);
    await expect(dlg).toContainText("« Voyage ou vacances » :");
    await expect(dlg).toContainText("de Myrtille à Mandarine");
    // Cancelling changes nothing.
    await dlg.getByRole("button", { name: "Annuler" }).click();
    await page.goto("/");
    expect(await barColor(page, "Semaine de ski aux Arcs")).toBe(before);
    // Confirming does.
    await openTypes(page);
    await card(page, "trip").locator("select").selectOption("6");
    await save(page).click();
    await dlg.getByRole("button", { name: "Appliquer" }).click();
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
    await expect(card(page, "trip").locator("select")).toHaveValue("6");
    await page.goto("/");
    expect(await barColor(page, "Semaine de ski aux Arcs")).toBe("rgb(226, 110, 0)"); // --p6-bar, light
    await expect(page.locator('.sw.cat-trip')).toHaveCount(1);
  });

  test("two types can swap their colors", async ({ page }) => {
    await openTypes(page);
    await card(page, "trip").locator("select").selectOption("11");
    await card(page, "family_gathering").locator("select").selectOption("9");
    await expect(card(page, "trip").locator(".type-dup")).toHaveText("");
    await save(page).click();
    await page.getByRole("dialog").getByRole("button", { name: "Appliquer" }).click();
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
    await page.goto("/");
    expect(await barColor(page, "Semaine de ski aux Arcs")).toBe("rgb(229, 83, 61)"); // trip is Tomate now
  });

  test("a new type gets a free color, can be used for events, filtered, and deleted", async ({ page }) => {
    await openTypes(page);
    await page.getByRole("button", { name: "Ajouter une catégorie" }).click();
    await expect(page.locator(".type-card")).toHaveCount(10);
    const c = card(page, "custom_1");
    await expect(c.locator("select")).toHaveValue("1"); // first free color: Lavande
    await field(page, "custom_1", "fr", "singular").fill("Activité sportive");
    await field(page, "custom_1", "fr", "plural").fill("Sport");
    await field(page, "custom_1", "en", "singular").fill("Sports activity");
    await field(page, "custom_1", "en", "plural").fill("Sports");
    await c.locator("select").selectOption("7"); // Paon
    await save(page).click(); // nothing to recolor: applied at once
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
    await expect(page.getByText("2 couleur(s) libre(s) dans Google Agenda.")).toBeVisible();

    await page.goto("/");
    await expect(page.locator("#legend [data-cat]")).toHaveCount(9);
    await expect(page.locator('[data-cat="custom_1"]')).toHaveText("Sport");
    // Create an event of the new type.
    await page.getByRole("button", { name: "Ajouter un événement", exact: true }).click();
    await page.getByLabel("Titre").fill("Tournoi de judo");
    await page.locator("#fCat").selectOption("custom_1");
    await page.getByLabel("Premier jour").fill("2026-11-07");
    await page.getByLabel("Dernier jour").fill("2026-11-08");
    await page.getByRole("button", { name: "Ajouter", exact: true }).click();
    await expect(bar(page, "Tournoi de judo")).toBeVisible();
    await expect(bar(page, "Tournoi de judo")).toHaveClass(/cat-custom_1/);
    expect(await barColor(page, "Tournoi de judo")).toBe("rgb(15, 160, 219)"); // --p7-bar, light
    await bar(page, "Tournoi de judo").click();
    await expect(page.locator("#panel")).toContainText("Activité sportive");
    await page.locator("#closePanel").click();
    // Filter it out.
    await page.locator('[data-cat="custom_1"]').click();
    await expect(page.locator(".bar", { hasText: "Tournoi de judo" })).toHaveCount(0);
    await page.locator('[data-cat="custom_1"]').click();
    await expect(bar(page, "Tournoi de judo")).toBeVisible();

    // Deleting the type reports its events, which become unclassified.
    await openTypes(page, 10);
    await card(page, "custom_1").getByRole("button", { name: /Supprimer la catégorie/ }).click();
    await save(page).click();
    const dlg = page.getByRole("dialog");
    await expect(dlg).toContainText("1 événement(s) deviendront « Non classés ».");
    await dlg.getByRole("button", { name: "Appliquer" }).click();
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
    await page.goto("/");
    await expect(page.locator('[data-cat="custom_1"]')).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Non classés" })).toBeVisible();
    await expect(bar(page, "Tournoi de judo")).toBeVisible();
  });

  test("a deleted category's code is not reused by a category added in the same save", async ({ page }) => {
    await openTypes(page);
    await page.getByRole("button", { name: "Ajouter une catégorie" }).click();
    await save(page).click();
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
    await card(page, "custom_1").getByRole("button", { name: /Supprimer la catégorie/ }).click();
    await page.getByRole("button", { name: "Ajouter une catégorie" }).click();
    await expect(card(page, "custom_1")).toHaveCount(0);
    await expect(card(page, "custom_2")).toHaveCount(1);
  });

  test("a save from a stale editor is refused with a message and keeps the edits", async ({ page, request }) => {
    await openTypes(page);
    await field(page, "trip", "fr", "plural").fill("Escapades");
    // Another browser adds a category and saves first.
    const current = (await (await request.get("/api/admin/types")).json()) as { types: unknown[] };
    const body = {
      types: [
        ...current.types,
        { code: "custom_1", color_id: "6", labels: { fr: { singular: "Sport", plural: "Sport" }, en: { singular: "Sport", plural: "Sport" } } },
      ],
    };
    const r = await request.put("/api/admin/types", { data: body, headers: { "X-Requested-With": "annee" } });
    expect(r.ok()).toBeTruthy();
    await save(page).click();
    await expect(page.getByRole("alert").filter({ hasText: "modifiées entre-temps" })).toBeVisible();
    await expect(field(page, "trip", "fr", "plural")).toHaveValue("Escapades");
    await page.reload();
    await expect(page.locator(".type-card")).toHaveCount(10);
    await expect(card(page, "custom_1")).toHaveCount(1);
  });

  test("an unused or new category is saved without a confirmation", async ({ page }) => {
    await openTypes(page);
    await page.getByRole("button", { name: "Ajouter une catégorie" }).click();
    await card(page, "custom_1").locator("select").selectOption("7");
    await save(page).click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
    // Recoloring it now touches no event: still no dialog.
    await card(page, "custom_1").locator("select").selectOption("1");
    await save(page).click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
  });

  test("at most three added categories", async ({ page }) => {
    await openTypes(page);
    const add = page.getByRole("button", { name: "Ajouter une catégorie" });
    for (let i = 0; i < 3; i++) await add.click();
    await expect(page.locator(".type-card")).toHaveCount(12);
    await expect(add).toBeDisabled();
    await expect(page.getByText("Trois catégories ajoutées au maximum")).toBeVisible();
    await expect(save(page)).toBeEnabled();
    await save(page).click();
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
    await page.goto("/");
    await expect(page.locator("#legend [data-cat]")).toHaveCount(11);
  });

  test("the demo reset restores the default catalog", async ({ page, request }) => {
    await openTypes(page);
    await field(page, "trip", "fr", "plural").fill("Escapades");
    await save(page).click();
    await expect(page.locator("#typesOk")).toHaveText("Catégories enregistrées.");
    await resetDemo(request);
    await openTypes(page);
    await expect(field(page, "trip", "fr", "plural")).toHaveValue("Voyages & vacances");
  });
});

