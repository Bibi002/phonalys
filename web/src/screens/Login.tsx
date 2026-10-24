import { useEffect, useState, type FormEvent } from "react";
import { api, ApiError, type Status } from "../api";
import { describeError, ErrorBox, Field, PasswordField, tabKeys } from "../components";
import { t } from "../i18n";
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

const STEP_UP =
  "Après 3 échecs, deux vérifications sont demandées pour protéger les dossiers : votre phrase de passe, puis votre clé biométrique ou le code à 6 chiffres.";

const METHODS = [
  ["passkey", "Clé biométrique"],
  ["totp", "Code à 6 chiffres"],
  ["recovery", "Code de récupération"],
] as const;

const describe = (e: unknown) => (e instanceof DOMException ? describeWebAuthnError(e) : describeError(e));
const isStepUp = (e: unknown) => e instanceof ApiError && /deux vérifications/.test(e.message);

export function Login({ onLoggedIn }: { onLoggedIn: () => void }) {
  const [status, setStatus] = useState<Status>();
  // `quick` : un seul geste ; `full` : phrase de passe + second facteur.
  const [mode, setMode] = useState<"quick" | "full">("full");

  useEffect(() => {
    api<Status | undefined>("GET", "/api/status")
      .then((s) => {
        if (!s) return;
        setStatus(s);
        if (s.quick_unlock && !s.step_up && webauthnAvailable()) setMode("quick");
      })
      .catch(() => undefined);
  }, []);

  return (
    <main className="narrow">
      {mode === "quick" ? (
        <QuickLogin
          onLoggedIn={onLoggedIn}
          onStepUp={() => {
            setStatus({ ...(status ?? { initialized: true }), step_up: true });
            setMode("full");
          }}
          onOther={() => setMode("full")}
        />
      ) : (
        <FullLogin
          onLoggedIn={onLoggedIn}
          stepUp={status?.step_up ?? false}
          onQuick={status?.quick_unlock && !status.step_up && webauthnAvailable() ? () => setMode("quick") : undefined}
        />
      )}
    </main>
  );
}

/** Connexion en un geste : la clé biométrique seule. */
function QuickLogin(props: { onLoggedIn: () => void; onStepUp: () => void; onOther: () => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  async function go() {
    setBusy(true);
    setError(undefined);
    try {
      const options = await api<Record<string, unknown>>("POST", "/api/login/quick/start");
      const { response, prfOutput } = await getAssertion(options);
      if (!prfOutput) throw new Error(t("Cet appareil ne fournit pas l'extension PRF : utilisez une autre méthode."));
      await api("POST", "/api/login/quick/finish", { response, prf_output: prfOutput });
      rememberMethod("passkey");
      props.onLoggedIn();
    } catch (e) {
      if (isStepUp(e)) return props.onStepUp();
      setError(describe(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="card login-quick">
      <h1>Phonalys</h1>
      <p className="muted">{t("Vos données sont chiffrées. Identifiez-vous pour les ouvrir.")}</p>
      <button type="button" className="big" disabled={busy} onClick={() => void go()} autoFocus>
        {busy ? t("Vérification…") : t("Se connecter avec ma clé biométrique")}
      </button>
      <p className="muted">{t("Empreinte, visage ou Windows Hello : un seul geste suffit.")}</p>
      <ErrorBox error={error} />
      <button type="button" className="ghost" onClick={props.onOther}>
        {t("Utiliser une autre méthode")}
      </button>
    </section>
  );
}

function FullLogin(props: { onLoggedIn: () => void; stepUp: boolean; onQuick: (() => void) | undefined }) {
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
          t("Ce coffre est lié au matériel, mais cet appareil ou ce navigateur ne fournit pas l'extension PRF. Utilisez une clé liée ou un code de récupération."),
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
        props.onLoggedIn();
      })
      .catch((err: unknown) => {
        if (err instanceof ApiError && err.status === 400 && /aucune clé biométrique/.test(err.message)) {
          setMethod("totp");
          setError(t(NO_PASSKEY));
          return;
        }
        setBlocked(err instanceof ApiError && (err.status === 429 || /bloquée/.test(err.message)));
        setError(describe(err));
      })
      .finally(() => setBusy(false));
  }

  return (
    <form className="card" onSubmit={submit}>
      <h1>Phonalys</h1>
      <p className="muted">{t("Vos données sont chiffrées. Identifiez-vous pour les ouvrir.")}</p>
      {props.stepUp && <p className="warning">{t(STEP_UP)}</p>}
      <div
        className="tabs"
        role="tablist"
        aria-label={t("Second facteur")}
        onKeyDown={tabKeys(METHODS.map(([m]) => m), method, choose)}
      >
        {METHODS.map(([m, label]) => (
          <button
            key={m}
            type="button"
            role="tab"
            tabIndex={method === m ? 0 : -1}
            aria-selected={method === m}
            className={method === m ? "tab active" : "tab"}
            onClick={() => choose(m)}
          >
            {t(label)}
          </button>
        ))}
      </div>
      <PasswordField
        label={t("Phrase de passe")}
        value={passphrase}
        onChange={setPassphrase}
        autoComplete="current-password"
        required
        autoFocus
        hint={t("Celle choisie à l'installation, exactement (majuscules, accents et espaces comptent).")}
      />
      {method === "totp" && (
        <Field
          label={t("Code de l'application d'authentification")}
          value={code}
          onChange={setCode}
          autoComplete="one-time-code"
          inputMode="numeric"
          required
        />
      )}
      {method === "recovery" && (
        <Field
          label={t("Code de récupération")}
          value={code}
          onChange={setCode}
          required
          hint={t("Format XXXX-XXXX-XXXX-XXXX. Chaque code ne sert qu'une fois et débloque aussi une connexion bloquée.")}
        />
      )}
      {method === "passkey" && (
        <p className="muted">
          {t(
            "Après la phrase de passe, votre appareil demande l'empreinte, le visage ou le code Windows Hello. La clé sert à chaque connexion, sans limite.",
          )}
        </p>
      )}
      {method === "passkey" && !webauthnAvailable() && (
        <p className="warning">
          {t("Les clés biométriques exigent une connexion HTTPS avec un nom de domaine et un certificat reconnu.")}
        </p>
      )}
      <ErrorBox error={error} />
      {blocked && method !== "recovery" && (
        <button type="button" className="ghost" onClick={() => choose("recovery")}>
          {t("Débloquer avec un code de récupération")}
        </button>
      )}
      <button type="submit" disabled={busy}>
        {busy ? t("Vérification…") : method === "passkey" ? t("Continuer avec ma clé") : t("Déverrouiller")}
      </button>
      {props.onQuick && (
        <button type="button" className="ghost" onClick={props.onQuick}>
          {t("Revenir à la connexion en un geste")}
        </button>
      )}
    </form>
  );
}
