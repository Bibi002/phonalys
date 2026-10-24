import { api, type AuditPage } from "../api";
import { ErrorBox, useLoad } from "../components";

const LABELS: Record<string, string> = {
  vault_created: "Coffre créé",
  login: "Connexion (code à 6 chiffres)",
  login_failed: "Échec de connexion",
  recovery_code_used: "Connexion par code de récupération",
  passkey_login: "Connexion par clé biométrique",
  passkey_registered: "Clé ajoutée",
  passkey_removed: "Clé retirée",
  factor_bound: "Clé liée au coffre",
  hardware_binding_changed: "Mode lié au matériel modifié",
  quick_unlock_changed: "Connexion en un geste modifiée",
  security_policy_changed: "Politique de connexion modifiée",
  password_changed: "Phrase de passe changée",
  unlock: "Déverrouillage",
  lock: "Verrouillage manuel",
  auto_lock: "Verrouillage automatique",
  patient_created: "Dossier créé",
  patient_viewed: "Dossier consulté",
  patient_updated: "Dossier modifié",
  patient_erased: "Dossier effacé",
  record_created: "Élément ajouté",
  record_viewed: "Élément consulté",
  record_updated: "Élément modifié",
  record_erased: "Élément effacé",
  settings_changed: "Réglages modifiés",
  audit_viewed: "Journal consulté",
  document_imported: "Document importé",
  document_opened: "Document ouvert",
  reauthenticated: "Identité confirmée (documents)",
  export: "Sauvegarde",
};

const KINDS: Record<string, string> = { seance: "séance", bilan: "bilan", objectif: "objectif", exercice: "exercice", document: "document" };

function describeTarget(t: string | null): string {
  if (!t) return "";
  const [kind, id] = t.includes("/") ? t.split("/") : ["dossier", t];
  return `${KINDS[kind ?? ""] ?? kind} ${id?.slice(0, 8) ?? ""}`;
}

export function Audit() {
  const page = useLoad(() => api<AuditPage>("GET", "/api/audit"));
  const p = page.data;
  return (
    <section className="card">
      <h1>Journal d'audit</h1>
      <p className="muted">
        Chaque accès et chaque modification est inscrit dans un journal chaîné par HMAC : toute altération est détectée
        à l'ouverture du coffre. Il ne contient que des identifiants pseudonymes, jamais de nom.
      </p>
      <ErrorBox error={page.error} />
      {p && (
        <>
          <p className={p.verified ? "notice" : "error"}>
            {p.verified ? "Intégrité du journal vérifiée" : "Intégrité non vérifiée"} · {p.total} entrée(s)
            {p.total > p.entries.length && ` (les ${p.entries.length} plus récentes sont affichées)`}
          </p>
          <div className="table-wrap">
            <table className="results read">
              <thead>
                <tr>
                  <th>Date</th>
                  <th>Événement</th>
                  <th>Cible</th>
                </tr>
              </thead>
              <tbody>
                {p.entries.map((e) => (
                  <tr key={e.seq}>
                    <td className="small">{new Date(e.at).toLocaleString("fr-BE")}</td>
                    <td>{LABELS[e.action] ?? e.action}</td>
                    <td className="muted small">{describeTarget(e.target)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </section>
  );
}
