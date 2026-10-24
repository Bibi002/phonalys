import { useMemo, useState, type FormEvent } from "react";
import { api, records, type Settings } from "../api";
import type { Navigate } from "../App";
import { ConfirmButton, Empty, ErrorBox, Field, Select, TextArea, useAction, useLoad } from "../components";
import { BATTERIES, batteryName, OTHER_BATTERY } from "../data/batteries";
import { usePatients, useRecords } from "../hooks";
import { DraftAssistant } from "./Assistant";
import {
  age,
  BILAN_TYPE,
  DOMAINES,
  formatDate,
  fullName,
  MESURE,
  normalCdf,
  today,
  toZ,
  type Bilan,
  type BilanType,
  type Mesure,
  type Patient,
  type Resultat,
} from "../model";

const EMPTY_ROW: Resultat = { epreuve: "", mesure: "z" };

export function newBilan(patientId: string): Bilan {
  return { patient_id: patientId, date: today(), type: "initial", batterie: BATTERIES[0]?.id ?? OTHER_BATTERY, resultats: [{ ...EMPTY_ROW }] };
}

function fmt(n: number | undefined, digits = 2): string {
  return n === undefined ? "" : n.toLocaleString("fr-BE", { maximumFractionDigits: digits, minimumFractionDigits: 0 });
}

/** Lecture neutre d'un score : position par rapport à la moyenne, sans étiquette clinique. */
export function zSummary(r: Resultat): string {
  const z = toZ(r.mesure, r.valeur);
  if (z === undefined) return "";
  return `z ≈ ${fmt(z)} · percentile ≈ ${fmt(normalCdf(z) * 100, 0)}`;
}

// --- Page « Bilans » : tous les bilans du cabinet ----------------------------------------

export function Bilans({ navigate }: { navigate: Navigate }) {
  const patients = usePatients();
  const bilans = useRecords<Bilan>("bilan");
  const [editing, setEditing] = useState<Bilan>();
  const [viewing, setViewing] = useState<Bilan>();
  const [newFor, setNewFor] = useState("");

  const sorted = useMemo(() => [...(bilans.data ?? [])].sort((a, b) => b.date.localeCompare(a.date)), [bilans.data]);

  if (viewing)
    return <BilanView bilan={viewing} patient={patients.byId.get(viewing.patient_id)} onClose={() => setViewing(undefined)} onEdit={() => { setEditing(viewing); setViewing(undefined); }} />;
  if (editing)
    return (
      <BilanEditor
        value={editing}
        patient={patients.byId.get(editing.patient_id)}
        previous={sorted.filter((b) => b.patient_id === editing.patient_id && b.id !== editing.id)}
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
          <h1>Bilans</h1>
        </div>
        <p className="muted">
          Saisissez les résultats des tests que vous faites passer avec votre matériel sous licence (EVALEO, EXALang,
          ELO…). Phonalys ne reproduit aucun item ni aucune norme de ces tests : vous reportez les scores obtenus avec le
          manuel ou le logiciel de l'éditeur, et Phonalys les présente, les compare dans le temps et met en page le
          compte rendu.
        </p>
        <div className="inline-form">
          <Select
            label="Nouveau bilan pour"
            value={newFor}
            onChange={setNewFor}
            options={[["", "— Choisir un patient —"] as const, ...(patients.data ?? []).map((p) => [p.id, fullName(p)] as const)]}
          />
          <button type="button" disabled={!newFor} onClick={() => setEditing(newBilan(newFor))}>
            Commencer le bilan
          </button>
        </div>
      </section>
      <section className="card">
        <h2>Bilans enregistrés</h2>
        <ErrorBox error={bilans.error ?? patients.error} />
        {sorted.length === 0 ? (
          <Empty>Aucun bilan pour l'instant.</Empty>
        ) : (
          <ul className="list">
            {sorted.map((b) => (
              <li key={b.id}>
                <button type="button" className="row" onClick={() => setViewing(b)}>
                  <span>
                    <strong>{fullName(patients.byId.get(b.patient_id))}</strong> · {BILAN_TYPE[b.type]} ·{" "}
                    {batteryName(b.batterie, b.batterie_nom)}
                  </span>
                  <span className="muted">{formatDate(b.date)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
        <button type="button" className="ghost" onClick={() => navigate("patients")}>
          Voir les dossiers patients
        </button>
      </section>
    </>
  );
}

// --- Saisie ------------------------------------------------------------------------------

export function BilanEditor(props: {
  value: Bilan;
  patient: Patient | undefined;
  previous: Bilan[];
  onDone: () => void;
  onCancel: () => void;
}) {
  const [b, setB] = useState<Bilan>(props.value);
  const { busy, error, run } = useAction();
  const set = <K extends keyof Bilan>(k: K) => (v: Bilan[K]) => setB({ ...b, [k]: v });
  const setRow = (i: number, patch: Partial<Resultat>) =>
    setB({ ...b, resultats: b.resultats.map((r, k) => (k === i ? { ...r, ...patch } : r)) });

  const template = props.previous.find((p) => p.batterie === b.batterie && p.resultats.some((r) => r.epreuve));

  function save(e: FormEvent) {
    e.preventDefault();
    void run(async () => {
      await records.save("bilan", { ...b, resultats: b.resultats.filter((r) => r.epreuve.trim()) });
      props.onDone();
    });
  }

  return (
    <form className="card" onSubmit={save}>
      <h1>
        {b.id ? "Bilan" : "Nouveau bilan"} — {fullName(props.patient)}
      </h1>
      {props.patient?.naissance && <p className="muted">Âge à la date du bilan : {age(props.patient.naissance, b.date)}</p>}
      <div className="grid3">
        <Field label="Date" type="date" value={b.date} onChange={set("date")} required />
        <Select label="Type" value={b.type} onChange={set("type")} options={Object.entries(BILAN_TYPE) as [BilanType, string][]} />
        <Select
          label="Batterie"
          value={b.batterie ?? OTHER_BATTERY}
          onChange={set("batterie")}
          options={[...BATTERIES.map((x) => [x.id, `${x.nom} (${x.editeur})`] as const), [OTHER_BATTERY, "Autre / épreuves libres"] as const]}
        />
      </div>
      {b.batterie === OTHER_BATTERY && (
        <Field label="Nom de la batterie ou des épreuves" value={b.batterie_nom ?? ""} onChange={set("batterie_nom")} />
      )}
      <TextArea label="Motif de la demande" value={b.demande ?? ""} onChange={set("demande")} rows={2} />
      <TextArea label="Observations pendant la passation" value={b.observations ?? ""} onChange={set("observations")} rows={3} hint="Attention, fatigue, comportement, stratégies…" />

      <h2>Résultats</h2>
      <p className="muted">
        Reportez le score indiqué par le manuel ou le logiciel de l'éditeur. Phonalys convertit seulement les échelles
        usuelles (écart-type, percentile, note standard) pour les comparer : il n'applique aucune norme.
      </p>
      {template && b.resultats.every((r) => !r.epreuve) && (
        <button
          type="button"
          className="ghost"
          onClick={() => setB({ ...b, resultats: template.resultats.map((r) => ({ epreuve: r.epreuve, domaine: r.domaine ?? "", mesure: r.mesure })) })}
        >
          Reprendre les épreuves du bilan du {formatDate(template.date)}
        </button>
      )}
      <div className="table-wrap">
        <table className="results">
          <thead>
            <tr>
              <th>Épreuve</th>
              <th>Domaine</th>
              <th>Score brut</th>
              <th>Échelle</th>
              <th>Valeur</th>
              <th>Lecture</th>
              <th>Commentaire</th>
              <th aria-label="Retirer" />
            </tr>
          </thead>
          <tbody>
            {b.resultats.map((r, i) => (
              <tr key={i}>
                <td>
                  <input aria-label="Épreuve" value={r.epreuve} onChange={(e) => setRow(i, { epreuve: e.target.value })} />
                </td>
                <td>
                  <select aria-label="Domaine" value={r.domaine ?? ""} onChange={(e) => setRow(i, { domaine: e.target.value })}>
                    <option value="">—</option>
                    {DOMAINES.map((d) => (
                      <option key={d}>{d}</option>
                    ))}
                  </select>
                </td>
                <td>
                  <input aria-label="Score brut" value={r.brut ?? ""} onChange={(e) => setRow(i, { brut: e.target.value })} />
                </td>
                <td>
                  <select aria-label="Échelle" value={r.mesure} onChange={(e) => setRow(i, { mesure: e.target.value as Mesure })}>
                    {(Object.entries(MESURE) as [Mesure, string][]).map(([m, l]) => (
                      <option key={m} value={m}>
                        {l}
                      </option>
                    ))}
                  </select>
                </td>
                <td>
                  <input
                    aria-label="Valeur"
                    inputMode="decimal"
                    disabled={r.mesure === "aucune"}
                    value={r.valeur ?? ""}
                    onChange={(e) => setRow(i, { valeur: e.target.value })}
                  />
                </td>
                <td className="muted small">{zSummary(r)}</td>
                <td>
                  <input aria-label="Commentaire" value={r.commentaire ?? ""} onChange={(e) => setRow(i, { commentaire: e.target.value })} />
                </td>
                <td>
                  <button
                    type="button"
                    className="ghost"
                    aria-label="Retirer la ligne"
                    onClick={() => setB({ ...b, resultats: b.resultats.filter((_, k) => k !== i) })}
                  >
                    ×
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <button type="button" className="ghost" onClick={() => setB({ ...b, resultats: [...b.resultats, { ...EMPTY_ROW }] })}>
        Ajouter une épreuve
      </button>

      <DraftAssistant
        bilan={b}
        onUse={(conclusion, projet, mode) =>
          setB({
            ...b,
            conclusion: mode === "append" && b.conclusion ? `${b.conclusion}\n\n${conclusion}` : conclusion,
            projet: mode === "append" && b.projet ? `${b.projet}\n\n${projet}` : projet || (b.projet ?? ""),
          })
        }
      />
      <TextArea label="Conclusion" value={b.conclusion ?? ""} onChange={set("conclusion")} rows={5} hint="Votre analyse clinique : Phonalys ne pose aucun diagnostic." />
      <TextArea label="Projet thérapeutique" value={b.projet ?? ""} onChange={set("projet")} rows={4} />
      <ErrorBox error={error} />
      <div className="actions">
        <button type="submit" disabled={busy}>
          Enregistrer le bilan
        </button>
        <button type="button" className="ghost" onClick={props.onCancel}>
          Annuler
        </button>
        {b.id && (
          <ConfirmButton
            label="Supprimer"
            confirmLabel="Supprimer le bilan"
            onConfirm={() =>
              void run(async () => {
                await records.remove("bilan", b.id as string);
                props.onDone();
              })
            }
          />
        )}
      </div>
    </form>
  );
}

// --- Compte rendu imprimable ---------------------------------------------------------------

export function Profile({ resultats }: { resultats: Resultat[] }) {
  const rows = resultats
    .map((r) => ({ r, z: toZ(r.mesure, r.valeur) }))
    .filter((x): x is { r: Resultat; z: number } => x.z !== undefined);
  if (rows.length === 0) return null;
  const W = 600;
  const label = 220;
  const scale = (z: number) => label + ((Math.max(-4, Math.min(4, z)) + 4) / 8) * (W - label - 10);
  const H = rows.length * 26 + 30;
  return (
    <figure className="profile">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Profil des scores, en écarts-types par rapport à la moyenne">
        {[-3, -2, -1, 0, 1, 2, 3].map((z) => (
          <g key={z}>
            <line x1={scale(z)} x2={scale(z)} y1={4} y2={H - 22} className={z === 0 ? "axis zero" : "axis"} />
            <text x={scale(z)} y={H - 6} textAnchor="middle" className="tick">
              {z > 0 ? `+${z}` : z}
            </text>
          </g>
        ))}
        {rows.map(({ r, z }, i) => {
          const y = 8 + i * 26;
          const x0 = scale(0);
          const x1 = scale(z);
          return (
            <g key={i}>
              <text x={label - 8} y={y + 13} textAnchor="end" className="lbl">
                {r.epreuve.length > 30 ? `${r.epreuve.slice(0, 29)}…` : r.epreuve}
              </text>
              <rect x={Math.min(x0, x1)} y={y + 2} width={Math.max(2, Math.abs(x1 - x0))} height={16} rx={3} className="bar" />
            </g>
          );
        })}
      </svg>
      <figcaption className="muted small">Écarts-types par rapport à la moyenne de l'étalonnage de chaque test (0 = moyenne).</figcaption>
    </figure>
  );
}

export function BilanView(props: { bilan: Bilan; patient: Patient | undefined; onClose: () => void; onEdit: () => void }) {
  const { bilan: b, patient } = props;
  const settings = useLoad(() => api<Settings>("GET", "/api/settings"));
  const pr = settings.data?.practitioner ?? {};
  return (
    <article className="card report">
      <div className="actions no-print">
        <button type="button" onClick={() => window.print()}>
          Imprimer ou enregistrer en PDF
        </button>
        <button type="button" className="ghost" onClick={props.onEdit}>
          Modifier
        </button>
        <button type="button" className="ghost" onClick={props.onClose}>
          Retour
        </button>
      </div>
      <header className="report-head">
        <div>
          <strong>{pr.nom || "Cabinet de logopédie"}</strong>
          {pr.titre && <div>{pr.titre}</div>}
          {pr.inami && <div>INAMI : {pr.inami}</div>}
          {pr.adresse && <div>{pr.adresse}</div>}
          {(pr.telephone || pr.email) && <div>{[pr.telephone, pr.email].filter(Boolean).join(" · ")}</div>}
        </div>
        <div className="right">
          <div>{formatDate(b.date)}</div>
        </div>
      </header>
      <h1>{BILAN_TYPE[b.type]}</h1>
      <p>
        <strong>{fullName(patient)}</strong>
        {patient?.naissance && <> · né(e) le {formatDate(patient.naissance)} · {age(patient.naissance, b.date)} à la date du bilan</>}
        {patient?.classe && <> · {patient.classe}</>}
      </p>
      <p>Outil : {batteryName(b.batterie, b.batterie_nom)}</p>
      {b.demande && (
        <section>
          <h2>Motif de la demande</h2>
          <p className="pre">{b.demande}</p>
        </section>
      )}
      {b.observations && (
        <section>
          <h2>Observations</h2>
          <p className="pre">{b.observations}</p>
        </section>
      )}
      {b.resultats.length > 0 && (
        <section>
          <h2>Résultats</h2>
          <div className="table-wrap">
            <table className="results read">
              <thead>
                <tr>
                  <th>Épreuve</th>
                  <th>Brut</th>
                  <th>Score</th>
                  <th>Lecture</th>
                  <th>Commentaire</th>
                </tr>
              </thead>
              <tbody>
                {b.resultats.map((r, i) => (
                  <tr key={i}>
                    <td>
                      {r.epreuve}
                      {r.domaine && <div className="muted small">{r.domaine}</div>}
                    </td>
                    <td>{r.brut}</td>
                    <td>{r.mesure === "aucune" ? "" : `${r.valeur ?? ""} (${MESURE[r.mesure].split(" (")[0]})`}</td>
                    <td className="small">{zSummary(r)}</td>
                    <td>{r.commentaire}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <Profile resultats={b.resultats} />
        </section>
      )}
      {b.conclusion && (
        <section>
          <h2>Conclusion</h2>
          <p className="pre">{b.conclusion}</p>
        </section>
      )}
      {b.projet && (
        <section>
          <h2>Projet thérapeutique</h2>
          <p className="pre">{b.projet}</p>
        </section>
      )}
      <footer className="signature">
        <p>{pr.nom || ""}</p>
        <p className="muted small">Document confidentiel, couvert par le secret professionnel.</p>
      </footer>
    </article>
  );
}

// --- Évolution entre bilans ---------------------------------------------------------------

export function BilanCompare({ bilans }: { bilans: Bilan[] }) {
  const ordered = [...bilans].sort((a, b) => a.date.localeCompare(b.date));
  const names = [...new Set(ordered.flatMap((b) => b.resultats.map((r) => r.epreuve).filter(Boolean)))];
  if (ordered.length < 2 || names.length === 0) return null;
  return (
    <section className="card">
      <h2>Évolution entre les bilans</h2>
      <p className="muted small">Scores exprimés en écarts-types (z) pour pouvoir comparer des échelles différentes.</p>
      <div className="table-wrap">
        <table className="results read">
          <thead>
            <tr>
              <th>Épreuve</th>
              {ordered.map((b) => (
                <th key={b.id}>{formatDate(b.date)}</th>
              ))}
              <th>Écart</th>
            </tr>
          </thead>
          <tbody>
            {names.map((n) => {
              const zs = ordered.map((b) => {
                const r = b.resultats.find((x) => x.epreuve === n);
                return r ? toZ(r.mesure, r.valeur) : undefined;
              });
              const known = zs.filter((z): z is number => z !== undefined);
              const delta = known.length >= 2 ? (known[known.length - 1] as number) - (known[0] as number) : undefined;
              return (
                <tr key={n}>
                  <td>{n}</td>
                  {zs.map((z, i) => (
                    <td key={i}>{fmt(z)}</td>
                  ))}
                  <td>{delta === undefined ? "" : `${delta > 0 ? "+" : ""}${fmt(delta)}`}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
