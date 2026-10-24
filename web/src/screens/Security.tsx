import { useEffect, useState } from "react";
import { api, type BindingResult, type PasskeyInfo, type SecurityStatus } from "../api";
import { ConfirmButton, describeError, ErrorBox, Field, RecoveryCodes, useAction } from "../components";
import { createCredential, describeWebAuthnError, getAssertion, webauthnAvailable } from "../webauthn";

const describe = (e: unknown) => (e instanceof DOMException ? describeWebAuthnError(e) : describeError(e));

export function Security() {
  const [status, setStatus] = useState<SecurityStatus>();
  const [keys, setKeys] = useState<PasskeyInfo[]>([]);
  const [label, setLabel] = useState("");
  const [notice, setNotice] = useState<string>();
  const [bindingId, setBindingId] = useState<string>();
  const [passphrase, setPassphrase] = useState("");
  const [newCodes, setNewCodes] = useState<string[]>();
  const { busy, error, run } = useAction();

  function reload() {
    void run(async () => {
      const [s, k] = await Promise.all([
        api<SecurityStatus>("GET", "/api/security"),
        api<PasskeyInfo[]>("GET", "/api/passkeys"),
      ]);
      setStatus(s);
      setKeys(k);
    });
  }
  useEffect(reload, []);

  function register() {
    setNotice(undefined);
    void run(async () => {
      const options = await api<Record<string, unknown>>("POST", "/api/passkeys/register/start");
      const { response, prfSupported } = await createCredential(options);
      setKeys(await api<PasskeyInfo[]>("POST", "/api/passkeys/register/finish", { response, label }));
      setLabel("");
      setNotice(
        prfSupported === false
          ? "ClÃ© enregistrÃ©e. Elle ne prend pas en charge l'extension PRF : elle pourra servir Ã  la connexion, mais pas Ãªtre liÃ©e au coffre."
          : "ClÃ© enregistrÃ©e. Vous pouvez maintenant la lier au coffre.",
      );
      setStatus(await api<SecurityStatus>("GET", "/api/security"));
    }, describe);
  }

  function bind(id: string) {
    void run(async () => {
      const options = await api<Record<string, unknown>>("POST", "/api/passkeys/bind/start", { credential_id: id });
      const { response, prfOutput } = await getAssertion(options);
      if (!prfOutput) throw new Error("Cet appareil ou ce navigateur ne fournit pas l'extension PRF : liaison impossible.");
      setKeys(await api<PasskeyInfo[]>("POST", "/api/passkeys/bind/finish", { passphrase, response, prf_output: prfOutput }));
      setPassphrase("");
      setBindingId(undefined);
      setStatus(await api<SecurityStatus>("GET", "/api/security"));
      setNotice("ClÃ© liÃ©e au coffre.");
    }, describe);
  }

  function remove(id: string) {
    void run(async () => {
      await api("DELETE", `/api/passkeys/${encodeURIComponent(id)}`);
      reload();
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
      const r = await api<BindingResult>("POST", "/api/security/hardware-binding", { passphrase, enabled });
      setPassphrase("");
      if (r.recovery_codes) setNewCodes(r.recovery_codes);
      setStatus(await api<SecurityStatus>("GET", "/api/security"));
    });
  }

  if (newCodes) return <RecoveryCodes codes={newCodes} onDone={() => setNewCodes(undefined)} />;
  if (!status) return <section className="card"><ErrorBox error={error} /><p className="muted">Chargementâ€¦</p></section>;

  return (
    <>
      <section className="card">
        <h2>Ã‰tat de la protection</h2>
        <ul className="status">
          <li className={status.hardware_bound ? "ok" : "todo"}>
            {status.hardware_bound
              ? "Coffre liÃ© au matÃ©riel : la phrase de passe seule ne suffit pas Ã  le dÃ©chiffrer."
              : "Coffre protÃ©gÃ© par la phrase de passe (mode liÃ© au matÃ©riel dÃ©sactivÃ©)."}
          </li>
          <li className={status.require_passkey ? "ok" : "todo"}>
            {status.require_passkey ? "ClÃ© biomÃ©trique obligatoire Ã  la connexion." : "Le code Ã  6 chiffres est acceptÃ©."}
          </li>
          <li className={status.recovery_codes_remaining >= 3 ? "ok" : "todo"}>
            {status.recovery_codes_remaining} code(s) de rÃ©cupÃ©ration restant(s).
          </li>
        </ul>
        <ErrorBox error={error} />
        {notice && <p className="notice" role="status">{notice}</p>}
      </section>

      <section className="card">
        <h2>ClÃ©s biomÃ©triques et de sÃ©curitÃ©</h2>
        <p className="muted">
          Windows Hello, Touch ID, Face ID, empreinte Android ou clÃ© FIDO2 (YubiKey). L'empreinte ou le visage
          sont vÃ©rifiÃ©s par l'appareil : aucune donnÃ©e biomÃ©trique ne parvient Ã  Phonalys.
        </p>
        {keys.length > 0 && (
          <ul className="list">
            {keys.map((k) => (
              <li key={k.id} className="keyrow">
                <span>
                  <strong>{k.label}</strong>
                  {k.bound && <span className="badge ok">liÃ©e au coffre</span>}
                  {k.backup_eligible && <span className="badge">synchronisable</span>}
                </span>
                <span className="actions">
                  {!k.bound && (
                    <button type="button" className="ghost" onClick={() => setBindingId(bindingId === k.id ? undefined : k.id)}>
                      Lier au coffre
                    </button>
                  )}
                  <ConfirmButton label="Retirer" confirmLabel="Retirer la clÃ©" onConfirm={() => remove(k.id)} />
                </span>
                {bindingId === k.id && (
                  <div className="inline-form">
                    <Field
                      label="Phrase de passe (confirmation)"
                      type="password"
                      value={passphrase}
                      onChange={setPassphrase}
                      autoComplete="current-password"
                    />
                    <button type="button" disabled={busy || !passphrase} onClick={() => bind(k.id)}>
                      Lier avec ma biomÃ©trie
                    </button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
        {webauthnAvailable() ? (
          <div className="inline-form">
            <Field label="Nom de la clÃ©" value={label} onChange={setLabel} hint="Par exemple : Â« PC du cabinet Â», Â« iPhone Â», Â« YubiKey de secours Â»." />
            <button type="button" disabled={busy} onClick={register}>
              Ajouter une clÃ©
            </button>
          </div>
        ) : (
          <p className="warning">
            Indisponible ici : les clÃ©s exigent HTTPS, un nom de domaine et un certificat reconnu par cet appareil.
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
          Exiger une clÃ© biomÃ©trique (le code Ã  6 chiffres n'est plus acceptÃ©)
        </label>
      </section>

      <section className="card">
        <h2>Mode liÃ© au matÃ©riel</h2>
        <p>
          La clÃ© du coffre dÃ©pend alors de votre phrase de passe <strong>et</strong> d'un secret qui ne quitte jamais
          vos clÃ©s liÃ©es. Un voleur qui obtiendrait le fichier du coffre et votre phrase de passe ne pourrait
          toujours pas le lire.
        </p>
        <p className="warning">
          Liez au moins <strong>deux</strong> clÃ©s (par exemple Windows Hello et une YubiKey rangÃ©e Ã  part) et
          conservez les codes de rÃ©cupÃ©ration : si tout est perdu, le coffre est dÃ©finitivement illisible.
        </p>
        <Field
          label="Phrase de passe (confirmation)"
          type="password"
          value={bindingId ? "" : passphrase}
          onChange={setPassphrase}
          autoComplete="current-password"
        />
        {status.hardware_bound ? (
          <ConfirmButton label="DÃ©sactiver" confirmLabel="DÃ©sactiver le mode liÃ©" disabled={!passphrase} onConfirm={() => setBinding(false)} />
        ) : (
          <button type="button" disabled={busy || !passphrase || status.bound_passkeys === 0} onClick={() => setBinding(true)}>
            Activer ({status.bound_passkeys} clÃ©{status.bound_passkeys > 1 ? "s" : ""} liÃ©e{status.bound_passkeys > 1 ? "s" : ""})
          </button>
        )}
      </section>
    </>
  );
}
