import { useCallback, useState } from "react";
import { records } from "../api";
import { ErrorBox, Select, useAction } from "../components";
import { EXERCISES, type ExerciseDef } from "../exercises";
import type { Outcome } from "../exercises/common";
import { usePatients } from "../hooks";
import { fullName, today, type ExerciceResultat } from "../model";

type Stage = { def: ExerciseDef; niveau: number; startedAt: number; outcome?: Outcome; run: number };

export function Exercises({ patientId }: { patientId: string | undefined }) {
  const patients = usePatients();
  const [patient, setPatient] = useState(patientId ?? "");
  const [selected, setSelected] = useState<ExerciseDef>();
  const [niveau, setNiveau] = useState("1");
  const [stage, setStage] = useState<Stage>();
  const [saved, setSaved] = useState(false);
  const { busy, error, run } = useAction();

  const finish = useCallback((outcome: Outcome) => setStage((s) => (s && !s.outcome ? { ...s, outcome } : s)), []);

  function start(def: ExerciseDef, n: number) {
    setSaved(false);
    setStage({ def, niveau: n, startedAt: Date.now(), run: (stage?.run ?? 0) + 1 });
  }

  function save() {
    if (!stage?.outcome || !patient) return;
    const { outcome, def, niveau: n, startedAt } = stage;
    void run(async () => {
      await records.save<ExerciceResultat>("exercice", {
        patient_id: patient,
        exercice: def.id,
        date: today(),
        niveau: n,
        score: outcome.score,
        total: outcome.total,
        duree_s: Math.round((Date.now() - startedAt) / 1000),
        details: outcome.details ?? "",
      });
      setSaved(true);
    });
  }

  if (stage) {
    const { def, outcome } = stage;
    return (
      <section className="card stage">
        <div className="toolbar">
          <h1>{def.titre}</h1>
          <button type="button" className="ghost" onClick={() => setStage(undefined)}>
            {outcome ? "Retour aux exercices" : "Arrêter"}
          </button>
        </div>
        {outcome ? (
          <div className="quiz">
            <p className="big-word">{def.empan ? `Empan : ${outcome.score}` : `${outcome.score} / ${outcome.total}`}</p>
            {outcome.details && <p className="muted">{outcome.details}</p>}
            <ErrorBox error={error} />
            {saved && <p className="notice" role="status">Résultat enregistré dans le dossier.</p>}
            <div className="actions">
              {patient ? (
                <button type="button" disabled={busy || saved} onClick={save}>
                  Enregistrer pour {fullName(patients.byId.get(patient))}
                </button>
              ) : (
                <p className="muted">Choisissez un patient avant l'exercice pour enregistrer le résultat.</p>
              )}
              <button type="button" className="ghost" onClick={() => start(def, stage.niveau)}>
                Recommencer
              </button>
            </div>
          </div>
        ) : (
          <def.Component key={stage.run} niveau={stage.niveau} onFinish={finish} />
        )}
      </section>
    );
  }

  const domaines = [...new Set(EXERCISES.map((e) => e.domaine))];
  return (
    <>
      <section className="card">
        <h1>Exercices</h1>
        <p className="muted">
          Exercices d'entraînement originaux Phonalys, à faire en séance (écran partagé, tablette). Ils ne sont pas
          étalonnés : ils servent au suivi de la progression, pas au diagnostic.
        </p>
        <Select
          label="Patient (pour enregistrer les résultats)"
          value={patient}
          onChange={setPatient}
          options={[["", "— Aucun —"] as const, ...(patients.data ?? []).map((p) => [p.id, fullName(p)] as const)]}
        />
      </section>
      {domaines.map((d) => (
        <section key={d} className="card">
          <h2>{d}</h2>
          <div className="exercise-grid">
            {EXERCISES.filter((e) => e.domaine === d).map((e) => (
              <div key={e.id} className={selected?.id === e.id ? "exercise selected" : "exercise"}>
                <h3>{e.titre}</h3>
                <p className="muted small">{e.description}</p>
                {selected?.id === e.id ? (
                  <div className="inline-form">
                    <Select
                      label="Niveau"
                      value={niveau}
                      onChange={setNiveau}
                      options={e.niveaux.map((l, i) => [String(i + 1), l] as const)}
                    />
                    <button type="button" onClick={() => start(e, Number(niveau))}>
                      Commencer
                    </button>
                  </div>
                ) : (
                  <button type="button" className="ghost" onClick={() => { setSelected(e); setNiveau("1"); }}>
                    Choisir
                  </button>
                )}
              </div>
            ))}
          </div>
        </section>
      ))}
    </>
  );
}
