# CLAUDE.md — instructions for Claude Code

You are building **Family Year Planner** from scratch, autonomously. The full specification is
`docs/SPEC.md`. The visual design is locked by `design/prototype.html`.

## Golden rules

1. **The spec is the contract.** Implement what `docs/SPEC.md` says. Do not add product
   features that are not in it. Do not drop features that are in it.
2. **The prototype is the design.** `design/prototype.html` is the source of truth for layout,
   geometry, colors, typography, interactions and French copy. When the spec and the prototype
   disagree on a visual detail, the prototype wins. Never edit `design/prototype.html`.
   SPEC §8 lists the few deliberate differences (new fields, edit mode, settings page, etc.).
3. **Work autonomously.** When something is ambiguous, choose the simplest option consistent
   with the spec, implement it, and record it in `docs/DECISIONS.md` (create it): one entry per
   decision with context, choice, and alternatives considered. Do not stop to ask questions.
4. **No real credentials, ever.** Tests, local runs and CI use the in-memory demo store
   (`APP_MODE=demo`). Never require Google credentials, a domain, or network access to Google
   to build or test. External HTTP sources (school holidays dataset, CIMD documents) are tested
   with recorded fixtures committed to the repo.
5. **Human-only steps are not blockers.** Anything that needs the Google Cloud console, DNS, or
   the server is documented in `docs/SETUP.md`. Keep that guide accurate whenever your
   implementation changes a name, path, environment variable, route or URL.

## Languages

- User-facing text (web app, kiosk, print view, OAuth consent page, error messages shown to users): **French**.
- Code, identifiers, comments, commit messages, logs, technical docs: **English**.
- MCP tool names and parameters: English. MCP tool descriptions: English, mentioning French labels where useful.

## Stack (summary; details in SPEC §12)

- Backend: Rust (stable), Cargo workspace, `axum` + `tokio`. One binary.
- Frontend: TypeScript, **no UI framework**, built with Vite, embedded in the binary.
- Data: Google Calendar holds all calendar data. One SQLite file holds auth state and import runs.
- Deployment: Docker image (multi-arch amd64/arm64) on GHCR, run with Docker Compose behind Caddy.

## How to work

- Follow the milestones in SPEC §17 in order. Each milestone ends with all checks green,
  a commit (`feat(m3): …`), and a short entry in `docs/PROGRESS.md` (what was done, what is next).
- Checks to run before each commit:
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all`
  - `npm --prefix web run typecheck && npm --prefix web run build`
  - `npm --prefix web run test:e2e` (Playwright against the binary in demo mode), from milestone M4 onward
- Keep the `domain` crate free of I/O. Put everything Google-specific behind the
  `CalendarStore` trait so the demo store and the Google store are interchangeable.
- Prefer the crates listed in SPEC §12. If you must deviate, log it in `docs/DECISIONS.md`.
- Verify facts you are unsure about against the official documentation (Google Calendar API,
  data.education.gouv.fr, MCP specification, rmcp crate docs) before relying on them, and note
  what you verified in `docs/DECISIONS.md`.

## When you are done

Write a final message listing: milestones completed, anything not implemented and why,
and the exact checklist the human must follow next (pointing to `docs/SETUP.md`).
