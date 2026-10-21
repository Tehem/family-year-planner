import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import type { Page } from "@playwright/test";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";
import { expect, openApp, resetDemo, test } from "./helpers";

// SPEC §16.2 visual check: the app against design/prototype.html with the same fixed date.
const here = dirname(fileURLToPath(import.meta.url));
const PROTOTYPE = pathToFileURL(resolve(here, "../../design/prototype.html")).href;
const FONT_DIR = resolve(here, "../src/fonts");
const MAX_DIFF = 0.03;

const FONT_CSS = `
@font-face{font-family:'Bricolage Grotesque';font-style:normal;font-weight:200 800;font-stretch:100%;
  src:url(https://fonts.gstatic.com/local/latin-ext.woff2) format('woff2');
  unicode-range:U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C4,U+2113,U+2C60-2C7F,U+A720-A7FF}
@font-face{font-family:'Bricolage Grotesque';font-style:normal;font-weight:200 800;font-stretch:100%;
  src:url(https://fonts.gstatic.com/local/latin.woff2) format('woff2');
  unicode-range:U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD}
`;

/** Offline and deterministic: the prototype's Google Fonts requests get the self-hosted files. */
async function isolate(page: Page): Promise<void> {
  await page.route("**/*", async (route) => {
    const url = new URL(route.request().url());
    if (url.hostname === "fonts.googleapis.com") return route.fulfill({ contentType: "text/css", body: FONT_CSS });
    if (url.hostname === "fonts.gstatic.com") {
      const file = url.pathname.endsWith("latin-ext.woff2") ? "bricolage-latin-ext.woff2" : "bricolage-latin.woff2";
      return route.fulfill({ contentType: "font/woff2", body: readFileSync(resolve(FONT_DIR, file)) });
    }
    if (/^(http:\/\/127\.0\.0\.1|file:|data:|about:|blob:)/.test(route.request().url())) return route.continue();
    return route.abort();
  });
}

async function settle(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await Promise.all(["400", "600", "800"].map((w) => document.fonts.load(`${w} 16px "Bricolage Grotesque"`)));
    await document.fonts.ready;
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  });
  await page.mouse.move(0, 0);
}

async function boardClip(page: Page): Promise<{ x: number; y: number; width: number; height: number }> {
  return page.evaluate(() => {
    const r = document.getElementById("board")!.getBoundingClientRect();
    return { x: 0, y: Math.round(r.top), width: innerWidth, height: innerHeight - Math.round(r.top) };
  });
}

for (const scheme of ["light", "dark"] as const) {
  for (const view of ["week", "month"] as const) {
    test(`visual check: ${scheme} theme, ${view === "week" ? "Weeks" : "Months"} view`, async ({ page, context, request }, info) => {
      await resetDemo(request);
      const fixed = "2026-10-16T10:00:00";
      // Prototype
      const proto = await context.newPage();
      await proto.emulateMedia({ colorScheme: scheme });
      await proto.clock.setFixedTime(fixed);
      await isolate(proto);
      await proto.goto(PROTOTYPE);
      await proto.locator(`[data-view="${view}"]`).click();
      await settle(proto);

      // App
      await page.emulateMedia({ colorScheme: scheme });
      await page.clock.setFixedTime(fixed);
      await isolate(page);
      await openApp(page, view);
      await settle(page);

      // The header legitimately differs (extra buttons) but must not change the grid's size.
      const clipP = await boardClip(proto);
      const clipA = await boardClip(page);
      expect(clipA, "same board box").toEqual(clipP);
      expect(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--cs"))).toBe(
        await proto.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--cs")),
      );

      const a = PNG.sync.read(await page.screenshot({ clip: clipA }));
      const p = PNG.sync.read(await proto.screenshot({ clip: clipP }));
      expect([a.width, a.height]).toEqual([p.width, p.height]);
      const diff = new PNG({ width: a.width, height: a.height });
      const n = pixelmatch(a.data, p.data, diff.data, a.width, a.height, { threshold: 0.1 });
      const ratio = n / (a.width * a.height);
      const name = `visual-${scheme}-${view}`;
      mkdirSync(info.outputDir, { recursive: true });
      writeFileSync(resolve(info.outputDir, `${name}-app.png`), PNG.sync.write(a));
      writeFileSync(resolve(info.outputDir, `${name}-prototype.png`), PNG.sync.write(p));
      writeFileSync(resolve(info.outputDir, `${name}-diff.png`), PNG.sync.write(diff));
      console.log(`${name}: ${(ratio * 100).toFixed(3)}% of pixels differ (${n}/${a.width * a.height})`);
      expect(ratio).toBeLessThanOrEqual(MAX_DIFF);
      await proto.close();
    });
  }
}
