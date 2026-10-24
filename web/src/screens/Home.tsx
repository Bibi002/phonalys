import { useMemo } from "react";
import { api, type SecurityStatus } from "../api";
import type { Navigate } from "../App";
import { Empty, ErrorBox, useLoad } from "../components";
import { usePatients, useRecords } from "../hooks";
import { formatDate, fullName, SEANCE_STATUT, today, type Seance } from "../model";

export function Home({ navigate }: { navigate: Navigate }) {
  const patients = usePatients();
  const seances = useRecords<Seance>("seance");
  const security = useLoad(() => api<SecurityStatus>("GET", "/api/security"));
  const now = today();

  const todays = useMemo(
    () => (seances.data ?? []).filter((s) => s.date === now).sort((a, b) => (a.heure ?? "").localeCompare(b.heure ?? "")),
    [seances.data, now],
  );

  // Prises en charge proches de la fin des séances accordées.
  const renewals = useMemo(() => {
    const done = (seances.data ?? []).filter((s) => s.statut === "realisee");
    return (patients.data ?? [])
      .filter((p) => (p.statut ?? "actif") === "actif")
      .map((p) => {
        const pec = p.prise_en_charge ?? {};
        const accordees = Number(pec.seances_accordees) || 0;
        const faites = done.filter((s) => s.patient_id === p.id && (!pec.date_accord || s.date >= pec.date_accord)).length;
        return { p, accordees, faites };
      })
      .filter((x) => x.accordees > 0 && x.faites >= x.accordees - 2);
  }, [patients.data, seances.data]);

  const actifs = (patients.data ?? []).filter((p) => (p.statut ?? "actif") === "actif").length;
  const s = security.data;

  return (
    <>
      <section className="card">
        <h1>Bonjour</h1>
        <p className="muted">{formatDate(now)} · {actifs} patient(s) en suivi</p>
        <div className="actions">
          <button type="button" onClick={() => navigate("patients")}>Dossiers patients</button>
          <button type="button" className="ghost" onClick={() => navigate("agenda")}>Agenda</button>
          <button type="button" className="ghost" onClick={() => navigate("bilans")}>Nouveau bilan</button>
          <button type="button" className="ghost" onClick={() => navigate("exercices")}>Exercices</button>
        </div>
      </section>

      <section className="card">
        <h2>Aujourd'hui</h2>
        <ErrorBox error={seances.error ?? patients.error} />
        {todays.length === 0 ? (
          <Empty>Aucune séance prévue aujourd'hui.</Empty>
        ) : (
          <ul className="list">
            {todays.map((x) => (
              <li key={x.id}>
                <button type="button" className="row" onClick={() => navigate("patients", x.patient_id)}>
                  <span>
                    <strong>{x.heure}</strong> {fullName(patients.byId.get(x.patient_id))}
                  </span>
                  <span className="muted">{SEANCE_STATUT[x.statut]}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      {renewals.length > 0 && (
        <section className="card warning-card">
          <h2>Prises en charge à prolonger</h2>
          <ul className="list">
            {renewals.map(({ p, accordees, faites }) => (
              <li key={p.id}>
                <button type="button" className="row" onClick={() => navigate("patients", p.id)}>
                  <span>{fullName(p)}</span>
                  <span className="muted">{faites} / {accordees} séances</span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      {s && (s.recovery_codes_remaining < 3 || (s.quick_passkeys ?? 0) === 0) && (
        <section className="card">
          <h2>Sécurité</h2>
          <ul className="status">
            {s.recovery_codes_remaining < 3 && (
              <li className="todo">Il ne reste que {s.recovery_codes_remaining} code(s) de récupération.</li>
            )}
            {(s.quick_passkeys ?? 0) === 0 && <li className="todo">La connexion en un geste n'est pas activée.</li>}
          </ul>
          <button type="button" className="ghost" onClick={() => navigate("security")}>Ouvrir la sécurité</button>
        </section>
      )}
    </>
  );
}
