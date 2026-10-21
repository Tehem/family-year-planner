import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, devices } from "@playwright/test";

// PW_PORT lets several checkouts run the suite side by side (a server already on the port is reused).
const PORT = Number(process.env["PW_PORT"] ?? 4173);
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const env: Record<string, string> = {};
for (const [k, v] of Object.entries(process.env)) if (v !== undefined) env[k] = v;

export default defineConfig({
  testDir: "./tests",
  // Tests share one demo server and mutate its data: run them one after the other.
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 30_000,
  expect: { timeout: 7_000 },
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    viewport: { width: 1440, height: 860 },
    locale: "fr-FR",
    timezoneId: "Europe/Paris",
    trace: "retain-on-failure",
  },
  projects: [
    { name: "setup", testMatch: /auth\.setup\.ts/ },
    {
      name: "chromium",
      testIgnore: /auth\.setup\.ts/,
      use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 860 }, storageState: "playwright/.auth/state.json" },
      dependencies: ["setup"],
    },
  ],
  webServer: {
    // Debug builds read web/dist from disk, so a fresh `npm run build` is enough.
    command: "cargo run -q -- demo",
    cwd: repoRoot,
    env: {
      ...env,
      BIND: `127.0.0.1:${PORT}`,
      APP_MODE: "demo",
      APP_FAKE_TODAY: "2026-10-16",
    },
    url: `http://127.0.0.1:${PORT}/healthz`,
    reuseExistingServer: true,
    timeout: 15 * 60_000,
    stdout: "ignore",
    stderr: "pipe",
  },
});
