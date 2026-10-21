import { expect, test as setup } from "@playwright/test";

// Signs in once through the demo login page and stores the session for the other tests.
setup("demo login", async ({ page }) => {
  await page.goto("/connexion");
  await page.getByRole("button", { name: "Entrer en mode démo" }).click();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.locator(".cell").first()).toBeVisible();
  await page.context().storageState({ path: "playwright/.auth/state.json" });
});
