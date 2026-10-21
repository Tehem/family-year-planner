import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";
import type { Page } from "@playwright/test";
import { clearAccess, expect, openApp, resetDemo, test } from "./helpers";

const H = { "X-Requested-With": "annee" };

test.beforeEach(async ({ request }) => {
  await resetDemo(request);
  await clearAccess(request);
});

const cellOf = (page: Page, d: string) => page.locator(`.cell[data-d="${d}"]`);

async function refreshMinutes(page: Page): Promise<number> {
  return page.evaluate(async () => ((await (await fetch("/api/config")).json()) as { kiosk: { refresh_minutes: number } }).kiosk.refresh_minutes);
}

/** Serves `today` from /api/config with a value the test controls (the demo server's clock is fixed). */
async function overrideToday(page: Page): Promise<{ set(d: string): void }> {
  let today: string | null = null;
  await page.route("**/api/config", async (route) => {
    const res = await route.fetch();
    const j = (await res.json()) as { today: string };
    if (today) j.today = today;
    await route.fulfill({ response: res, json: j });
  });
  return { set: (d) => (today = d) };
}

test.describe("kiosk screen", () => {
  test("title, year, static legend and a grid; nothing interactive", async ({ page }) => {
    await page.goto("/kiosk?view=week");
    await expect(page.locator(".bar").first()).toBeVisible();
    await expect(page.getByRole("heading", { name: "Année en famille" })).toBeVisible();
    await expect(page.locator("#yl")).toHaveText("2026");
    const chips = page.locator("#legend .chip.static");
    expect(await chips.count()).toBeGreaterThanOrEqual(9);
    await expect(chips.first()).toContainText("Anniversaires");
    await expect(chips.last()).toContainText("Les deux");
    expect(await page.locator("#legend .chip:not(.static)").count()).toBe(0);
    const interactive = await page.locator("button, input, select, textarea, a[href], [tabindex], dialog[open]").count();
    expect(interactive).toBe(0);
    await expect(cellOf(page, "2026-10-16")).toHaveClass(/today/);
    // hovering shows no tooltip
    await cellOf(page, "2026-10-16").hover();
    await expect(page.locator("#tip")).not.toHaveClass(/on/);
  });

  test("view comes from ?view= and falls back to the configured default", async ({ page }) => {
    await page.goto("/kiosk?view=month");
    await expect(page.locator("#canvas")).toHaveClass(/v-month/);
    await page.goto("/kiosk?view=week");
    await expect(page.locator("#canvas")).toHaveClass(/v-week/);
    const def = await page.evaluate(async () => ((await (await fetch("/api/config")).json()) as { kiosk: { default_view: string } }).kiosk.default_view);
    await page.goto("/kiosk?view=bogus");
    await expect(page.locator("#canvas")).toHaveClass(def === "month" ? /v-month/ : /v-week/);
    await page.goto("/kiosk");
    await expect(page.locator("#canvas")).toHaveClass(def === "month" ? /v-month/ : /v-week/);
  });

  test("tapping a day opens a read-only panel that closes after 20 s", async ({ page, request }) => {
    const ev = ((await (await request.get("/api/events?year=2026")).json()) as { events: { title: string; start: string }[] }).events.find(
      (e) => e.start.startsWith("2026"),
    )!;
    await page.clock.install({ time: new Date("2026-10-16T10:00:00+02:00") });
    await page.goto("/kiosk?view=week");
    await expect(page.locator(".bar").first()).toBeVisible();
    await cellOf(page, ev.start).click();
    const panel = page.locator("#panel");
    await expect(panel).toHaveClass(/open/);
    await expect(panel.locator("h2")).toHaveText(/\d{4}$/);
    await expect(panel).toContainText(ev.title);
    expect(await panel.locator("button, a, input, [data-open], [data-add]").count()).toBe(0);
    await page.clock.fastForward(19_000);
    await expect(panel).toHaveClass(/open/);
    await page.clock.fastForward(2_000);
    await expect(panel).not.toHaveClass(/open/);
  });

  test("tapping outside closes the panel; an empty day says so", async ({ page }) => {
    await page.goto("/kiosk?view=week");
    await expect(page.locator(".bar").first()).toBeVisible();
    await cellOf(page, "2026-10-16").click();
    await expect(page.locator("#panel")).toHaveClass(/open/);
    await page.getByRole("heading", { name: "Année en famille" }).click();
    await expect(page.locator("#panel")).not.toHaveClass(/open/);
    // a tap inside the panel keeps it open
    await cellOf(page, "2026-10-16").click();
    await page.locator("#panel").click({ position: { x: 20, y: 300 } });
    await expect(page.locator("#panel")).toHaveClass(/open/);
  });

  test("offline: last data stays, an indicator appears, and goes away after recovery", async ({ page }) => {
    await page.clock.install({ time: new Date("2026-10-16T10:00:00+02:00") });
    await page.goto("/kiosk?view=week");
    await expect(page.locator(".bar").first()).toBeVisible();
    const bars = await page.locator(".bar").count();
    const minutes = await refreshMinutes(page);
    await expect(page.locator("#offline")).toBeHidden();

    await page.route(/\/api\/events/, (route) => route.abort("failed"));
    await page.clock.fastForward(minutes * 60_000 + 1_000);
    const badge = page.locator("#offline");
    await expect(badge).toBeVisible();
    await expect(badge).toHaveText(/^Hors ligne depuis 10:0\d$/);
    expect(await page.locator(".bar").count()).toBe(bars);
    const box = await badge.boundingBox();
    const vp = page.viewportSize()!;
    expect(box!.x + box!.width).toBeGreaterThan(vp.width - 40);
    expect(box!.y + box!.height).toBeGreaterThan(vp.height - 40);
    const since = await badge.textContent();

    // still failing: the time of the FIRST failure is kept
    await page.clock.fastForward(31_000);
    await expect(badge).toHaveText(since!);

    await page.unroute(/\/api\/events/);
    await page.clock.fastForward(31_000);
    await expect(badge).toBeHidden();
    expect(await page.locator(".bar").count()).toBe(bars);
  });

  test("refresh re-renders without blanking the grid", async ({ page, request }) => {
    await page.clock.install({ time: new Date("2026-10-16T10:00:00+02:00") });
    await page.goto("/kiosk?view=week");
    await expect(page.locator(".bar").first()).toBeVisible();
    const minutes = await refreshMinutes(page);
    // record every moment the canvas has no cell
    await page.evaluate(() => {
      const w = window as unknown as { __blank: number };
      w.__blank = 0;
      new MutationObserver(() => {
        if (!document.querySelector("#canvas .cell")) w.__blank++;
      }).observe(document.getElementById("canvas")!, { childList: true, subtree: true });
    });
    const r = await request.post("/api/events", {
      headers: H,
      data: { type: "trip", title: "Voyage kiosque", start: "2026-10-20", end: "2026-10-22", members: [] },
    });
    expect(r.ok()).toBeTruthy();
    await page.clock.fastForward(minutes * 60_000 + 1_000);
    await expect(page.locator(".bar", { hasText: "Voyage kiosque" }).first()).toBeVisible();
    expect(await page.evaluate(() => (window as unknown as { __blank: number }).__blank)).toBe(0);
  });

  test("the today ring moves at local midnight", async ({ page }) => {
    await page.clock.install({ time: new Date("2026-10-16T23:59:40+02:00") });
    const cfg = await overrideToday(page);
    await page.goto("/kiosk?view=week");
    await expect(cellOf(page, "2026-10-16")).toHaveClass(/today/);
    cfg.set("2026-10-17");
    await page.clock.fastForward(25_000);
    await expect(cellOf(page, "2026-10-17")).toHaveClass(/today/);
    await expect(cellOf(page, "2026-10-16")).not.toHaveClass(/today/);
  });

  test("the year changes on 1 January", async ({ page }) => {
    await page.clock.install({ time: new Date("2026-12-31T23:59:40+01:00") });
    const cfg = await overrideToday(page);
    await page.goto("/kiosk?view=month");
    await expect(page.locator("#yl")).toHaveText("2026");
    await expect(cellOf(page, "2026-10-16")).toBeVisible();
    cfg.set("2027-01-01");
    const req = page.waitForRequest(/\/api\/events\?year=2027/);
    await page.clock.fastForward(25_000);
    await req;
    await expect(page.locator("#yl")).toHaveText("2027");
    await expect(cellOf(page, "2027-01-01")).toHaveClass(/today/);
    await expect(cellOf(page, "2026-10-16")).toHaveCount(0);
  });

  test("the page reloads itself at 03:00", async ({ page }) => {
    await page.clock.install({ time: new Date("2026-10-16T02:59:40+02:00") });
    await page.goto("/kiosk?view=week");
    await expect(page.locator(".bar").first()).toBeVisible();
    await page.evaluate(() => ((window as unknown as { __m: number }).__m = 1));
    await page.clock.fastForward(30_000);
    await expect
      .poll(() => page.evaluate(() => (window as unknown as { __m?: number }).__m).catch(() => 1), { timeout: 10_000 })
      .toBeUndefined();
    await expect(page.locator(".bar").first()).toBeVisible();
  });

  test("a screen wake lock is requested, and again when the page becomes visible", async ({ page }) => {
    await page.addInitScript(() => {
      const w = window as unknown as { __wl: number };
      w.__wl = 0;
      Object.defineProperty(navigator, "wakeLock", {
        configurable: true,
        value: {
          request: async () => {
            w.__wl++;
            return { released: false, type: "screen", release: async () => undefined, addEventListener() {}, removeEventListener() {} };
          },
        },
      });
    });
    await page.goto("/kiosk?view=week");
    const count = () => page.evaluate(() => (window as unknown as { __wl: number }).__wl);
    await expect.poll(count).toBe(1);
    const setVisibility = (v: string) =>
      page.evaluate((state) => {
        Object.defineProperty(document, "visibilityState", { configurable: true, get: () => state });
        document.dispatchEvent(new Event("visibilitychange"));
      }, v);
    await setVisibility("hidden");
    await setVisibility("visible");
    await expect.poll(count).toBe(2);
  });

  test("the pointer is hidden after 5 s without movement", async ({ page }) => {
    await page.clock.install({ time: new Date("2026-10-16T10:00:00+02:00") });
    await page.goto("/kiosk?view=week");
    await expect(page.locator(".bar").first()).toBeVisible();
    await page.mouse.move(200, 200);
    await expect(page.locator("body")).not.toHaveClass(/idle/);
    await page.clock.fastForward(5_100);
    await expect(page.locator("body")).toHaveClass(/idle/);
    expect(await cellOf(page, "2026-10-16").evaluate((el) => getComputedStyle(el).cursor)).toBe("none");
    await page.mouse.move(240, 220);
    await expect(page.locator("body")).not.toHaveClass(/idle/);
  });

  test("dark theme", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "dark" });
    await page.goto("/kiosk");
    await expect(page.locator(".cell").first()).toBeVisible();
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe("rgb(17, 20, 26)");
  });
});

test.describe("kiosk link", () => {
  test("the token is exchanged for a read-only session and disappears from the URL", async ({ browser, request, baseURL }) => {
    const created = await request.post("/api/admin/tokens", { headers: H, data: { name: "", kiosk: true } });
    expect(created.status()).toBe(201);
    const { kiosk_url, id } = (await created.json()) as { kiosk_url: string; id: string };
    const u = new URL(kiosk_url);
    const ctx = await browser.newContext({ baseURL, storageState: { cookies: [], origins: [] }, locale: "fr-FR", timezoneId: "Europe/Paris", viewport: { width: 1440, height: 860 } });
    const page = await ctx.newPage();
    await page.goto(u.pathname + u.search);
    await expect(page).toHaveURL(`${baseURL}/kiosk`);
    expect(page.url()).not.toContain("token");
    await expect(page.locator(".bar").first()).toBeVisible();
    const me = await page.evaluate(async () => (await (await fetch("/api/me")).json()) as { kiosk: boolean });
    expect(me.kiosk).toBe(true);
    const write = await page.evaluate(async () => (await fetch("/api/events", { method: "POST", headers: { "X-Requested-With": "annee", "Content-Type": "application/json" }, body: "{}" })).status);
    expect(write).toBeGreaterThanOrEqual(400);
    // the main screen needs a web session
    await page.goto("/");
    await expect(page).toHaveURL(/\/connexion/);
    // revoking the token ends the session
    await page.goto("/kiosk");
    await expect(page.locator(".bar").first()).toBeVisible();
    expect((await request.delete(`/api/admin/tokens/${id}`, { headers: H })).status()).toBe(204);
    await page.goto("/kiosk");
    await expect(page).toHaveURL(/\/connexion/);
    await ctx.close();
  });

  test("an unknown token leads to the login page with an explanation", async ({ browser, baseURL }) => {
    const ctx = await browser.newContext({ baseURL, storageState: { cookies: [], origins: [] }, locale: "fr-FR" });
    const page = await ctx.newPage();
    await page.goto("/kiosk?token=fy_pat_inconnu");
    await expect(page).toHaveURL(/\/connexion\?error=kiosk/);
    await expect(page.getByRole("alert")).toHaveText("Ce lien kiosque n’est plus valide.");
    await ctx.close();
  });
});

// The kiosk reuses the main screen's renderer: its grid must look the same.
for (const scheme of ["light", "dark"] as const) {
  for (const view of ["week", "month"] as const) {
    test(`kiosk grid equals the main screen's grid: ${scheme}, ${view}`, async ({ page, context }, info) => {
      await page.emulateMedia({ colorScheme: scheme });
      await page.clock.setFixedTime("2026-10-16T10:00:00");
      await openApp(page, view);
      const kiosk = await context.newPage();
      await kiosk.emulateMedia({ colorScheme: scheme });
      await kiosk.clock.setFixedTime("2026-10-16T10:00:00");
      await kiosk.goto(`/kiosk?view=${view}`);
      await expect(kiosk.locator(".bar").first()).toBeVisible();
      const settle = async (p: Page) =>
        p.evaluate(async () => {
          await Promise.all(["400", "600", "800"].map((w) => document.fonts.load(`${w} 16px "Bricolage Grotesque"`)));
          await document.fonts.ready;
          await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
        });
      await settle(page);
      await settle(kiosk);
      // same cell size on both, so that only the rendering is compared
      const cs = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--cs").trim());
      await kiosk.evaluate((v) => document.documentElement.style.setProperty("--cs", v), cs);
      await page.mouse.move(0, 0);
      await kiosk.mouse.move(0, 0);
      const a = PNG.sync.read(await page.locator("#canvas").screenshot());
      const k = PNG.sync.read(await kiosk.locator("#canvas").screenshot());
      expect([k.width, k.height]).toEqual([a.width, a.height]);
      const diff = new PNG({ width: a.width, height: a.height });
      const n = pixelmatch(a.data, k.data, diff.data, a.width, a.height, { threshold: 0.1 });
      const ratio = n / (a.width * a.height);
      const name = `kiosk-${scheme}-${view}`;
      mkdirSync(info.outputDir, { recursive: true });
      writeFileSync(resolve(info.outputDir, `${name}-diff.png`), PNG.sync.write(diff));
      console.log(`${name}: ${(ratio * 100).toFixed(3)}% of pixels differ`);
      expect(ratio).toBeLessThanOrEqual(0.03);
      await kiosk.close();
    });
  }
}
