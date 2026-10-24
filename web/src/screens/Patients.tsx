import { useEffect, useMemo, useState, type FormEvent } from "react";
import { api, type Patient } from "../api";
import { ConfirmButton, ErrorBox, Field, useAction } from "../components";

type Draft = Omit<Patient, "id">;
const EMPTY: Draft = { nom: "", prenom: "", naissance: "", notes: "" };

function fullName(p: Patient) {
  return [p.nom?.toUpperCase(), p.prenom].filter(Boolean).join(" ") || "(sans nom)";
}

export function Patients() {
  const [patients, setPatients] = useState<Patient[]>();
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<{ id?: string; draft: Draft }>();
  const { busy, error, run } = useAction();

  function reload() {
    void run(async () => {
      const list = await api<Patient[]>("GET", "/api/patients");
      list.sort((a, b) => fullName(a).localeCompare(fullName(b), "fr"));
      setPatients(list);
    });
  }
  useEffect(reload, []);

  const shown = useMemo(() => {
    const q = query.trim().toLocaleLowerCase("fr");
    return (patients ?? []).filter((p) => !q || fullName(p).toLocaleLowerCase("fr").includes(q));
  }, [patients, query]);

  function save(e: FormEvent) {
    e.preventDefault();
    if (!editing) return;
    const { id, draft } = editing;
    void run(async () => {
      if (id) await api("PUT", `/api/patients/${id}`, draft);
      else await api("POST", "/api/patients", draft);
      setEditing(undefined);
      reload();
    });
  }

  function remove(id: string) {
    void run(async () => {
      await api("DELETE", `/api/patients/${id}`);
      setEditing(undefined);
      reload();
    });
  }

  if (editing) {
    const set = (k: keyof Draft) => (v: string) => setEditing({ ...editing, draft: { ...editing.draft, [k]: v } });
    return (
      <form className="card" onSubmit={save}>
        <h2>{editing.id ? "Dossier patient" : "Nouveau patient"}</h2>
        <div className="grid2">
          <Field label="Nom" value={editing.draft.nom ?? ""} onChange={set("nom")} required autoFocus />
          <Field label="PrÃ©nom" value={editing.draft.prenom ?? ""} onChange={set("prenom")} />
          <Field label="Date de naissance" type="date" value={editing.draft.naissance ?? ""} onChange={set("naissance")} />
        </div>
        <label className="field">
          <span>Notes</span>
          <textarea
            rows={6}
            value={editing.draft.notes ?? ""}
            onChange={(e) => set("notes")(e.target.value)}
          />
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
              label="Effacer le dossier"
              confirmLabel="Effacer dÃ©finitivement"
              onConfirm={() => remove(editing.id as string)}
            />
          )}
        </div>
      </form>
    );
  }

  return (
    <section className="card">
      <div className="toolbar">
        <h2>Patients</h2>
        <button type="button" onClick={() => setEditing({ draft: { ...EMPTY } })}>
          Nouveau patient
        </button>
      </div>
      <Field label="Rechercher" value={query} onChange={setQuery} />
      <ErrorBox error={error} />
      {patients === undefined ? (
        <p className="muted">Chargementâ€¦</p>
      ) : shown.length === 0 ? (
        <p className="muted">{patients.length === 0 ? "Aucun dossier pour l'instant." : "Aucun rÃ©sultat."}</p>
      ) : (
        <ul className="list">
          {shown.map((p) => (
            <li key={p.id}>
              <button
                type="button"
                className="row"
                onClick={() => {
                  const { id, ...draft } = p;
                  setEditing({ id, draft: { ...EMPTY, ...draft } });
                }}
              >
                <span>{fullName(p)}</span>
                <span className="muted">{p.naissance ? new Date(p.naissance).toLocaleDateString("fr-BE") : ""}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
