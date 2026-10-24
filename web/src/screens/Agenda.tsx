import { useMemo, useState } from "react";
import type { Navigate } from "../App";
import { ErrorBox } from "../components";
import { usePatients, useRecords } from "../hooks";
import { fullName, SEANCE_STATUT, today, type Seance } from "../model";
import { newSeance, SeanceForm } from "./SeanceForm";

function addDays(iso: string, n: number): string {
  const [y, m, d] = iso.split("-").map(Number);
  return today(new Date(y ?? 1970, (m ?? 1) - 1, (d ?? 1) + n));
}

/** Lundi de la semaine contenant `iso`. */
export function mondayOf(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  const date = new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1);
  const shift = (date.getDay() + 6) % 7;
  return addDays(iso, -shift);
}

function dayLabel(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1).toLocaleDateString("fr-BE", { weekday: "long", day: "numeric", month: "long" });
}

export function Agenda({ navigate }: { navigate: Navigate }) {
  const patients = usePatients();
  const seances = useRecords<Seance>("seance");
  const [week, setWeek] = useState(() => mondayOf(today()));
  const [editing, setEditing] = useState<Seance>();
  const [weekend, setWeekend] = useState(false);

  const days = useMemo(() => Array.from({ length: weekend ? 7 : 6 }, (_, i) => addDays(week, i)), [week, weekend]);
  const byDay = useMemo(() => {
    const map = new Map<string, Seance[]>();
    for (const s of seances.data ?? []) {
      const list = map.get(s.date) ?? [];
      list.push(s);
      map.set(s.date, list);
    }
    map.forEach((l) => l.sort((a, b) => (a.heure ?? "").localeCompare(b.heure ?? "")));
    return map;
  }, [seances.data]);

  if (editing)
    return (
      <SeanceForm
        value={editing}
        patients={(patients.data ?? []).filter((p) => (p.statut ?? "actif") !== "termine" || p.id === editing.patient_id)}
        onDone={() => {
          setEditing(undefined);
          seances.reload();
        }}
        onCancel={() => setEditing(undefined)}
      />
    );

  const count = days.reduce((n, d) => n + (byDay.get(d)?.filter((s) => s.statut !== "annulee").length ?? 0), 0);
  return (
    <section className="card">
      <div className="toolbar">
        <h1>Agenda</h1>
        <div className="actions">
          <button type="button" className="ghost" onClick={() => setWeek(addDays(week, -7))} aria-label="Semaine précédente">
            ‹
          </button>
          <button type="button" className="ghost" onClick={() => setWeek(mondayOf(today()))}>
            Cette semaine
          </button>
          <button type="button" className="ghost" onClick={() => setWeek(addDays(week, 7))} aria-label="Semaine suivante">
            ›
          </button>
        </div>
      </div>
      <p className="muted">
        Semaine du {dayLabel(week)} · {count} séance(s)
        <label className="check inline">
          <input type="checkbox" checked={weekend} onChange={(e) => setWeekend(e.target.checked)} /> Afficher le dimanche
        </label>
      </p>
      <ErrorBox error={seances.error ?? patients.error} />
      <div className="week">
        {days.map((d) => (
          <div key={d} className={d === today() ? "day today" : "day"}>
            <div className="day-head">
              <strong>{dayLabel(d)}</strong>
              <button
                type="button"
                className="ghost small-btn"
                aria-label={`Ajouter une séance le ${dayLabel(d)}`}
                onClick={() => setEditing(newSeance("", d))}
              >
                +
              </button>
            </div>
            {(byDay.get(d) ?? []).map((s) => (
              <button key={s.id} type="button" className={`slot ${s.statut}`} onClick={() => setEditing(s)}>
                <span className="slot-time">{s.heure}</span> {fullName(patients.byId.get(s.patient_id))}
                {s.statut !== "prevue" && <span className="muted small"> · {SEANCE_STATUT[s.statut]}</span>}
              </button>
            ))}
          </div>
        ))}
      </div>
      <button type="button" className="ghost" onClick={() => navigate("patients")}>
        Ouvrir les dossiers patients
      </button>
    </section>
  );
}
