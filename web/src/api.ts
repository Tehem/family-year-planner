// Typed client for the JSON API of SPEC §7.
import { t } from "./i18n";

export interface TypeInfo {
  code: string;
  label: string;
  label_plural: string;
  calendar: "reperes" | "famille" | null;
  writable: boolean;
  render: "tint" | "bar";
  /** Google colorId (SPEC §19.4); `null` for `unclassified`. */
  color_id: string | null;
  /** A category the family added (SPEC §19.3). */
  custom: boolean;
}

export interface Config {
  /** Language of this request and of `types` (SPEC §19.2). */
  lang: string;
  default_language: string;
  members: string[];
  types: TypeInfo[];
  timezone: string;
  school_zone: string;
  today: string;
  base_url: string;
  mcp_url: string;
  kiosk: { refresh_minutes: number; default_view: string };
}

export interface Me {
  email: string;
  name: string;
  kiosk: boolean;
  demo: boolean;
}

export interface ApiEvent {
  id: string;
  series_id: string | null;
  calendar: string;
  type: string;
  title: string;
  start: string;
  end: string;
  start_time: string | null;
  end_time: string | null;
  members: string[];
  location: string | null;
  notes: string | null;
  birth_year: number | null;
  age: number | null;
  source: string;
  read_only: boolean;
  created_by: string | null;
  updated_by: string | null;
  google_url: string | null;
}

export interface EventInput {
  type?: string;
  title?: string;
  start?: string;
  end?: string;
  members?: string[];
  location?: string | null;
  notes?: string | null;
  birth_year?: number | null;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

let loginUrl = (): string => {
  const here = window.location.pathname + window.location.search;
  return here === "/" ? "/connexion?next=/" : `/connexion?next=${encodeURIComponent(here)}`;
};
/** Overrides where a `401` sends the browser (the kiosk page shows its own message). */
export function setLoginUrl(url: string): void {
  loginUrl = () => url;
}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = { Accept: "application/json" };
  const init: RequestInit = { method, headers, credentials: "same-origin" };
  if (method !== "GET") headers["X-Requested-With"] = "annee";
  if (body !== undefined) {
    headers["Content-Type"] = "application/json";
    init.body = JSON.stringify(body);
  }
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch {
    throw new ApiError(0, "network", t("err.network"));
  }
  if (res.status === 401) {
    window.location.assign(loginUrl());
    throw new ApiError(401, "unauthorized", t("err.session"));
  }
  if (!res.ok) {
    let code = "error";
    let message = t("err.generic");
    try {
      const j = (await res.json()) as { error?: { code?: string; message?: string } };
      if (j.error?.message) message = j.error.message;
      if (j.error?.code) code = j.error.code;
    } catch {
      /* not JSON: keep the generic message */
    }
    throw new ApiError(res.status, code, message);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export const getMe = (): Promise<Me> => call("GET", "/api/me");
export const getConfig = (): Promise<Config> => call("GET", "/api/config");
export const getEvents = (year: number): Promise<{ year: number; events: ApiEvent[] }> =>
  call("GET", `/api/events?year=${year}`);
export const createEvent = (input: EventInput): Promise<ApiEvent> => call("POST", "/api/events", input);
export const updateEvent = (id: string, input: EventInput): Promise<ApiEvent> =>
  call("PATCH", `/api/events/${encodeURIComponent(id)}`, input);
export const deleteEvent = (id: string): Promise<void> => call("DELETE", `/api/events/${encodeURIComponent(id)}`);
export const resetDemo = (): Promise<void> => call("POST", "/api/demo/reset", {});

// ---- Réglages (SPEC §8.9) ----
export type Scope = "read" | "read_write";
export interface Token {
  id: string;
  name: string;
  scope: Scope;
  created_at: string;
  last_used_at: string | null;
  kiosk: boolean;
}
export interface NewToken {
  id: string;
  token: string;
  kiosk_url?: string;
}
export interface Grant {
  id: string;
  client_name: string;
  redirect_host: string;
  scope: string;
  created_at: string;
  last_used_at: string | null;
}
export interface ImportRun {
  id: string | number;
  started_at: string;
  finished_at: string | null;
  status: "running" | "success" | "failed";
  created: number;
  updated: number;
  deleted: number;
  unchanged: number;
  error: string | null;
  trigger: string;
}
export interface ImportStatus {
  running: boolean;
  runs: ImportRun[];
}

export const listTokens = (): Promise<Token[]> => call("GET", "/api/admin/tokens");
export const createToken = (input: { name: string; scope?: Scope; kiosk?: boolean }): Promise<NewToken> =>
  call("POST", "/api/admin/tokens", input);
export const revokeToken = (id: string): Promise<void> => call("DELETE", `/api/admin/tokens/${encodeURIComponent(id)}`);
export const listGrants = (): Promise<Grant[]> => call("GET", "/api/admin/oauth-grants");
export const revokeGrant = (id: string): Promise<void> =>
  call("DELETE", `/api/admin/oauth-grants/${encodeURIComponent(id)}`);
export const getImports = (): Promise<ImportStatus> => call("GET", "/api/admin/import");
export const startImport = (): Promise<{ running: boolean }> => call("POST", "/api/admin/import", {});

// ---- Catégories (SPEC §19.7) ----
export interface TypeNames {
  singular: string;
  plural: string;
}
export interface EditableType {
  code: string;
  custom?: boolean;
  color_id: string | null;
  labels: { fr: TypeNames; en: TypeNames };
}
export interface PaletteEntry {
  color_id: string;
  name_fr: string;
  name_en: string;
}
export interface MigrationState {
  status: string;
  error: string | null;
  patched: number | null;
}
export interface TypesEditor {
  /** Identifies the catalog the editor was loaded with; sent back on save. */
  revision: string;
  palette: PaletteEntry[];
  max_custom: number;
  max_name: number;
  free_colors: string[];
  types: EditableType[];
  migration: MigrationState | null;
}
export interface TypeChange {
  code: string;
  from_color_id: string | null;
  to_color_id: string | null;
  events: number;
}
export interface TypesResult {
  changes: TypeChange[];
  orphans: number;
  applied: boolean;
}

export const getTypes = (): Promise<TypesEditor> => call("GET", "/api/admin/types");
export const putTypes = (types: EditableType[], dryRun: boolean, revision: string): Promise<TypesResult> =>
  call("PUT", "/api/admin/types", {
    types: types.map(({ code, color_id, labels }) => ({ code, color_id, labels })),
    dry_run: dryRun,
    revision,
  });
export const resumeTypes = (): Promise<{ resumed: boolean }> => call("POST", "/api/admin/types/resume", {});
