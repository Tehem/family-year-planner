import { bar, cell, dragCells, expect, openApp, resetDemo, test } from "./helpers";

test.beforeEach(async ({ request }) => {
  await resetDemo(request);
});

const panel = (page: import("@playwright/test").Page) => page.locator("#panel");

test.describe("views, navigation, filters", () => {
  test("Weeks view renders the year with today highlighted", async ({ page }) => {
    await openApp(page);
    await expect(page.locator("#yl")).toHaveText("2026");
    await expect(page.locator(".cell")).toHaveCount(365);
    await expect(page.locator("#canvas")).toHaveClass(/v-week/);
    await expect(cell(page, "2026-10-16")).toHaveClass(/today/);
    await expect(bar(page, "Semaine de ski aux Arcs")).toBeVisible();
    await expect(page.locator("#legend [data-cat]")).toHaveCount(8);
    await expect(page.getByRole("button", { name: "Anniversaires" })).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByRole("button", { name: "Non classés" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Semaines" })).toHaveAttribute("aria-pressed", "true");
    // geometry: --cs is clamped to 22..80 px
    const cs = await page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--cs")));
    expect(cs).toBeGreaterThanOrEqual(22);
    expect(cs).toBeLessThanOrEqual(80);
  });

  test("Months view renders and the view is remembered", async ({ page }) => {
    await openApp(page, "month");
    await expect(page.locator("#canvas")).toHaveClass(/v-month/);
    await expect(page.locator(".cell")).toHaveCount(365);
    await expect(page.locator(".bar.h").first()).toBeVisible();
    expect(await page.evaluate(() => localStorage.getItem("annee.view"))).toBe("month");
    await page.reload();
    await expect(page.locator("#canvas")).toHaveClass(/v-month/);
    await expect(page.getByRole("button", { name: "Mois" })).toHaveAttribute("aria-pressed", "true");
  });

  test("year navigation", async ({ page }) => {
    await openApp(page);
    await page.getByRole("button", { name: "Année suivante" }).click();
    await expect(page.locator("#yl")).toHaveText("2027");
    await expect(cell(page, "2027-01-01")).toBeVisible();
    await expect(bar(page, "Semaine de ski aux Arcs")).toBeVisible();
    await expect(cell(page, "2027-10-16")).not.toHaveClass(/today/);
    await page.getByRole("button", { name: "Année précédente" }).click();
    await page.getByRole("button", { name: "Année précédente" }).click();
    await expect(page.locator("#yl")).toHaveText("2025");
    await expect(page.locator(".bar").first()).toBeVisible();
  });

  test("category filters hide tints and bars", async ({ page }) => {
    await openApp(page);
    await expect(page.locator(".cell.sc").first()).toBeVisible();
    await page.locator('[data-cat="trip"]').click();
    await expect(page.locator('[data-cat="trip"]')).toHaveAttribute("aria-pressed", "false");
    await expect(page.locator(".bar", { hasText: "Semaine de ski" })).toHaveCount(0);
    await expect(bar(page, "Week-end des cousins")).toBeVisible();
    await page.locator('[data-cat="school"]').click();
    await expect(page.locator(".cell.sc")).toHaveCount(0);
    await page.locator('[data-cat="birthday"]').click();
    await expect(page.locator(".cell.bd")).toHaveCount(0);
    await page.locator('[data-cat="holiday"]').click();
    await expect(page.locator(".cell.ph")).toHaveCount(0);
    await page.locator('[data-cat="trip"]').click();
    await expect(bar(page, "Semaine de ski")).toBeVisible();
  });

  test("person filter: members, whole family, tints that always stay", async ({ page }) => {
    await openApp(page);
    await expect(bar(page, "Les enfants chez Mamie")).toBeVisible();
    await expect(bar(page, "Mariage de Julien")).toBeVisible();
    await page.getByRole("button", { name: "Noé", exact: true }).click();
    await expect(bar(page, "Les enfants chez Mamie")).toBeVisible();
    await expect(page.locator(".bar", { hasText: "Mariage de Julien" })).toHaveCount(0);
    await expect(bar(page, "Semaine de ski aux Arcs")).toBeVisible(); // members: [] means everyone
    await expect(page.locator(".cell.dc").first()).toBeVisible();
    await page.getByRole("button", { name: "Camille", exact: true }).click();
    await expect(page.locator(".bar", { hasText: "Les enfants chez Mamie" })).toHaveCount(0);
    await expect(bar(page, "Mariage de Julien")).toBeVisible();
    await expect(page.locator(".cell.dc")).toHaveCount(0); // daycare closures are Noé's
    await expect(page.locator(".cell.bd").first()).toBeVisible(); // birthdays always shown
    await expect(page.locator(".cell.sc").first()).toBeVisible(); // school holidays always shown
    await page.getByRole("button", { name: "Tous" }).click();
    await expect(page.locator(".cell.dc").first()).toBeVisible();
  });

  test("hover tooltip lists the items of the day", async ({ page }) => {
    await openApp(page);
    const b = await cell(page, "2026-02-21").boundingBox();
    await page.mouse.move(b!.x + 5, b!.y + 5);
    const tip = page.locator("#tip");
    await expect(tip).toHaveClass(/on/);
    await expect(tip).toContainText("Sam. 21 févr.");
    await expect(tip).toContainText("Semaine de ski aux Arcs");
    await expect(tip).toContainText("Vacances d’hiver");
    const e = await cell(page, "2026-03-14").boundingBox();
    await page.mouse.move(e!.x + 5, e!.y + 5);
    await expect(tip).toContainText("Rien de prévu");
  });

  test("portrait hint on narrow screens", async ({ page }) => {
    await page.setViewportSize({ width: 400, height: 800 });
    await openApp(page);
    await expect(page.locator(".hint")).toBeVisible();
    await expect(page.locator(".hint")).toHaveText("Tournez votre téléphone pour voir l’année entière.");
  });

  test("reduced motion removes the panel transition", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await openApp(page);
    const d = await panel(page).evaluate((el) => getComputedStyle(el).transitionDuration);
    expect(d).toBe("0s");
  });

  test("header: links and demo reset", async ({ page }) => {
    await openApp(page);
    await expect(page.getByRole("link", { name: "Réglages" })).toHaveAttribute("href", "/reglages");
    await expect(page.getByRole("button", { name: "Se déconnecter" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Imprimer", exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Réinitialiser la démo" })).toBeVisible();
    await page.getByRole("button", { name: "Ajouter un événement", exact: true }).click();
    await expect(page.locator("#fStart")).toHaveValue("2026-10-16"); // the server's "today"
    await expect(page.locator("#fCat")).toHaveValue("celebration");
    await expect(page.locator("#dlgTitle")).toHaveText("Nouvel événement");
  });
});

test.describe("panel", () => {
  test("day panel", async ({ page }) => {
    await openApp(page);
    await cell(page, "2026-03-14").click();
    await expect(panel(page)).toHaveClass(/open/);
    await expect(page.locator("#panelBody h2")).toHaveText("Samedi 14 mars 2026");
    await expect(page.getByText("Rien de prévu ce jour-là.")).toBeVisible();
    await expect(page.getByRole("button", { name: "Ajouter un événement ce jour-là" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(panel(page)).not.toHaveClass(/open/);
    await cell(page, "2026-02-21").click();
    await expect(page.locator("#panelBody li")).toHaveCount(2); // school holidays + ski trip
  });

  test("event panel shows the full definition list", async ({ page }) => {
    await openApp(page);
    await bar(page, "Semaine de ski aux Arcs").click();
    await expect(panel(page)).toHaveClass(/open/);
    await expect(page.locator("#panelBody h2")).toHaveText("Semaine de ski aux Arcs");
    await expect(page.locator("#panelBody .sub")).toHaveText("Du sam. 21 févr. au sam. 28 févr. 2026");
    const dl = page.locator("#panelBody dl");
    await expect(dl).toContainText("CatégorieVoyage ou vacances");
    await expect(dl).toContainText("Durée8 jours");
    await expect(dl).toContainText("QuiToute la famille");
    const link = page.getByRole("link", { name: "Ouvrir dans Google Agenda" });
    await expect(link).toHaveAttribute("target", "_blank");
    await expect(link).toHaveAttribute("rel", /noopener/);
    await expect(page.getByRole("button", { name: "Modifier" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Supprimer" })).toBeVisible();
  });

  test("imported events show 'Importé automatiquement' and no buttons", async ({ page }) => {
    await openApp(page);
    await cell(page, "2026-10-17").click();
    await page.locator("#panelBody li button", { hasText: "Vacances de la Toussaint" }).click();
    await expect(page.locator("#panelBody h2")).toHaveText("Vacances de la Toussaint");
    await expect(page.getByText("Importé automatiquement")).toBeVisible();
    await expect(page.getByRole("button", { name: "Modifier" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Supprimer" })).toHaveCount(0);
    await expect(page.locator("#panelBody dl")).toContainText("CatégorieVacances scolaires");
  });

  test("keyboard: bars are buttons, Enter and Space open the panel, Escape closes it", async ({ page }) => {
    await openApp(page);
    const b = bar(page, "Week-end des cousins");
    await expect(b).toHaveAttribute("role", "button");
    await expect(b).toHaveAttribute("tabindex", "0");
    await expect(b).toHaveAttribute("aria-label", /Week-end des cousins, du sam\. 3 oct\. au dim\. 4 oct\. 2026/);
    await b.focus();
    await page.keyboard.press("Enter");
    await expect(panel(page)).toHaveClass(/open/);
    await page.keyboard.press("Escape");
    await expect(panel(page)).not.toHaveClass(/open/);
    await expect(b).toBeFocused();
    await page.keyboard.press("Space");
    await expect(panel(page)).toHaveClass(/open/);
    await page.getByRole("button", { name: "Fermer" }).click();
    await expect(panel(page)).not.toHaveClass(/open/);
  });
});

test.describe("create, edit, delete", () => {
  test("drag across days opens the dialog prefilled; the event is created", async ({ page }) => {
    await openApp(page);
    await dragCells(page, "2026-09-10", "2026-09-12");
    const dlg = page.locator("#dlg");
    await expect(dlg).toBeVisible();
    await expect(page.locator("#dlgTitle")).toHaveText("Nouvel événement");
    await expect(page.locator("#fStart")).toHaveValue("2026-09-10");
    await expect(page.locator("#fEnd")).toHaveValue("2026-09-12");
    await expect(page.locator("#fCat")).toHaveValue("trip"); // a range defaults to "Voyage ou vacances"
    await expect(page.locator("#fMem input:checked")).toHaveCount(4);
    await page.locator("#fTitle").fill("Test de glisser");
    await page.locator("#fLoc").fill("Annecy");
    await page.locator("#submit").click();
    await expect(dlg).not.toBeVisible();
    await expect(bar(page, "Test de glisser")).toBeVisible();
    await bar(page, "Test de glisser").click();
    await expect(page.locator("#panelBody dl")).toContainText("LieuAnnecy");
    await expect(page.locator("#panelBody dl")).toContainText("Ajouté parAlex");
  });

  test("drag across months", async ({ page }) => {
    await openApp(page);
    await dragCells(page, "2026-05-30", "2026-06-02");
    await expect(page.locator("#fStart")).toHaveValue("2026-05-30");
    await expect(page.locator("#fEnd")).toHaveValue("2026-06-02");
    await page.getByRole("button", { name: "Annuler" }).click();
    await expect(page.locator("#dlg")).not.toBeVisible();
    await expect(page.locator(".cell.sel")).toHaveCount(0);
  });

  test("a click without moving opens the day panel, not the dialog", async ({ page }) => {
    await openApp(page, "month");
    await cell(page, "2026-09-15").click();
    await expect(panel(page)).toHaveClass(/open/);
    await expect(page.locator("#dlg")).not.toBeVisible();
    await page.getByRole("button", { name: "Ajouter un événement ce jour-là" }).click();
    await expect(page.locator("#fStart")).toHaveValue("2026-09-15");
    await expect(page.locator("#fCat")).toHaveValue("celebration");
    await expect(panel(page)).not.toHaveClass(/open/);
  });

  test("client-side validation messages", async ({ page }) => {
    await openApp(page);
    await page.getByRole("button", { name: "Ajouter un événement", exact: true }).click();
    const err = page.locator("#fErr");
    await page.locator("#submit").click();
    await expect(err).toHaveText("Donnez un titre à l’événement.");
    await page.locator("#fTitle").fill("x");
    await page.locator("#fEnd").fill("2026-10-01");
    await page.locator("#submit").click();
    await expect(err).toHaveText("Le dernier jour ne peut pas être avant le premier.");
    await page.locator("#fEnd").fill("2026-10-16");
    for (const cb of await page.locator("#fMem input").all()) await cb.uncheck();
    await page.locator("#submit").click();
    await expect(err).toHaveText("Choisissez au moins une personne.");
    await expect(page.locator("#dlg")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator("#dlg")).not.toBeVisible();
  });

  test("birthday: birth year field, no last day, age in the panel", async ({ page }) => {
    await openApp(page);
    await cell(page, "2026-03-12").click();
    await page.locator("#panelBody li button", { hasText: "Anniversaire de Sacha" }).click();
    await expect(page.locator("#panelBody h2")).toHaveText("Anniversaire de Sacha");
    await page.getByRole("button", { name: "Modifier" }).click();
    await expect(page.locator("#dlgTitle")).toHaveText("Modifier l’événement");
    await expect(page.locator("#fCat")).toHaveValue("birthday");
    await expect(page.locator("#fEnd")).toBeHidden();
    await expect(page.locator("#fBirth")).toBeVisible();
    await page.locator("#fBirth").fill("2018");
    await page.locator("#submit").click();
    await expect(page.locator("#dlg")).not.toBeVisible();
    await cell(page, "2026-03-12").click();
    await page.locator("#panelBody li button", { hasText: "Anniversaire de Sacha" }).click();
    await expect(page.locator("#panelBody h2")).toHaveText("Anniversaire de Sacha (8 ans)");
  });

  test("edit via the panel, with the saving state", async ({ page }) => {
    await openApp(page);
    await bar(page, "Week-end des cousins").click();
    await page.getByRole("button", { name: "Modifier" }).click();
    await expect(page.locator("#fTitle")).toHaveValue("Week-end des cousins");
    await expect(page.locator("#fCat")).toHaveValue("family_gathering");
    await page.locator("#fTitle").fill("Week-end des cousins et cousines");
    await page.locator("#fLoc").fill("Rennes");
    await page.locator("#fNotes").fill("Apporter les jeux\nde société");
    await page.getByRole("checkbox", { name: "Sacha" }).uncheck();
    await page.route("**/api/events/*", async (route) => {
      await new Promise((r) => setTimeout(r, 400));
      await route.continue();
    });
    await page.locator("#submit").click();
    await expect(page.locator("#submit")).toBeDisabled();
    await expect(page.locator("#submit")).toHaveText("Enregistrement…");
    await expect(page.locator("#dlg")).not.toBeVisible();
    await expect(panel(page)).not.toHaveClass(/open/);
    await page.unroute("**/api/events/*");
    const b = bar(page, "Week-end des cousins et cousines");
    await expect(b).toBeVisible();
    await b.click();
    const dl = page.locator("#panelBody dl");
    await expect(dl).toContainText("LieuRennes");
    await expect(dl).toContainText("Qui" + "Alex, Camille, Noé");
    await expect(dl).toContainText("Notes");
    await expect(dl).toContainText("Modifié parAlex");
  });

  test("server errors are shown in the dialog, which stays open", async ({ page }) => {
    await openApp(page);
    await bar(page, "Week-end des cousins").click();
    await page.getByRole("button", { name: "Modifier" }).click();
    await page.route("**/api/events/*", (route) =>
      route.fulfill({
        status: 400,
        contentType: "application/json",
        body: JSON.stringify({ error: { code: "validation_failed", message: "Le titre est trop long (120 caractères maximum)." } }),
      }),
    );
    await page.locator("#submit").click();
    await expect(page.locator("#fErr")).toHaveText("Le titre est trop long (120 caractères maximum).");
    await expect(page.locator("#dlg")).toBeVisible();
    await expect(page.locator("#submit")).toBeEnabled();
    await expect(page.locator("#submit")).toHaveText("Enregistrer");
  });

  test("delete asks for confirmation inside the panel", async ({ page }) => {
    await openApp(page);
    await bar(page, "Road trip en Italie").click();
    await page.getByRole("button", { name: "Supprimer" }).click();
    await expect(page.getByText("Supprimer « Road trip en Italie » ?")).toBeVisible();
    await expect(page.getByText("se répète chaque année")).toHaveCount(0);
    await page.getByRole("button", { name: "Annuler" }).click();
    await expect(page.getByRole("button", { name: "Modifier" })).toBeVisible();
    await expect(bar(page, "Road trip en Italie")).toBeVisible();
    await page.getByRole("button", { name: "Supprimer" }).click();
    await page.locator(".confirm").getByRole("button", { name: "Supprimer" }).click();
    await expect(panel(page)).not.toHaveClass(/open/);
    await expect(page.locator(".bar", { hasText: "Road trip en Italie" })).toHaveCount(0);
  });

  test("deleting a birthday warns that every year goes away", async ({ page }) => {
    await openApp(page);
    await cell(page, "2026-03-12").click();
    await page.locator("#panelBody li button", { hasText: "Anniversaire de Sacha" }).click();
    await page.getByRole("button", { name: "Supprimer" }).click();
    await expect(page.getByText("Supprimer « Anniversaire de Sacha » ?")).toBeVisible();
    await expect(
      page.getByText("Cet événement se répète chaque année : toutes les occurrences seront supprimées."),
    ).toBeVisible();
    await page.locator(".confirm").getByRole("button", { name: "Supprimer" }).click();
    await expect(cell(page, "2026-03-12")).not.toHaveClass(/bd/);
    await page.getByRole("button", { name: "Année suivante" }).click();
    await expect(cell(page, "2027-03-12")).toBeVisible();
    await expect(cell(page, "2027-03-12")).not.toHaveClass(/bd/);
  });

  test("Réinitialiser la démo restores the sample data", async ({ page }) => {
    await openApp(page);
    await bar(page, "Road trip en Italie").click();
    await page.getByRole("button", { name: "Supprimer" }).click();
    await page.locator(".confirm").getByRole("button", { name: "Supprimer" }).click();
    await expect(page.locator(".bar", { hasText: "Road trip en Italie" })).toHaveCount(0);
    await page.getByRole("button", { name: "Réinitialiser la démo" }).click();
    await expect(bar(page, "Road trip en Italie")).toBeVisible();
  });
});

test.describe("unclassified events, loading, errors, session", () => {
  const unclassified = {
    id: "f:unclassified1",
    series_id: null,
    calendar: "famille",
    type: "unclassified",
    title: "Rendez-vous mystère",
    start: "2026-09-05",
    end: "2026-09-06",
    start_time: null,
    end_time: null,
    members: [],
    location: null,
    notes: null,
    birth_year: null,
    age: null,
    source: "manual",
    read_only: false,
    created_by: null,
    updated_by: null,
    google_url: null,
  };

  test("the 'Non classés' chip and a placeholder category appear only when needed", async ({ page }) => {
    await page.route("**/api/events?year=2026", async (route) => {
      const res = await route.fetch();
      const json = await res.json();
      json.events.push(unclassified);
      await route.fulfill({ response: res, json });
    });
    await openApp(page);
    const chip = page.getByRole("button", { name: "Non classés" });
    await expect(chip).toBeVisible();
    await expect(bar(page, "Rendez-vous mystère")).toHaveClass(/cat-unclassified/);
    expect(await bar(page, "Rendez-vous mystère").evaluate((el) => getComputedStyle(el).backgroundColor)).toBe("rgb(138, 147, 163)");
    await chip.click();
    await expect(page.locator(".bar", { hasText: "Rendez-vous mystère" })).toHaveCount(0);
    await chip.click();
    await bar(page, "Rendez-vous mystère").click();
    await expect(page.locator("#panelBody dl")).toContainText("CatégorieNon classé");
    await page.getByRole("button", { name: "Modifier" }).click();
    await expect(page.locator("#fCat option:checked")).toHaveText("Choisir une catégorie");
    await expect(page.locator("#fCat option:checked")).toHaveAttribute("disabled", "");
    await page.locator("#submit").click();
    await expect(page.locator("#fErr")).toHaveText("Choisissez une catégorie.");
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Année suivante" }).click();
    await expect(page.getByRole("button", { name: "Non classés" })).toHaveCount(0);
  });

  test("load failure shows an alert banner with a retry button", async ({ page }) => {
    let fail = true;
    await page.route("**/api/events?*", (route) => (fail ? route.abort() : route.continue()));
    await page.goto("/");
    const banner = page.getByRole("alert").filter({ hasText: "Impossible de charger l’agenda." });
    await expect(banner).toBeVisible();
    await expect(page.locator(".cell")).toHaveCount(365); // the empty grid is still there
    fail = false;
    await banner.getByRole("button", { name: "Réessayer" }).click();
    await expect(banner).toBeHidden();
    await expect(page.locator(".bar").first()).toBeVisible();
  });

  test("401 sends the user to the login page", async ({ page }) => {
    await page.route("**/api/events?*", (route) => route.fulfill({ status: 401, json: { error: { code: "unauthorized", message: "x" } } }), {
      times: 1,
    });
    const login = page.waitForRequest((r) => new URL(r.url()).pathname === "/connexion" && r.url().includes("next=/"));
    await page.goto("/");
    await login;
  });

  test("logout returns to the login page", async ({ page, context }) => {
    await openApp(page);
    await page.getByRole("button", { name: "Se déconnecter" }).click();
    await expect(page).toHaveURL(/\/connexion/);
    await expect(page.getByRole("button", { name: "Entrer en mode démo" })).toBeVisible();
    await context.clearCookies();
  });
});
