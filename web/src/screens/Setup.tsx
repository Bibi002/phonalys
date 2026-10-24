import { useState, type FormEvent } from "react";
import { api, ApiError, type SetupResult } from "../api";
import { ErrorBox, Field, PasswordField, RecoveryCodes, useAction } from "../components";

/**
 * Jeton d'installation transmis par l'application de bureau dans le fragment de l'URL
 * (`#setup=…`), qui n'est jamais envoyé sur le réseau. Il est effacé dès sa lecture.
 */
export function takeSetupToken(): string {
  const match = /^#setup=([A-Z2-7]+)$/.exec(window.location.hash);
  if (!match?.[1]) return "";
  history.replaceState(null, "", window.location.pathname + window.location.search);
  return match[1];
}

/** Envoie un fichier de sauvegarde au serveur (installation vierge uniquement). */
export async function uploadRestore(token: string, file: Blob): Promise<void> {
  const res = await fetch("/api/restore", {
    method: "POST",
    headers: { "x-phonalys-request": "1", "x-phonalys-setup-token": token.trim(), "content-type": "application/octet-stream" },
    body: file,
    credentials: "same-origin",
    cache: "no-store",
  });
  if (!res.ok) {
    let message = `erreur ${res.status}`;
    try {
      const data = (await res.json()) as { error?: unknown };
      if (typeof data.error === "string") message = data.error;
    } catch {
      // réponse sans JSON
    }
    throw new ApiError(res.status, message);
  }
}

function Restore({ token, onDone }: { token: string; onDone: () => void }) {
  const [file, setFile] = useState<File>();
  const { busy, error, run } = useAction();
  return (
    <div className="restore">
      <h2>Vous avez déjà une sauvegarde ?</h2>
      <p className="muted">
        Sur un nouvel ordinateur, restaurez votre fichier <code>.phonalys-backup</code> au lieu de créer un coffre vide.
        Vous vous connecterez ensuite avec la phrase de passe et le second facteur en vigueur lors de la sauvegarde.
      </p>
      <label className="field">
        <span>Fichier de sauvegarde</span>
        <input type="file" accept=".phonalys-backup" onChange={(e) => setFile(e.target.files?.[0])} />
      </label>
      <ErrorBox error={error} />
      <button
        type="button"
        className="ghost"
        disabled={busy || !file || !token.trim()}
        onClick={() =>
          void run(async () => {
            if (file) await uploadRestore(token, file);
            onDone();
          })
        }
      >
        {busy ? "Restauration…" : "Restaurer cette sauvegarde"}
      </button>
    </div>
  );
}

export function Setup({ onDone }: { onDone: () => void }) {
  const [token, setToken] = useState(takeSetupToken);
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
        <PasswordField
          label="Phrase de passe"
          value={passphrase}
          onChange={setPassphrase}
          autoComplete="new-password"
          required
          hint="Au moins 14 caractères ; idéalement 5 mots ou plus. Un gestionnaire de mots de passe (Bitwarden, KeePassXC) peut la générer et la retenir."
        />
        <PasswordField
          label="Confirmation"
          value={confirm}
          onChange={setConfirm}
          autoComplete="new-password"
          required
        />
        <p className="muted">
          Si votre navigateur propose de générer ou d'enregistrer un mot de passe, vérifiez avec « Afficher » que
          c'est bien celui que vous retiendrez.
        </p>
        <p className="warning">
          Sans cette phrase de passe, le coffre est définitivement illisible. Il n'existe aucune
          réinitialisation.
        </p>
        <ErrorBox error={error} />
        <button type="submit" disabled={busy}>
          {busy ? "Création du coffre…" : "Créer le coffre"}
        </button>
        <Restore token={token} onDone={onDone} />
      </form>
    </main>
  );
}
