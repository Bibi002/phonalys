import { useEffect, useState, type FormEvent } from "react";
import { api, type Settings as SettingsData } from "../api";
import { ErrorBox, Field, Select, useAction, useLoad } from "../components";
import { AssistantSettings } from "./Assistant";

const IDLE = [
  ["5", "5 minutes"],
  ["10", "10 minutes"],
  ["15", "15 minutes (recommandé)"],
  ["30", "30 minutes"],
  ["60", "1 heure"],
] as const;

const HIDDEN = [
  ["1", "après 1 minute"],
  ["5", "après 5 minutes (recommandé)"],
  ["15", "après 15 minutes"],
  ["0", "jamais (seul le délai d'inactivité s'applique)"],
] as const;

const FIELDS = [
  ["nom", "Nom du praticien ou du cabinet"],
  ["titre", "Titre", "Ex. Logopède"],
  ["inami", "Numéro INAMI"],
  ["adresse", "Adresse"],
  ["telephone", "Téléphone"],
  ["email", "E-mail"],
] as const;

export function Settings({ onIdleChanged, onHiddenLockChanged }: { onIdleChanged: (secs: number) => void; onHiddenLockChanged?: (secs: number) => void }) {
  const loaded = useLoad(() => api<SettingsData>("GET", "/api/settings"));
  const [s, setS] = useState<SettingsData>();
  const [saved, setSaved] = useState(false);
  const { busy, error, run } = useAction();

  useEffect(() => {
    if (loaded.data) setS(loaded.data);
  }, [loaded.data]);

  function save(e: FormEvent) {
    e.preventDefault();
    if (!s) return;
    void run(async () => {
      const r = await api<SettingsData>("PUT", "/api/settings", s);
      setS(r);
      onIdleChanged(r.idle_timeout_minutes * 60);
      onHiddenLockChanged?.((r.hidden_lock_minutes ?? 5) * 60);
      setSaved(true);
    });
  }

  if (!s) return <section className="card"><ErrorBox error={loaded.error} /><p className="muted">Chargement…</p></section>;
  const pr = s.practitioner;
  return (
    <>
    <form onSubmit={save}>
      <section className="card">
        <h1>Réglages</h1>
        <h2>Verrouillage automatique</h2>
        <p className="muted">
          Sans activité (clavier, souris, écran tactile) pendant ce délai, Phonalys se verrouille et efface les clés de
          la mémoire. Un compte à rebours apparaît deux minutes avant.
        </p>
        <Select
          label="Délai d'inactivité"
          value={String(s.idle_timeout_minutes) as (typeof IDLE)[number][0]}
          onChange={(v) => { setSaved(false); setS({ ...s, idle_timeout_minutes: Number(v) }); }}
          options={IDLE}
        />
        <Select
          label="Application en arrière-plan"
          value={String(s.hidden_lock_minutes ?? 5) as (typeof HIDDEN)[number][0]}
          onChange={(v) => { setSaved(false); setS({ ...s, hidden_lock_minutes: Number(v) }); }}
          options={HIDDEN}
          hint="Fenêtre réduite, autre onglet, téléphone en veille : l'appareil se déconnecte. La mise en veille de l'ordinateur verrouille toujours tout."
        />
      </section>
      <section className="card">
        <h2>Cabinet</h2>
        <p className="muted">Repris en en-tête des bilans imprimés.</p>
        <div className="grid2">
          {FIELDS.map(([k, label, hint]) => (
            <Field
              key={k}
              label={label}
              value={pr[k] ?? ""}
              onChange={(v) => { setSaved(false); setS({ ...s, practitioner: { ...pr, [k]: v } }); }}
              {...(hint ? { hint } : {})}
            />
          ))}
        </div>
      </section>
      <section className="card">
        <ErrorBox error={error} />
        {saved && <p className="notice" role="status">Réglages enregistrés.</p>}
        <button type="submit" disabled={busy}>Enregistrer</button>
      </section>
    </form>
    <AssistantSettings />
    </>
  );
}
