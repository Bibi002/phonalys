import { useEffect, useState } from "react";
import { api } from "../api";

interface PairingInfo {
  available: boolean;
  ca_fingerprint?: string | null;
  origin?: string | null;
}

interface PairingQr {
  url: string;
  qr_svg: string;
  ca_fingerprint: string;
}

/** Premier et dernier groupes de l'empreinte : assez pour une comparaison à l'œil. */
export function shortFingerprint(fp: string): string {
  const parts = fp.split(":");
  return parts.length > 8 ? `${parts.slice(0, 4).join(":")} … ${parts.slice(-4).join(":")}` : fp;
}

export function platform(ua: string): "ios" | "android" | "autre" {
  if (/iPhone|iPad|iPod/i.test(ua) || (/Macintosh/i.test(ua) && /Mobile/i.test(ua))) return "ios";
  if (/Android/i.test(ua)) return "android";
  return "autre";
}

/** Section de l'écran « Sécurité » : QR code à scanner avec le téléphone. */
export function PairingSection() {
  const [qr, setQr] = useState<PairingQr>();
  const [unavailable, setUnavailable] = useState(false);
  useEffect(() => {
    api<PairingQr>("GET", "/api/pairing/qr")
      .then(setQr)
      .catch(() => setUnavailable(true));
  }, []);
  return (
    <section className="card">
      <h2>Téléphones et tablettes</h2>
      {unavailable || !qr ? (
        <p className="muted">
          {unavailable
            ? "Cette installation n'est accessible que depuis cet ordinateur. Pour connecter un téléphone, lancez le serveur du cabinet (phonalys-server) sur le réseau local : voir le guide d'installation."
            : "Chargement…"}
        </p>
      ) : (
        <>
          <p>
            Scannez ce code avec l'appareil photo du téléphone, puis suivez les étapes affichées. L'appareil installe
            une seule fois l'autorité du cabinet ; elle ne vaut que pour les adresses du réseau local.
          </p>
          <div className="pairing">
            <img className="qr" alt={`QR code vers ${qr.url}`} src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(qr.qr_svg)}`} />
            <div>
              <p className="muted small">Adresse : {qr.url.replace("/#appairage", "")}</p>
              <p>
                Empreinte à comparer sur le téléphone :<br />
                <code className="fingerprint">{qr.ca_fingerprint}</code>
              </p>
            </div>
          </div>
        </>
      )}
    </section>
  );
}

/** Page publique ouverte par le QR code, sur le téléphone. */
export function Pairing({ onDone }: { onDone: () => void }) {
  const [info, setInfo] = useState<PairingInfo>();
  const [error, setError] = useState(false);
  useEffect(() => {
    api<PairingInfo>("GET", "/api/pairing/info")
      .then(setInfo)
      .catch(() => setError(true));
  }, []);
  const os = platform(navigator.userAgent);
  const fp = info?.ca_fingerprint ?? "";

  return (
    <main className="narrow">
      <section className="card">
        <h1>Connecter cet appareil au cabinet</h1>
        {error && <p className="error">Serveur injoignable : vérifiez que l'appareil est sur le réseau du cabinet.</p>}
        {info && !info.available && <p>Ce serveur n'accepte pas d'autres appareils (installation limitée à l'ordinateur).</p>}
        {info?.available && (
          <>
            <p>
              Étape 1 : vérifiez que l'empreinte ci-dessous est <strong>identique</strong> à celle affichée sur
              l'ordinateur du cabinet (écran Sécurité). Si elle diffère, arrêtez-vous : quelqu'un s'interpose.
            </p>
            <p>
              <code className="fingerprint">{fp}</code>
              <br />
              <span className="muted small">En abrégé : {shortFingerprint(fp)}</span>
            </p>
            {os === "ios" && (
              <ol className="steps">
                <li>
                  <a className="button" href="/api/pairing/phonalys.mobileconfig">Télécharger le profil</a> puis
                  « Autoriser ».
                </li>
                <li>Réglages › Profil téléchargé › Installer (code de l'appareil demandé).</li>
                <li>
                  Réglages › Général › Informations › <em>Réglages des certificats</em> : activez « Phonalys - autorité
                  locale du cabinet ». Vérifiez-y l'empreinte SHA-256.
                </li>
                <li>Revenez ici et touchez « Continuer ».</li>
              </ol>
            )}
            {os === "android" && (
              <ol className="steps">
                <li>
                  <a className="button" href="/api/pairing/ca.crt">Télécharger le certificat</a>
                </li>
                <li>
                  Paramètres › Sécurité › Chiffrement et identifiants › Installer un certificat › <em>Certificat CA</em>,
                  puis choisissez « phonalys-cabinet.crt ». (Le chemin varie selon le fabricant : cherchez « certificat
                  CA » dans les paramètres.)
                </li>
                <li>Revenez ici et touchez « Continuer ».</li>
              </ol>
            )}
            {os === "autre" && (
              <ol className="steps">
                <li>
                  <a className="button" href="/api/pairing/ca.crt">Télécharger le certificat</a> (ou le{" "}
                  <a href="/api/pairing/phonalys.mobileconfig">profil pour Mac</a>).
                </li>
                <li>Installez-le comme autorité racine de confiance du système ou du navigateur.</li>
                <li>Rechargez cette page : le navigateur ne doit plus afficher d'avertissement.</li>
              </ol>
            )}
            <p className="muted small">
              Ensuite, ajoutez Phonalys à l'écran d'accueil (Partager › Sur l'écran d'accueil sur iPhone, menu ⋮ ›
              Installer l'application sur Android) et activez la connexion biométrique dans « Sécurité ».
            </p>
          </>
        )}
        <button
          type="button"
          onClick={() => {
            history.replaceState(null, "", "/");
            onDone();
          }}
        >
          Continuer
        </button>
      </section>
    </main>
  );
}
