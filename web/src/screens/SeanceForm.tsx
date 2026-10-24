import { useState, type FormEvent } from "react";
import { records } from "../api";
import { ConfirmButton, ErrorBox, Field, Select, TextArea, useAction } from "../components";
import { fullName, SEANCE_STATUT, today, type Patient, type Seance, type SeanceStatut } from "../model";

export function newSeance(patientId: string, date = today()): Seance {
  return { patient_id: patientId, date, heure: "09:00", duree: 30, lieu: "Cabinet", statut: "prevue" };
}

const STATUTS = Object.entries(SEANCE_STATUT) as [SeanceStatut, string][];

/** Création ou modification d'une séance (agenda et dossier patient). */
export function SeanceForm(props: {
  value: Seance;
  patients?: Patient[];
  onDone: () => void;
  onCancel: () => void;
}) {
  const [s, setS] = useState<Seance>(props.value);
  const { busy, error, run } = useAction();
  const set = <K extends keyof Seance>(k: K) => (v: Seance[K]) => setS({ ...s, [k]: v });

  function save(e: FormEvent) {
    e.preventDefault();
    if (!s.patient_id) return;
    void run(async () => {
      await records.save("seance", s);
      props.onDone();
    });
  }

  return (
    <form className="card" onSubmit={save}>
      <h2>{s.id ? "Séance" : "Nouvelle séance"}</h2>
      {props.patients && (
        <Select
          label="Patient"
          value={s.patient_id}
          onChange={set("patient_id")}
          options={[["", "— Choisir —"] as const, ...props.patients.map((p) => [p.id, fullName(p)] as const)]}
        />
      )}
      <div className="grid3">
        <Field label="Date" type="date" value={s.date} onChange={set("date")} required />
        <Field label="Heure" type="time" value={s.heure ?? ""} onChange={set("heure")} />
        <Field
          label="Durée (min)"
          type="number"
          value={String(s.duree ?? "")}
          onChange={(v) => setS({ ...s, duree: Number(v) || 0 })}
        />
        <Field label="Lieu" value={s.lieu ?? ""} onChange={set("lieu")} hint="Cabinet, domicile, école…" />
        <Select label="Statut" value={s.statut} onChange={set("statut")} options={STATUTS} />
      </div>
      <TextArea label="Contenu de la séance" value={s.contenu ?? ""} onChange={set("contenu")} rows={4} hint="Objectifs travaillés, activités, matériel." />
      <TextArea label="Observations" value={s.observations ?? ""} onChange={set("observations")} rows={3} />
      <TextArea label="Pour la prochaine fois" value={s.suite ?? ""} onChange={set("suite")} rows={2} />
      <ErrorBox error={error} />
      <div className="actions">
        <button type="submit" disabled={busy || !s.patient_id}>
          Enregistrer
        </button>
        <button type="button" className="ghost" onClick={props.onCancel}>
          Annuler
        </button>
        {s.id && (
          <ConfirmButton
            label="Supprimer"
            confirmLabel="Supprimer la séance"
            onConfirm={() =>
              void run(async () => {
                await records.remove("seance", s.id as string);
                props.onDone();
              })
            }
          />
        )}
      </div>
    </form>
  );
}
