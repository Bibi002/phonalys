import { useEffect, useMemo, useState, type FormEvent } from "react";
import { api, records } from "../api";
import type { Navigate } from "../App";
import { ConfirmButton, Empty, ErrorBox, Field, Select, Tabs, TextArea, useAction } from "../components";
import { exerciseTitle } from "../exercises";
import { usePatients, useRecords } from "../hooks";
import {
  age,
  BILAN_TYPE,
  DOMAINES,
  formatDate,
  fullName,
  OBJECTIF_STATUT,
  PATIENT_STATUT,
  SEANCE_STATUT,
  today,
  type Bilan,
  type Contact,
  type ExerciceResultat,
  type Objectif,
  type ObjectifStatut,
  type Patient,
  type PatientStatut,
  type Seance,
} from "../model";
import { BilanCompare, BilanEditor, BilanView, newBilan } from "./Bilans";
import { newSeance, SeanceForm } from "./SeanceForm";

type Draft = Omit<Patient, "id">;
const STATUTS = Object.entries(PATIENT_STATUT) as [PatientStatut, string][];

export function Patients({ navigate, openId }: { navigate: Navigate; openId: string | undefined }) {
  const patients = usePatients();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<PatientStatut | "tous">("actif");
  const [open, setOpen] = useState<string | undefined>(openId);
  const [creating, setCreating] = useState(false);

  useEffect(() => setOpen(openId), [openId]);

  const shown = useMemo(() => {
    const q = query.trim().toLocaleLowerCase("fr");
    return (patients.data ?? []).filter(
      (p) =>
        (filter === "tous" || (p.statut ?? "actif") === filter) &&
        (!q || fullName(p).toLocaleLowerCase("fr").includes(q)),
    );
  }, [patients.data, query, filter]);

  if (creating)
    return (
      <NewPatient
        onCreated={(id) => {
          setCreating(false);
          patients.reload();
          setOpen(id);
        }}
        onCancel={() => setCreating(false)}
      />
    );

  const current = open ? patients.byId.get(open) : undefined;
  if (open && current)
    return (
      <PatientFile
        patient={current}
        navigate={navigate}
        onClose={() => setOpen(undefined)}
        onChanged={patients.reload}
        onErased={() => {
          setOpen(undefined);
          patients.reload();
        }}
      />
    );

  return (
    <section className="card">
      <div className="toolbar">
        <h1>Patients</h1>
        <button type="button" onClick={() => setCreating(true)}>
          Nouveau patient
        </button>
      </div>
      <div className="grid2">
        <Field label="Rechercher" value={query} onChange={setQuery} />
        <Select
          label="Afficher"
          value={filter}
          onChange={setFilter}
          options={[...STATUTS, ["tous", "Tous les dossiers"] as const]}
        />
      </div>
      <ErrorBox error={patients.error} />
      {patients.data === undefined ? (
        <p className="muted">Chargement…</p>
      ) : shown.length === 0 ? (
        <Empty>{patients.data.length === 0 ? "Aucun dossier pour l'instant." : "Aucun résultat."}</Empty>
      ) : (
        <ul className="list">
          {shown.map((p) => (
            <li key={p.id}>
              <button type="button" className="row" onClick={() => setOpen(p.id)}>
                <span>
                  <strong>{fullName(p)}</strong>
                  {p.classe && <span className="muted"> · {p.classe}</span>}
                </span>
                <span className="muted">{age(p.naissance)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function NewPatient({ onCreated, onCancel }: { onCreated: (id: string) => void; onCancel: () => void }) {
  const [d, setD] = useState<Draft>({ nom: "", prenom: "", naissance: "", statut: "actif" });
  const { busy, error, run } = useAction();
  function save(e: FormEvent) {
    e.preventDefault();
    void run(async () => {
      const { id } = await api<{ id: string }>("POST", "/api/patients", d);
      onCreated(id);
    });
  }
  return (
    <form className="card" onSubmit={save}>
      <h1>Nouveau patient</h1>
      <div className="grid3">
        <Field label="Nom" value={d.nom ?? ""} onChange={(v) => setD({ ...d, nom: v })} required autoFocus />
        <Field label="Prénom" value={d.prenom ?? ""} onChange={(v) => setD({ ...d, prenom: v })} />
        <Field label="Date de naissance" type="date" value={d.naissance ?? ""} onChange={(v) => setD({ ...d, naissance: v })} />
      </div>
      <ErrorBox error={error} />
      <div className="actions">
        <button type="submit" disabled={busy}>
          Créer le dossier
        </button>
        <button type="button" className="ghost" onClick={onCancel}>
          Annuler
        </button>
      </div>
    </form>
  );
}

type FileTab = "identite" | "anamnese" | "pec" | "seances" | "bilans" | "objectifs" | "exercices";
const FILE_TABS: readonly (readonly [FileTab, string])[] = [
  ["identite", "Identité"],
  ["anamnese", "Anamnèse"],
  ["pec", "Prise en charge"],
  ["seances", "Séances"],
  ["bilans", "Bilans"],
  ["objectifs", "Objectifs"],
  ["exercices", "Exercices"],
];

function PatientFile(props: {
  patient: Patient;
  navigate: Navigate;
  onClose: () => void;
  onChanged: () => void;
  onErased: () => void;
}) {
  const { patient } = props;
  const [tab, setTab] = useState<FileTab>("identite");
  return (
    <>
      <section className="card">
        <div className="toolbar">
          <div>
            <h1>{fullName(patient)}</h1>
            <p className="muted">
              {[age(patient.naissance), patient.classe, PATIENT_STATUT[patient.statut ?? "actif"]].filter(Boolean).join(" · ")}
            </p>
          </div>
          <button type="button" className="ghost" onClick={props.onClose}>
            Tous les patients
          </button>
        </div>
        <Tabs label="Dossier" value={tab} onChange={setTab} tabs={FILE_TABS} />
      </section>
      {(tab === "identite" || tab === "anamnese" || tab === "pec") && (
        <PatientForm key={patient.id + tab} patient={patient} tab={tab} onSaved={props.onChanged} onErased={props.onErased} />
      )}
      {tab === "seances" && <PatientSeances patient={patient} />}
      {tab === "bilans" && <PatientBilans patient={patient} />}
      {tab === "objectifs" && <PatientObjectifs patient={patient} />}
      {tab === "exercices" && <PatientExercices patient={patient} navigate={props.navigate} />}
    </>
  );
}

function PatientForm(props: { patient: Patient; tab: "identite" | "anamnese" | "pec"; onSaved: () => void; onErased: () => void }) {
  const { id, ...initial } = props.patient;
  const [d, setD] = useState<Draft>(initial);
  const [saved, setSaved] = useState(false);
  const { busy, error, run } = useAction();
  const seances = useRecords<Seance>("seance", id);
  const set = (k: keyof Draft) => (v: string) => {
    setSaved(false);
    setD({ ...d, [k]: v });
  };
  const pec = d.prise_en_charge ?? {};
  const setPec = (k: keyof typeof pec) => (v: string) => {
    setSaved(false);
    setD({ ...d, prise_en_charge: { ...pec, [k]: v } });
  };
  const contacts = d.contacts ?? [];
  const setContact = (i: number, patch: Partial<Contact>) => {
    setSaved(false);
    setD({ ...d, contacts: contacts.map((c, k) => (k === i ? { ...c, ...patch } : c)) });
  };

  function save(e: FormEvent) {
    e.preventDefault();
    void run(async () => {
      await api("PUT", `/api/patients/${id}`, d);
      setSaved(true);
      props.onSaved();
    });
  }

  const realisees = (seances.data ?? []).filter((s) => s.statut === "realisee");
  const sinceAccord = pec.date_accord ? realisees.filter((s) => s.date >= (pec.date_accord as string)).length : realisees.length;
  const accordees = Number(pec.seances_accordees) || 0;

  return (
    <form className="card" onSubmit={save}>
      {props.tab === "identite" && (
        <>
          <h2>Identité</h2>
          <div className="grid3">
            <Field label="Nom" value={d.nom ?? ""} onChange={set("nom")} required />
            <Field label="Prénom" value={d.prenom ?? ""} onChange={set("prenom")} />
            <Field label="Date de naissance" type="date" value={d.naissance ?? ""} onChange={set("naissance")} />
            <Field label="Sexe" value={d.sexe ?? ""} onChange={set("sexe")} />
            <Field label="Langue(s) parlée(s)" value={d.langues ?? ""} onChange={set("langues")} hint="Utile pour un enfant bilingue." />
            <Select label="Statut du dossier" value={d.statut ?? "actif"} onChange={set("statut")} options={STATUTS} />
            <Field label="École" value={d.ecole ?? ""} onChange={set("ecole")} />
            <Field label="Classe" value={d.classe ?? ""} onChange={set("classe")} hint="Ex. 3e primaire" />
            <Field label="Médecin traitant" value={d.medecin ?? ""} onChange={set("medecin")} />
          </div>
          <h3>Contacts</h3>
          {contacts.map((c, i) => (
            <div key={i} className="contact">
              <div className="grid4">
                <Field label="Lien" value={c.lien} onChange={(v) => setContact(i, { lien: v })} hint="Mère, père, tuteur…" />
                <Field label="Nom" value={c.nom} onChange={(v) => setContact(i, { nom: v })} />
                <Field label="Téléphone" type="tel" value={c.telephone} onChange={(v) => setContact(i, { telephone: v })} />
                <Field label="E-mail" type="email" value={c.email} onChange={(v) => setContact(i, { email: v })} />
              </div>
              <button type="button" className="ghost" onClick={() => setD({ ...d, contacts: contacts.filter((_, k) => k !== i) })}>
                Retirer ce contact
              </button>
            </div>
          ))}
          <button
            type="button"
            className="ghost"
            onClick={() => setD({ ...d, contacts: [...contacts, { lien: "", nom: "", telephone: "", email: "" }] })}
          >
            Ajouter un contact
          </button>
        </>
      )}
      {props.tab === "anamnese" && (
        <>
          <h2>Anamnèse</h2>
          <TextArea label="Motif de consultation" value={d.motif ?? ""} onChange={set("motif")} rows={3} />
          <TextArea
            label="Anamnèse"
            value={d.anamnese ?? ""}
            onChange={set("anamnese")}
            rows={12}
            hint="Grossesse et naissance, développement moteur et langagier, santé (audition, vision, ORL), scolarité, contexte familial et linguistique, suivis antérieurs."
          />
          <TextArea label="Notes" value={d.notes ?? ""} onChange={set("notes")} rows={5} />
        </>
      )}
      {props.tab === "pec" && (
        <>
          <h2>Prise en charge</h2>
          <div className="grid3">
            <Field label="Médecin prescripteur" value={pec.prescripteur ?? ""} onChange={setPec("prescripteur")} />
            <Field label="Date de la prescription" type="date" value={pec.date_prescription ?? ""} onChange={setPec("date_prescription")} />
            <Field label="Mutualité" value={pec.mutuelle ?? ""} onChange={setPec("mutuelle")} />
            <Field label="Date de l'accord du médecin-conseil" type="date" value={pec.date_accord ?? ""} onChange={setPec("date_accord")} />
            <Field label="Séances accordées" type="number" value={pec.seances_accordees ?? ""} onChange={setPec("seances_accordees")} />
          </div>
          {accordees > 0 && (
            <p>
              <meter min={0} max={accordees} value={Math.min(sinceAccord, accordees)} high={accordees * 0.85} optimum={0} />{" "}
              {sinceAccord} séance(s) réalisée(s) sur {accordees} accordée(s)
              {sinceAccord >= accordees - 2 && <strong className="warn-text"> · pensez à la demande de prolongation</strong>}
            </p>
          )}
          <TextArea label="Remarques" value={pec.remarques ?? ""} onChange={setPec("remarques")} rows={3} hint="Nomenclature, numéro d'accord, échéances…" />
        </>
      )}
      <ErrorBox error={error} />
      {saved && <p className="notice" role="status">Dossier enregistré.</p>}
      <div className="actions">
        <button type="submit" disabled={busy}>
          Enregistrer
        </button>
        {props.tab === "identite" && (
          <ConfirmButton
            label="Effacer le dossier"
            confirmLabel="Effacer définitivement le dossier, ses séances, bilans et résultats"
            onConfirm={() =>
              void run(async () => {
                await api("DELETE", `/api/patients/${id}`);
                props.onErased();
              })
            }
          />
        )}
      </div>
    </form>
  );
}

function PatientSeances({ patient }: { patient: Patient }) {
  const seances = useRecords<Seance>("seance", patient.id);
  const [editing, setEditing] = useState<Seance>();
  const sorted = [...(seances.data ?? [])].sort((a, b) => `${b.date}${b.heure ?? ""}`.localeCompare(`${a.date}${a.heure ?? ""}`));
  if (editing)
    return (
      <SeanceForm
        value={editing}
        onDone={() => {
          setEditing(undefined);
          seances.reload();
        }}
        onCancel={() => setEditing(undefined)}
      />
    );
  return (
    <section className="card">
      <div className="toolbar">
        <h2>Séances</h2>
        <button type="button" onClick={() => setEditing(newSeance(patient.id))}>
          Nouvelle séance
        </button>
      </div>
      <ErrorBox error={seances.error} />
      {sorted.length === 0 ? (
        <Empty>Aucune séance enregistrée.</Empty>
      ) : (
        <ul className="list">
          {sorted.map((s) => (
            <li key={s.id}>
              <button type="button" className="row" onClick={() => setEditing(s)}>
                <span>
                  <strong>{formatDate(s.date)}</strong> {s.heure} · {SEANCE_STATUT[s.statut]}
                  {s.contenu && <span className="muted"> · {s.contenu.slice(0, 80)}</span>}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function PatientBilans({ patient }: { patient: Patient }) {
  const bilans = useRecords<Bilan>("bilan", patient.id);
  const [editing, setEditing] = useState<Bilan>();
  const [viewing, setViewing] = useState<Bilan>();
  const sorted = [...(bilans.data ?? [])].sort((a, b) => b.date.localeCompare(a.date));
  if (viewing)
    return <BilanView bilan={viewing} patient={patient} onClose={() => setViewing(undefined)} onEdit={() => { setEditing(viewing); setViewing(undefined); }} />;
  if (editing)
    return (
      <BilanEditor
        value={editing}
        patient={patient}
        previous={sorted.filter((b) => b.id !== editing.id)}
        onDone={() => {
          setEditing(undefined);
          bilans.reload();
        }}
        onCancel={() => setEditing(undefined)}
      />
    );
  return (
    <>
      <section className="card">
        <div className="toolbar">
          <h2>Bilans</h2>
          <button type="button" onClick={() => setEditing(newBilan(patient.id))}>
            Nouveau bilan
          </button>
        </div>
        <ErrorBox error={bilans.error} />
        {sorted.length === 0 ? (
          <Empty>Aucun bilan pour ce patient.</Empty>
        ) : (
          <ul className="list">
            {sorted.map((b) => (
              <li key={b.id}>
                <button type="button" className="row" onClick={() => setViewing(b)}>
                  <span>
                    <strong>{BILAN_TYPE[b.type]}</strong> · {b.resultats.length} épreuve(s)
                  </span>
                  <span className="muted">{formatDate(b.date)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
      <BilanCompare bilans={sorted} />
    </>
  );
}

function PatientObjectifs({ patient }: { patient: Patient }) {
  const objectifs = useRecords<Objectif>("objectif", patient.id);
  const [editing, setEditing] = useState<Objectif>();
  const { busy, error, run } = useAction();

  function save(e: FormEvent) {
    e.preventDefault();
    if (!editing) return;
    void run(async () => {
      await records.save("objectif", editing);
      setEditing(undefined);
      objectifs.reload();
    });
  }

  if (editing) {
    const set = <K extends keyof Objectif>(k: K) => (v: Objectif[K]) => setEditing({ ...editing, [k]: v });
    return (
      <form className="card" onSubmit={save}>
        <h2>{editing.id ? "Objectif" : "Nouvel objectif"}</h2>
        <Select label="Domaine" value={editing.domaine} onChange={set("domaine")} options={DOMAINES.map((d) => [d, d] as const)} />
        <Field label="Objectif" value={editing.intitule} onChange={set("intitule")} required hint="Formulé de façon observable, ex. « segmente des mots de 3 syllabes »." />
        <Field label="Critère de réussite" value={editing.critere ?? ""} onChange={set("critere")} hint="Ex. 8 réussites sur 10, sur deux séances consécutives." />
        <div className="grid3">
          <Field label="Début" type="date" value={editing.debut ?? ""} onChange={set("debut")} />
          <Field label="Échéance" type="date" value={editing.echeance ?? ""} onChange={set("echeance")} />
          <Select label="Statut" value={editing.statut} onChange={set("statut")} options={Object.entries(OBJECTIF_STATUT) as [ObjectifStatut, string][]} />
        </div>
        <label className="field">
          <span>Progression : {editing.progression} %</span>
          <input type="range" min={0} max={100} step={5} value={editing.progression} onChange={(e) => set("progression")(Number(e.target.value))} />
        </label>
        <ErrorBox error={error} />
        <div className="actions">
          <button type="submit" disabled={busy}>
            Enregistrer
          </button>
          <button type="button" className="ghost" onClick={() => setEditing(undefined)}>
            Annuler
          </button>
          {editing.id && (
            <ConfirmButton
              label="Supprimer"
              confirmLabel="Supprimer l'objectif"
              onConfirm={() =>
                void run(async () => {
                  await records.remove("objectif", editing.id as string);
                  setEditing(undefined);
                  objectifs.reload();
                })
              }
            />
          )}
        </div>
      </form>
    );
  }

  const list = objectifs.data ?? [];
  return (
    <section className="card">
      <div className="toolbar">
        <h2>Objectifs thérapeutiques</h2>
        <button
          type="button"
          onClick={() =>
            setEditing({ patient_id: patient.id, domaine: DOMAINES[0], intitule: "", statut: "en_cours", progression: 0, debut: today() })
          }
        >
          Nouvel objectif
        </button>
      </div>
      <ErrorBox error={objectifs.error} />
      {list.length === 0 ? (
        <Empty>Aucun objectif défini. Les objectifs structurent le projet thérapeutique et le suivi de l'évolution.</Empty>
      ) : (
        <ul className="list">
          {list.map((o) => (
            <li key={o.id}>
              <button type="button" className="row" onClick={() => setEditing(o)}>
                <span>
                  <strong>{o.intitule}</strong>
                  <span className="muted"> · {o.domaine} · {OBJECTIF_STATUT[o.statut]}</span>
                </span>
                <meter min={0} max={100} value={o.progression} aria-label={`Progression ${o.progression} %`} />
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function PatientExercices({ patient, navigate }: { patient: Patient; navigate: Navigate }) {
  const results = useRecords<ExerciceResultat>("exercice", patient.id);
  const sorted = [...(results.data ?? [])].sort((a, b) => b.date.localeCompare(a.date));
  return (
    <section className="card">
      <div className="toolbar">
        <h2>Exercices</h2>
        <button type="button" onClick={() => navigate("exercices", patient.id)}>
          Lancer un exercice
        </button>
      </div>
      <ErrorBox error={results.error} />
      {sorted.length === 0 ? (
        <Empty>Aucun résultat d'exercice pour ce patient.</Empty>
      ) : (
        <div className="table-wrap">
          <table className="results read">
            <thead>
              <tr>
                <th>Date</th>
                <th>Exercice</th>
                <th>Niveau</th>
                <th>Score</th>
                <th>Détails</th>
              </tr>
            </thead>
            <tbody>
              {sorted.map((r) => (
                <tr key={r.id}>
                  <td>{formatDate(r.date)}</td>
                  <td>{exerciseTitle(r.exercice)}</td>
                  <td>{r.niveau}</td>
                  <td>
                    <meter min={0} max={Math.max(1, r.total)} value={r.score} /> {r.score}/{r.total}
                  </td>
                  <td className="small">{r.details}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
