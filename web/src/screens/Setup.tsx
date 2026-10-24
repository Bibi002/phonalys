import { useState, type FormEvent } from "react";
import { api, type SetupResult } from "../api";
import { ErrorBox, Field, RecoveryCodes, useAction } from "../components";

export function Setup({ onDone }: { onDone: () => void }) {
  const [token, setToken] = useState("");
  const [name, setName] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [result, setResult] = useState<SetupResult>();
  const [step, setStep] = useState<"totp" | "codes">("totp");
  const { busy, error, setError, run } = useAction();

  function submit(e: FormEvent) {
    e.preventDefault();
    if (passphrase !== confirm) {
      setError("Les deux phrases de passe sont différentes.");
      return;
    }
    void run(async () => {
      const body: Record<string, string> = { setup_token: token.trim(), passphrase };
      if (name.trim()) body.practitioner_name = name.trim();
      setResult(await api<SetupResult>("POST", "/api/setup", body));
      setPassphrase("");
      setConfirm("");
    });
  }

  if (result && step === "totp") {
    return (
      <main className="narrow">
        <section className="card">
          <h1>Second facteur</h1>
          <p>
            Scannez ce code avec une application d'authentification hors ligne (Aegis, 2FAS, Ente Auth)
            ou avec votre gestionnaire de mots de passe (Bitwarden, KeePassXC).
          </p>
          <img
            className="qr"
            alt="QR code du second facteur"
            src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(result.totp_qr_svg)}`}
          />
          <details>
            <summary>Saisie manuelle</summary>
            <code className="secret">{result.totp_secret}</code>
          </details>
          <p className="muted">Après la première connexion, vous pourrez ajouter une clé biométrique.</p>
          <button type="button" onClick={() => setStep("codes")}>
            C'est fait
          </button>
        </section>
      </main>
    );
  }
  if (result) return <main className="narrow"><RecoveryCodes codes={result.recovery_codes} onDone={onDone} /></main>;

  return (
    <main className="narrow">
      <form className="card" onSubmit={submit}>
        <h1>Installation de Phonalys</h1>
        <p>
          Vos données restent sur cet appareil, chiffrées. Personne d'autre, pas même l'éditeur, ne peut les
          lire.
        </p>
        <Field
          label="Jeton d'installation"
          value={token}
          onChange={setToken}
          required
          autoFocus
          hint="Affiché dans la console au démarrage du serveur."
        />
        <Field label="Votre nom (facultatif)" value={name} onChange={setName} autoComplete="name" />
        <Field
          label="Phrase de passe"
          type="password"
          value={passphrase}
          onChange={setPassphrase}
          autoComplete="new-password"
          required
          hint="Au moins 14 caractères ; idéalement 5 mots ou plus. Un gestionnaire de mots de passe (Bitwarden, KeePassXC) peut la générer et la retenir."
        />
        <Field
          label="Confirmation"
          type="password"
          value={confirm}
          onChange={setConfirm}
          autoComplete="new-password"
          required
        />
        <p className="warning">
          Sans cette phrase de passe, le coffre est définitivement illisible. Il n'existe aucune
          réinitialisation.
        </p>
        <ErrorBox error={error} />
        <button type="submit" disabled={busy}>
          {busy ? "Création du coffre…" : "Créer le coffre"}
        </button>
      </form>
    </main>
  );
}
