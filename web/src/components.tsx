import { useCallback, useEffect, useId, useState, type KeyboardEvent, type ReactNode } from "react";
import { ApiError } from "./api";
import { t } from "./i18n";

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
  const hintId = useId();
  const labelId = useId();
  return (
    <label className="field">
      <span id={labelId}>{props.label}</span>
      <span className="with-button">
        <input
          type={visible ? "text" : "password"}
          aria-labelledby={labelId}
        aria-describedby={props.hint ? hintId : undefined}
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
          {visible ? t("Masquer") : t("Afficher")}
        </button>
      </span>
      {props.hint ? <small id={hintId}>{props.hint}</small> : null}
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
  const hintId = useId();
  const labelId = useId();
  return (
    <label className="field">
      <span id={labelId}>{props.label}</span>
      <input
        type={props.type ?? "text"}
        aria-labelledby={labelId}
        aria-describedby={props.hint ? hintId : undefined}
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
        autoComplete={props.autoComplete ?? "off"}
        required={props.required ?? false}
        inputMode={props.inputMode}
        autoFocus={props.autoFocus ?? false}
        spellCheck={false}
      />
      {props.hint ? <small id={hintId}>{props.hint}</small> : null}
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
        {t("Annuler")}
      </button>
    </span>
  );
}

/** Affichage unique des codes de récupération. */
export function RecoveryCodes({ codes, onDone }: { codes: string[]; onDone: () => void }) {
  const [saved, setSaved] = useState(false);
  return (
    <section className="card warning-card">
      <h2>{t("Codes de récupération")}</h2>
      <p>
        <strong>{t("Ces codes ne seront plus jamais affichés.")}</strong>{" "}
        {t(
          "Chacun permet une seule connexion si vous perdez votre appareil. Imprimez-les ou notez-les et rangez-les hors ligne, à l'écart de votre ordinateur.",
        )}
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
        {t("J'ai mis ces codes en lieu sûr.")}
      </label>
      <button type="button" disabled={!saved} onClick={onDone}>
        {t("Continuer")}
      </button>
    </section>
  );
}

export function TextArea(props: { label: string; value: string; onChange: (v: string) => void; rows?: number; hint?: ReactNode }) {
  const hintId = useId();
  const labelId = useId();
  return (
    <label className="field">
      <span id={labelId}>{props.label}</span>
      <textarea
        rows={props.rows ?? 4}
        value={props.value}
        aria-labelledby={labelId}
        aria-describedby={props.hint ? hintId : undefined}
        onChange={(e) => props.onChange(e.target.value)}
      />
      {props.hint ? <small id={hintId}>{props.hint}</small> : null}
    </label>
  );
}

export function Select<T extends string>(props: {
  label: string;
  value: T;
  onChange: (v: T) => void;
  options: readonly (readonly [T, string])[];
  hint?: ReactNode;
}) {
  const hintId = useId();
  const labelId = useId();
  return (
    <label className="field">
      <span id={labelId}>{props.label}</span>
      <select value={props.value} aria-labelledby={labelId}
        aria-describedby={props.hint ? hintId : undefined} onChange={(e) => props.onChange(e.target.value as T)}>
        {props.options.map(([v, l]) => (
          <option key={v} value={v}>
            {l}
          </option>
        ))}
      </select>
      {props.hint ? <small id={hintId}>{props.hint}</small> : null}
    </label>
  );
}

/**
 * Navigation au clavier d'une liste d'onglets (motif ARIA) : flèches gauche et droite,
 * Début et Fin ; un seul onglet est atteignable par Tab.
 */
export function tabKeys<T>(keys: readonly T[], value: T, onChange: (v: T) => void) {
  return (e: KeyboardEvent<HTMLElement>) => {
    const i = keys.indexOf(value);
    const next =
      e.key === "ArrowRight" ? (i + 1) % keys.length
      : e.key === "ArrowLeft" ? (i - 1 + keys.length) % keys.length
      : e.key === "Home" ? 0
      : e.key === "End" ? keys.length - 1
      : -1;
    if (next < 0) return;
    e.preventDefault();
    onChange(keys[next] as T);
    const buttons = e.currentTarget.querySelectorAll<HTMLElement>('[role="tab"]');
    buttons[next]?.focus();
  };
}

/** Onglets accessibles (rôle tablist). */
export function Tabs<T extends string>(props: {
  label: string;
  value: T;
  onChange: (v: T) => void;
  tabs: readonly (readonly [T, string])[];
}) {
  const keys = props.tabs.map(([v]) => v);
  return (
    <div className="tabs" role="tablist" aria-label={props.label} onKeyDown={tabKeys(keys, props.value, props.onChange)}>
      {props.tabs.map(([v, l]) => (
        <button
          key={v}
          type="button"
          role="tab"
          tabIndex={props.value === v ? 0 : -1}
          aria-selected={props.value === v}
          className={props.value === v ? "tab active" : "tab"}
          onClick={() => props.onChange(v)}
        >
          {l}
        </button>
      ))}
    </div>
  );
}

/** Charge une donnée au montage et à la demande (`reload`). */
export function useLoad<T>(load: () => Promise<T>, deps: readonly unknown[] = []) {
  const [data, setData] = useState<T>();
  const [error, setError] = useState<string>();
  // Les dépendances sont fournies par l'appelant.
  const run = useCallback(load, deps);
  const reload = useCallback(() => {
    setError(undefined);
    run()
      .then(setData)
      .catch((e: unknown) => setError(describeError(e)));
  }, [run]);
  useEffect(reload, [reload]);
  return { data, error, reload, setData };
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="muted empty">{children}</p>;
}