# Family Year Planner — Specification (V1)

Status: frozen for V1. Owner: Alex. Implementer: Claude Code (autonomous).
Design reference: `design/prototype.html` (locked).

---

## 1. Overview

**Family Year Planner** shows a whole family year on a single landscape screen. Each day is a
square. Context days (school holidays, daycare closures, birthdays, public holidays) are shown
as **background tints**; family events (gatherings, stays in family homes, trips, celebrations)
are shown as **labelled bars**. The data lives in **two Google Calendars**. The app is a web
app with a wall-tablet **kiosk mode**, a **print/PDF export**, and an **MCP server** that lets any
AI agent read and edit the family year.

### Glossary

| Term | Meaning |
|---|---|
| Repères | Google calendar for context: school holidays, daycare closures, birthdays, public holidays. Rendered as tints. |
| Famille | Google calendar for family events: gatherings, family homes, trips, celebrations. Rendered as bars. |
| Type | The category of an event (see §4.2). Derived **only** from the Google `colorId`. |
| Tint | Background color of a day cell. |
| Bar | Rounded rectangle spanning the days of an event, with its title. |
| Weeks view | Default layout: two half-years, one block of week columns per month (GitHub-style). |
| Months view | Alternate layout: 12 rows (months) × 37 weekday-aligned columns. |
| PAT | Personal access token for agents that authenticate with a static header. |
| Demo mode | `APP_MODE=demo`: in-memory calendar store seeded with the prototype's sample data. |

### Users

- The family: Alex, Camille, Sacha, Noé. Two adults log in (allowlist of email addresses).
- AI agents (Claude, Grok, Codex, others) through the MCP server.
- A wall tablet in kiosk mode (read-only).

---

## 2. Scope

### In scope (V1)

- Year grid, Weeks and Months views, light and dark themes, French UI (as in the prototype).
- Read, create, edit and delete events, from the grid (click, drag) and the side panel.
- Filters by category and by person (default: "Tous").
- Birthdays as yearly recurring events, with optional birth year and computed age.
- Automatic import of school holidays (zone B, académie de Rennes) and French public holidays
  (metropolitan France) into the Repères calendar.
- Kiosk mode (`/kiosk`), read-only, auto-refreshing.
- Print/PDF export of the current view (A3 or A4 landscape) through the browser's print dialog.
- MCP server (Streamable HTTP) with OAuth 2.1 and personal access tokens.
- "Réglages" page: agent access (tokens, OAuth clients), import status.
- Self-hosted deployment: Docker image, Docker Compose, Caddy, custom domain.

### Out of scope (V1)

Installable PWA, drag to move or resize existing events, offline mode, multi-family /
multi-tenant, notifications, English UI, events with attendees/invitations, a native mobile app.

---

## 3. Configuration

All configuration is in one TOML file (path from env `APP_CONFIG`, default `/config/config.toml`).
Secrets are in environment variables or files referenced by environment variables (§15.1).

```toml
[app]
base_url = "https://annee.example.fr"      # public URL, no trailing slash
timezone = "Europe/Paris"
allowed_emails = ["alex@example.com", "camille@example.com"]

[family]
members = ["Alex", "Camille", "Sacha", "Noé"]   # order = display order of the person filter

[calendars]
reperes_id = "xxxxxxxx@group.calendar.google.com"
famille_id = "yyyyyyyy@group.calendar.google.com"

[imports]
school_zone = "Zone B"
school_location = "Rennes"          # académie, as written in the dataset's `location` field
public_holidays = "metropole"       # only value supported in V1
schedule = "0 0 4 1 * *"            # cron (sec min hour dom mon dow), Europe/Paris: 1st of month 04:00
school_years_ahead = 1              # import current school year + N following
public_holiday_years_ahead = 2      # import current year + N following

[kiosk]
refresh_minutes = 5
default_view = "week"               # "week" or "month"
```

The server refuses to start with a clear error if the file is invalid. In demo mode the
`[calendars]` values are ignored.

---

## 4. Data model on Google Calendar

### 4.1 Calendars

| Calendar | Config key | Content | Written by |
|---|---|---|---|
| Repères | `reperes_id` | school holidays, daycare closures, birthdays, public holidays | import job, users, agents |
| Famille | `famille_id` | gatherings, family homes, trips, celebrations, unclassified | users, agents |

Both calendars are owned by a family Google account and shared with the service account
("Make changes to events"). The server accesses Google **only** through the service account.

### 4.2 Types

The type of an event is derived **only from its `colorId`**. Each colorId is used by at most one
type, across both calendars. The calendar an event lives in decides where new events of a type
are written, not how existing events are interpreted.

| Code | French label (singular / legend) | Calendar | colorId | Google color name | Rendering | Editable in app |
|---|---|---|---|---|---|---|
| `school_holiday` | Vacances scolaires / Vacances scolaires | Repères | 5 | Banana | yellow tint | no (imported) |
| `daycare_closure` | Fermeture de la crèche ou du centre de loisirs / Crèche / loisirs fermés | Repères | 4 | Flamingo | rose tint | yes |
| `birthday` | Anniversaire / Anniversaires | Repères | 2 | Sage | green tint + 🎂 | yes |
| `public_holiday` | Jour férié / Jours fériés | Repères | 8 | Graphite | weekend tint + red day number | no (imported) |
| `family_gathering` | Réunion de famille / Réunions de famille | Famille | 11 | Tomato | tomato-red bar | yes |
| `family_home` | Séjour en maison de famille / Maisons de famille | Famille | 10 | Basil | green bar | yes |
| `trip` | Voyage ou vacances / Voyages & vacances | Famille | 9 | Blueberry | blue bar | yes |
| `celebration` | Fête ou cérémonie / Fêtes & cérémonies | Famille | 3 | Grape | purple bar | yes |
| `unclassified` | Non classé / Non classés | either | none or unmapped | — | grey bar | yes (to assign a type) |

- colorIds 1 (Lavender), 6 (Tangerine), 7 (Peacock) are reserved for future types.
- An event with no `colorId`, or an unmapped one, is `unclassified`, whatever its calendar.
- The app's palette is its own (see the prototype); Google colors are only keys.
- "Both closed" (school holiday and daycare closure on the same day) is a rendering rule,
  not a type: the orange tint.

### 4.3 Extended properties

Stored in `extendedProperties.private`. Keys are prefixed `fy_`. All values are strings.

| Key | Values | Meaning |
|---|---|---|
| `fy_schema` | `"1"` | Metadata format version. Written on every create/update. |
| `fy_members` | `"Sacha,Noé"` | Comma-separated names from `family.members`. Absent or empty = whole family. |
| `fy_birth_year` | `"2018"` | Birthdays only. Optional. |
| `fy_source` | `manual`, `import:education`, `import:public_holidays` | Origin of the event. Absent = `manual`. |
| `fy_source_id` | e.g. `edu:2025-2026:vacances-de-noel`, `ferie:2026-05-14` | Stable key for imported events (upsert). |
| `fy_created_by` | e.g. `Alex`, `Grok API`, `Claude` | Who created it through the app/MCP. Absent if created elsewhere. |
| `fy_updated_by` | same | Last writer through the app/MCP. |

Rules:
- The type is **never** stored in metadata (colorId is the single source of truth).
- Unknown member names in `fy_members` are ignored when reading and logged at `debug`.
- Never write a key with an empty value; remove the key instead.

### 4.4 Native Google fields used

`summary` (title), `start`/`end`, `colorId`, `location`, `description` (notes),
`recurrence` (birthdays), `extendedProperties.private`, `htmlLink` (read only), `status`
(cancelled events are ignored), `recurringEventId` (instances).

### 4.5 Dates and times

- All dates are interpreted in `app.timezone` (Europe/Paris).
- Internally and in the API, events have an **inclusive** `start` and `end` date
  (`YYYY-MM-DD`). Google all-day events use an **exclusive** `end.date`: convert in exactly one
  place (the `gcal` crate), covered by unit tests.
- Events created by the app are **all-day** events.
- Timed events created elsewhere (e.g. 15:00–23:00) are read as covering every local date they
  touch: first day = local date of start; last day = local date of `end − 1 second`. Their times
  are kept (`start_time`, `end_time`, `HH:MM`) for display in the side panel only. Editing a
  timed event in the app keeps it timed if only title/type/members/location/notes change; if
  dates change, it becomes an all-day event (log this rule in DECISIONS.md if refined).

### 4.6 Recurrence

- Birthdays are created as a yearly series: `RRULE:FREQ=YEARLY`. The first occurrence is the
  birth date if `birth_year` is known, otherwise the date in the current year.
- A birthday on 29 February uses `RRULE:FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=-1` so it shows on
  28 February in non-leap years.
- The app reads with `singleEvents=true`; instances carry `recurringEventId`.
- Any edit or delete of an instance of a recurring event (birthday or other) applies to the
  **whole series**. The UI warns before deleting a series (§8.5).
- Age shown for a birthday occurrence = occurrence year − `fy_birth_year`, displayed as
  `Anniversaire de Sacha (8 ans)`; `1 an` for one year; nothing if no birth year or age ≤ 0.

### 4.7 Event identity

API and MCP event ids are `<cal>:<googleId>` where `<cal>` is `r` (Repères) or `f` (Famille)
and `<googleId>` is the Google event id (instance id for occurrences). Each event also exposes
`series_id` (`<cal>:<recurringEventId>`) when it is an occurrence. Writes to an occurrence
resolve to the series.

---

## 5. Domain rules

### 5.1 Normalized event (domain model)

```text
Event {
  id: String,                 // "f:abc123"
  series_id: Option<String>,  // "r:xyz" for occurrences
  calendar: Reperes | Famille,
  kind: EventType,            // §4.2
  title: String,              // 1..=120 chars, trimmed
  start: NaiveDate,           // inclusive
  end: NaiveDate,             // inclusive, >= start
  start_time: Option<HH:MM>,  // timed events only
  end_time: Option<HH:MM>,
  members: Vec<String>,       // empty = whole family
  location: Option<String>,   // <= 200 chars
  notes: Option<String>,      // <= 2000 chars
  birth_year: Option<i32>,    // birthdays only, 1900..=current year
  age: Option<u32>,           // computed for the occurrence
  source: Manual | ImportEducation | ImportPublicHolidays,
  read_only: bool,            // true for imported events
  created_by: Option<String>,
  updated_by: Option<String>,
  google_url: Option<String>, // htmlLink
}
```

### 5.2 Validation (create and update)

| Rule | French error message |
|---|---|
| Title is required after trimming | `Donnez un titre à l’événement.` |
| Title ≤ 120 characters | `Le titre est trop long (120 caractères maximum).` |
| Start and end are valid dates | `Choisissez un premier et un dernier jour.` |
| End ≥ start | `Le dernier jour ne peut pas être avant le premier.` |
| Span ≤ 366 days | `Un événement ne peut pas dépasser un an.` |
| Type is writable (not `school_holiday`, `public_holiday`, `unclassified`) | `Ce type d’événement ne peut pas être créé à la main.` |
| Birthday spans exactly one day | `Un anniversaire dure un seul jour.` |
| Members ⊆ configured members | `Personne inconnue : {name}.` |
| Birth year in 1900..=current year | `Année de naissance invalide.` |
| Target event is not read-only | `Cet événement est importé automatiquement et ne peut pas être modifié.` |

At least one person must be selected **in the web form** (`Choisissez au moins une personne.`);
the API and MCP accept an empty list, meaning "whole family". The web form sends an empty list
when all members are checked.

### 5.3 Writes

- **Create:** calendar and colorId come from the type; metadata `fy_schema`, `fy_members`,
  `fy_birth_year`, `fy_source=manual`, `fy_created_by`, `fy_updated_by`.
- **Update:** patch only changed fields; set `fy_updated_by`. Changing the type to one that
  lives in the other calendar uses Google `events.move` then patches `colorId`.
- **Delete:** deletes the event, or the series for an occurrence.
- **Writer name:** the logged-in member's name for web users (map email → member name with
  `[app] user_names = { "alex@…" = "Alex" }` if present, else the email's local part);
  the agent's name for MCP calls (PAT name or OAuth client name).
- **Concurrency:** last write wins in V1. Updates send the Google `etag` with `If-Match` when
  available and, on `412`, re-read and retry once; if it still fails, return a French error
  `Cet événement a été modifié entre-temps. Rechargez la page.`

### 5.4 Reading a year

- `list_year(Y)` returns all events overlapping `[Y-01-01, Y-12-31]` from both calendars,
  `singleEvents=true`, paginated until exhausted, cancelled events dropped.
- Results are cached in memory per (calendar, year) for 60 seconds; any write through the app
  or MCP invalidates the cache for the affected years. External edits (Google apps) appear
  within the TTL.

### 5.5 Free periods (used by the MCP tool `find_free_periods`)

Inputs: `from`, `to` (inclusive), `min_days` (≥1), `members` (default: all),
`during` (`any` | `school_holidays` | `outside_school_holidays`), `ignore_types` (default empty).

A day is **busy** if any event of type `family_gathering`, `family_home`, `trip`, `celebration`
or `unclassified` (minus `ignore_types`) covers it and involves at least one of the requested
members (an event with no members involves everyone). Birthdays, daycare closures and public
holidays never make a day busy. With `during=school_holidays`, only days inside a school
holiday are candidates; with `outside_school_holidays`, only days outside. Output: maximal runs
of consecutive free candidate days of length ≥ `min_days`, as `{start, end, days}`, sorted by
start, at most 50.

### 5.6 Rendering rules (shared by front-end, kiosk and print)

- Tints per day: `school` (yellow), `daycare` (rose), `both` (orange) when school and daycare
  overlap, `birthday` (green, with 🎂). Weekend **and** public holidays use the weekend tint;
  public holidays also color the day number red. A weekend/public-holiday shade overlays tints.
- A birthday on a school or daycare day keeps the green tint and shows the other tint as a thin
  band at the bottom of the cell (as in the prototype).
- Bars: every non-tint event (all Famille types and `unclassified`), single-day or multi-day.
  Two lanes maximum per day; extra events add a grey dot marker on the affected days.
- Filters: hidden categories disappear entirely (tints and bars). The person filter applies to
  bars and to daycare closures; school holidays, public holidays and birthdays are always shown.

---

## 6. Repères import job

Fills the Repères calendar with school holidays and public holidays. Users never edit these
events (they are `read_only`).

### 6.1 School holidays

- Source: data.education.gouv.fr dataset `fr-en-calendrier-scolaire`, Explore API v2.1:
  `GET https://data.education.gouv.fr/api/explore/v2.1/catalog/datasets/fr-en-calendrier-scolaire/records`
  with a `where` filter on `location` (config `school_location`) and `annee_scolaire`, `limit=100`.
  Verify the exact field names and value formats against the live API during M5 and record a
  fixture (`crates/server/tests/fixtures/education_*.json`). Expected fields: `description`,
  `population`, `start_date`, `end_date`, `location`, `zones`, `annee_scolaire`.
- Keep records whose `population` is `-` or `Élèves` (drop `Enseignants`), and whose `zones`
  equals config `school_zone`.
- School years imported: the current one (the year containing today, school years start in
  September) plus `school_years_ahead` following ones, when published.
- Day range: the holiday covers the days **without school**, from the first day after the last
  class day to the day before classes resume. Derive it from `start_date`/`end_date` converted
  to Europe/Paris, and lock the rule with a test. **Acceptance:** for zone B, the 2025-2026
  "Vacances de la Toussaint" import as 2025-10-18 → 2025-11-02 (inclusive). Verify this against
  the official calendar on education.gouv.fr when recording the fixture.
- Summer holidays: if `end_date` is missing, end on the day before the next school year's
  start if known, otherwise on 31 August.
- Title: the dataset's `description`, normalized to French sentence case
  (`Vacances de Noël`, `Vacances d’hiver`, `Pont de l’Ascension`, …).
- `colorId=5`, all-day, `fy_source=import:education`,
  `fy_source_id=edu:{annee_scolaire}:{slug(description)}:{start}`.

### 6.2 Public holidays (metropolitan France)

Computed locally (no network), for the current year plus `public_holiday_years_ahead`:

| Date | Title |
|---|---|
| 1 Jan | Jour de l’An |
| Easter Monday | Lundi de Pâques |
| 1 May | Fête du Travail |
| 8 May | Victoire 1945 |
| Easter + 39 days | Ascension |
| Easter + 50 days | Lundi de Pentecôte |
| 14 Jul | Fête nationale |
| 15 Aug | Assomption |
| 1 Nov | Toussaint |
| 11 Nov | Armistice 1918 |
| 25 Dec | Noël |

Easter is computed with the anonymous Gregorian algorithm (as in the prototype).
`colorId=8`, all-day, `fy_source=import:public_holidays`, `fy_source_id=ferie:{YYYY-MM-DD}`.

### 6.3 Upsert algorithm

1. Build the desired set of events for the import window.
2. List existing Repères events in the window whose `fy_source` starts with `import:`
   (Google `privateExtendedProperty` filter).
3. Create missing ones, update those whose title or dates differ, delete imported events in the
   window that are no longer desired. Never touch events whose `fy_source` is not `import:*`.
4. Record the run in SQLite (`import_runs`): start, end, status, counts (created / updated /
   deleted / unchanged), error message.

A second run with unchanged sources must report 0 created / 0 updated / 0 deleted.

### 6.4 Scheduling and failures

- Runs on the cron in `imports.schedule`, at startup when the last successful run is older than
  31 days (or never ran), from the Réglages page ("Mettre à jour maintenant"), and from the CLI
  (`annee import [--dry-run]`).
- One run at a time (async mutex); a request while running returns the running status.
- HTTP failures: 3 attempts with exponential backoff (1 s, 4 s, 16 s). On failure the existing
  events are kept, the run is recorded as failed, and the Réglages page shows the error.
- In demo mode the import uses the recorded fixture and the in-memory store.

---

## 7. HTTP API (used by the web app)

Base path `/api`. JSON, `snake_case`. Auth: web session cookie (§11.1). Kiosk sessions may only
call `GET /api/config`, `GET /api/events` and `GET /api/me`. Mutating requests require the
header `X-Requested-With: annee` (CSRF guard) and a same-origin `Origin` when present.

| Method & path | Body / query | Response |
|---|---|---|
| `GET /api/me` | — | `{ "email", "name", "kiosk": bool, "demo": bool }` |
| `GET /api/config` | — | `{ "members", "types": [TypeInfo], "timezone", "school_zone", "today" }` |
| `GET /api/events?year=2026` | — | `{ "year", "events": [Event] }` |
| `POST /api/events` | `EventInput` | `201` + `Event` |
| `PATCH /api/events/{id}` | partial `EventInput` | `Event` |
| `DELETE /api/events/{id}` | — | `204` |
| `GET /api/admin/tokens` | — | `[ { "id", "name", "scope", "created_at", "last_used_at", "kiosk": bool } ]` |
| `POST /api/admin/tokens` | `{ "name", "scope": "read" \| "read_write", "kiosk": bool }` | `201` + `{ "id", "token", "kiosk_url"? }` (token shown once) |
| `DELETE /api/admin/tokens/{id}` | — | `204` |
| `GET /api/admin/oauth-grants` | — | `[ { "id", "client_name", "redirect_host", "scope", "created_at", "last_used_at" } ]` |
| `DELETE /api/admin/oauth-grants/{id}` | — | `204` |
| `GET /api/admin/import` | — | `{ "running": bool, "runs": [ImportRun] }` (last 10) |
| `POST /api/admin/import` | — | `202` + `{ "running": true }` |

`TypeInfo`: `{ "code", "label", "label_plural", "calendar": "reperes"|"famille", "writable": bool, "render": "tint"|"bar" }`.

`Event` (JSON of §5.1): `id, series_id, calendar, type, title, start, end, start_time, end_time,
members, location, notes, birth_year, age, source, read_only, created_by, updated_by, google_url`.

`EventInput`: `type, title, start, end, members, location, notes, birth_year` (all optional on
`PATCH`; `members: []` means whole family; omitting a field leaves it unchanged; `null` clears
`location`, `notes`, `birth_year`).

Errors: `{ "error": { "code": "validation_failed", "message": "<French message>" } }` with
`400` (validation), `401` (no session → the front-end redirects to `/connexion`), `403`
(kiosk or read-only), `404`, `409` (concurrent modification), `502` (Google error:
`Google Agenda ne répond pas. Réessayez dans un instant.`).

---

## 8. Web app

### 8.1 Design lock

- Port the prototype's CSS **as is** into `web/src/styles.css` (tokens, light and dark themes,
  geometry) and its rendering logic into TypeScript modules. Keep class names where practical
  so the two can be compared.
- Geometry (all in `--cs` units, `cs` = cell size computed to fit the screen, clamped 22–80 px):
  - Weeks view: left gutter `0.9`, month header `0.75`, gap between halves `0.55`, gap between
    month blocks `0.45`; January–June on top, July–December below; each month is its own block
    of week columns; a week straddling two months appears in both blocks.
  - Months view: left gutter `1.35`, weekday header `0.7`, 37 weekday-aligned columns, 12 rows.
  - Bars: thickness `0.3`, lane gap `0.03`, inner margin `0.06`, end inset `0.05`, 2 lanes,
    vertical text in Weeks view, horizontal in Months view, ellipsis on overflow.
- Typeface: Bricolage Grotesque (SIL OFL), **self-hosted** woff2 (latin + latin-ext) with the
  prototype's fallback stack. No third-party requests at runtime.
- Theme follows `prefers-color-scheme` (no toggle in V1). Print is always light (§10).
- New UI pieces (edit dialog fields, Réglages page, login page, consent page, banners) reuse
  the prototype's components: chips, `.btn`, `.btn.ghost`, `.btn.danger`, panel, dialog, fields.

### 8.2 Routes (served by the Rust binary)

| Route | Content |
|---|---|
| `/` | Main year view (requires web session; else redirect to `/connexion`) |
| `/connexion` | Login page |
| `/reglages` | Settings (web session only) |
| `/kiosk` | Kiosk mode (§9) |
| `/oauth/authorize` | OAuth consent page (§11.4), server-rendered |
| `/assets/*` | Hashed static assets, `Cache-Control: public, max-age=31536000, immutable` |

### 8.3 Main screen — differences from the prototype

The header keeps the prototype's layout: title, year navigation, Semaines/Mois switch, category
chips, person chips, and on the right: `Ajouter un événement`, then **`Imprimer`** (ghost),
**`Réglages`** (ghost), **`Se déconnecter`** (ghost). `Réinitialiser la démo` only exists in
demo mode. The legend chip `Non classés` (grey, `--c-unclassified`: light `#8A93A3`, dark
`#6E7787`) appears only when the displayed year has at least one unclassified event. The "Les
deux" static chip stays.

Default state: current year, Weeks view (or the last view stored in `localStorage`
`annee.view`), all categories shown, person filter `Tous`. Only the view is persisted.

### 8.4 Interactions (as in the prototype)

- Hover a day: tooltip with the date and all items of the day (mouse only).
- Click a day: side panel for the day. Click a bar: side panel for the event.
- Mouse drag across days (across months too): selection highlight, then the dialog opens
  prefilled with the range. Touch: tap opens the day panel; scrolling is never blocked.
- Bars are focusable (`role="button"`, `tabindex=0`); Enter/Space opens the event.
- Escape closes the panel or dialog.

### 8.5 Side panel

Day panel: heading `Samedi 14 mars 2026`, list of items (events clickable, imported items
plain), button `Ajouter un événement ce jour-là`.

Event panel:
- Title (birthdays: with age, e.g. `Anniversaire de Sacha (8 ans)`), subtitle with dates
  (`Du sam. 25 juil. au dim. 2 août 2026` or a single long date), plus `De 15:00 à 23:00` for
  timed events.
- Definition list: `Catégorie`, `Durée` (`9 jours`), `Qui` (`Toute la famille` when empty),
  `Lieu` (if any), `Notes` (if any, multi-line, links not auto-activated), `Ajouté par`
  (if `created_by`), `Modifié par` (if different from `created_by`).
- Link `Ouvrir dans Google Agenda` (`google_url`, new tab, `rel="noopener"`).
- Buttons `Modifier` and `Supprimer` for editable events. Imported events show
  `Importé automatiquement` instead of buttons.
- Delete asks for confirmation inside the panel: `Supprimer « {titre} » ?` with
  `Supprimer` (danger) and `Annuler`. For recurring events the text adds:
  `Cet événement se répète chaque année : toutes les occurrences seront supprimées.`

### 8.6 Event dialog (create and edit)

Title `Nouvel événement` or `Modifier l’événement`. Fields in order:

1. `Titre` (required)
2. `Catégorie`: the six writable types (§4.2). When editing an unclassified event, the select
   shows a disabled placeholder `Choisir une catégorie` and saving requires a choice.
3. `Premier jour`, `Dernier jour` (date inputs). For `Anniversaire`, `Dernier jour` is hidden
   and equals `Premier jour`.
4. `Année de naissance` (number, optional) — only for `Anniversaire`.
5. `Qui est concerné ?` — one checkbox per member, all checked by default.
6. `Lieu` (optional), `Notes` (optional, textarea, 4 rows).

Buttons: `Annuler`, and `Ajouter` (create) or `Enregistrer` (edit). While saving the primary
button shows `Enregistrement…` and is disabled. Validation errors (client-side and server
`message`) appear in the error line of the dialog; the dialog stays open on error. Default
category: `Fête ou cérémonie` for a single day, `Voyage ou vacances` for a range (prototype
behaviour).

### 8.7 Loading, errors, session

- First load: header and empty grid render immediately; events fill in when loaded.
- Load failure: a banner under the header (`role="alert"`): `Impossible de charger l’agenda.`
  with a `Réessayer` button.
- `401` on any call: redirect to `/connexion?next=/`.
- After any successful write: refetch the year and re-render; close dialog/panel as in the
  prototype.

### 8.8 Login page (`/connexion`)

Centered card in the app's visual language: `Année en famille`, one line
`Connectez-vous pour voir l’année de la famille.`, button `Se connecter avec Google`. If the
Google account is not in the allowlist: `Ce compte Google n’est pas autorisé.` with the email.

### 8.9 Réglages page (`/reglages`)

Same header style with a `← Retour à l’année` link. Sections:

1. **Accès des agents**
   - Table of personal tokens: `Nom`, `Droits` (`Lecture seule` / `Lecture et écriture`),
     `Créé le`, `Dernière utilisation` (`Jamais` if null), `Révoquer`.
   - Form: `Nom` (e.g. `Grok API`), `Droits` (radio, default `Lecture seule`),
     button `Créer le jeton`. The new token is shown once in a highlighted box with `Copier` and
     `Copiez ce jeton maintenant : il ne sera plus affiché.`
   - Table of OAuth connections: `Application`, `Redirection`, `Autorisé le`,
     `Dernière utilisation`, `Révoquer`.
   - `Adresse du serveur MCP` with the URL `{base_url}/mcp` and `Copier`.
2. **Tablette (mode kiosque)**: button `Créer un lien kiosque` → creates a read-only token named
   `Kiosque – {date}` flagged `kiosk`, shows `{base_url}/kiosk?token=…` once with `Copier`.
   Kiosk tokens are listed in the token table with a `Kiosque` badge.
3. **Imports**: last runs (date, `Réussi` / `Échec`, counts, error), status of the running import,
   button `Mettre à jour maintenant`.

Revoking always asks for confirmation (`Révoquer l’accès de « {nom} » ?`).

### 8.10 Accessibility and responsiveness

Keep the prototype's behaviour: visible focus, `aria-label` on bars and icon buttons, tooltip
`role="tooltip"`, reduced motion respected, portrait phone hint
`Tournez votre téléphone pour voir l’année entière.`, horizontal scroll when the grid does not fit.

---

## 9. Kiosk mode (`/kiosk`)

Purpose: a wall tablet showing the year, always up to date, with no interaction needed.

- **Access:** `/kiosk?token=fy_pat_…` (a read-only kiosk token) sets a kiosk session cookie
  `annee_kiosk` (HttpOnly, Secure, SameSite=Lax, 400 days) bound to the token id, then redirects
  to `/kiosk` without the token in the URL. A normal web session also works. Revoking the token
  ends the kiosk session at the next request.
- **Screen:** title, year and a static (non-clickable) legend, then the grid. No buttons, filters,
  tooltips, drag or dialogs. Tapping a day opens a read-only day panel that closes after 20 s or
  on tap outside. View from `?view=week|month`, else `kiosk.default_view`.
- **Freshness:** refetch every `kiosk.refresh_minutes` and re-render without flicker; at local
  midnight move the today ring and switch year on 1 January; full page reload every day at
  03:00 to pick up new versions.
- **Device care:** request a Screen Wake Lock when available (re-request on visibility change);
  hide the cursor after 5 s without movement.
- **Offline:** keep showing the last data and show a discreet indicator in the bottom-right corner:
  `Hors ligne depuis 14:32`. Remove it after the next successful refresh.

---

## 10. Print and PDF export

- `Imprimer` opens a small popover: `Format` → `A3 paysage` (default) or `A4 paysage`, and a
  button `Imprimer`. It sets the page size (`@page { size: A3 landscape; margin: 10mm }`),
  computes `cs` to fit the printable area, and calls `window.print()`. Users choose
  "Enregistrer au format PDF" in the browser dialog to get a PDF.
- Print layout: always light theme; `print-color-adjust: exact`; header reduced to title, year
  and a static legend; if filters are active, a line `Filtres : {catégories masquées} · {personne}`;
  grid of the current view with bar labels; no tooltip, panel, buttons or banners; footer
  `Imprimé le 16 octobre 2026`.
- Must fit on exactly **one** page in both formats and both views.
- Acceptance: Playwright `page.pdf({ format: 'A3', landscape: true })` and A4 produce a
  single-page PDF in both views (demo data).

---

## 11. Authentication and authorization

### 11.1 Web login (household members)

- Google Sign-In with OpenID Connect, authorization code flow with PKCE, scopes `openid email`.
  Verify the ID token (`iss`, `aud`, `exp`, `email_verified`). Google is used for **identity
  only**; the app never requests calendar scopes for users.
- The email must be in `app.allowed_emails` (case-insensitive), else show the refusal on
  `/connexion`.
- Routes: `GET /auth/google/start?next=/…` (only relative `next`), `GET /auth/google/callback`,
  `POST /auth/logout`.
- Session: encrypted and signed cookie `annee_session` (HttpOnly, Secure, SameSite=Lax,
  Path=/), 30 days, renewed on activity. Contains email, member name, issued-at.
- Member name for `fy_created_by`: `[app.user_names]` map if present, else the email's local part.

```toml
[app.user_names]
"alex@example.com" = "Alex"
"camille@example.com" = "Camille"
```

### 11.2 Authorization matrix

| Principal | Read events | Write events | Réglages / admin API | MCP read tools | MCP write tools |
|---|---|---|---|---|---|
| Web session (allowlisted member) | ✓ | ✓ | ✓ | — | — |
| Kiosk session | ✓ | — | — | — | — |
| PAT `read` | — | — | — | ✓ | — |
| PAT `read_write` | — | — | — | ✓ | ✓ |
| OAuth token, scope `calendar:read` | — | — | — | ✓ | — |
| OAuth token, scope `calendar:write` (implies read) | — | — | — | ✓ | ✓ |

PATs and OAuth tokens are only accepted on `/mcp`. Session cookies are never accepted on `/mcp`.

### 11.3 Personal access tokens (PAT)

- Format: `fy_pat_` + 32 random bytes, base64url without padding.
- Stored as SHA-256 hash; the clear token is returned once at creation.
- Name: 1–60 characters, unique among active tokens.
- `last_used_at` updated at most once per minute per token.
- Sent by agents as `Authorization: Bearer fy_pat_…`. For robustness, also accept the token
  without the scheme (`Authorization: fy_pat_…`), since some agent platforms pass the configured
  value as is.

### 11.4 OAuth 2.1 authorization server (for MCP clients)

Follows the MCP authorization specification (protected resource metadata, authorization server
metadata, PKCE S256, DCR and Client ID Metadata Documents). Requirements:

**Protected resource**
- Resource URL: `{base_url}/mcp`.
- Unauthenticated or invalid token on `/mcp` → `401` with
  `WWW-Authenticate: Bearer resource_metadata="{base_url}/.well-known/oauth-protected-resource"`.
  The `401` status is mandatory (some clients ignore the header on other statuses).
- `GET /.well-known/oauth-protected-resource` and `/.well-known/oauth-protected-resource/mcp`:
  `{ "resource": "{base_url}/mcp", "authorization_servers": ["{base_url}"],
  "scopes_supported": ["calendar:read", "calendar:write"], "bearer_methods_supported": ["header"] }`.
  `resource` must equal the MCP URL exactly.

**Authorization server metadata** — `GET /.well-known/oauth-authorization-server`:
`issuer` = `{base_url}`; `authorization_endpoint` = `/oauth/authorize`; `token_endpoint` =
`/oauth/token`; `registration_endpoint` = `/oauth/register`; `revocation_endpoint` =
`/oauth/revoke`; `response_types_supported` = `["code"]`; `grant_types_supported` =
`["authorization_code", "refresh_token"]`; `code_challenge_methods_supported` = `["S256"]`;
`token_endpoint_auth_methods_supported` = `["none", "client_secret_post", "client_secret_basic"]`;
`scopes_supported` = `["calendar:read", "calendar:write", "offline_access"]`;
`client_id_metadata_document_supported` = `true`. Absolute URLs.

**Client registration**
- DCR (`POST /oauth/register`, `application/json`): accepts `client_name`, `redirect_uris`
  (required; `https://…`, or loopback `http://localhost|127.0.0.1|[::1]`),
  `token_endpoint_auth_method` (default `none`). Returns `client_id` (and `client_secret` for
  confidential clients). Max 100 registered clients; rate-limited (10/hour per IP).
- CIMD: when `client_id` is an `https://` URL, fetch it (5 s timeout, 64 KB max, cache 24 h),
  require its `client_id` field to equal the URL, and use its `client_name` and `redirect_uris`.

**Authorization endpoint** — `GET /oauth/authorize`
- Params: `response_type=code`, `client_id`, `redirect_uri`, `code_challenge`,
  `code_challenge_method=S256`, `scope` (default `calendar:read calendar:write`), `state`,
  `resource` (optional; if present must equal the resource URL).
- Redirect URI check: exact match with a registered URI; for loopback URIs the port is ignored,
  for both `localhost` and `127.0.0.1`.
- No web session → go through Google sign-in (§11.1) and come back.
- Consent page (French, app styles): `{client_name} demande l’accès à l’agenda familial.`;
  permission list (`Voir les événements`, and if write: `Ajouter, modifier et supprimer des
  événements`); the redirect host in bold: `Vous serez redirigé vers {host}`; for loopback hosts
  an extra warning `Cette application tourne sur un ordinateur (adresse locale). Ne continuez que
  si vous venez de la lancer.`; buttons `Autoriser` and `Refuser`.
- Approve → authorization code (32 random bytes, single use, 5 minutes) → redirect with `code`,
  `state` and `iss`. Refuse → `error=access_denied`.

**Token endpoint** — `POST /oauth/token`
- Must accept `application/x-www-form-urlencoded` (and only that).
- `authorization_code`: verify code, client, `redirect_uri`, PKCE verifier. Issue:
  - access token: JWT signed with EdDSA (Ed25519) using `OAUTH_SIGNING_KEY` as the 32-byte seed,
    claims `iss`, `aud` = resource URL, `sub` = email, `client_id`, `scope`, `grant_id`, `jti`,
    `exp` = 1 hour;
  - refresh token: opaque, 32 random bytes, stored hashed, valid 30 days, **rotated on every
    use**; presenting an already-rotated refresh token revokes the whole grant.
- `refresh_token`: rotate; respond in well under 1 second.
- Errors per RFC 6749 (`invalid_grant` for unknown, expired or reused refresh tokens and bad
  codes; `invalid_client`; `invalid_request`; `unsupported_grant_type`).

**Revocation** — `POST /oauth/revoke` (RFC 7009). Revoking a grant in Réglages revokes its
refresh tokens; access tokens carry `grant_id`, checked against revoked grants on each request
(in-memory cache, 30 s).

---

## 12. Architecture and stack

### 12.1 Overview

One Rust binary serves the web app, the JSON API, the MCP endpoint and the OAuth endpoints, and
runs the import scheduler. Google Calendar is the system of record. SQLite stores only tokens,
OAuth state and import runs.

```text
Browser / tablet ──► Caddy (TLS) ──► annee (axum)
                                       ├─ static web app (embedded)
                                       ├─ /api  ──┐
                                       ├─ /mcp  ──┼─► domain ─► CalendarStore ─► Google Calendar API
                                       ├─ /oauth  │                 (service account)
                                       └─ scheduler (imports) ──┘
                                       └─ SQLite (/data/annee.db)
```

### 12.2 Repository layout

```text
Cargo.toml                 # workspace
crates/domain/             # types, colorId mapping, validation, dates, free periods, rendering helpers — no I/O
crates/gcal/               # CalendarStore trait, GoogleStore (service account, REST), DemoStore (in-memory)
crates/server/             # axum app: web, api, auth, oauth, mcp, imports, scheduler, storage, CLI
crates/server/migrations/  # SQLite migrations
crates/server/tests/       # integration tests + fixtures
web/                       # TypeScript + Vite front-end (index, kiosk, reglages, connexion entry points)
web/tests/                 # Playwright e2e + visual tests
design/prototype.html      # locked design reference (read-only)
deploy/                    # docker-compose.yml, Caddyfile, config.example.toml, .env.example
Dockerfile
.github/workflows/         # ci.yml, release.yml
docs/                      # SPEC.md, SETUP.md, DECISIONS.md, PROGRESS.md
```

### 12.3 Crates (preferred)

`axum` (0.8), `tokio`, `tower-http` (trace, compression, headers), `axum-extra` (private
cookies), `serde`, `serde_json`, `toml`, `chrono` + `chrono-tz`, `reqwest` (rustls),
`gcp_auth` or `yup-oauth2` (service-account access tokens), `openidconnect` (Google sign-in),
`jsonwebtoken` (EdDSA) or `ed25519-dalek` + manual JWT, `rmcp` (official MCP Rust SDK, Streamable HTTP
server), `sqlx` (sqlite, migrate), `tokio-cron-scheduler`, `rust-embed`, `tracing`,
`tracing-subscriber` (JSON), `thiserror`, `anyhow`, `governor` (rate limiting), `sha2`, `rand`,
`base64`, `clap`. Tests: `wiremock`, `tokio::test`.

Call the Google Calendar REST API directly with typed structs (only `events.list`, `get`,
`insert`, `patch`, `delete`, `move` are needed), rather than a generated client.

### 12.4 Front-end

- `web/` with Vite and TypeScript `strict`. No UI framework, no runtime dependencies.
- Suggested modules: `api.ts`, `state.ts`, `dates.ts`, `layout.ts` (both views' geometry),
  `render.ts`, `tooltip.ts`, `panel.ts`, `dialog.ts`, `filters.ts`, `print.ts`, `kiosk.ts`,
  `reglages.ts`, `styles.css`, entry points `main.ts`, `kiosk-main.ts`, `reglages-main.ts`,
  `connexion-main.ts`.
- Dev dependencies only: `vite`, `typescript`, `@playwright/test`, `pixelmatch`, `pngjs`.
- `npm run dev` proxies `/api` to the binary running in demo mode.

### 12.5 CLI

`annee serve` (default), `annee import [--dry-run]`, `annee check-config` (validates config,
secrets and Google access: lists both calendars' names and checks write access with a
create+delete of a temporary event in each), `annee demo` (= serve with `APP_MODE=demo`).

---

## 13. MCP server

### 13.1 Transport and identity

- Endpoint `{base_url}/mcp`, Streamable HTTP (stateless mode is acceptable), implemented with
  `rmcp`.
- Server name `family-year-planner`, version from Cargo. `instructions` (English): explains the two
  calendars, the type codes and their French labels, that dates are ISO `YYYY-MM-DD` with
  **inclusive** end dates in Europe/Paris, that members are first names from `get_context`,
  that an empty member list means the whole family, that titles should be written in French,
  and that imported events (school and public holidays) are read-only.
- Validate the `Origin` header if present (must be `base_url` or absent).
- Rate limit: 120 requests per minute per token.
- The writer name recorded in `fy_created_by` / `fy_updated_by` is the PAT name or the OAuth
  client name.

### 13.2 Tools

Every tool returns `structuredContent` and the same JSON as text content. Errors are tool errors
(`isError: true`) with a French message and an English code.

| Tool | Scope | Annotations | Input | Output |
|---|---|---|---|---|
| `get_context` | read | readOnly | — | `{ today, timezone, members, types: [{code, label, calendar, writable}], school_zone }` |
| `list_events` | read | readOnly | `from`, `to` (dates, max span 400 days), `types?` (codes), `member?`, `query?` (case-insensitive substring of title, location, notes) | `{ events: [Event] }` (max 500, sorted by start) |
| `get_day` | read | readOnly | `date` | `{ date, weekday, is_weekend, public_holiday?, school_holiday?, daycare_closed, birthdays: [Event], events: [Event] }` |
| `find_free_periods` | read | readOnly | §5.5 inputs | `{ periods: [{start, end, days}] }` |
| `create_event` | write | — | `type`, `title`, `start`, `end?` (default `start`), `members?`, `location?`, `notes?`, `birth_year?` | `Event` |
| `update_event` | write | idempotent | `id`, then any of `type`, `title`, `start`, `end`, `members`, `location`, `notes`, `birth_year` | `Event` |
| `delete_event` | write | destructive | `id` | `{ deleted: true, id, scope: "event" \| "series" }` |

`Event` is the JSON of §5.1 / §7. Same validation as the API (§5.2). Calling a write tool with a
read-only credential returns the error `insufficient_scope` /
`Ce jeton ne permet que la lecture.`

Tool descriptions must be precise enough for an agent with no other context, e.g. for
`find_free_periods`: "Find periods when the requested family members have no family events
(gatherings, family-home stays, trips, celebrations). Birthdays, daycare closures and public
holidays do not make a day busy. Use `during` to restrict to school holidays or exclude them."

---

## 14. Persistence (SQLite)

File at `DATABASE_URL` (default `sqlite:///data/annee.db`), WAL mode, migrations with `sqlx`.

```sql
CREATE TABLE pat_tokens (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, token_hash BLOB NOT NULL UNIQUE,
  scope TEXT NOT NULL CHECK (scope IN ('read','read_write')), kiosk INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL, created_by TEXT NOT NULL, last_used_at TEXT, revoked_at TEXT
);
CREATE TABLE oauth_clients (
  client_id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK (kind IN ('dcr','cimd')),
  client_name TEXT NOT NULL, redirect_uris TEXT NOT NULL,         -- JSON array
  secret_hash BLOB, created_at TEXT NOT NULL, metadata_fetched_at TEXT
);
CREATE TABLE oauth_codes (
  code_hash BLOB PRIMARY KEY, client_id TEXT NOT NULL, redirect_uri TEXT NOT NULL,
  code_challenge TEXT NOT NULL, scope TEXT NOT NULL, email TEXT NOT NULL,
  expires_at TEXT NOT NULL, used_at TEXT
);
CREATE TABLE oauth_grants (
  id TEXT PRIMARY KEY, client_id TEXT NOT NULL, email TEXT NOT NULL, scope TEXT NOT NULL,
  redirect_host TEXT NOT NULL, created_at TEXT NOT NULL, last_used_at TEXT, revoked_at TEXT
);
CREATE TABLE oauth_refresh_tokens (
  token_hash BLOB PRIMARY KEY, grant_id TEXT NOT NULL REFERENCES oauth_grants(id),
  created_at TEXT NOT NULL, expires_at TEXT NOT NULL, rotated_at TEXT
);
CREATE TABLE import_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT, started_at TEXT NOT NULL, finished_at TEXT,
  status TEXT NOT NULL CHECK (status IN ('running','success','failed')),
  created INTEGER, updated INTEGER, deleted INTEGER, unchanged INTEGER, error TEXT, trigger TEXT
);
```

Expired codes and refresh tokens are purged daily.

---

## 15. Security, operations and deployment

### 15.1 Environment variables

| Variable | Example | Purpose |
|---|---|---|
| `APP_MODE` | `prod` / `demo` | Demo = in-memory store with sample data, fake login (single "Alex" user), no Google. |
| `APP_CONFIG` | `/config/config.toml` | Config file path. |
| `BIND` | `0.0.0.0:8080` | Listen address. |
| `DATABASE_URL` | `sqlite:///data/annee.db` | SQLite file. |
| `GOOGLE_SERVICE_ACCOUNT_FILE` | `/secrets/service-account.json` | Service account key (prod). |
| `GOOGLE_OAUTH_CLIENT_ID` / `GOOGLE_OAUTH_CLIENT_SECRET` | — | Google Sign-In web client (prod). |
| `SESSION_KEY` | 64 random bytes, base64 | Cookie encryption/signing. |
| `OAUTH_SIGNING_KEY` | base64 of 32 random bytes (`openssl rand -base64 32`) | Ed25519 seed for access-token signing. |
| `APP_FAKE_TODAY` | `2026-10-16` | Demo/test only: fixes "today". Ignored in prod. |
| `RUST_LOG` | `info,annee=debug` | Log filter. |

The server refuses to start in prod if a required secret is missing, and prints which one.

### 15.2 Security

- Response headers: `Content-Security-Policy: default-src 'self'; img-src 'self' data:;
  style-src 'self'; font-src 'self'; script-src 'self'; connect-src 'self'; frame-ancestors 'none';
  base-uri 'none'; form-action 'self'` (the OAuth consent form posts to self, then the server
  redirects), `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`.
  HSTS is set by Caddy.
- CSRF: `SameSite=Lax` cookies + `X-Requested-With: annee` on mutating API calls + `Origin` check.
- Never log tokens, codes, cookies or Google credentials. Log emails only at `debug`.
- Rate limits: `/auth/*` 30/min/IP, `/oauth/register` 10/h/IP, `/oauth/token` 60/min/IP,
  `/mcp` 120/min/token.
- Container runs as non-root; only `/data` is writable.

### 15.3 Observability

- Structured JSON logs (tracing): request id, route, status, latency; MCP tool name and agent
  name; import run summaries.
- `GET /healthz` → `200 {"status":"ok","version":"x.y.z","mode":"prod"}` (no auth, no Google call).

### 15.4 Deployment artefacts (to produce)

- `Dockerfile`, multi-stage: Node 22 builds `web/`; Rust (stable, bookworm) builds the release
  binary with embedded assets; runtime `gcr.io/distroless/cc-debian12:nonroot`. Exposes 8080.
  Multi-arch (`linux/amd64`, `linux/arm64`).
- `deploy/docker-compose.yml`: services `annee` (image `ghcr.io/<owner>/family-year-planner:latest`,
  env file, volumes `./config:/config:ro`, `./secrets:/secrets:ro`, `./data:/data`,
  healthcheck on `/healthz`, `restart: unless-stopped`) and `caddy` (`caddy:2`, ports 80/443,
  `./Caddyfile`, volumes for `caddy_data` and `caddy_config`).
- `deploy/Caddyfile`:
  ```text
  {$DOMAIN} {
      encode zstd gzip
      reverse_proxy annee:8080
  }
  ```
- `deploy/config.example.toml` (§3) and `deploy/.env.example` (§15.1, with placeholders and
  comments on how to generate each secret).
- `.github/workflows/ci.yml`: on push/PR — fmt, clippy, tests, web typecheck/build, Playwright
  e2e + visual tests against `annee demo`.
- `.github/workflows/release.yml`: on tag `v*` and on `main` — build and push the multi-arch image
  to GHCR with tags `latest` (main), `vX.Y.Z` (tags) and `sha-<short>`.

---

## 16. Testing and acceptance

### 16.1 Demo store and fixtures

- The demo store is seeded with **exactly** the prototype's sample data (events, school-holiday
  ranges, family names), mapped to types through the domain layer (types → colorIds →
  calendars), for any displayed year, like the prototype's `mock(Y)`.
- Demo login: `/connexion` shows `Entrer en mode démo` and creates a session for `Alex`.
- Fixtures: recorded data.education.gouv.fr responses (zone B, two school years), a CIMD document,
  Google Calendar API responses for `gcal` unit tests.

### 16.2 Required automated tests

Domain (`cargo test`):
- colorId ↔ type mapping (all 8 types + unmapped → unclassified).
- Exclusive ↔ inclusive end conversion; timed events across midnight and DST changes
  (last Sunday of March and October).
- Birthdays: yearly RRULE, 29 February rule, age computation.
- Validation rules of §5.2 with exact French messages.
- Free periods (§5.5): members, `during` modes, `ignore_types`, `min_days`, boundaries.
- Rendering helpers: tints per day including "both", lanes and overflow.

Server (`cargo test`, `wiremock` for Google and education APIs):
- Import: fixture → expected events (including the Toussaint 2025-2026 acceptance case);
  idempotency (second run 0/0/0); deletion of obsolete imported events; manual events untouched;
  failure keeps existing events.
- API: CRUD, validation errors, read-only imported events, series deletion, kiosk restrictions,
  CSRF header required, 401 redirect behaviour.
- PAT: creation shown once, hash storage, revocation, scope enforcement on MCP tools.
- OAuth: discovery documents; DCR; CIMD (fetched from a mock server); authorize with PKCE;
  loopback redirect with any port for `localhost` and `127.0.0.1`; exact match otherwise;
  token exchange with form-urlencoded only; refresh rotation and reuse detection (grant revoked);
  `invalid_grant` codes; `401` + `WWW-Authenticate` on `/mcp`.
- MCP: each tool through an `rmcp` client against the demo store, read vs write scopes,
  `created_by` set to the agent name.

Front-end (Playwright, against `annee demo` with `APP_FAKE_TODAY=2026-10-16`):
- Weeks and Months views render; year navigation; filters by category and person.
- Drag to create; edit via panel; delete with confirmation; recurring delete warning.
- Imported events show `Importé automatiquement` and no buttons.
- Réglages: create a PAT (shown once), revoke it; create a kiosk link.
- Kiosk: token exchange removes the token from the URL; no interactive controls; offline
  indicator when the API fails.
- Print: single-page PDF in A3 and A4, both views.
- **Visual check:** screenshots of the app (light theme, 1440×860, year 2026, both views) compared
  with screenshots of `design/prototype.html` rendered with the same fixed date
  (`page.clock.setFixedTime('2026-10-16T10:00:00')`). Header areas that legitimately differ
  (extra buttons) are masked. Grid areas must differ by at most 3% of pixels (`pixelmatch`,
  threshold 0.1). Repeat for dark theme.

---

## 17. Milestones

Each milestone ends with green checks, a commit and a `docs/PROGRESS.md` entry.

| # | Milestone | Done when |
|---|---|---|
| M1 | Workspace skeleton | Cargo workspace, `web/` Vite project, CI running fmt/clippy/tests/build, `annee serve` serving a placeholder page and `/healthz`. |
| M2 | Domain crate | §4–§5 implemented and unit-tested (mapping, dates, recurrence, validation, free periods, rendering helpers). |
| M3 | Calendar stores + API | `CalendarStore` trait, `DemoStore` seeded from the prototype data, `GoogleStore` (service account, REST, etag retry) tested with `wiremock`; §7 API in demo mode with a demo session. |
| M4 | Front-end port | Main screen identical to the prototype on demo data (visual check passing), plus §8 additions: edit dialog, panel details, delete confirmation, unclassified chip, loading/error states. |
| M5 | Imports | §6 complete with fixtures, scheduler, CLI `annee import`, Réglages "Imports" section. |
| M6 | Web auth | Google Sign-In (§11.1), allowlist, sessions, `/connexion`, logout; demo login kept for demo mode. |
| M7 | MCP + PAT | `/mcp` with all tools (§13), PAT creation/revocation in Réglages, scopes enforced. |
| M8 | OAuth server | §11.4 complete, consent page, grants listed and revocable in Réglages, all OAuth tests passing. |
| M9 | Kiosk + print | §9 and §10 complete with their tests. |
| M10 | Packaging | Dockerfile, compose, Caddyfile, examples, release workflow, `annee check-config`; `docs/SETUP.md` updated to match the implementation exactly. |

---

## 18. Definition of done

- [ ] All milestones M1–M10 complete; CI green on `main`.
- [ ] Visual check against the prototype passes (light and dark, both views).
- [ ] `annee demo` runs locally with no secrets and shows the full app with sample data.
- [ ] Every user-facing string is French; every code identifier and log is English.
- [ ] No third-party requests from the browser (fonts self-hosted, CSP enforced).
- [ ] The MCP server works with a PAT (`Authorization: Bearer`) and with OAuth (DCR and CIMD),
      verified by automated tests.
- [ ] Imports are idempotent and never touch manual events.
- [ ] `docs/SETUP.md` matches the implementation (names, paths, variables, routes).
- [ ] `docs/DECISIONS.md` lists every choice made where the spec was silent.

---

## 19. V1.1 — Languages and configurable types

Requested evolution. Where this section contradicts §1–§18, **this section wins**.

### 19.1 Goals

1. The interface exists in **French (default) and English**.
2. Every event type can be **renamed** and **recolored**; the color is a Google Calendar `colorId`.
3. Users can **add types**, within the colors Google Calendar still has free.

### 19.2 Languages

- `Lang` = `fr` | `en`. New config key `[app] default_language = "fr"` (`"fr"` or `"en"`).
- The language is **per browser**: cookie `annee_lang` (`fr`/`en`, one year, `SameSite=Lax`, not
  `HttpOnly`, set by the front-end). Without a cookie the instance default applies; the browser's
  `Accept-Language` is deliberately ignored. `GET /kiosk?lang=en` and `GET /connexion?lang=en`
  set the cookie too (the wall tablet cannot switch language by itself).
- The header (main screen, Réglages, login) shows a small `FR | EN` switch (`.seg`). Switching
  sets the cookie and reloads the page in place.
- Localized: every front-end string (main screen, dialog, panel, Réglages, kiosk, print, login),
  server-rendered pages (consent, errors), API and MCP error messages, validation messages, date
  formatting (`fr-FR` / `en-GB`, 24-hour times), `<html lang>`, page titles, `aria-label`s, the
  product title (`Année en famille` / `Family year`).
- API and MCP error messages follow the request language (cookie, else default). MCP requests have no
  cookie: they use the instance default language. Error **codes** stay English.
- Imported events keep the French title stored in Google Calendar, but are **translated at read
  time** for display: public holidays by their stable key (`fy_source_id` `ferie:YYYY-MM-DD`),
  school holidays by the normalized slug of the dataset description. Unknown titles are shown as
  stored. User-written titles are never translated.
- Person names, event titles, places and notes are user data and are not translated.
- Type names are user-editable in both languages (§19.3).

### 19.3 Type catalog

The fixed table of §4.2 becomes the **default catalog**. The effective catalog is the defaults
overridden by the database (`type_settings`, §19.6).

| Property | Built-in types | Custom types |
|---|---|---|
| Codes | the nine codes of §4.2 | `custom_1`, `custom_2`, `custom_3` (slot, reused after deletion) |
| Names (singular + plural, FR + EN) | editable | editable, required |
| Color (`colorId` 1–11) | editable, except `unclassified` (none) | required, editable |
| Calendar, rendering, writable, busy | fixed (§4.2, §5.5, §5.6) | Famille, bar, writable, busy |
| Delete | no | yes |

Rules:

- A `colorId` is used by **at most one type** (§4.2 still holds). Google offers 11 event colors,
  the 8 colored built-in types use 8, so **at most 3 custom types** exist. The UI states how many
  colors remain free.
- Names: 1–60 characters after trimming; every type (built-in or custom) has four names:
  singular and plural, in French and in English. Default names are the French labels of §4.2 and their English equivalents (`defaults` below).
- Renaming never touches Google Calendar. Behaviour attached to a built-in type (imports,
  birthday rules, "closure" tints, public-holiday rendering, `makes_busy`) follows its **code**,
  not its name or color.
- An event whose `colorId` maps to no type is `unclassified` (unchanged).

English default names (singular / plural):

| Code | English singular | English plural |
|---|---|---|
| `school_holiday` | School holiday | School holidays |
| `daycare_closure` | Daycare or leisure-centre closure | Daycare / leisure closed |
| `birthday` | Birthday | Birthdays |
| `public_holiday` | Public holiday | Public holidays |
| `family_gathering` | Family gathering | Family gatherings |
| `family_home` | Stay at a family home | Family homes |
| `trip` | Trip or vacation | Trips & vacations |
| `celebration` | Party or ceremony | Parties & ceremonies |
| `unclassified` | Unclassified | Unclassified |

### 19.4 Colors

Google's 11 event colors (name FR / EN): 1 Lavande / Lavender, 2 Sauge / Sage, 3 Raisin / Grape,
4 Flamant / Flamingo, 5 Banane / Banana, 6 Mandarine / Tangerine, 7 Paon / Peacock,
8 Graphite / Graphite, 9 Myrtille / Blueberry, 10 Basilic / Basil, 11 Tomate / Tomato.

The app keeps its own palette keyed by `colorId`, as CSS custom properties `--p<colorId>-bar` and
`--p<colorId>-tint` in `web/src/styles.css` (light, dark and print variants; `web/src/colors.ts` points each
type at the entry of its color): for each id, a **bar color**
and a **tint color**, each in light and dark. The defaults for ids 2, 3, 4, 5, 9, 10, 11 reproduce
the prototype's colors exactly (so the default catalog renders identically, visual diff 0 %);
ids 1, 6, 7, 8 get new values in the same family. A type's rendering takes its color from the palette
entry of its current `colorId`: bar types use the bar color, tint types (`school_holiday`,
`daycare_closure`, `birthday`) use the tint color. `public_holiday` keeps the weekend tint with a
red day number whatever its color; the "both closed" orange tint is fixed. `unclassified` is
always grey.

### 19.5 Changing a color: migration

Google Calendar is the source of truth, so changing a type's `colorId` rewrites `colorId` on its
existing events (both calendars, all years, series masters, imported events included):

1. `PUT /api/admin/types` with `dry_run: true` returns, for each type whose color changes, the
   number of existing events concerned; the UI shows it in a confirmation
   (`23 événements seront recolorés dans Google Agenda.`). When the chosen color is currently used
   by events of **no** type (e.g. an old event colored Tangerine), the count of those events that
   will become that type is shown too.
2. The final catalog is validated as a whole (unique colors, name lengths, ≤ 3 custom types), so
   two types can **swap** colors in one save. The migration is a single pass that maps each event's
   old `colorId` to its new one, so swaps are safe.
3. User writes and import runs hold a shared guard, the migration an exclusive one: it starts only when the
   writes in flight are done. The pass runs in a task detached from the HTTP request. The importer compares the
   type of existing events, so an event written with a stale color heals on the next import.
   An event that Google refuses for good (a 400, or a 403 whose reason is a known permanent one: `forbidden`, `forbiddenForNonOrganizer`,
   `requiredAccessLevel`, `notACalendarUser`) or that vanished is skipped instead of aborting the pass; any other 403
   (the `usageLimits` family: rate limits, daily limit, quota) is retryable and fails the pass. The ids of the skipped
   events are stored with the migration (`type_migrations.skipped`) and logged; `PUT /api/admin/types` returns their
   count (`skipped`), `GET /api/admin/types` returns `skipped: {count, ids}` for the last finished pass, and the
   Catégories section shows a notice until the next color pass.
   The pass is **resumable**: before starting, the target catalog and a migration id are stored in
   `type_migrations` (status `running`); every patched event gets the private property
   `fy_migration=<id>` and is skipped if it already carries it. On startup a `running` migration is
   resumed. The catalog switches to the new colors only when the pass completes (reads during the
   pass use the old catalog). Writes through the app are refused with `409`
   (`Les catégories sont en cours de modification. Réessayez dans un instant.`) while a migration
   runs.
4. Deleting a custom type clears the `colorId` of its events (they become `unclassified`), after a
   confirmation showing the count.
5. Demo store: no Google call; the catalog change takes effect immediately (events keep their type).
6. Name-only changes are applied immediately, without migration.

### 19.6 Persistence

```sql
CREATE TABLE type_settings (
  code TEXT PRIMARY KEY,                 -- built-in code or custom_N
  color_id TEXT,                         -- '1'..'11', NULL for unclassified
  label_fr TEXT NOT NULL, label_plural_fr TEXT NOT NULL,
  label_en TEXT NOT NULL, label_plural_en TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE type_migrations (
  id TEXT PRIMARY KEY, started_at TEXT NOT NULL, finished_at TEXT,
  status TEXT NOT NULL CHECK (status IN ('running','done','failed')),
  target TEXT NOT NULL,                  -- JSON: the full target catalog
  mapping TEXT NOT NULL,                 -- JSON: { "<old colorId>": "<new colorId>|null" }
  total INTEGER, patched INTEGER, error TEXT
);
```

A row exists only for types the user changed (defaults otherwise); custom types always have a row.
The catalog is kept in memory (`TypeRegistry`) and shared with the calendar store.

### 19.7 API changes

- `GET /api/config` also returns `lang`, `default_language`, and `types` localized in `lang`:
  `TypeInfo` gains `color_id` (`string|null`) and `custom` (bool); `label` / `label_plural` are in
  the request language.
- `GET /api/admin/types` →
  `{ "revision", "palette": [{ "color_id", "name_fr", "name_en" }], "max_custom": 3,
     "types": [{ "code", "custom", "color_id", "labels": { "fr": {"singular","plural"}, "en": {…} } }] }`
- `PUT /api/admin/types` body `{ "types": [same shape without palette], "dry_run": bool, "revision": string }` — the
  **complete** desired catalog (omitted custom types are deleted; built-ins must all be present).
  Response `{ "changes": [{ "code", "from_color_id", "to_color_id", "events": n }],
  "orphans": n, "applied": bool }`; `changes` and `orphans` are computed on a dry run only (they cost a scan
  of both calendars), an applied save answers `changes: []`. `revision` identifies the catalog the editor was loaded
  with; a `PUT` carrying another one (a stale tab) is refused with `409 stale_catalog`, because a missing custom
  type would otherwise read as a deletion. Validation failures: `400` with a localized message.
- Both are web-session only, members only (not kiosk, not MCP).

### 19.8 MCP

- `get_context` returns, for each type: `code`, `label` (instance default language), `labels`
  (`{fr, en}`), `calendar`, `writable`, `color_id`, `custom`. Tool and parameter names stay
  English; the `type` parameters are plain strings (no `enum`: the catalog can change while an agent keeps a
  cached `tools/list`, and the stateless server cannot notify it) that accept any code of the current catalog
  (custom codes included);
  tool descriptions stay English and say that the list of valid types is returned by
  `get_context`. `instructions` mention that types are configurable.
- Titles of imported events are localized like in the API (§19.2).

### 19.9 Réglages

A new section **Catégories** (EN: *Categories*) above *Accès des agents*: one row per type with
its four names (FR/EN, singular/plural), a color select (swatch + localized Google color name,
duplicates flagged, `Save` disabled while invalid), a `Delete` button on custom types, and
`Ajouter une catégorie` while fewer than 3 exist (`Il reste {n} couleur(s) libre(s) dans Google
Agenda` is shown; at 3 custom types the button is disabled with an explanation). `Enregistrer`
runs the dry run, shows the confirmation when events are concerned, then applies.

### 19.10 Edge cases

- The legend, filters, event dialog select and panel use the localized plural/singular labels;
  custom types appear after the built-in bar types and are filterable like the others.
- Print and kiosk use the same palette and labels.
- The event dialog default category rules (§8.6) refer to `celebration` / `trip` by code.
- A custom type's chip, bar and panel entries have no special behaviour.

### 19.11 Milestones

| # | Milestone | Done when |
|---|---|---|
| M11 | Catalog core | `TypeCatalog`/`Lang` in `domain`; stores use the catalog; localized validation; migration tables; unit tests. |
| M12 | Server | `TypeRegistry`, `type_settings`, language resolution, localized errors/pages/imports, `/api/admin/types` with resumable migration, MCP changes; integration tests. |
| M13 | Front-end | i18n dictionaries, language switch, palette, dynamic categories, Catégories section, print/kiosk; Playwright (FR identical to before, EN covered, catalog editing). |
| M14 | Docs | SPEC/SETUP/DECISIONS/PROGRESS aligned. |
