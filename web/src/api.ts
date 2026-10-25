// Accès à l'API du serveur Phonalys (même origine uniquement).

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
    readonly retryAfter?: number,
    /** Le serveur demande de confirmer son identité (documents du coffre). */
    readonly reauth = false,
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
    let reauth = false;
    try {
      const data = (await res.json()) as { error?: unknown; reauth?: unknown };
      if (typeof data.error === "string") message = data.error;
      reauth = data.reauth === true;
    } catch {
      // réponse sans JSON
    }
    const retry = Number(res.headers.get("retry-after"));
    if (res.status === 401 && !PUBLIC_ROUTES.some((r) => path.startsWith(r))) onUnauthorized?.();
    throw new ApiError(res.status, message, Number.isFinite(retry) && retry > 0 ? retry : undefined, reauth);
  }
  onActivity?.();
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

// --- Types de l'API ------------------------------------------------------------------

export interface Status {
  initialized: boolean;
  /** Au moins une clé permet la connexion en un geste. */
  quick_unlock?: boolean;
  /** Après plusieurs échecs, deux vérifications sont exigées. */
  step_up?: boolean;
}

export interface SetupResult {
  totp_uri: string;
  totp_secret: string;
  totp_qr_svg: string;
  recovery_codes: string[];
}

export interface SessionInfo {
  idle_timeout_secs: number;
  /** Identité confirmée récemment : les documents du coffre sont consultables. */
  elevated?: boolean;
  elevated_secs?: number;
  /** Au-delà de cette durée en arrière-plan, l'appareil ferme sa session (0 : jamais). */
  hidden_lock_secs?: number;
}

export type { Patient } from "./model";

export interface PasskeyInfo {
  id: string;
  label: string;
  backup_eligible: boolean;
  aaguid: string;
  bound: boolean;
  quick?: boolean;
}

export interface SecurityStatus {
  require_passkey: boolean;
  hardware_bound: boolean;
  passkeys: number;
  bound_passkeys: number;
  quick_passkeys?: number;
  recovery_codes_remaining: number;
  /** Altération du journal d'audit constatée à l'ouverture du coffre. */
  audit_alert?: string | null;
}

export interface BindingResult {
  hardware_bound: boolean;
  recovery_codes?: string[];
}

export interface Settings {
  idle_timeout_minutes: number;
  practitioner: Record<string, string>;
  auto_backup: boolean;
  backup_extra_dir?: string | null;
  hidden_lock_minutes?: number;
}

export interface AuditEntry {
  seq: number;
  at: string;
  action: string;
  target: string | null;
}

export interface AuditPage {
  verified: boolean;
  alert?: string | null;
  total: number;
  entries: AuditEntry[];
}

// --- Dossier clinique -------------------------------------------------------------------

/** Types d'enregistrements acceptés par le serveur (liste blanche). */
export type RecordKind = "seance" | "bilan" | "objectif" | "exercice";

export const records = {
  list<T>(kind: RecordKind, patientId?: string): Promise<(T & { id: string })[]> {
    const q = patientId ? `?patient_id=${encodeURIComponent(patientId)}` : "";
    return api("GET", `/api/records/${kind}${q}`);
  },
  async save<T extends { id?: string }>(kind: RecordKind, value: T): Promise<string> {
    const { id, ...data } = value;
    if (id) {
      await api("PUT", `/api/records/${kind}/${id}`, data);
      return id;
    }
    return (await api<{ id: string }>("POST", `/api/records/${kind}`, data)).id;
  },
  remove(kind: RecordKind, id: string): Promise<void> {
    return api("DELETE", `/api/records/${kind}/${id}`);
  },
};