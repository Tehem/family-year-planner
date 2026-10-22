# Progress

## M1 — Workspace skeleton ✅

- Cargo workspace (`domain`, `gcal`, `server`), binary `annee` with `serve` / `demo`, `/healthz`,
  embedded `web/dist` (placeholder page until the front-end is built).
- `web/` Vite + TypeScript (strict) project, multi-page build config.
- CI workflow (fmt, clippy, test, web typecheck/build, e2e once configured).
- Next: M2 domain crate (written in the following commit).

## M2 — Domain crate ✅

- `annee-domain`: type/colorId mapping, `Event`/`EventId`, validation with the exact French
  messages, inclusive/exclusive dates and timed events (incl. DST), Easter + French public
  holidays, birthdays (RRULE, 29 February, age), free periods, rendering helpers (tints, lanes).
- 66 unit tests.
- Next: M3 calendar stores (`CalendarStore`, `DemoStore`, `GoogleStore`) and the JSON API.

## M3 — Calendar stores + API ✅

- `annee-gcal`: `CalendarStore` trait, `DemoStore` (prototype sample data, yearly birthday series),
  `GoogleStore` (service-account JWT auth, typed REST client, 60 s per-year cache, etag/`If-Match`
  retry, `events.move` on cross-calendar type changes, imported-event helpers) — 41 tests incl.
  `wiremock` suites.
- `annee-server`: config (TOML + env, validation), sessions (encrypted cookie, sliding expiry),
  security headers/CSRF guards, JSON API (`/api/me|config|events`), demo login, page routes.
  16 API integration tests.
- Next: M4 front-end port (parallel with M5 imports).

## M5 — Imports ✅ (backend; Réglages UI follows with the front-end pass)

- SQLite storage + migrations (all §14 tables), `annee import [--dry-run]`.
- School holidays (data.education.gouv.fr, verified formats, 3 recorded fixtures), public holidays,
  upsert/plan/apply with idempotency, retry/backoff (3 attempts), single-run lock, run history,
  cron scheduler (Europe/Paris), startup catch-up, `GET|POST /api/admin/import`.
- 19 import tests (wiremock for the dataset API, algorithm tests over the demo store).
- Remaining for the Réglages page: the "Imports" section UI (front-end pass).
- Next: M6 Google web login.

## M6 — Web auth ✅

- Google Sign-In (OIDC code + PKCE, ID-token verification, allowlist, user names), sessions,
  logout, demo login kept for demo mode, `/auth/*` rate limit. 12 tests against a mocked Google.
- Front-end note: `/connexion` must handle `?error=invalid|denied|failed|not_allowed&email=`.
- Next: M7 MCP server + personal access tokens.

## M7 — MCP + PAT ✅ (backend; Réglages UI follows with the front-end pass)

- Personal access tokens (hash-only storage, shown once, unique active names, `last_used_at`
  throttled to once a minute), `GET|POST|DELETE /api/admin/tokens`, kiosk-link tokens.
- `/mcp` over rmcp 3.5 (stateless Streamable HTTP): `get_context`, `list_events`, `get_day`,
  `find_free_periods`, `create_event`, `update_event`, `delete_event`; 401 + `WWW-Authenticate`,
  Origin check, 120 req/min/token, scope enforcement, agent name recorded in `fy_*_by`.
- 12 tests through the official rmcp client plus 5 unit tests.
- Next: M8 OAuth 2.1 authorization server (DCR, CIMD, PKCE, refresh rotation).

## M8 — OAuth server ✅ (backend; consent stylesheet + Réglages UI follow with the front-end pass)

- Discovery documents, DCR, CIMD, authorize + consent (server-rendered), token (form only),
  refresh rotation with reuse detection, revocation, grants API, access-token auth on `/mcp`.
- 22 integration tests + unit tests; verified end to end with the rmcp client.
- Next: merge the M4 front-end branch, then the remaining pages (Réglages, consent, kiosk), M9, M10.

## M9 (server side) + M10 — kiosk sessions, packaging ✅

- Kiosk: `/kiosk?token=` exchange → `annee_kiosk` cookie (400 days, bound to the token id), read-only
  principal, revocation ends the session; 5 tests.
- Packaging: multi-stage `Dockerfile` (Node 22 → Rust bookworm → distroless nonroot, 76 MB, verified
  building and answering `/healthz`), `deploy/` (Compose, Caddyfile, `config.example.toml`,
  `.env.example`), `release.yml` (multi-arch GHCR image), `annee check-config`, `annee healthcheck`,
  `docs/SETUP.md` aligned with the implementation.
- Front-end pages for Réglages, kiosk and the consent screen are being built separately.

## M4 + front-end pass ✅ (main screen, print, Réglages, kiosk, consent, login)

- Main screen pixel-identical to the prototype (0.000% in light/dark, both views), edit dialog, panel,
  print (single page A3/A4), Réglages (tokens, OAuth connections, kiosk link, imports), kiosk, consent.
- Playwright: 88 tests passing. Fixed a CSP/redirect bug on the consent decision (D26).

## V1.1 — languages and configurable types (SPEC §19) ✅

- M11 `domain`/`gcal`: `Lang`, `TypeCatalog` (names, colors, custom types, color mapping), localized
  validation, catalog-aware stores, `count_colors` / `recolor` (wiremock + demo tests).
- M12 server: request-scoped language layer, `type_settings` / `type_migrations`, `TypeAdmin` (preview,
  resumable migration), `/api/admin/types`, dynamic MCP tool schemas, localized errors/consent page/imported titles.
- M13 front-end: `i18n.ts` (FR/EN), language switch, palette CSS + `colors.ts`, dynamic legend and dialog,
  Catégories editor in Réglages. Playwright: 103 tests, visual diff still 0.000 %.
- M14 docs: SPEC §19, DECISIONS D30–D38, SETUP, README.
- Not verified here: a real color migration against Google Calendar (covered by wiremock tests only).

## Definition of Done

All milestones M1–M10 are implemented. Not verifiable here: CI on a GitHub remote, and anything
needing real Google credentials (see `docs/SETUP.md`).
