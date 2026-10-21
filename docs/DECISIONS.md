# Decisions

One entry per decision taken where the spec was silent or ambiguous, with context, choice and
alternatives considered.

## D1 — Rust toolchain installed via Homebrew `rustup`

- **Context:** the development machine had no Rust toolchain.
- **Choice:** `brew install rustup` + `rustup default stable` (Rust 1.99, edition 2024).
- **Alternatives:** build only inside Docker (slow edit/compile loop).

## D2 — Vite 6 (not 7)

- **Context:** the dev machine runs Node 20.10; Vite 7 needs Node ≥ 20.19. CI uses Node 22.
- **Choice:** Vite 6, which supports Node 18/20/22.

## D3 — `web/dist` is created by `build.rs`

- **Context:** `rust-embed` needs the folder at compile time, but `cargo test` must work before
  the front-end has been built.
- **Choice:** `crates/server/build.rs` creates an empty `web/dist` if missing; the server then
  serves a French placeholder page for `/` until the bundle is built.

## D4 — Inclusive/exclusive date conversion lives in `domain::dates`, called only from `gcal`

- **Context:** §4.5 says to convert "in exactly one place (the `gcal` crate)", while §16.2 lists the
  conversion and timed-event tests under the *domain* tests.
- **Choice:** the pure functions (`inclusive_from_exclusive`, `exclusive_from_inclusive`,
  `timed_span`) are in `domain::dates` with their tests; only `gcal`'s mapping module calls them.
- **Alternatives:** duplicating them in `gcal` (two sources of truth).

## D5 — Members normalization

- **Context:** §5.2/§7 say an empty list means "whole family" and the form sends `[]` when all
  boxes are checked, but an API/MCP client may list everybody explicitly.
- **Choice:** `validation::normalize_members` dedupes, orders by the configured order and turns
  "everybody" into `[]`. Consumers treat `[]` as everybody.

## D6 — Validation details not in the §5.2 table

- Missing `type` → `Choisissez une catégorie.`; unknown type code → `Catégorie inconnue : {code}.`
- `location` > 200 chars → `Le lieu est trop long (200 caractères maximum).`;
  `notes` > 2000 chars → `Les notes sont trop longues (2000 caractères maximum).`
- A missing `end` on create defaults to `start` (MCP `create_event` documents this); an unparsable
  date gives `Choisissez un premier et un dernier jour.`
- On update, the "writable type" rule is only checked when the patch sets `type`, so that title
  or notes of an `unclassified` event can be changed without picking a type (the web form still
  requires a choice, §8.6).
- `birth_year` is dropped for non-birthday types. A 29 February birthday with a non-leap birth
  year is `Année de naissance invalide.`
- On update of a birthday, an omitted `end` follows a changed `start` (a birthday lasts one day).

## D7 — Birthday series start

- **Context:** §4.6 says the first occurrence is "the date in the current year" when the birth
  year is unknown.
- **Choice:** the series starts on the date chosen by the user (so a birthday added while viewing
  2024 shows in 2024), not on today's year. With a birth year it starts on the birth date.

## D8 — `jsonwebtoken` with the pure-Rust `rust_crypto` backend

- **Context:** jsonwebtoken 11 requires choosing a crypto backend (`aws_lc_rs` or `rust_crypto`).
  It signs the service-account assertion (RS256) and, later, the OAuth access tokens (EdDSA).
- **Choice:** `rust_crypto`: no C toolchain, simple multi-arch Docker builds.
- **Alternatives:** `aws_lc_rs` (faster, but needs C/cmake and complicates cross builds).

## D9 — Server crate is a library plus a thin binary

- **Context:** integration tests (`crates/server/tests`) drive the real router in-process.
- **Choice:** `annee_server` library (all modules) and `annee` binary (`src/main.rs`, CLI only).

## D10 — Calendar store caching and event ids

- Cache granularity is the year (both calendars merged); **any** write clears the whole cache
  (recurring events affect every year, and it keeps the code obviously correct).
- Birthday/recurring writes return the occurrence the user was looking at (`<series>_<yyyymmdd>`),
  fetched from Google when possible, otherwise derived from the series.
- Editing an occurrence of a recurring event patches the series master. A birthday series keeps
  its start year unless a birth year is set; other recurring series are shifted by the distance
  the occurrence moved.

## D11 — Demo store specifics

- Birthdays are real yearly series starting in 1900, so every displayed year shows them and
  deleting one deletes all years (exercising the recurring-delete warning in e2e tests).
- Other sample events are generated lazily per displayed year, like the prototype's `mock(Y)`;
  sample titles keep the prototype's spelling (straight apostrophes).
- Sample school holidays (prototype's 2026/2027 ranges) are `import:education` events with a
  `demo:` source id; the import job never touches `demo:` events, so a demo import does not
  alter the sample data that the visual check compares with the prototype.
- Public holidays in the demo store use the same `ferie:YYYY-MM-DD` ids as the real import.

## D12 — Web session details

- The session cookie holds `{email, name, iat, seen}`; it is renewed (sliding expiry) when the
  last renewal is more than one hour old. Removing an email from `allowed_emails` ends its sessions.
- The `Secure` cookie flag follows `app.base_url` (`https://` → Secure), so that plain-HTTP local
  runs (`http://localhost`) work with Safari/Firefox; production uses HTTPS behind Caddy.
- Demo login: `POST /auth/demo` (404 outside demo mode) signs in as "Alex".
- `Origin` check: an `Origin` header is accepted when it equals the origin of `base_url` or the
  request's `Host` header (works behind Caddy and for local ports).

## D13 — API additions beyond §7

- `GET /api/config` also returns `base_url`, `mcp_url` and `kiosk: { refresh_minutes, default_view }`
  (needed by Réglages and the kiosk page).
- `POST /api/demo/reset` (demo mode only) backs the `Réinitialiser la démo` button.
- `GET /api/events` defaults `year` to today's year and rejects years outside 1900–2200.
- A kiosk session may not write: 403 `Cette session est en lecture seule.`

## D14 — Inline styles under the CSP

- **Context:** the required CSP has `style-src 'self'`, which blocks `style="…"` attributes and
  `<style>` elements, whereas the prototype positions everything with inline styles.
- **Choice:** the front-end builds DOM nodes and sets geometry through the CSSOM
  (`el.style.setProperty`), which the CSP allows. No `'unsafe-inline'`.

## D15 — Education dataset: verified field formats and the date rule

- **Verified** against the live API on 2026-10-16 (`fr-en-calendrier-scolaire`, `location="Rennes"`):
  fields `description`, `population` (`-`, `Élèves`, `Enseignants`), `start_date`, `end_date`
  (RFC 3339 instants at *Paris midnight*, expressed in UTC: e.g. `2025-10-17T22:00:00+00:00`),
  `location`, `zones` (`Zone B`), `annee_scolaire`. Fixtures recorded in
  `crates/server/tests/fixtures/education_{2024-2025,2025-2026,2026-2027}.json`.
- **Rule:** `start = Paris date of start_date` (first day without school), `end = Paris date of
  end_date − 1 day` (classes resume on `end_date`). Toussaint 2025-2026 zone B → 2025-10-18 →
  2025-11-02, the acceptance case; locked by tests for three school years (summer and winter time).
- The dataset has a malformed record (Ascension bridge 2027: `start_date == end_date`). It is
  imported as a single day (`end = max(start, resume − 1)`), and corrected automatically by the
  next run once upstream fixes it.
- The 2026-2027 *Élèves* summer record ends on 2027-09-01 as published (kept as is).
- Missing `end_date` (not seen in practice): the holiday ends on 31 August of the school year's
  second calendar year (the spec's "day before the next school year's start" is not knowable from the dataset).
- Title normalization: `'` → `’`, lowercase except the first word and proper nouns
  (Noël, Toussaint, Ascension, Pâques, Pentecôte, …).

## D16 — Import scope and safety

- A school year only counts as managed when the dataset returned at least one record for it
  (unpublished years are left untouched). Deletion only concerns events whose `fy_source_id`
  belongs to a managed school year (`edu:{year}:…`) or a managed calendar year (`ferie:{year}-…`),
  so older history and `demo:` sample data are never deleted, and manual events are never listed.
- Duplicates (same `fy_source_id` more than once) are collapsed: extras count as deleted.
- Demo mode does not import at startup or on a schedule (the sample data is already there); the
  `Mettre à jour maintenant` button and `annee import` use the recorded fixtures.
- Startup catch-up and the cron run are skipped when an import is in progress; runs left
  `running` after a crash are marked failed at the next start.
- `annee import` logs to stderr and prints its summary on stdout.
- SQLite: demo mode uses a private in-memory database (single connection); production opens
  `DATABASE_URL` with WAL and runs the embedded migrations.

## D17 — Google Sign-In implemented directly (no `openidconnect` crate)

- **Context:** SPEC §12.3 lists `openidconnect`; the flow needed is small (code + PKCE, one
  ID-token verification) and the crate adds a large, fast-moving API surface.
- **Choice:** `google_login.rs` does the redirect, the code exchange (`reqwest`) and verifies the ID
  token with `jsonwebtoken` against Google's JWKS (cached 1 h, refreshed on unknown `kid`):
  RS256 only, `iss` ∈ {`https://accounts.google.com`, `accounts.google.com`}, `aud` = client id,
  `exp`, `nonce`, `email_verified` (boolean or `"true"`).
- The CSRF `state`, `nonce`, PKCE verifier and `next` live in a 10-minute encrypted cookie
  `annee_oauth`, consumed on the first callback.
- Failure redirects: `/connexion?error=` `invalid` (state/cookie), `denied` (user cancelled),
  `failed` (exchange/verification), `not_allowed&email=…` (allowlist). The front-end shows French texts.
- Rate limits use `governor` (keyed by client IP: first `X-Forwarded-For` entry set by Caddy, else
  the socket address). `/auth/*` 30/min/IP; the other limits (§15.2) are wired in M7/M8.

## D18 — MCP implementation with `rmcp` 3.5 (verified)

- **Verified** against the crate source (rmcp 3.5.1, MCP spec 2026-07-28 in the changelog): the server
  is a hand-written `ServerHandler` (`list_tools` / `call_tool`) behind `StreamableHttpService`, in
  **stateless** mode (`legacy_session_mode = false`, `json_response = true`), which the spec allows.
  Tool schemas are written explicitly (no macros) so descriptions, enums and annotations are exact.
- rmcp validates `Host` (set to the host of `base_url`; disabled in demo mode for local ports) and
  `Origin` (`allowed_origins = [origin of base_url]`); our own middleware also rejects foreign `Origin`s.
- Authentication is an axum middleware in front of rmcp: it resolves a PAT or (M8) an OAuth token,
  rate-limits per token (120/min), then stores a `McpAuth` in the request extensions; tool handlers
  read it back from the injected `http::request::Parts`. Scope is checked per tool.
- Tool errors are `isError: true` results with `structuredContent {code, message}` (English code,
  French message); successful results carry the JSON both as `structuredContent` and as text.
- `get_day.weekday` is the English lowercase weekday name (`saturday`) — easier for agents to
  reason on; `events` there lists everything that is not a birthday / public or school holiday
  (daycare closures included, with `daycare_closed` as a shortcut).
- Kiosk-flagged tokens are refused on `/mcp` (they only open the read-only tablet view).
- Bearer credentials may omit the scheme (`Authorization: fy_pat_…`).

## D19 — sqlx 0.9 and dynamic SQL

- sqlx 0.9 refuses non-literal SQL strings; all queries are literal strings with bound parameters.

## D20 — OAuth authorization server details

- Implemented in `crates/server/src/oauth/` (`clients`, `authorize`, `token`, `jwt`). Access tokens are
  EdDSA JWTs (`typ at+jwt`) with the spec's claims plus an extra `client_name` (saves a lookup per
  request and gives the writer name). Refresh tokens: 32 random bytes, SHA-256 hashed, 30 days,
  single-use rotation; reusing a rotated token revokes the whole grant.
- A grant is created at the first code exchange (one grant per authorization). The revoked-grant
  set is cached in memory for 30 s; revocations made through this process take effect immediately.
- `scope` is normalised: `calendar:write` implies `calendar:read`; `offline_access` is accepted (refresh
  tokens are always issued); unknown scopes give `invalid_scope`. A refresh may only narrow the scope.
- Redirect URI errors (unknown client, unmatched URI) render an error page and never redirect.
  Other errors redirect with `error`, `state` and `iss`. Loopback matching ignores the port but
  not the host (`localhost` ≠ `127.0.0.1`); `[::1]` is also accepted.
- The consent screen is server-rendered into the Vite-built `consent.html` template (placeholder
  `<!--BODY-->`, so it links the hashed stylesheet without inline styles). CSRF: encrypted one-shot
  cookie `annee_consent` compared with a hidden field, plus the `Origin` check.
- CIMD: fetched with a 5 s timeout, 64 KB cap, no redirects, https only, with an SSRF guard
  (resolved addresses must be public and the connection is pinned to the checked address); cached
  24 h in `oauth_clients`; a stale copy is used if a refresh fails. Tests use a flag to allow `http://`.
- DCR: when 100 clients exist, unused ones (no active grant) older than a day are forgotten first;
  otherwise registration is refused with 429. 10 registrations/hour/IP.
- Housekeeping task: expired codes/refresh tokens purged daily, rate-limit state hourly.
- `GET /.well-known/oauth-authorization-server` also advertises
  `authorization_response_iss_parameter_supported: true` (we always send `iss`).
## D21 — Self-hosted Bricolage Grotesque

- **Context:** SPEC §8.1 asks for self-hosted woff2 (latin + latin-ext) and no third-party request.
- **Choice:** the two files served by the Google Fonts CSS API for
  `family=Bricolage+Grotesque:opsz,wght@12..96,400..800` (a Chrome user agent gets woff2) are committed
  in `web/src/fonts/` untouched: one variable font per unicode-range subset, weights 400–800 and the
  `opsz` axis (automatic optical sizing, like the prototype). No further subsetting: the files are 31 kB
  and 77 kB. `styles.css` declares them with the same `unicode-range` values as Google, so Vite hashes
  them into `/assets/`. The vietnamese subset is left out (not needed for French). `OFL.txt` (copied from
  the font's upstream repository) sits next to them. The prototype's fallback stack is unchanged.
- **Verified:** the e2e test `no third-party request…` asserts that the face is `loaded` and that no
  request leaves the app's origin.

## D22 — Front-end structure and interaction state machine

- **Modules** (all under `web/src`): `api` (typed fetch client, `X-Requested-With` on writes, French
  error messages, 401 → `/connexion?next=/`), `state`, `dates`, `layout` (geometry as data, no HTML),
  `render` (DOM building and `fit()`), `tooltip`, `interact` (pointer), `panel`, `dialog`, `filters`,
  `print`, `app` (year loading, banner), `main`, `connexion-main`, `dom` (CSSOM helpers, see D14).
- **Category vocabulary:** the prototype's short names (`gathering`, `stay`, `school`, `daycare`,
  `holiday`) are kept as CSS classes (`cat-*`) and mapped from the API type codes in `state.ts`
  (`family_gathering` → `gathering`, …) so that the stylesheet stays identical to the prototype.
  Legend labels (`label_plural`) and the select labels (`label`) come from `/api/config`.
- **Tints are events now:** school and public holidays arrive as events of the year; the grid derives the
  tint sets from them (public holidays use the weekend tint and a red number, §5.6). Everything is filtered
  by one function, `visible()`: hidden categories vanish, `members: []` means everybody, and the person
  filter never hides birthdays, school holidays or public holidays.
- **Pointer state machine** (`interact.ts`): idle → `pointerdown` on a cell (or on a bar) creates a drag
  record → `pointermove` over other cells extends the selection (`moved`) → `pointerup` opens the dialog
  (moved over ≥ 2 days), else the event panel (press started on a bar), else the day panel. `pointercancel`
  (touch scroll) only clears the selection, and touch never drags, so scrolling is never blocked.
  Deviation from the prototype: a press on a bar that lands in the 3 px gap between two cells still opens
  the event (a two-day bar is centred on that gap).
- **Panel:** one function per view (`openDay`, `openEvent(id, confirming)`); the delete confirmation is the
  same view re-rendered with a `.confirm` block (no extra state). A closed panel is `inert` (the prototype's
  off-screen panel was reachable with Tab); focus goes to the heading on open and back on close.
- **Dialog:** a single native `<dialog>` for create and edit (`editing` is the event or `null`); one
  `submit()` validates client-side, calls the API, shows the server `message` in the error line, and on
  success refetches (and follows the event to its year when it was created outside the displayed one,
  as the prototype does). The web form sends `members: []` when everybody is checked.
- **Day panel items:** SPEC §8.5 says imported items are plain, but §8.5 and §16.2 also require the
  `Importé automatiquement` panel, and imported events have no bar. All items of the day panel are
  therefore clickable and open the event panel (read-only for imported events).
- **First load:** the empty grid renders with the browser year; once `/api/config` answers, the year is
  `config.today`'s year (never the browser clock) and the events are fetched. Year navigation shows the
  empty grid then the events (stale responses are discarded).
- **Persistence:** only the view (`localStorage` `annee.view`, every access in try/catch).

## D23 — Print layout (§10)

- **Page size:** `@page a3 { size: A3 landscape; margin: 10mm }` and `@page a4 { … }` are static named pages
  (the CSP forbids injected `<style>`); `html[data-paper]` selects one with `page:`.
- **One page by construction:** in print, the `body` is a fixed-size sheet in millimetres (printable area
  400×277 mm for A3, 277×190 mm for A4, minus 1 mm), made of a fixed-height header (20 mm), the grid
  and a fixed-height footer (6 mm), with `overflow: hidden`. The only computed value is the cell size
  `--print-cs` (`print.ts`), the largest size that fits the grid area, with no 22–80 px clamp.
  `preparePrint()` runs when `Imprimer` is pressed and on `beforeprint` (Ctrl+P uses the last format).
- **Always light:** the light tokens are re-declared in `@media print` with a higher specificity, so a
  dark system theme does not leak into the PDF; `print-color-adjust: exact` on everything.
- **Filters line** is `Filtres : sans {catégories masquées} · {personne}` (the word `sans` makes it clear
  that the listed categories are hidden); the static legend lists only the categories still shown.
- **Page count in tests:** Chromium writes uncompressed page objects, so `print.spec.ts` counts
  `/Type /Page` not followed by a letter (`/Pages` excluded) in `page.pdf()` output, for A3 and A4, both
  views, once with the explicit format and once with `preferCSSPageSize` (which exercises the named pages).

## D24 — Playwright setup and visual check

- One worker, serial (tests share one demo server); every spec resets the demo data in `beforeEach`.
  A `setup` project signs in through the real demo login page and stores the session in
  `web/playwright/.auth/state.json` (ignored by `web/.gitignore`).
- An automatic fixture asserts, after each test, that no request left the app's origin and that no
  `securitypolicyviolation` / CSP console error occurred.
- **Visual check:** the app and `design/prototype.html` are opened with
  `page.clock.setFixedTime('2026-10-16T10:00:00')`, same `colorScheme`, same viewport; the network is
  closed except for the local server, and the prototype's Google Fonts requests are fulfilled with the
  committed woff2 files (so both use the same font offline). Only the area under the header (`#board`) is
  compared; the test first asserts that both boards have the same box and the same `--cs`, i.e. that the
  extra header buttons did not change the header height. `pixelmatch` threshold 0.1, budget 3 %.
  Result: 0.000 % (0 differing pixels) in the four combinations (light/dark × Weeks/Months).
- `playwright.config.ts` resolves paths with `import.meta.url` (the package is `"type": "module"` and
  Node 20.10 has no `import.meta.dirname`).

## D25 — Réglages, kiosk and consent front-end

- **Réglages** (`reglages-main.ts`): plain DOM building; revocation always goes through one native `<dialog>`
  (`Révoquer l’accès de « {nom} » ? / Annuler / Révoquer`); secrets (new token, kiosk URL) live in a read-only
  `<input>` that is also the copy fallback (`navigator.clipboard`, else select + `execCommand("copy")`),
  and exist only in the DOM of the page that created them. Imports are polled every second while `running`.
  Dates are formatted by hand (`16 oct. 2026 à 14:32`) in `config.timezone`.
- **Kiosk** (`kiosk-main.ts`) reuses `render()` and `openDay(d, true)` (read-only panel, no `Ajouter`, items are
  not buttons). Bars lose their `role`/`tabindex` there. A refresh fetches `/api/config` then the events of
  `config.today`'s year, and repaints only when something changed (`replaceChildren` on the canvas is atomic).
  A 30 s timer (shorter near midnight) detects the local day change (in `config.timezone`), refetches
  `config` (the server's `today` wins; offline, the tablet's own date is used) and reloads the page after 03:00
  local. Retry every 30 s while offline; the indicator keeps the time of the first failure.
  A kiosk 401 goes to `/connexion?error=kiosk`; other pages to `/connexion?next={current path}`.
- **Consent** (`consent.html`): `<main class="card"><!--BODY--></main>`; Vite keeps the comment (checked in
  `dist/consent.html`). It also emits a tiny stylesheet-only script that the page does not need.
- **Tests:** DCR is limited to 10 registrations/hour/IP, so each spec file registers one client with two redirect
  URIs. Playwright does not route redirects of a navigation, so the consent tests capture the `Location` of the
  `POST /oauth/authorize` answer instead of following it. `PW_PORT` overrides the test port.
- **Backend issue found (not fixed, `crates/` is out of scope):** the global CSP has `form-action 'self'`.
  Chromium applies it to the redirect that answers a form post, so approving or denying on the consent page
  is blocked when the redirect URI is on another origin (observed: "Sending form data to
  '/oauth/authorize' violates … form-action 'self'"). `/oauth/authorize` responses need a CSP without
  `form-action` (or with the redirect URI's origin).

## D26 — CSP of the consent decision

- Chromium applies `form-action` to the redirect answering a form post, so the global
  `form-action 'self'` blocked redirecting to a client's own redirect URI after Autoriser/Refuser.
  Only the `POST /oauth/authorize` response carries the CSP without `form-action`; every other
  response keeps the full policy (§15.2). Found by the front-end agent; covered by a test.

## D27 — Leisure-centre closures share the daycare closure type

- **Context:** the family also has to track closures of the *centre de loisirs*; Noé attends the
  daycare only until September 2027, then the leisure centre.
- **Choice:** keep the single `daycare_closure` type (colorId 4, rose tint, "both closed"
  rendering) and only change its French labels: "Fermeture de la crèche ou du centre de loisirs"
  (singular) and "Crèche / loisirs fermés" (legend). No Google Calendar change; existing events stay
  valid. The code identifier `daycare_closure` is unchanged (MCP API stability).
- **Alternatives:** a new type with its own colorId and a three-way "all closed" tint (more work,
  and the daycare would linger after 2027); hiding the daycare after September 2027.

## D28 — Preparing the repository for publication

- **Context:** the repository was developed privately and is going public as a fresh history.
- **Choices:**
  - MIT license (`LICENSE`, `license` in `Cargo.toml` and `web/package.json`).
  - Test RSA keys are generated at test time (`rsa` + `rand` 0.8 as dev-dependencies; `rsa` and
    `num-bigint-dig` are optimised in the dev profile because key generation is slow otherwise), so
    no private key is committed.
  - The family's first names became generic ones (Alex, Camille, Sacha, Noé), including in
    `design/prototype.html` and the demo data, so that the visual diff stays at 0 %. This is a
    deliberate exception to "never edit the prototype".
  - Example domain `famille.example.org`; the "shared server" section of `docs/SETUP.md` is generic
    (`<proxy-network>`, `<proxy-container>`, `<ssh-port>`).
  - The public repository starts from a single commit.
- **Alternatives:** rewriting history with `git filter-repo` (risk of leftovers); keeping the
  history (author e-mail and first names stay visible).

## D29 — Repository and image name

- **Choice:** the public repository and the container image are named `family-year-planner`
  (`ghcr.io/<owner>/family-year-planner`); the product is called "Family Year Planner" in code and docs. The French UI keeps its title
  "Année en famille" (page titles, headings, consent page), as do the `annee` binary and `/opt/annee`.
  The MCP server name is `family-year-planner`.
- **Alternatives:** keeping the French image name; `family-calendar` (too generic).

## D30 — Type catalog stored in SQLite, edited in Réglages (V1.1)

- **Context:** SPEC §19 makes type names, colors and extra types configurable.
- **Choice:** tables `type_settings` (only types that differ from the defaults, plus every custom type) and
  `type_migrations`; the effective catalog lives in memory (`TypeRegistry`, crate `gcal`) and is shared with the
  calendar stores. An invalid stored catalog falls back to the defaults with an error log.
- **Alternatives:** `[[types]]` in `config.toml` (no UI, restart needed); storing the catalog in Google Calendar
  (no natural place for it).

## D31 — Added types are slots `custom_1`…`custom_3`

- **Context:** each Google `colorId` serves one type and Google has 11 colors; the 8 colored built-in types use 8.
- **Choice:** `EventType::Custom(1..=3)`, code `custom_N`, always a Famille bar, writable, "busy". The slot is
  independent of the color, so recoloring never changes a code. The limit of 3 follows from the colors, it is not
  an extra rule. Deleting a custom type clears the `colorId` of its events (they become unclassified).
- **Alternatives:** free-form slug codes (non-`Copy` type, much more churn, ugly collisions); letting a custom
  type live in Repères or render as a tint (many edge cases: legend, print, busy days).

## D32 — Language per browser, resolved by a request-scoped layer

- **Context:** SPEC §19.2 wants a French default and per-person English; messages are produced deep in the
  server (validation, store errors, the consent page, imported titles).
- **Choice:** cookie `annee_lang` (set by the front-end switch or `?lang=`), else `app.default_language` (`fr`).
  `Accept-Language` is ignored on purpose ("FR par défaut"). A middleware runs each request inside a `tokio`
  task-local, so code asks `i18n::current()` without threading a parameter. rmcp runs every MCP call in its own
  spawned task, where the task-local is gone: `mcp_auth` stores the language in the request extensions and
  `call_tool` re-enters the scope (`i18n::scoped`). MCP agents send no cookie, so they get the instance default.
- **Alternatives:** an `Accept-Language` negotiation (surprising for a family tablet); passing `Lang` through every
  function (large churn); a process-wide default (breaks tests that run several configs).

## D33 — Imported titles are translated at read time

- **Context:** imported holidays are stored in Google Calendar with a French title and are upserted by
  `fy_source_id`; the app must show English titles to English users.
- **Choice:** a fixed table maps the French titles of public holidays and of the usual school holidays to English
  when the request language is English. Unknown titles (new dataset descriptions) are shown as stored. SPEC §19.2
  says "by key"; matching the stored title is equivalent because the importer normalizes titles.
- **Alternatives:** storing both languages in the event (breaks idempotent upserts); a translation per `fy_source_id`.

## D34 — Color migration: one pass, resumable, writes blocked meanwhile

- **Context:** Google Calendar is the source of truth, so recoloring a type means rewriting its events; a
  swap of two colors must not lose track of which event moved.
- **Choice:** the whole target catalog is validated first (so swaps are legal), then a single pass over the series
  masters of both calendars rewrites `colorId` by an `old → new` map, tagging each event with `fy_migration=<id>`.
  The migration row is stored before the pass; a failed or interrupted pass keeps writes blocked, is retried 3
  times, resumed at startup and through `POST /api/admin/types/resume`. The pass runs in a detached task, so a
  client disconnect cannot drop it. A write gate in `TypeRegistry` makes user writes and import runs hold a shared
  guard while the migration takes it exclusively: it starts only when writes in flight are done, and new ones are
  refused (`409 migrating`; imports are recorded as skipped). The importer also compares the type of existing
  events, so an event written with a stale color heals on the next import. The in-memory catalog switches only
  when the pass completes.
- **Alternatives:** blocking recoloring when events exist; recoloring one type at a time (no swaps); a cancel path.
- **Also:** an event Google refuses for good (a 400, or a 403 that is not a rate limit) or that disappeared is
  skipped and logged, so that one event the service account cannot modify never freezes the family's calendar;
  the migration row records how many were left as they were. Rate-limit 403s stay retryable.
- **Limit:** while a pass is suspended after repeated failures, reads use the old catalog, so already-recolored
  events appear under another type until the migration is resumed. There is no rollback (see the open question
  on the PR).

## D35 — Palette: one CSS entry per Google color

- **Context:** a type's color now comes from a Google `colorId` chosen by the user; the design (light, dark, print)
  must stay identical to the prototype for the default catalog, and the CSP forbids inline styles.
- **Choice:** `--p<colorId>-bar` / `--p<colorId>-tint` (light, dark, print) in `styles.css`; the front-end only
  points each type at the entry of its color (`colors.ts`), so themes and the always-light print need no logic.
  Ids 2, 3, 4, 5, 9, 10, 11 reproduce the prototype colors (visual diff unchanged); 1, 6, 7, 8 are new.
  The "both closed" orange, the public-holiday rendering and the `unclassified` grey are fixed whatever the colors.
- **Alternatives:** generating a stylesheet from JS (blocked by the CSP); per-type hex sent by the server.

## D36 — MCP follows the catalog

- **Context:** agents must be able to use added types and see renamed ones; SPEC §13.1 still asks for French titles.
- **Choice:** `get_context` is the source of truth and returns
  `label`, `labels {fr,en}`, `color_id`, `custom`; the `type` parameters of the tools are plain strings whose
  description lists the current codes (an `enum` would go stale in an agent's cached `tools/list`, and this
  stateless server cannot send `list_changed`). Tool descriptions and the "titles in French" instruction stay as
  in SPEC §13.1; one sentence is appended about configurable types. Messages use the instance default language (D32).
- **Alternatives:** per-token language; letting the instructions say titles may be in any language (rejected: the
  shared calendar is written in French).

## D37 — Demo mode

- **Context:** the demo store holds types, not Google colors, but must behave like the real one in e2e tests.
- **Choice:** a catalog change re-derives types from the target catalog; "Reset the demo" also restores the default
  catalog, so e2e tests stay independent.
- **Alternatives:** giving demo events a `colorId` (more code, no new behavior); leaving the catalog across resets
  (tests would depend on each other).

## D38 — Verification notes

- **Context:** CLAUDE.md asks to verify uncertain facts against official documentation.
- **Choice:** the 11 Google event colors and their names come from SPEC §4.2 and the Calendar API `colors`
  resource; the documentation site was not reachable from the build sandbox (`getaddrinfo ENOTFOUND`), so the names
  were not re-checked live. `PW_CHROMIUM` lets Playwright use a Chromium whose version differs from the installed one.
- **Alternatives:** none worth the cost; a live check belongs to the human setup (`annee check-config`).

## D39 — Slots of added types are never reused within one save

- **Context:** the server identifies a type by its code; deleting `custom_1` and adding a new category in the same
  save would send `custom_1` in both catalogs, which reads as a rename and re-types the old events.
- **Choice:** the editor offers only slots absent from both the draft and the saved catalog; after a deletion, the
  user saves first. A deleted type's events are cleared, never silently re-typed.
- **Alternatives:** random identities for added types (breaks the stable `custom_N` codes agents rely on);
  server-side detection (cannot tell a rename from a replacement).

## D40 — A save carries the catalog revision it was edited from

- **Context:** the editor `PUT`s the whole catalog, and two adults can have Réglages open at once; a stale tab
  lacking a category added elsewhere would delete it (and clear its events' colors).
- **Choice:** `GET /api/admin/types` returns a `revision` (a short hash of the catalog); a `PUT` with another
  revision, dry run included, is refused with `409 stale_catalog` and the unsaved edits stay on screen.
- **Alternatives:** last write wins (silent data loss); locking the editor per session (needs presence tracking).
