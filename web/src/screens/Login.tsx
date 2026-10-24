import { useState, type FormEvent } from "react";
import { api } from "../api";
import { describeError, ErrorBox, Field, useAction } from "../components";
import { describeWebAuthnError, getAssertion, prfSalt, webauthnAvailable } from "../webauthn";

type Method = "passkey" | "totp" | "recovery";

export function Login({ onLoggedIn }: { onLoggedIn: () => void }) {
  const [method, setMethod] = useState<Method>(webauthnAvailable() ? "passkey" : "totp");
  const [passphrase, setPassphrase] = useState("");
  const [code, setCode] = useState("");
  const { busy, error, run } = useAction();

  function done() {
    setPassphrase("");
    setCode("");
    onLoggedIn();
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    if (method === "passkey") {
      void run(
        async () => {
          const options = await api<Record<string, unknown>>("POST", "/api/login/passkey/start", { passphrase });
          const { response, prfOutput } = await getAssertion(options);
          if (prfSalt(options) && !prfOutput) {
            throw new Error(
              "Ce coffre est lié au matériel, mais cet appareil ou ce navigateur ne fournit pas l'extension PRF. Utilisez une clé liée ou un code de récupération.",
            );
          }
          await api("POST", "/api/login/passkey/finish", prfOutput ? { response, prf_output: prfOutput } : { response });
          done();
        },
        (e) => (e instanceof DOMException ? describeWebAuthnError(e) : describeError(e)),
      );
    } else {
      const body =
        method === "totp" ? { passphrase, code: code.replace(/\s/g, "") } : { passphrase, recovery_code: code };
      void run(async () => {
        await api("POST", "/api/login", body);
        done();
      });
    }
  }

  return (
    <main className="narrow">
      <form className="card" onSubmit={submit}>
        <h1>Phonalys</h1>
        <p className="muted">Coffre verrouillé.</p>
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
              onClick={() => {
                setMethod(m);
                setCode("");
              }}
            >
              {label}
            </button>
          ))}
        </div>
        <Field
          label="Phrase de passe"
          type="password"
          value={passphrase}
          onChange={setPassphrase}
          autoComplete="current-password"
          required
          autoFocus
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
            hint="Format XXXX-XXXX-XXXX-XXXX. Chaque code ne sert qu'une fois."
          />
        )}
        {method === "passkey" && !webauthnAvailable() && (
          <p className="warning">
            Les clés biométriques exigent une connexion HTTPS avec un nom de domaine et un certificat reconnu.
          </p>
        )}
        <ErrorBox error={error} />
        <button type="submit" disabled={busy}>
          {busy ? "Vérification…" : method === "passkey" ? "Continuer avec ma clé" : "Déverrouiller"}
        </button>
      </form>
    </main>
  );
}
