import { useEffect, useState } from "react";
import { api, type Settings } from "../api";
import { Empty, ErrorBox, Field, useAction, useLoad } from "../components";

export interface BackupEntry {
  name: string;
  created_at: number;
  size: number;
  records: number;
  audit_entries: number;
  verified: boolean;
  copied: boolean;
  copy_error?: string | null;
}

interface BackupStatus {
  dir: string;
  extra_dir: string | null;
  auto: boolean;
  last: BackupEntry | null;
  files: BackupEntry[];
}

function size(n: number): string {
  if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toLocaleString("fr-BE", { maximumFractionDigits: 1 })} Mo`;
  return `${Math.max(1, Math.round(n / 1024))} Ko`;
}

function when(secs: number): string {
  return secs ? new Date(secs * 1000).toLocaleString("fr-BE") : "date inconnue";
}

export function Backups() {
  const status = useLoad(() => api<BackupStatus>("GET", "/api/backups"));
  const settings = useLoad(() => api<Settings>("GET", "/api/settings"));
  const [extra, setExtra] = useState("");
  const [notice, setNotice] = useState<string>();
  const { busy, error, run } = useAction();

  useEffect(() => setExtra(settings.data?.backup_extra_dir ?? ""), [settings.data]);

  function backupNow() {
    setNotice(undefined);
    void run(async () => {
      const e = await api<BackupEntry>("POST", "/api/backups");
      setNotice(
        `Sauvegarde créée et vérifiée : ${e.records} enregistrement(s) déchiffré(s) avec succès.${e.copy_error ? ` Attention : ${e.copy_error}.` : e.copied ? " Copie déposée dans le dossier supplémentaire." : ""}`,
      );
      status.reload();
    });
  }

  function verify(name: string) {
    setNotice(undefined);
    void run(async () => {
      const e = await api<BackupEntry>("POST", `/api/backups/${encodeURIComponent(name)}/verify`);
      setNotice(`Test de restauration réussi : ${e.records} enregistrement(s), journal d'audit intact.`);
      status.reload();
    });
  }

  function saveSettings(patch: Partial<Settings>) {
    if (!settings.data) return;
    setNotice(undefined);
    void run(async () => {
      settings.setData(await api<Settings>("PUT", "/api/settings", { ...settings.data, ...patch }));
      setNotice("Réglage enregistré.");
      status.reload();
    });
  }

  const s = status.data;
  const lastAge = s?.last ? Date.now() / 1000 - s.last.created_at : Infinity;
  return (
    <>
      <section className="card">
        <h1>Sauvegardes</h1>
        <p className="muted">
          Les sauvegardes sont chiffrées comme le coffre : elles ne s'ouvrent qu'avec votre phrase de passe (celle en
          vigueur au moment de la sauvegarde). Chacune est aussitôt soumise à un <strong>test de restauration</strong> :
          tous les enregistrements sont déchiffrés et le journal d'audit est vérifié.
        </p>
        <ul className="status">
          <li className={lastAge < 2 * 86400 ? "ok" : "todo"}>
            {s?.last ? `Dernière sauvegarde : ${when(s.last.created_at)}` : "Aucune sauvegarde pour l'instant."}
          </li>
          <li className={s?.auto ? "ok" : "todo"}>
            {s?.auto ? "Sauvegarde automatique chaque jour pendant l'utilisation." : "Sauvegarde automatique désactivée."}
          </li>
          <li className={s?.extra_dir ? "ok" : "todo"}>
            {s?.extra_dir
              ? `Copie supplémentaire vers ${s.extra_dir}.`
              : "Pas de copie sur un second support : en cas de panne du disque, tout serait perdu."}
          </li>
        </ul>
        <ErrorBox error={error ?? status.error} />
        {notice && <p className="notice" role="status">{notice}</p>}
        <button type="button" disabled={busy} onClick={backupNow}>
          {busy ? "Sauvegarde en cours…" : "Sauvegarder maintenant"}
        </button>
      </section>

      <section className="card">
        <h2>Règle 3-2-1</h2>
        <p className="muted">
          Trois copies, sur deux supports différents, dont une hors du cabinet. Indiquez un second dossier (clé USB, disque
          externe, NAS) : chaque sauvegarde y sera copiée. Emportez régulièrement un support hors du cabinet.
        </p>
        <div className="inline-form">
          <Field label="Dossier supplémentaire" value={extra} onChange={setExtra} hint="Chemin complet, par exemple E:\Sauvegardes Phonalys" />
          <button type="button" className="ghost" disabled={busy} onClick={() => saveSettings({ backup_extra_dir: extra })}>
            Enregistrer
          </button>
        </div>
        {settings.data && (
          <label className="check">
            <input type="checkbox" checked={settings.data.auto_backup} onChange={(e) => saveSettings({ auto_backup: e.target.checked })} />
            Sauvegarde automatique quotidienne
          </label>
        )}
        {s && <p className="muted small">Dossier principal : {s.dir}</p>}
      </section>

      <section className="card">
        <h2>Sauvegardes disponibles</h2>
        {!s || s.files.length === 0 ? (
          <Empty>Aucune sauvegarde dans le dossier principal.</Empty>
        ) : (
          <ul className="list">
            {s.files.map((f) => (
              <li key={f.name} className="keyrow">
                <span>
                  <strong>{when(f.created_at)}</strong> · {size(f.size)}
                  {f.verified ? <span className="badge ok">vérifiée</span> : <span className="badge">non vérifiée</span>}
                  {f.copied && <span className="badge ok">copiée</span>}
                </span>
                <span className="actions">
                  <button type="button" className="ghost" disabled={busy} onClick={() => verify(f.name)}>
                    Tester la restauration
                  </button>
                  <a className="button ghost" href={`/api/backups/${encodeURIComponent(f.name)}`} download={f.name}>
                    Télécharger
                  </a>
                </span>
              </li>
            ))}
          </ul>
        )}
        <p className="muted small">
          Pour restaurer sur un nouvel ordinateur : installez Phonalys, puis choisissez « Restaurer une sauvegarde » sur
          l'écran d'installation.
        </p>
      </section>
    </>
  );
}
