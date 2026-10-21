import { createHash, randomBytes } from "node:crypto";
import { expect, test as base, type APIRequestContext, type Locator, type Page } from "@playwright/test";

/**
 * `test` with an automatic guard on every page: no request may leave the app's origin
 * (SPEC §18: no third-party requests) and the CSP must never be violated.
 */
export const test = base.extend<{ guard: void }>({
  guard: [
    async ({ page, baseURL }, use) => {
      const origin = new URL(baseURL ?? "http://127.0.0.1:4173").origin;
      const foreign: string[] = [];
      const violations: string[] = [];
      page.on("request", (r) => {
        const u = r.url();
        if (/^(data|blob|about):/.test(u)) return;
        const url = new URL(u);
        // `.invalid` hosts never resolve: OAuth tests use them as redirect targets they intercept.
        if (url.origin !== origin && !url.hostname.endsWith(".invalid")) foreign.push(u);
      });
      page.on("console", (m) => {
        const t = m.text();
        if (/Content Security Policy|CSPVIOLATION/i.test(t)) violations.push(t);
      });
      await page.addInitScript(() => {
        document.addEventListener("securitypolicyviolation", (e) => {
          console.error(`CSPVIOLATION ${e.violatedDirective} ${e.blockedURI}`);
        });
      });
      await use();
      expect(foreign, "third-party requests").toEqual([]);
      expect(violations, "CSP violations").toEqual([]);
    },
    { auto: true },
  ],
});
export { expect };

export async function resetDemo(request: APIRequestContext): Promise<void> {
  const r = await request.post("/api/demo/reset", { headers: { "X-Requested-With": "annee" }, data: {} });
  expect(r.ok()).toBeTruthy();
}

export const cell = (page: Page, d: string): Locator => page.locator(`.cell[data-d="${d}"]`);
export const bar = (page: Page, title: string): Locator => page.locator(".bar", { hasText: title }).first();

export async function openApp(page: Page, view: "week" | "month" = "week"): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".bar").first()).toBeVisible();
  if (view === "month") await page.getByRole("button", { name: "Mois" }).click();
  else await page.getByRole("button", { name: "Semaines" }).click();
}

/** Mouse drag from the centre of one cell to the centre of another. */
export async function dragCells(page: Page, from: string, to: string): Promise<void> {
  const a = await cell(page, from).boundingBox();
  const b = await cell(page, to).boundingBox();
  if (!a || !b) throw new Error("cells not found");
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 10 });
  await page.mouse.up();
}

// ---- OAuth helpers (a single client per spec file: registrations are rate limited to 10/hour/IP) ----

export const REDIRECT = "https://agent.invalid/callback";
export const LOOPBACK = "http://127.0.0.1:8123/callback";

export function pkce(): { verifier: string; challenge: string } {
  const verifier = randomBytes(32).toString("base64url");
  return { verifier, challenge: createHash("sha256").update(verifier).digest("base64url") };
}

export async function registerClient(request: APIRequestContext, name: string): Promise<string> {
  const r = await request.post("/oauth/register", { data: { client_name: name, redirect_uris: [REDIRECT, LOOPBACK] } });
  expect(r.status(), await r.text()).toBe(201);
  return ((await r.json()) as { client_id: string }).client_id;
}

export function authorizePath(clientId: string, challenge: string, o: { redirect?: string; scope?: string; state?: string } = {}): string {
  const q = new URLSearchParams({
    response_type: "code",
    client_id: clientId,
    redirect_uri: o.redirect ?? REDIRECT,
    code_challenge: challenge,
    code_challenge_method: "S256",
    state: o.state ?? "st-123",
  });
  if (o.scope) q.set("scope", o.scope);
  return `/oauth/authorize?${q.toString()}`;
}

/**
 * Captures the redirect that answers the consent form (the redirect URI is unreachable and Playwright
 * does not route redirects). The browser never follows it, so the tests do not exercise the CSP check
 * described in D25: Chromium applies `form-action 'self'` to that redirect (backend change needed).
 */
export async function interceptConsent(page: Page): Promise<{ redirectedTo(): Promise<URL> }> {
  let resolve!: (u: URL) => void;
  const redirected = new Promise<URL>((r) => (resolve = r));
  await page.route(/\/oauth\/authorize/, async (route) => {
    const res = await route.fetch({ maxRedirects: 0 });
    if (route.request().method() !== "GET") {
      // The client's redirect URI is unreachable (and Playwright does not route redirects): capture it.
      resolve(new URL(res.headers()["location"] ?? "", route.request().url()));
      return route.fulfill({ status: 200, contentType: "text/html", body: "<p>Redirection capturée par le test.</p>" });
    }
    await route.fulfill({ response: res });
  });
  return { redirectedTo: () => redirected };
}

/** Revokes every personal token and OAuth connection (they live in SQLite, not in the demo store). */
export async function clearAccess(request: APIRequestContext): Promise<void> {
  const h = { "X-Requested-With": "annee" };
  const tokens = (await (await request.get("/api/admin/tokens")).json()) as { id: string }[];
  for (const t of tokens) await request.delete(`/api/admin/tokens/${t.id}`, { headers: h });
  const grants = (await (await request.get("/api/admin/oauth-grants")).json()) as { id: string }[];
  for (const g of grants) await request.delete(`/api/admin/oauth-grants/${g.id}`, { headers: h });
}
