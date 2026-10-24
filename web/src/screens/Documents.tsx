import { useEffect, useRef, useState, type FormEvent } from "react";
import { api, ApiError, type SecurityStatus, type SessionInfo } from "../api";
import { ConfirmButton, Empty, ErrorBox, Field, PasswordField, Select, useAction, useLoad } from "../components";
import { usePatients } from "../hooks";
import { fullName } from "../model";
import { describeWebAuthnError, getAssertion, webauthnAvailable } from "../webauthn";

export interface DocumentInfo {
  id: string;
  patient_id?: string;
  name: string;
  mime: string;
  size: number;
  category: string;
  created_at: number;
}

export const DOC_CATEGORIES = [
  "Prescription",
  "Compte rendu médical",
  "Bilan externe",
  "Courrier",
  "Consentement",
  "Scolarité",
  "Autre",
] as const;
type Category = (typeof DOC_CATEGORIES)[number];

/** Doit rester égal à `MAX_DOCUMENT_BYTES` côté serveur. */
export const MAX_DOCUMENT_BYTES = 20 * 1024 * 1024;
const ACCEPT = ".pdf,.png,.jpg,.jpeg,.txt,.docx,.odt,.ods,.xlsx";

export function contentUrl(id: string, download = false): string {
  return `/api/documents/${encodeURIComponent(id)}/content${download ? "?download=1" : ""}`;
}

/** Ce que la visionneuse intégrée sait afficher ; le reste se télécharge. */
export function viewerKind(mime: string): "image" | "pdf" | "text" | null {
  if (mime === "image/png" || mime === "image/jpeg") return "image";
  if (mime === "application/pdf") return "pdf";
  if (mime.startsWith("text/plain")) return "text";
  return null;
}

export function formatSize(n: number): string {
  if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toLocaleString("fr-BE", { maximumFractionDigits: 1 })} Mo`;
  return `${Math.max(1, Math.round(n / 1024))} Ko`;
}

export async function uploadDocument(file: Blob, name: string, category: string, patientId?: string): Promise<DocumentInfo> {
  const q = new URLSearchParams({ name, category });
  if (patientId) q.set("patient_id", patientId);
  const res = await fetch(`/api/documents?${q.toString()}`, {
    method: "POST",
    headers: { "x-phonalys-request": "1", "content-type": "application/octet-stream", accept: "application/json" },
    body: file,
    credentials: "same-origin",
    cache: "no-store",
  });
  const data = (await res.json().catch(() => ({}))) as { error?: unknown };
  if (!res.ok) throw new ApiError(res.status, typeof data.error === "string" ? data.error : `erreur ${res.status}`);
  return data as DocumentInfo;
}

/**
 * Période pendant laquelle les documents sont ouverts, après confirmation d'identité.
 * Le serveur reste seul juge : cet état ne sert qu'à l'affichage.
 */
function useElevation() {
  const [until, setUntil] = useState(0);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    api<SessionInfo>("GET", "/api/session")
      .then((s) => setUntil(s.elevated_secs ? Date.now() + s.elevated_secs * 1000 : 0))
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    if (until <= now) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [until, now]);
  const left = Math.max(0, Math.ceil((until - now) / 1000));
  return {
    left,
    grant: (secs: number) => {
      setNow(Date.now());
      setUntil(Date.now() + secs * 1000);
    },
    revoke: () => setUntil(0),
  };
}

/** Confirmation d'identité : clé biométrique, ou phrase de passe et code à 6 chiffres. */
export function Reauth(props: { onDone: (secs: number) => void; onCancel: () => void; reason?: string }) {
  const security = useLoad(() => api<SecurityStatus>("GET", "/api/security"));
  const [passphrase, setPassphrase] = useState("");
  const [code, setCode] = useState("");
  const { busy, error, run } = useAction();
  const s = security.data;
  const canKey = webauthnAvailable() && (s?.passkeys ?? 0) > 0;

  function withKey() {
    void run(async () => {
      const options = await api<Record<string, unknown>>("POST", "/api/reauth/passkey/start");
      const { response } = await getAssertion(options);
      const r = await api<{ elevated_secs: number }>("POST", "/api/reauth/passkey/finish", { response });
      props.onDone(r.elevated_secs);
    }, describeWebAuthnError);
  }

  function withCode(e: FormEvent) {
    e.preventDefault();
    void run(async () => {
      const r = await api<{ elevated_secs: number }>("POST", "/api/reauth", { passphrase, code: code.replace(/\s/g, "") });
      setPassphrase("");
      props.onDone(r.elevated_secs);
    });
  }

  return (
    <section className="card reauth" aria-labelledby="reauth-title">
      <h2 id="reauth-title">Confirmez votre identité</h2>
      <p className="muted">
        {props.reason ??
          "Les documents sont protégés par une vérification supplémentaire, valable 5 minutes sur cet appareil. Une session restée ouverte ne suffit pas à les consulter."}
      </p>
      <ErrorBox error={error ?? security.error} />
      {canKey && (
        <div className="actions">
          <button type="button" disabled={busy} onClick={withKey}>
            Confirmer avec ma clé biométrique
          </button>
        </div>
      )}
      {s && !s.require_passkey && (
        <form onSubmit={withCode}>
          {canKey && <p className="muted small">ou</p>}
          <PasswordField label="Phrase de passe" value={passphrase} onChange={setPassphrase} autoComplete="current-password" required />
          <Field label="Code à 6 chiffres" value={code} onChange={setCode} inputMode="numeric" autoComplete="one-time-code" required />
          <div className="actions">
            <button type="submit" disabled={busy}>
              Confirmer
            </button>
          </div>
        </form>
      )}
      <button type="button" className="ghost" onClick={props.onCancel}>
        Annuler
      </button>
    </section>
  );
}

function Upload(props: { patientId?: string | undefined; onUploaded: () => void }) {
  const [file, setFile] = useState<File>();
  const [category, setCategory] = useState<Category>("Prescription");
  const [notice, setNotice] = useState<string>();
  const input = useRef<HTMLInputElement>(null);
  const { busy, error, setError, run } = useAction();

  function submit(e: FormEvent) {
    e.preventDefault();
    setNotice(undefined);
    if (!file) return setError("Choisissez un fichier.");
    if (file.size > MAX_DOCUMENT_BYTES) return setError(`Fichier trop volumineux (maximum ${formatSize(MAX_DOCUMENT_BYTES)}).`);
    void run(async () => {
      const d = await uploadDocument(file, file.name, category, props.patientId);
      setNotice(`« ${d.name} » est chiffré dans le coffre.`);
      setFile(undefined);
      if (input.current) input.current.value = "";
      props.onUploaded();
    });
  }

  return (
    <form className="card" onSubmit={submit}>
      <h2>Ajouter un document</h2>
      <p className="muted small">
        PDF, images (PNG, JPEG), texte, Word, OpenDocument ou Excel, jusqu'à {formatSize(MAX_DOCUMENT_BYTES)}. Le fichier
        est chiffré dès son arrivée sur le serveur ; pensez à supprimer l'original de votre dossier Téléchargements.
      </p>
      <div className="inline-form">
        <label className="field">
          <span>Fichier</span>
          <input ref={input} type="file" accept={ACCEPT} onChange={(e) => setFile(e.target.files?.[0])} />
        </label>
        <Select label="Catégorie" value={category} onChange={setCategory} options={DOC_CATEGORIES.map((c) => [c, c] as const)} />
        <button type="submit" disabled={busy || !file}>
          {busy ? "Chiffrement…" : "Ajouter"}
        </button>
      </div>
      <ErrorBox error={error} />
      {notice && <p className="notice" role="status">{notice}</p>}
    </form>
  );
}

function Viewer({ doc, onClose }: { doc: DocumentInfo; onClose: () => void }) {
  const kind = viewerKind(doc.mime);
  return (
    <section className="card viewer" aria-label={`Aperçu de ${doc.name}`}>
      <div className="toolbar">
        <h2>{doc.name}</h2>
        <span className="actions">
          <a className="button ghost" href={contentUrl(doc.id)} target="_blank" rel="noopener noreferrer">
            Nouvel onglet
          </a>
          <button type="button" className="ghost" onClick={onClose}>
            Fermer
          </button>
        </span>
      </div>
      {kind === "image" && <img src={contentUrl(doc.id)} alt={doc.name} />}
      {kind === "pdf" && <iframe src={contentUrl(doc.id)} title={doc.name} />}
      {/* Cadre isolé : aucun script, même si le serveur en laissait passer. */}
      {kind === "text" && <iframe src={contentUrl(doc.id)} title={doc.name} sandbox="" />}
    </section>
  );
}

/** Coffre documentaire, pour un patient ou pour tout le cabinet. */
export function DocumentsPanel(props: { patientId?: string | undefined }) {
  const { patientId } = props;
  const docs = useLoad(
    () => api<DocumentInfo[]>("GET", patientId ? `/api/documents?patient_id=${encodeURIComponent(patientId)}` : "/api/documents"),
    [patientId],
  );
  const patients = usePatients();
  const elevation = useElevation();
  const [asking, setAsking] = useState(false);
  const [viewing, setViewing] = useState<DocumentInfo>();
  const { busy, error, run } = useAction();
  const open = elevation.left > 0;

  useEffect(() => {
    if (!open) setViewing(undefined);
  }, [open]);

  function patientName(id?: string): string {
    if (!id) return "Cabinet";
    return fullName(patients.byId.get(id));
  }

  function remove(d: DocumentInfo) {
    void run(async () => {
      try {
        await api("DELETE", `/api/documents/${encodeURIComponent(d.id)}`);
      } catch (e) {
        if (e instanceof ApiError && e.reauth) {
          elevation.revoke();
          setAsking(true);
        }
        throw e;
      }
      if (viewing?.id === d.id) setViewing(undefined);
      docs.reload();
    });
  }

  const list = docs.data ?? [];
  return (
    <>
      <Upload patientId={patientId} onUploaded={docs.reload} />
      {asking && !open && (
        <Reauth
          onDone={(secs) => {
            elevation.grant(secs);
            setAsking(false);
          }}
          onCancel={() => setAsking(false)}
        />
      )}
      {viewing && open && <Viewer doc={viewing} onClose={() => setViewing(undefined)} />}
      <section className="card">
        <div className="toolbar">
          <h2>Documents</h2>
          {open ? (
            <span className="badge ok" role="status">
              Ouverts encore {Math.floor(elevation.left / 60)}:{String(elevation.left % 60).padStart(2, "0")}
            </span>
          ) : (
            list.length > 0 && (
              <button type="button" onClick={() => setAsking(true)}>
                Ouvrir les documents
              </button>
            )
          )}
        </div>
        <ErrorBox error={error ?? docs.error} />
        {list.length === 0 ? (
          <Empty>Aucun document.</Empty>
        ) : (
          <ul className="list">
            {list.map((d) => (
              <li key={d.id} className="keyrow">
                <span>
                  <strong>{d.name}</strong>
                  {d.category && <span className="badge">{d.category}</span>}
                  <br />
                  <span className="muted small">
                    {!patientId && `${patientName(d.patient_id)} · `}
                    {new Date(d.created_at * 1000).toLocaleDateString("fr-BE")} · {formatSize(d.size)}
                  </span>
                </span>
                {open && (
                  <span className="actions">
                    {viewerKind(d.mime) && (
                      <button type="button" className="ghost" onClick={() => setViewing(d)}>
                        Aperçu
                      </button>
                    )}
                    <a className="button ghost" href={contentUrl(d.id, true)} download={d.name}>
                      Télécharger
                    </a>
                    <ConfirmButton label="Supprimer" confirmLabel="Supprimer définitivement" disabled={busy} onConfirm={() => remove(d)} />
                  </span>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </>
  );
}

export function Documents() {
  return (
    <>
      <section className="card">
        <h1>Coffre documentaire</h1>
        <p className="muted">
          Prescriptions, comptes rendus médicaux, courriers, consentements : chaque document est chiffré dans le coffre
          avec le reste du dossier, inclus dans les sauvegardes, et effacé avec le dossier du patient. Les documents d'un
          patient s'ajoutent aussi depuis l'onglet « Documents » de son dossier.
        </p>
      </section>
      <DocumentsPanel />
    </>
  );
}
