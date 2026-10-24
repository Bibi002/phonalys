import { useEffect, useState } from "react";
import { api, type BindingResult, type PasskeyInfo, type SecurityStatus } from "../api";
import { ConfirmButton, describeError, ErrorBox, Field, PasswordField, RecoveryCodes, useAction } from "../components";
import { createCredential, describeWebAuthnError, getAssertion, webauthnAvailable } from "../webauthn";
import { PairingSection } from "./Pairing";

const describe = (e: unknown) => (e instanceof DOMException ? describeWebAuthnError(e) : describeError(e));

type Purpose = "quick" | "vault";

export function Security() {
  const [status, setStatus] = useState<SecurityStatus>();
  const [keys, setKeys] = useState<PasskeyInfo[]>([]);
  const [label, setLabel] = useState("");
  const [notice, setNotice] = useState<string>();
  const [pending, setPending] = useState<{ id: string; purpose: Purpose }>();
  const [passphrase, setPassphrase] = useState("");
  const [bindingPass, setBindingPass] = useState("");
  const [newCodes, setNewCodes] = useState<string[]>();
  const { busy, error, run } = useAction();

  async function refresh() {
    const [s, k] = await Promise.all([api<SecurityStatus>("GET", "/api/security"), api<PasskeyInfo[]>("GET", "/api/passkeys")]);
    setStatus(s);
    setKeys(k);
  }
  useEffect(() => void run(refresh), []);

  function register() {
    setNotice(undefined);
    void run(async () => {
      const options = await api<Record<string, unknown>>("POST", "/api/passkeys/register/start");
      const { response, prfSupported } = await createCredential(options);
      const list = await api<PasskeyInfo[]>("POST", "/api/passkeys/register/finish", { response, label });
      setKeys(list);
      setLabel("");
      const added = list.find((k) => k.id === response.id) ?? list[list.length - 1];
      if (prfSupported === false) {
        setNotice(
          "Clé enregistrée. Elle ne prend pas en charge l'extension PRF : elle servira avec la phrase de passe, mais pas pour la connexion en un geste.",
        );
      } else {
        setNotice("Clé enregistrée. Confirmez votre phrase de passe pour activer la connexion en un geste.");
        if (added) setPending({ id: added.id, purpose: "quick" });
      }
      setStatus(await api<SecurityStatus>("GET", "/api/security"));
    }, describe);
  }

  function bind(id: string, purpose: Purpose) {
    void run(async () => {
      const options = await api<Record<string, unknown>>("POST", "/api/passkeys/bind/start", { credential_id: id, purpose });
      const { response, prfOutput } = await getAssertion(options);
      if (!prfOutput) throw new Error("Cet appareil ou ce navigateur ne fournit pas l'extension PRF : activation impossible.");
      setKeys(await api<PasskeyInfo[]>("POST", "/api/passkeys/bind/finish", { passphrase, response, prf_output: prfOutput }));
      setPassphrase("");
      setPending(undefined);
      setStatus(await api<SecurityStatus>("GET", "/api/security"));
      setNotice(purpose === "quick" ? "Connexion en un geste activée pour cette clé." : "Clé liée au coffre.");
    }, describe);
  }

  function removeQuick(id: string) {
    void run(async () => {
      setKeys(await api<PasskeyInfo[]>("DELETE", `/api/passkeys/${encodeURIComponent(id)}/quick`));
      setStatus(await api<SecurityStatus>("GET", "/api/security"));
    });
  }

  function remove(id: string) {
    void run(async () => {
      await api("DELETE", `/api/passkeys/${encodeURIComponent(id)}`);
      await refresh();
    });
  }

  function setPolicy(require_passkey: boolean) {
    void run(async () => {
      await api("PUT", "/api/security/policy", { require_passkey });
      setStatus(await api<SecurityStatus>("GET", "/api/security"));
    });
  }

  function setBinding(enabled: boolean) {
    void run(async () => {
      const r = await api<BindingResult>("POST", "/api/security/hardware-binding", { passphrase: bindingPass, enabled });
      setBindingPass("");
      if (r.recovery_codes) setNewCodes(r.recovery_codes);
      setStatus(await api<SecurityStatus>("GET", "/api/security"));
    });
  }

  if (newCodes) return <RecoveryCodes codes={newCodes} onDone={() => setNewCodes(undefined)} />;
  if (!status) return <section className="card"><ErrorBox error={error} /><p className="muted">Chargement…</p></section>;
  const quick = status.quick_passkeys ?? 0;

  return (
    <>
      <section className="card">
        <h1>Sécurité</h1>
        <ul className="status">
          <li className={quick > 0 ? "ok" : "todo"}>
            {quick > 0
              ? "Connexion en un geste active. Après 3 échecs, la phrase de passe et un second facteur sont redemandés."
              : "Connexion en un geste désactivée : la phrase de passe et un second facteur sont demandés à chaque fois."}
          </li>
          <li className={status.hardware_bound ? "ok" : "todo"}>
            {status.hardware_bound
              ? "Coffre lié au matériel : la phrase de passe seule ne suffit pas à le déchiffrer."
              : "Coffre protégé par la phrase de passe (mode lié au matériel désactivé)."}
          </li>
          <li className={status.require_passkey ? "ok" : "todo"}>
            {status.require_passkey ? "Clé biométrique obligatoire à la connexion." : "Le code à 6 chiffres est accepté."}
          </li>
          <li className={status.recovery_codes_remaining >= 3 ? "ok" : "todo"}>
            {status.recovery_codes_remaining} code(s) de récupération restant(s).
          </li>
        </ul>
        <ErrorBox error={error} />
        {notice && <p className="notice" role="status">{notice}</p>}
      </section>

      <section className="card">
        <h2>Clés biométriques et de sécurité</h2>
        <p className="muted">
          Windows Hello, Touch ID, Face ID, empreinte Android ou clé FIDO2 (YubiKey). L'empreinte ou le visage sont
          vérifiés par l'appareil : aucune donnée biométrique ne parvient à Phonalys. Une clé sert sans limite.
        </p>
        {keys.length > 0 && (
          <ul className="list">
            {keys.map((k) => (
              <li key={k.id} className="keyrow">
                <span>
                  <strong>{k.label}</strong>
                  {k.quick && <span className="badge ok">en un geste</span>}
                  {k.bound && <span className="badge ok">liée au coffre</span>}
                  {k.backup_eligible && <span className="badge">synchronisable</span>}
                </span>
                <span className="actions">
                  {k.quick ? (
                    <button type="button" className="ghost" onClick={() => removeQuick(k.id)}>
                      Désactiver le geste seul
                    </button>
                  ) : (
                    <button type="button" className="ghost" onClick={() => setPending({ id: k.id, purpose: "quick" })}>
                      Activer la connexion en un geste
                    </button>
                  )}
                  {!k.bound && (
                    <button type="button" className="ghost" onClick={() => setPending({ id: k.id, purpose: "vault" })}>
                      Lier au coffre
                    </button>
                  )}
                  <ConfirmButton label="Retirer" confirmLabel="Retirer la clé" onConfirm={() => remove(k.id)} />
                </span>
                {pending?.id === k.id && (
                  <div className="inline-form">
                    <PasswordField
                      label="Phrase de passe (confirmation)"
                      value={passphrase}
                      onChange={setPassphrase}
                      autoComplete="current-password"
                      hint={
                        pending.purpose === "quick"
                          ? "Exigée pour qu'une personne devant une session restée ouverte ne puisse pas s'ajouter un accès."
                          : "La clé du coffre dépendra de la phrase de passe et de cette clé."
                      }
                    />
                    <button type="button" disabled={busy || !passphrase} onClick={() => bind(k.id, pending.purpose)}>
                      Confirmer avec ma biométrie
                    </button>
                    <button type="button" className="ghost" onClick={() => setPending(undefined)}>
                      Annuler
                    </button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
        {webauthnAvailable() ? (
          <div className="inline-form">
            <Field label="Nom de la clé" value={label} onChange={setLabel} hint="Par exemple : « PC du cabinet », « iPhone », « YubiKey de secours »." />
            <button type="button" disabled={busy} onClick={register}>
              Ajouter une clé
            </button>
          </div>
        ) : (
          <p className="warning">
            Indisponible ici : les clés exigent HTTPS, un nom de domaine et un certificat reconnu par cet appareil.
          </p>
        )}
      </section>

      <section className="card">
        <h2>Politique de connexion</h2>
        <label className="check">
          <input
            type="checkbox"
            checked={status.require_passkey}
            disabled={busy || status.hardware_bound || (status.passkeys === 0 && !status.require_passkey)}
            onChange={(e) => setPolicy(e.target.checked)}
          />
          Exiger une clé biométrique (le code à 6 chiffres n'est plus accepté)
        </label>
      </section>

      <section className="card">
        <h2>Mode lié au matériel</h2>
        <p>
          La clé du coffre dépend alors de votre phrase de passe <strong>et</strong> d'un secret qui ne quitte jamais vos
          clés liées. Un voleur qui obtiendrait le fichier du coffre et votre phrase de passe ne pourrait toujours pas le
          lire.
        </p>
        <p className="warning">
          Liez au moins <strong>deux</strong> clés (par exemple Windows Hello et une YubiKey rangée à part) et conservez
          les codes de récupération : si tout est perdu, le coffre est définitivement illisible.
        </p>
        <PasswordField label="Phrase de passe (confirmation)" value={bindingPass} onChange={setBindingPass} autoComplete="current-password" />
        {status.hardware_bound ? (
          <ConfirmButton label="Désactiver" confirmLabel="Désactiver le mode lié" disabled={!bindingPass} onConfirm={() => setBinding(false)} />
        ) : (
          <button type="button" disabled={busy || !bindingPass || status.bound_passkeys === 0} onClick={() => setBinding(true)}>
            Activer ({status.bound_passkeys} clé{status.bound_passkeys > 1 ? "s" : ""} liée{status.bound_passkeys > 1 ? "s" : ""})
          </button>
        )}
      </section>
      <PairingSection />
    </>
  );
}
