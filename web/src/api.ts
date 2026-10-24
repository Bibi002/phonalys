// Accès à l'API du serveur Phonalys (même origine uniquement).

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
    readonly retryAfter?: number,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/** Appelé après chaque requête réussie : le serveur prolonge alors la session. */
let onActivity: (() => void) | undefined;
/** Appelé quand le serveur répond 401 sur une route protégée (session expirée). */
let onUnauthorized: (() => void) | undefined;

export function setApiHooks(hooks: { activity?: () => void; unauthorized?: () => void }) {
  onActivity = hooks.activity;
  onUnauthorized = hooks.unauthorized;
}

const PUBLIC_ROUTES = ["/api/status", "/api/setup", "/api/login", "/api/health"];

export async function api<T = unknown>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = { accept: "application/json" };
  if (method !== "GET") headers["x-phonalys-request"] = "1"; // protection CSRF
  if (body !== undefined) headers["content-type"] = "application/json";

  const init: RequestInit = { method, headers, credentials: "same-origin", cache: "no-store" };
  if (body !== undefined) init.body = JSON.stringify(body);
  const res = await fetch(path, init);

  if (!res.ok) {
    let message = `erreur ${res.status}`;
    try {
      const data = (await res.json()) as { error?: unknown };
      if (typeof data.error === "string") message = data.error;
    } catch {
      // réponse sans JSON
    }
    const retry = Number(res.headers.get("retry-after"));
    if (res.status === 401 && !PUBLIC_ROUTES.some((r) => path.startsWith(r))) onUnauthorized?.();
    throw new ApiError(res.status, message, Number.isFinite(retry) && retry > 0 ? retry : undefined);
  }
  onActivity?.();
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

// --- Types de l'API ------------------------------------------------------------------

export interface Status {
  initialized: boolean;
}

export interface SetupResult {
  totp_uri: string;
  totp_secret: string;
  totp_qr_svg: string;
  recovery_codes: string[];
}

export interface SessionInfo {
  idle_timeout_secs: number;
}

export interface Patient {
  id: string;
  nom?: string;
  prenom?: string;
  naissance?: string;
  notes?: string;
}

export interface PasskeyInfo {
  id: string;
  label: string;
  backup_eligible: boolean;
  aaguid: string;
  bound: boolean;
}

export interface SecurityStatus {
  require_passkey: boolean;
  hardware_bound: boolean;
  passkeys: number;
  bound_passkeys: number;
  recovery_codes_remaining: number;
}

export interface BindingResult {
  hardware_bound: boolean;
  recovery_codes?: string[];
}
