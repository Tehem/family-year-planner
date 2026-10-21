import type { Page } from "@playwright/test";
import { expect, openApp, resetDemo, test } from "./helpers";

test.beforeEach(async ({ request }) => {
  await resetDemo(request);
});

/** Number of pages of a PDF: `/Type /Page` objects, not `/Type /Pages`. */
export function countPages(pdf: Buffer): number {
  return (pdf.toString("latin1").match(/\/Type\s*\/Page(?![A-Za-z])/g) ?? []).length;
}

async function startPrint(page: Page, format: "A3" | "A4"): Promise<void> {
  // headless Chromium has no print dialog: record the call instead
  await page.evaluate(() => {
    (window as unknown as { printed: number }).printed = 0;
    window.print = () => {
      (window as unknown as { printed: number }).printed++;
    };
  });
  await page.getByRole("button", { name: "Imprimer", exact: true }).click();
  const pop = page.locator("#printPop");
  await expect(pop).toBeVisible();
  await expect(pop.getByLabel("Format")).toHaveValue("a3"); // A3 is the default
  await pop.getByLabel("Format").selectOption({ label: `${format} paysage` });
  await pop.getByRole("button", { name: "Imprimer" }).click();
  expect(await page.evaluate(() => (window as unknown as { printed: number }).printed)).toBe(1);
}

for (const view of ["week", "month"] as const) {
  for (const format of ["A3", "A4"] as const) {
    test(`print: one page in ${format} landscape, ${view === "week" ? "Weeks" : "Months"} view`, async ({ page }) => {
      await openApp(page, view);
      await startPrint(page, format);
      const pdf = await page.pdf({ format, landscape: true });
      expect(countPages(pdf)).toBe(1);
      // the same layout, honouring the page size declared in CSS
      const css = await page.pdf({ preferCSSPageSize: true });
      expect(countPages(css)).toBe(1);
    });
  }
}

test("print layout: light header, legend, filters line, footer; no controls", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await openApp(page);
  await page.locator('[data-cat="holiday"]').click();
  await page.getByRole("button", { name: "Camille", exact: true }).click();
  await startPrint(page, "A4");
  await page.emulateMedia({ media: "print", colorScheme: "dark" });
  await expect(page.locator(".print-head")).toBeVisible();
  await expect(page.locator(".print-head h1")).toHaveText("Année en famille");
  await expect(page.locator("#printYear")).toHaveText("2026");
  await expect(page.locator("#printFilters")).toHaveText("Filtres : sans Jours fériés · Camille");
  await expect(page.locator("#printLegend")).toContainText("Anniversaires");
  await expect(page.locator("#printLegend")).not.toContainText("Jours fériés");
  await expect(page.locator("#printFoot")).toHaveText("Imprimé le 16 octobre 2026");
  await expect(page.locator(".top")).toBeHidden();
  await expect(page.locator("#panel")).toBeHidden();
  await expect(page.locator("#tip")).toBeHidden();
  await expect(page.locator("#banner")).toBeHidden();
  // always light, whatever the system theme
  expect(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--cell").trim())).toBe("#FFFFFF");
  expect(await page.evaluate(() => getComputedStyle(document.body).colorScheme)).toBe("light");
  expect(await page.evaluate(() => getComputedStyle(document.querySelector(".cell")!).getPropertyValue("print-color-adjust"))).toBe("exact");
  await expect(page.locator(".bar").first()).toBeVisible();
  // the grid fits inside the sheet
  const box = await page.locator("#canvas").boundingBox();
  const sheet = await page.evaluate(() => document.body.getBoundingClientRect().toJSON() as DOMRect);
  expect(box!.x).toBeGreaterThanOrEqual(sheet.x);
  expect(box!.x + box!.width).toBeLessThanOrEqual(sheet.x + sheet.width + 1);
  expect(box!.y + box!.height).toBeLessThanOrEqual(sheet.y + sheet.height + 1);
});

test("print popover can be dismissed", async ({ page }) => {
  await openApp(page);
  const btn = page.locator("#printBtn");
  await btn.click();
  await expect(page.locator("#printPop")).toBeVisible();
  await expect(btn).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("Escape");
  await expect(page.locator("#printPop")).toBeHidden();
  await expect(btn).toBeFocused();
});
