# Family Year Planner — Installation guide

This guide is for the person installing the app. It takes about an hour. You need:

- a Google account that will own the family calendars (your own is fine);
- a Linux server with a public IP address, where you can run Docker;
- a domain you control (this guide uses `famille.example.org`);
- the GitHub repository where Claude Code built the app.

Menu names in Google's consoles change from time to time. If a label below doesn't match
exactly, look for the closest equivalent.

---

## Step 1 — Create the two Google calendars

In Google Calendar on the web, with the account that will own the calendars:

1. **Settings** (gear icon) → **Add calendar** → **Create new calendar**.
2. Create **Repères** (description: "Vacances scolaires, crèche / centre de loisirs, anniversaires, jours fériés"),
   time zone **Europe/Paris**.
3. Create **Famille** (description: "Événements de la famille"), time zone **Europe/Paris**.
4. For each calendar, open its settings and copy the **Calendar ID** (section **Integrate
   calendar**, it looks like `abc123…@group.calendar.google.com`). Keep both IDs for step 5.
5. Optional: share both calendars with Camille's Google account ("Make changes to events") so they
   appear in her Google Calendar app too.

## Step 2 — Create the Google Cloud project and service account

The server reads and writes the calendars through a **service account** (a robot Google account).

1. Go to <https://console.cloud.google.com>, create a project named `family-year-planner`.
2. **APIs & Services** → **Library** → enable **Google Calendar API**.
3. **IAM & Admin** → **Service accounts** → **Create service account**:
   name `annee-calendar`, no roles needed. Copy its email
   (`annee-calendar@family-year-planner.iam.gserviceaccount.com`).
4. Open the service account → **Keys** → **Add key** → **Create new key** → **JSON**.
   A file downloads: this is `service-account.json`. Keep it secret.
   (If key creation is blocked, your Google Workspace organization forbids it; use a personal
   Google account for the project, or ask your admin.)

## Step 3 — Share the calendars with the service account

Back in Google Calendar, for **each** of the two calendars:

1. Settings of the calendar → **Share with specific people or groups** → **Add people**.
2. Paste the service account email, permission **Make changes to events**, **Send**.

## Step 4 — Create the Google Sign-In client

This lets Alex and Camille log in to the app with their Google accounts. The app only asks for
your email address, never for calendar access.

1. In the same Cloud project, open **Google Auth Platform** (or **APIs & Services** →
   **OAuth consent screen**).
2. **Branding**: app name `Family Year Planner`, support email, authorized domain `example.org`.
3. **Audience**: **External**. Under **Test users**, add Alex's and Camille's Google emails.
   (Because the app only uses sign-in scopes, staying in "Testing" with these two test users is
   enough.)
4. **Clients** → **Create client** → **Web application**, name `annee-web`:
   - Authorized JavaScript origin: `https://famille.example.org`
   - Authorized redirect URI: `https://famille.example.org/auth/google/callback`
5. Copy the **Client ID** and **Client secret**.

## Step 5 — Point the domain to the server

At your DNS provider, create an `A` record (and `AAAA` if the server has IPv6):
`famille.example.org → <server IP>`. Make sure ports **80** and **443** are open on the server's
firewall (Caddy needs both to obtain the HTTPS certificate).

## Step 6 — Prepare the server

1. Install Docker Engine and the Compose plugin (follow docs.docker.com for your distribution).
2. Create the folder and copy the deployment files from the repository's `deploy/` folder. The
   final layout on the server (`/opt/annee` is the root; two files go in the `config` subfolder,
   nothing else is needed):

   ```text
   /opt/annee/
   ├── docker-compose.yml        ← deploy/docker-compose.yml
   ├── Caddyfile                 ← deploy/Caddyfile
   ├── .env                      ← deploy/.env.example (renamed, then edited)
   ├── config/
   │   └── config.toml           ← deploy/config.example.toml (renamed, then edited)
   ├── secrets/
   │   └── service-account.json  ← the key downloaded in step 2
   └── data/                     ← created empty; the app stores annee.db here
   ```

   These files are copied and customised once: they hold your secrets and settings, so they are
   not updated automatically. With the repository cloned on your computer (here `./family-calendar`)
   and `SERVER` the SSH address of the server:

   ```bash
   ssh SERVER 'sudo mkdir -p /opt/annee/{config,secrets,data} && sudo chown -R $USER /opt/annee'
   scp family-calendar/deploy/docker-compose.yml family-calendar/deploy/Caddyfile SERVER:/opt/annee/
   scp family-calendar/deploy/.env.example SERVER:/opt/annee/.env
   scp family-calendar/deploy/config.example.toml SERVER:/opt/annee/config/config.toml
   ssh SERVER 'chmod 600 /opt/annee/.env'
   ```

   (If you work directly on the server, `cp` the same files into the same places.) All the
   following commands are run from `/opt/annee`.

   In `docker-compose.yml`, replace `OWNER` in the image name by your GitHub user or organisation
   (lowercase): the image is `ghcr.io/<owner>/family-year-planner:latest`, published by the
   repository's `release` workflow on every push to `main`.

   The container runs as a non-root user (uid `65532`), so give it the data folder:
   `sudo chown -R 65532:65532 /opt/annee/data`.

3. Put the service account key at `/opt/annee/secrets/service-account.json`, readable only by
   that user: `sudo chown 65532:65532 secrets/service-account.json && sudo chmod 600 secrets/service-account.json`.
4. Edit `config/config.toml`:
   - `base_url = "https://famille.example.org"`
   - `allowed_emails` = Alex's and Camille's Google emails
   - `[app.user_names]` = email → first name for both
   - `members = ["Alex", "Camille", "Sacha", "Noé"]`
   - `reperes_id` and `famille_id` = the two calendar IDs from step 1
   - `school_zone = "Zone B"`, `school_location = "Rennes"`
5. Edit `.env`:
   - `DOMAIN=famille.example.org`
   - `GOOGLE_OAUTH_CLIENT_ID=…` and `GOOGLE_OAUTH_CLIENT_SECRET=…` from step 4
   - `SESSION_KEY=` → output of `openssl rand -base64 64 | tr -d '\n'` (one line)
   - `OAUTH_SIGNING_KEY=` → output of `openssl rand -base64 32`
   - leave the other values as in the example (`GOOGLE_SERVICE_ACCOUNT_FILE`, `APP_CONFIG`,
     `DATABASE_URL` already match the folders above).
6. If the GitHub repository (and so the image) is private, log in to the registry once:
   `docker login ghcr.io` with your GitHub username and a token that has `read:packages`.

### Shared server: ports 80/443 already in use

If another Compose project already runs a reverse proxy (for example Caddy) on ports 80/443,
do **not** start our own. `annee` joins the proxy's Docker network, publishes no port, and the
existing proxy forwards your domain to it. Below, `<proxy-network>` is that Docker network
(`docker network ls`), `<proxy-container>` the proxy container and `<proxy-caddyfile>` the
Caddyfile it mounts.

1. **Folder.** Create `/opt/annee` owned by your user, with the same layout as above:

   ```bash
   sudo mkdir -p /opt/annee && sudo chown -R "$USER": /opt/annee
   cd /opt/annee && mkdir -p config secrets data
   ```

   Then, **from your computer** (repository root), copy the Compose variant (it replaces
   `docker-compose.yml`) and the examples:

   ```bash
   scp -P <ssh-port> deploy/docker-compose.shared-proxy.yml you@server:/opt/annee/docker-compose.yml
   scp -P <ssh-port> deploy/config.example.toml you@server:/opt/annee/config/config.toml
   scp -P <ssh-port> deploy/.env.example you@server:/opt/annee/.env
   ```

   Final layout: `/opt/annee/{docker-compose.yml, .env, config/config.toml, secrets/service-account.json, data/}`.

2. **Edit** (on the server, from `/opt/annee`):
   - `sed -i 's/OWNER/your-github-user/' docker-compose.yml` (image
     `ghcr.io/your-github-user/family-year-planner:latest`, lowercase);
   - `.env`: set `PROXY_NETWORK=<proxy-network>` and the other values (step 6.5); `chmod 600 .env`;
   - `config/config.toml`: step 6.4.
3. **Permissions** (the container runs as uid 65532):

   ```bash
   sudo chown -R 65532:65532 data
   sudo chown 65532:65532 secrets/service-account.json && sudo chmod 600 secrets/service-account.json
   ```

4. **Registry login** (only if the image is private, once): `docker login ghcr.io -u <user>` with a
   token that has `read:packages`. The image only exists after the `release` workflow has run.
5. **Check, then start the app** (still from `/opt/annee`):

   ```bash
   docker compose pull
   docker compose run --rm annee check-config      # must end with "all checks passed"
   docker compose up -d
   docker compose ps                                # annee should become "healthy"
   ```

6. **Add the site to the existing proxy.** With Caddy, append a block to its Caddyfile *in place*
   (an editor that replaces a bind-mounted file would break the mount), validate, reload:

   ```bash
   cat >> <proxy-caddyfile> <<'EOT'

   famille.example.org {
       encode zstd gzip
       reverse_proxy annee:8080
   }
   EOT
   docker exec <proxy-container> caddy validate --config /etc/caddy/Caddyfile
   docker exec <proxy-container> caddy reload --config /etc/caddy/Caddyfile
   docker logs --tail 20 <proxy-container>   # look for "certificate obtained successfully"
   ```

   The DNS record must already point to the server and ports 80/443 must be open, so the
   certificate is issued on the first request.
7. **Verify from your computer:**

   ```bash
   curl -s https://famille.example.org/healthz          # {"mode":"prod","status":"ok","version":"…"}
   curl -sI https://famille.example.org/mcp | head -3   # HTTP/2 401 + a www-authenticate header
   ```

Notes:
- **Updates:** `docker compose pull && docker compose up -d` in `/opt/annee`. The label in the
  Compose file keeps a `watchtower` container of another project from touching `annee`. Remove it
  if you prefer automatic updates (and give Watchtower registry credentials for a private image).
- **Client addresses:** the proxy must set `X-Forwarded-For` itself (Caddy does); the app uses it
  for its rate limits. If Docker hides the visitor's address behind its gateway (possible over
  IPv6), everyone shares one counter; for a family this is harmless.

If the proxy is not a Docker Caddy, the requirements are in short: HTTPS for the domain, original
`Host` header, `X-Forwarded-For` set to the real client address (replacing any client value), no
response buffering; forward to the container (`annee:8080` on a shared network, or publish
`127.0.0.1:8080:8080` in the Compose file and forward to that).

## Step 7 — Check and start

```bash
cd /opt/annee
docker compose pull
docker compose run --rm annee check-config   # validates config, secrets and calendar access
docker compose up -d
```

`check-config` must list both calendars by name and confirm write access (it creates and deletes
a temporary event in each). If it fails, see
"Troubleshooting" below.

Then:

1. Open <https://famille.example.org/healthz>: you should see `"status":"ok"`.
2. Open <https://famille.example.org>, click **Se connecter avec Google**, log in as Alex.
3. Go to **Réglages** → **Imports** → **Mettre à jour maintenant**. After a few seconds the run
   shows **Réussi**. Back on the year view, school holidays (yellow) and public holidays appear.
   Sanity check: the Toussaint 2025 holidays cover 18 October to 2 November 2025.
4. Add a test event, check it appears in Google Calendar, then delete it.

## Step 8 — Connect AI agents

The MCP server address is `https://famille.example.org/mcp` (also shown in **Réglages**).

**Claude (web, desktop, mobile):** Settings → Connectors → **Add custom connector** → paste the
MCP address → **Connect**. You are sent to the app: log in with Google if asked, then
**Autoriser**.

**Claude Code:**

```bash
claude mcp add --transport http annee https://famille.example.org/mcp
```

Then run `/mcp` in Claude Code to complete the sign-in. Alternatively, with a personal token:

```bash
claude mcp add --transport http annee https://famille.example.org/mcp \
  --header "Authorization: Bearer fy_pat_…"
```

**Agents that use a token (Grok API, Grok Build, Codex, scripts…):** in **Réglages** →
**Accès des agents**, create a token named after the agent (choose **Lecture et écriture** if it
should add events), copy it, and configure the agent with the MCP address and the header
`Authorization: Bearer fy_pat_…`. Examples:

- Grok Build: `grok mcp add --transport http annee https://famille.example.org/mcp --header "Authorization: Bearer fy_pat_…"`
- Grok API: add a remote MCP tool with `server_url` = the MCP address and `authorization` = the
  token (the server accepts it with or without the `Bearer ` prefix).
- Other agents: configure a remote "Streamable HTTP" MCP server with the address, then use
  either their OAuth sign-in or the bearer token header, as described in their documentation.

You can revoke any token or OAuth connection at any time in **Réglages**.

## Step 9 — Set up the wall tablet

1. In **Réglages** → **Tablette (mode kiosque)** → **Créer un lien kiosque**, copy the link.
2. Open it once on the tablet's browser. The token disappears from the address bar and the
   tablet stays connected (read-only) for over a year.
3. Lock the tablet on that page: on Android, a kiosk browser app or screen pinning; on iPad,
   Guided Access (Settings → Accessibility). Keep the tablet plugged in.

## Updates, backups, secrets

- **Update:** `cd /opt/annee && docker compose pull && docker compose up -d`.
- **Backup:** `/opt/annee/config`, `/opt/annee/secrets`, `.env` and `/opt/annee/data/annee.db`
  (tokens and import history). All calendar data lives in Google Calendar.
- **Rotate secrets:** changing `SESSION_KEY` logs everyone out (kiosk included); changing
  `OAUTH_SIGNING_KEY` forces agents to refresh their tokens; a new service account key requires
  replacing `service-account.json` and restarting.

## Troubleshooting

| Symptom | Likely cause and fix |
|---|---|
| `check-config`: 404 or 403 on a calendar | Wrong calendar ID in `config.toml`, or the calendar is not shared with the service account (step 3) with "Make changes to events". |
| Login: `Ce compte Google n’est pas autorisé.` | The email is missing from `allowed_emails` (case doesn't matter). |
| Login: Google shows `redirect_uri_mismatch` | The redirect URI in step 4 must be exactly `https://famille.example.org/auth/google/callback`. |
| Login: Google says the app is not available / access blocked | The account is not in the test users list (step 4.3). |
| `caddy` fails with `address already in use` | Ports 80/443 are taken by another service: use the shared-server variant above. |
| No HTTPS certificate | DNS not pointing to the server yet, or ports 80/443 closed. See `docker compose logs caddy`. |
| Import shows **Échec** | data.education.gouv.fr unreachable or changed; the error is shown in Réglages. Existing holidays are kept. Retry later. |
| An agent gets `401` | Token revoked or mistyped, or OAuth connection revoked; reconnect or create a new token. |
| `check-config` or the server: `unable to open database file` / permission denied | The `data` folder or `service-account.json` is not owned by uid 65532 (see step 6). |
| An agent can read but not add events | The token is read-only; create one with **Lecture et écriture**. |
