-- Personal access tokens (also kiosk links).
CREATE TABLE pat_tokens (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, token_hash BLOB NOT NULL UNIQUE,
  scope TEXT NOT NULL CHECK (scope IN ('read','read_write')), kiosk INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL, created_by TEXT NOT NULL, last_used_at TEXT, revoked_at TEXT
);

-- OAuth 2.1 authorization server state (for MCP clients).
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

-- Repères import job history.
CREATE TABLE import_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT, started_at TEXT NOT NULL, finished_at TEXT,
  status TEXT NOT NULL CHECK (status IN ('running','success','failed')),
  created INTEGER, updated INTEGER, deleted INTEGER, unchanged INTEGER, error TEXT, trigger TEXT
);
