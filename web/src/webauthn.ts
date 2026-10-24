// Conversion entre le JSON du serveur (base64url) et l'API WebAuthn du navigateur.
//
// La biométrie (empreinte, visage) est vérifiée par l'appareil lui-même : le navigateur
// ne transmet qu'une signature et, en mode lié, une sortie PRF dérivée d'un secret qui
// ne quitte jamais l'authentificateur.

type Json = Record<string, unknown>;

export function b64urlToBytes(s: string): Uint8Array<ArrayBuffer> {
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/").replace(/=+$/, "");
  const padded = b64 + "=".repeat((4 - (b64.length % 4)) % 4);
  const bin = atob(padded);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function bytesToB64url(data: ArrayBuffer | ArrayBufferView): string {
  const bytes =
    data instanceof ArrayBuffer
      ? new Uint8Array(data)
      : new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function str(v: unknown, field: string): string {
  if (typeof v !== "string") throw new Error(`champ WebAuthn invalide : ${field}`);
  return v;
}

function descriptors(list: unknown): PublicKeyCredentialDescriptor[] {
  return Array.isArray(list)
    ? list.map((c: Json) => ({ type: "public-key", id: b64urlToBytes(str(c.id, "id")) }))
    : [];
}

export function creationOptions(json: Json): PublicKeyCredentialCreationOptions {
  const user = json.user as Json;
  return {
    ...(json as unknown as PublicKeyCredentialCreationOptions),
    challenge: b64urlToBytes(str(json.challenge, "challenge")),
    user: {
      id: b64urlToBytes(str(user.id, "user.id")),
      name: str(user.name, "user.name"),
      displayName: str(user.displayName, "user.displayName"),
    },
    excludeCredentials: descriptors(json.excludeCredentials),
  };
}

/** Sel PRF demandé par le serveur, s'il y en a un. */
export function prfSalt(json: Json): string | undefined {
  const ext = json.extensions as { prf?: { eval?: { first?: unknown } } } | undefined;
  const first = ext?.prf?.eval?.first;
  return typeof first === "string" ? first : undefined;
}

export function requestOptions(json: Json): PublicKeyCredentialRequestOptions {
  const options: PublicKeyCredentialRequestOptions = {
    ...(json as unknown as PublicKeyCredentialRequestOptions),
    challenge: b64urlToBytes(str(json.challenge, "challenge")),
    allowCredentials: descriptors(json.allowCredentials),
  };
  const salt = prfSalt(json);
  if (salt) {
    options.extensions = { prf: { eval: { first: b64urlToBytes(salt) } } } as AuthenticationExtensionsClientInputs;
  }
  return options;
}

interface PrfResults {
  prf?: { enabled?: boolean; results?: { first?: ArrayBuffer | ArrayBufferView } };
}

export interface RegistrationJSON {
  id: string;
  clientDataJSON: string;
  attestationObject: string;
}

export function registrationJSON(cred: PublicKeyCredential): {
  response: RegistrationJSON;
  prfSupported: boolean | undefined;
} {
  const r = cred.response as AuthenticatorAttestationResponse;
  const ext = cred.getClientExtensionResults() as PrfResults;
  return {
    response: {
      id: bytesToB64url(cred.rawId),
      clientDataJSON: bytesToB64url(r.clientDataJSON),
      attestationObject: bytesToB64url(r.attestationObject),
    },
    prfSupported: ext.prf?.enabled,
  };
}

export interface AuthenticationJSON {
  id: string;
  clientDataJSON: string;
  authenticatorData: string;
  signature: string;
}

export function authenticationJSON(cred: PublicKeyCredential): {
  response: AuthenticationJSON;
  prfOutput: string | undefined;
} {
  const r = cred.response as AuthenticatorAssertionResponse;
  const first = (cred.getClientExtensionResults() as PrfResults).prf?.results?.first;
  return {
    response: {
      id: bytesToB64url(cred.rawId),
      clientDataJSON: bytesToB64url(r.clientDataJSON),
      authenticatorData: bytesToB64url(r.authenticatorData),
      signature: bytesToB64url(r.signature),
    },
    prfOutput: first ? bytesToB64url(first) : undefined,
  };
}

export function webauthnAvailable(): boolean {
  return typeof window !== "undefined" && "PublicKeyCredential" in window && window.isSecureContext;
}

/** Demande une signature à l'appareil (biométrie ou PIN de l'appareil). */
export async function getAssertion(options: Json) {
  const cred = (await navigator.credentials.get({ publicKey: requestOptions(options) })) as PublicKeyCredential | null;
  if (!cred) throw new Error("opération annulée");
  return authenticationJSON(cred);
}

export async function createCredential(options: Json) {
  const cred = (await navigator.credentials.create({ publicKey: creationOptions(options) })) as PublicKeyCredential | null;
  if (!cred) throw new Error("opération annulée");
  return registrationJSON(cred);
}

/** Message lisible pour les erreurs du navigateur. */
export function describeWebAuthnError(e: unknown): string {
  if (e instanceof DOMException) {
    if (e.name === "NotAllowedError") return "Opération annulée ou délai dépassé.";
    if (e.name === "InvalidStateError") return "Cette clé est déjà enregistrée.";
    if (e.name === "SecurityError")
      return "Le navigateur refuse : l'adresse doit être un nom de domaine en HTTPS avec un certificat reconnu.";
    if (e.name === "NotSupportedError") return "Cet appareil ne prend pas en charge ce type de clé.";
  }
  return e instanceof Error ? e.message : String(e);
}
