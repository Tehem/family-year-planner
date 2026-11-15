import { expect, test } from "@playwright/test";

test.describe("Mobile header menu", () => {
  test.use({ viewport: { width: 390, height: 800 } });

  test("filters and actions are collapsed behind a toggle on a phone", async ({ page }) => {
    await page.goto("/");
    const toggle = page.locator("#menuBtn");
    await expect(toggle).toBeVisible();
    await expect(page.locator("#legend")).toBeHidden();
    await expect(page.locator("#addBtn")).toBeHidden();
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    await expect(page.locator("#legend")).toBeVisible();
    await expect(page.locator("#addBtn")).toBeVisible();
    await toggle.click();
    await expect(page.locator("#legend")).toBeHidden();
  });
});

test("no toggle on a desktop screen", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#menuBtn")).toBeHidden();
  await expect(page.locator("#legend")).toBeVisible();
});
