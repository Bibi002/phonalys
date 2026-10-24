import { useState, type FormEvent } from "react";
import { api, ApiError } from "../api";
import { describeError, ErrorBox, Field, PasswordField } from "../components";
import { describeWebAuthnError, getAssertion, prfSalt, webauthnAvailable } from "../webauthn";

type Method = "passkey" | "totp" | "recovery";

// Simple préférence d'affichage (aucune donnée sensible) : la dernière méthode réussie.
const METHOD_KEY = "phonalys.loginMethod";

function savedMethod(): Method {
  try {
    const m = localStorage.getItem(METHOD_KEY);
    if (m === "passkey" && webauthnAvailable()) return "passkey";
  } catch {
    // stockage indisponible
  }
  // Juste après l'installation, aucune clé n'existe : le code à 6 chiffres d'abord.
  return "totp";
}

function rememberMethod(m: Method) {
  try {
    if (m !== "recovery") localStorage.setItem(METHOD_KEY, m);
  } catch {
    // stockage indisponible
  }
}

const NO_PASSKEY =
  "Aucune clé biométrique n'est encore enregistrée. Connectez-vous avec le code à 6 chiffres de votre application d'authentification ; vous pourrez ensuite activer l'empreinte, le visage ou Windows Hello dans « Sécurité ».";

export function Login({ onLoggedIn }: { onLoggedIn: () => void }) {
  const [method, setMethod] = useState<Method>(savedMethod);
  const [passphrase, setPassphrase] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [blocked, setBlocked] = useState(false);

  function choose(m: Method) {
    setMethod(m);
    setCode("");
    setError(undefined);
  }

  async function attempt() {
    if (method === "passkey") {
      const options = await api<Record<string, unknown>>("POST", "/api/login/passkey/start", { passphrase });
      const { response, prfOutput } = await getAssertion(options);
      if (prfSalt(options) && !prfOutput) {
        throw new Error(
          "Ce coffre est lié au matériel, mais cet appareil ou ce navigateur ne fournit pas l'extension PRF. Utilisez une clé liée ou un code de récupération.",
        );
      }
      await api("POST", "/api/login/passkey/finish", prfOutput ? { response, prf_output: prfOutput } : { response });
    } else if (method === "totp") {
      await api("POST", "/api/login", { passphrase, code: code.replace(/\s/g, "") });
    } else {
      await api("POST", "/api/login", { passphrase, recovery_code: code });
    }
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(undefined);
    attempt()
      .then(() => {
        rememberMethod(method);
        setPassphrase("");
        setCode("");
        onLoggedIn();
      })
      .catch((err: unknown) => {
        if (err instanceof ApiError && err.status === 400 && /aucune clé biométrique/.test(err.message)) {
          setMethod("totp");
          setError(NO_PASSKEY);
          return;
        }
        setBlocked(err instanceof ApiError && (err.status === 429 || /bloquée/.test(err.message)));
        setError(err instanceof DOMException ? describeWebAuthnError(err) : describeError(err));
      })
      .finally(() => setBusy(false));
  }

  return (
    <main className="narrow">
      <form className="card" onSubmit={submit}>
        <h1>Phonalys</h1>
        <p className="muted">Vos données sont chiffrées. Identifiez-vous pour les ouvrir.</p>
        <div className="tabs" role="tablist" aria-label="Méthode de connexion">
          {(
            [
              ["passkey", "Clé biométrique"],
              ["totp", "Code à 6 chiffres"],
              ["recovery", "Code de récupération"],
            ] as const
          ).map(([m, label]) => (
            <button
              key={m}
              type="button"
              role="tab"
              aria-selected={method === m}
              className={method === m ? "tab active" : "tab"}
              onClick={() => choose(m)}
            >
              {label}
            </button>
          ))}
        </div>
        <PasswordField
          label="Phrase de passe"
          value={passphrase}
          onChange={setPassphrase}
          autoComplete="current-password"
          required
          autoFocus
          hint="Celle choisie à l'installation, exactement (majuscules, accents et espaces comptent)."
        />
        {method === "totp" && (
          <Field
            label="Code de l'application d'authentification"
            value={code}
            onChange={setCode}
            autoComplete="one-time-code"
            inputMode="numeric"
            required
          />
        )}
        {method === "recovery" && (
          <Field
            label="Code de récupération"
            value={code}
            onChange={setCode}
            required
            hint="Format XXXX-XXXX-XXXX-XXXX. Chaque code ne sert qu'une fois et débloque aussi une connexion bloquée."
          />
        )}
        {method === "passkey" && (
          <p className="muted">
            Après la phrase de passe, votre appareil demande l'empreinte, le visage ou le code Windows Hello. La clé
            sert à chaque connexion, sans limite.
          </p>
        )}
        {method === "passkey" && !webauthnAvailable() && (
          <p className="warning">
            Les clés biométriques exigent une connexion HTTPS avec un nom de domaine et un certificat reconnu.
          </p>
        )}
        <ErrorBox error={error} />
        {blocked && method !== "recovery" && (
          <button type="button" className="ghost" onClick={() => choose("recovery")}>
            Débloquer avec un code de récupération
          </button>
        )}
        <button type="submit" disabled={busy}>
          {busy ? "Vérification…" : method === "passkey" ? "Continuer avec ma clé" : "Déverrouiller"}
        </button>
      </form>
    </main>
  );
}
