import { useState, type ReactNode } from "react";
import { ApiError } from "./api";

export function describeError(e: unknown): string {
  if (e instanceof ApiError) {
    const m = e.message;
    return m.charAt(0).toUpperCase() + m.slice(1) + (/[.!?]$/.test(m) ? "" : ".");
  }
  return e instanceof Error ? e.message : String(e);
}

/** Champ de phrase de passe avec bouton « Afficher » pour vérifier la saisie. */
export function PasswordField(props: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  autoComplete: "current-password" | "new-password";
  required?: boolean;
  autoFocus?: boolean;
  hint?: ReactNode;
}) {
  const [visible, setVisible] = useState(false);
  return (
    <label className="field">
      <span>{props.label}</span>
      <span className="with-button">
        <input
          type={visible ? "text" : "password"}
          aria-label={props.label}
          value={props.value}
          onChange={(e) => props.onChange(e.target.value)}
          autoComplete={props.autoComplete}
          required={props.required ?? false}
          autoFocus={props.autoFocus ?? false}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
        />
        <button
          type="button"
          className="ghost"
          aria-pressed={visible}
          onClick={() => setVisible(!visible)}
        >
          {visible ? "Masquer" : "Afficher"}
        </button>
      </span>
      {props.hint ? <small>{props.hint}</small> : null}
    </label>
  );
}

export function ErrorBox({ error }: { error: string | undefined }) {
  if (!error) return null;
  return (
    <p className="error" role="alert">
      {error}
    </p>
  );
}

export function Field(props: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  type?: string;
  autoComplete?: string;
  required?: boolean;
  hint?: ReactNode;
  inputMode?: "numeric" | "text";
  autoFocus?: boolean;
}) {
  return (
    <label className="field">
      <span>{props.label}</span>
      <input
        type={props.type ?? "text"}
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
        autoComplete={props.autoComplete ?? "off"}
        required={props.required ?? false}
        inputMode={props.inputMode}
        autoFocus={props.autoFocus ?? false}
        spellCheck={false}
      />
      {props.hint ? <small>{props.hint}</small> : null}
    </label>
  );
}

/** Exécute une action asynchrone en gérant l'état « en cours » et l'erreur. */
export function useAction() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  async function run(f: () => Promise<void>, describe: (e: unknown) => string = describeError) {
    setBusy(true);
    setError(undefined);
    try {
      await f();
    } catch (e) {
      setError(describe(e));
    } finally {
      setBusy(false);
    }
  }
  return { busy, error, setError, run };
}

/** Bouton à confirmation en deux temps (aucune boîte de dialogue du navigateur). */
export function ConfirmButton(props: { label: string; confirmLabel: string; onConfirm: () => void; disabled?: boolean }) {
  const [armed, setArmed] = useState(false);
  if (!armed)
    return (
      <button type="button" className="danger-outline" disabled={props.disabled ?? false} onClick={() => setArmed(true)}>
        {props.label}
      </button>
    );
  return (
    <span className="confirm">
      <button type="button" className="danger" onClick={props.onConfirm}>
        {props.confirmLabel}
      </button>
      <button type="button" className="ghost" onClick={() => setArmed(false)}>
        Annuler
      </button>
    </span>
  );
}

/** Affichage unique des codes de récupération. */
export function RecoveryCodes({ codes, onDone }: { codes: string[]; onDone: () => void }) {
  const [saved, setSaved] = useState(false);
  return (
    <section className="card warning-card">
      <h2>Codes de récupération</h2>
      <p>
        Ces codes ne seront <strong>plus jamais affichés</strong>. Chacun permet une seule connexion si vous
        perdez votre appareil. Imprimez-les ou notez-les et rangez-les hors ligne, à l'écart de votre
        ordinateur.
      </p>
      <ol className="codes">
        {codes.map((c) => (
          <li key={c}>
            <code>{c}</code>
          </li>
        ))}
      </ol>
      <label className="check">
        <input type="checkbox" checked={saved} onChange={(e) => setSaved(e.target.checked)} />
        J'ai mis ces codes en lieu sûr.
      </label>
      <button type="button" disabled={!saved} onClick={onDone}>
        Continuer
      </button>
    </section>
  );
}
