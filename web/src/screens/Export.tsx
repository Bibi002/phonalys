import { useState } from "react";
import { api, type SessionInfo } from "../api";
import { ErrorBox, useAction } from "../components";
import { Reauth } from "./Documents";

export function exportUrl(patientId: string): string {
  return `/api/patients/${encodeURIComponent(patientId)}/export`;
}

/**
 * Copie complète du dossier (RGPD art. 15 et 20 ; loi relative aux droits du patient,
 * art. 9) : identité, séances, bilans, objectifs, résultats et documents, en un fichier.
 */
export function ExportPatient({ patientId }: { patientId: string }) {
  const [step, setStep] = useState<"idle" | "reauth" | "ready">("idle");
  const { busy, error, run } = useAction();

  function start() {
    void run(async () => {
      const s = await api<SessionInfo>("GET", "/api/session");
      setStep(s.elevated ? "ready" : "reauth");
    });
  }

  return (
    <section className="card" aria-labelledby="export-title">
      <h2 id="export-title">Copie du dossier</h2>
      <p className="muted">
        Pour répondre à une demande d'accès ou de copie du patient ou de ses parents, ou pour transférer le dossier à un
        confrère : un fichier unique contenant l'identité, les séances, les bilans, les objectifs, les résultats
        d'exercices et les documents. Ce fichier n'est <strong>pas chiffré</strong> : remettez-le en main propre ou
        par un canal sûr, et supprimez-le ensuite de cet appareil.
      </p>
      <ErrorBox error={error} />
      {step === "idle" && (
        <button type="button" className="ghost" disabled={busy} onClick={start}>
          Exporter le dossier complet
        </button>
      )}
      {step === "reauth" && <Reauth
          reason="L'export du dossier complet demande une vérification supplémentaire, valable 5 minutes sur cet appareil."
          onDone={() => setStep("ready")}
          onCancel={() => setStep("idle")}
        />}
      {step === "ready" && (
        <div className="actions">
          <a className="button" href={exportUrl(patientId)} download>
            Télécharger le dossier (JSON)
          </a>
          <button type="button" className="ghost" onClick={() => setStep("idle")}>
            Fermer
          </button>
        </div>
      )}
    </section>
  );
}
