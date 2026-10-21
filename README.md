# Family Year Planner

A family year planner that shows a **whole calendar year on one landscape screen**: every day is a
square, context days (school holidays, daycare or leisure-centre closures, birthdays, public
holidays) are background tints, and family events (gatherings, stays, trips, celebrations) are
labelled bars. The data lives in **two Google Calendars**; the app is just a very good view on them.

- 🗓️ **Web app**: year view, weeks and months layouts, filters by family member, edit mode.
- 🖥️ **Kiosk mode**: a read-only link for a wall tablet, no login needed.
- 🖨️ **Print / PDF**: the year on one page.
- 🤖 **MCP server**: any AI agent (Claude, Grok, …) can read and edit the family year, with OAuth 2.1 or a personal access token.
- 🇫🇷 **Imports**: French school holidays and public holidays, kept up to date automatically.
- 🌍 **French and English** interface (per-browser `FR | EN` switch, French by default).
- 🎨 **Configurable categories**: rename them, recolor them (a Google Calendar event color) and add up to three of your own.

The interface is in French (default) and English; code, docs and MCP tools are in English.

## Quick look (no Google account needed)

```bash
npm --prefix web ci && npm --prefix web run build
APP_MODE=demo APP_FAKE_TODAY=2026-10-16 cargo run -- demo
# open http://localhost:8080 and click « Entrer en mode démo »
```

Demo mode uses an in-memory calendar store seeded with sample data.

## Install it for your family

[`docs/SETUP.md`](docs/SETUP.md) walks through the whole installation: two Google Calendars, a
service account, Google Sign-In, a domain, the Docker Compose stack (the image is published as
`ghcr.io/<owner>/family-year-planner`) and connecting AI agents.

## Documentation

| File | Purpose |
|---|---|
| [`docs/SPEC.md`](docs/SPEC.md) | Functional and technical specification |
| [`docs/SETUP.md`](docs/SETUP.md) | Installation guide (Google, DNS, server, agents) |
| [`docs/DECISIONS.md`](docs/DECISIONS.md) | Decisions taken where the spec was silent |
| [`docs/PROGRESS.md`](docs/PROGRESS.md) | Milestone log |
| [`design/prototype.html`](design/prototype.html) | The locked visual design, open it in a browser |
| [`CLAUDE.md`](CLAUDE.md) | Working rules for AI-assisted development |

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
npm --prefix web run typecheck && npm --prefix web run build
npm --prefix web run test:e2e          # Playwright against `annee demo`
```

Tests never need Google credentials: they run against the in-memory demo store and recorded
fixtures.

Layout: `crates/domain` (rules, no I/O), `crates/gcal` (calendar stores), `crates/server` (web, API,
auth, OAuth, MCP, imports), `web/` (TypeScript front-end), `deploy/` (Compose, Caddy, examples).
CLI: `annee serve | demo | import [--dry-run] | check-config | healthcheck`.

## License

[MIT](LICENSE).
