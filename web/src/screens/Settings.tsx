import { useEffect, useState, type FormEvent } from "react";
import { api, type Settings as SettingsData } from "../api";
import { ErrorBox, Field, Select, useAction, useLoad } from "../components";
import { getLang, LANGS, setLang, t, type Lang } from "../i18n";
import { AssistantSettings } from "./Assistant";

const IDLE = [
  ["5", "{n} minutes"],
  ["10", "{n} minutes"],
  ["15", "15 minutes (recommandé)"],
  ["30", "{n} minutes"],
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

const label = ([v, l]: readonly [string, string]) => [v, t(l, { n: v })] as const;

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

  if (!s) return <section className="card"><ErrorBox error={loaded.error} /><p className="muted">{t("Chargement…")}</p></section>;
  const pr = s.practitioner;
  return (
    <>
    <form onSubmit={save}>
      <section className="card">
        <h1>{t("Réglages")}</h1>
        <Select
          label={t("Langue de l'interface")}
          value={getLang()}
          onChange={(l: Lang) => setLang(l)}
          options={LANGS}
          hint={t("Propre à cet appareil. Les exercices et les lexiques restent en français.")}
        />
        <h2>{t("Verrouillage automatique")}</h2>
        <p className="muted">
          {t(
            "Sans activité (clavier, souris, écran tactile) pendant ce délai, Phonalys se verrouille et efface les clés de la mémoire. Un compte à rebours apparaît deux minutes avant.",
          )}
        </p>
        <Select
          label={t("Délai d'inactivité")}
          value={String(s.idle_timeout_minutes) as (typeof IDLE)[number][0]}
          onChange={(v) => { setSaved(false); setS({ ...s, idle_timeout_minutes: Number(v) }); }}
          options={IDLE.map(label)}
        />
        <Select
          label={t("Application en arrière-plan")}
          value={String(s.hidden_lock_minutes ?? 5) as (typeof HIDDEN)[number][0]}
          onChange={(v) => { setSaved(false); setS({ ...s, hidden_lock_minutes: Number(v) }); }}
          options={HIDDEN.map(label)}
          hint={t(
            "Fenêtre réduite, autre onglet, téléphone en veille : l'appareil se déconnecte. La mise en veille de l'ordinateur verrouille toujours tout.",
          )}
        />
      </section>
      <section className="card">
        <h2>{t("Cabinet")}</h2>
        <p className="muted">{t("Repris en en-tête des bilans imprimés.")}</p>
        <div className="grid2">
          {FIELDS.map(([k, label, hint]) => (
            <Field
              key={k}
              label={t(label)}
              value={pr[k] ?? ""}
              onChange={(v) => { setSaved(false); setS({ ...s, practitioner: { ...pr, [k]: v } }); }}
              {...(hint ? { hint: t(hint) } : {})}
            />
          ))}
        </div>
      </section>
      <section className="card">
        <ErrorBox error={error} />
        {saved && <p className="notice" role="status">{t("Réglages enregistrés.")}</p>}
        <button type="submit" disabled={busy}>{t("Enregistrer")}</button>
      </section>
    </form>
    <AssistantSettings />
    </>
  );
}
