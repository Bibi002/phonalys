import { useState, type FormEvent } from "react";
import { api, type Settings } from "../api";
import { ConfirmButton, Empty, ErrorBox, Field, Tabs, TextArea, useAction, useLoad } from "../components";

type Tab = "registre" | "information" | "violation" | "aipd";

const TABS: readonly (readonly [Tab, string])[] = [
  ["registre", "Registre des traitements"],
  ["information", "Information des patients"],
  ["violation", "Violation de données"],
  ["aipd", "Analyse d'impact"],
];

/** Texte affiché quand une coordonnée du cabinet n'est pas encore renseignée. */
const MISSING = "[à compléter dans Réglages › Cabinet]";

export interface Cabinet {
  nom: string;
  titre: string;
  inami: string;
  adresse: string;
  telephone: string;
  email: string;
}

export function cabinet(s: Settings | undefined): Cabinet {
  const p = s?.practitioner ?? {};
  const v = (k: string) => (p[k] ?? "").trim() || MISSING;
  return { nom: v("nom"), titre: v("titre"), inami: v("inami"), adresse: v("adresse"), telephone: v("telephone"), email: v("email") };
}

const today = () => new Date().toLocaleDateString("fr-BE", { day: "numeric", month: "long", year: "numeric" });

function Disclaimer() {
  return (
    <p className="warning no-print">
      Modèle prérempli à partir des réglages du cabinet. Relisez-le, complétez-le et faites-le valider (avocat ou
      conseiller en protection des données) : il ne constitue pas un avis juridique.
    </p>
  );
}

function PrintButton() {
  return (
    <div className="actions no-print">
      <button type="button" onClick={() => window.print()}>
        Imprimer ou enregistrer en PDF
      </button>
    </div>
  );
}

/** Documents de conformité RGPD propres au cabinet. */
export function Conformite() {
  const [tab, setTab] = useState<Tab>("registre");
  const settings = useLoad(() => api<Settings>("GET", "/api/settings"));
  const c = cabinet(settings.data);
  return (
    <>
      <section className="card no-print">
        <h1>Conformité</h1>
        <p className="muted">
          Les documents que le RGPD demande au cabinet de tenir ou de remettre, préremplis. Phonalys n'envoie rien :
          imprimez-les et conservez-les avec vos documents professionnels.
        </p>
        <ErrorBox error={settings.error} />
        <Tabs label="Documents de conformité" value={tab} onChange={setTab} tabs={TABS} />
      </section>
      {tab === "registre" && <Registre c={c} />}
      {tab === "information" && <Information c={c} />}
      {tab === "violation" && <Violation c={c} />}
      {tab === "aipd" && <Aipd c={c} />}
    </>
  );
}

function Registre({ c }: { c: Cabinet }) {
  return (
    <article className="card report legal">
      <Disclaimer />
      <PrintButton />
      <h2>Registre des activités de traitement (RGPD, art. 30)</h2>
      <p className="muted">Mis à jour le {today()}</p>
      <h3>Responsable du traitement</h3>
      <dl className="facts">
        <dt>Nom</dt>
        <dd>{c.nom}</dd>
        <dt>Qualité</dt>
        <dd>{c.titre} · INAMI {c.inami}</dd>
        <dt>Adresse</dt>
        <dd>{c.adresse}</dd>
        <dt>Contact</dt>
        <dd>
          {c.telephone} · {c.email}
        </dd>
        <dt>Délégué à la protection des données</dt>
        <dd>
          Non désigné : un professionnel de santé exerçant à titre individuel ne traite pas, en principe, des données «
          à grande échelle » (considérant 91 du RGPD). À réexaminer en cas d'association ou de cabinet de groupe.
        </dd>
      </dl>

      <h3>Traitement 1 : dossier patient et soins logopédiques</h3>
      <dl className="facts">
        <dt>Finalités</dt>
        <dd>
          Tenue du dossier patient ; bilans, rééducation et suivi ; rapports au médecin prescripteur et au médecin-conseil
          de la mutualité ; organisation des rendez-vous.
        </dd>
        <dt>Bases légales</dt>
        <dd>
          Art. 6.1.c (obligation de tenir un dossier : loi du 22 avril 2019 relative à la qualité de la pratique des soins
          de santé) et 6.1.b (exécution du contrat de soins). Données de santé : art. 9.2.h et 9.3 (professionnel de
          santé tenu au secret professionnel, art. 458 du Code pénal).
        </dd>
        <dt>Personnes concernées</dt>
        <dd>Patients (souvent mineurs) ; leurs parents ou représentants légaux ; médecins prescripteurs ; enseignants ou autres intervenants cités au dossier.</dd>
        <dt>Catégories de données</dt>
        <dd>
          Identification et contact ; données de santé (anamnèse, bilans, résultats d'épreuves et d'exercices, objectifs,
          comptes rendus de séance) ; scolarité ; documents joints (prescriptions, comptes rendus, courriers).
        </dd>
        <dt>Destinataires</dt>
        <dd>
          Médecin prescripteur et médecin-conseil (bilans et demandes d'accord) ; autres professionnels de la prise en
          charge, avec l'accord du patient ou de ses représentants. <strong>Aucun sous-traitant informatique</strong> :
          les données sont hébergées sur le matériel du cabinet et l'éditeur de Phonalys n'y a pas accès.
        </dd>
        <dt>Transferts hors de l'Union européenne</dt>
        <dd>Aucun.</dd>
        <dt>Durée de conservation</dt>
        <dd>
          Au moins 30 ans et au plus 50 ans après le dernier contact avec le patient (loi du 22 avril 2019, art. 35).
          Effacement sur demande dans les limites de cette obligation.
        </dd>
        <dt>Mesures de sécurité (art. 32)</dt>
        <dd>
          Coffre chiffré (une clé par enregistrement) ; connexion à deux facteurs (phrase de passe et clé biométrique ou
          code TOTP) ; verrouillage automatique et à la mise en veille ; réauthentification pour ouvrir les documents et
          exporter un dossier ; journal d'audit chaîné ; sauvegardes chiffrées testées à chaque création ; TLS 1.3 sur
          le réseau local, avec une autorité limitée à ce réseau.
        </dd>
      </dl>

      <h3>Traitement 2 : aide à la rédaction par IA locale (si activée)</h3>
      <dl className="facts">
        <dt>Finalité</dt>
        <dd>Proposer un projet de conclusion et de projet thérapeutique que le logopède relit, corrige et valide.</dd>
        <dt>Données</dt>
        <dd>
          Résultats du bilan pseudonymisés : nom, école, médecin et contacts remplacés par des jetons, date de naissance
          remplacée par l'âge.
        </dd>
        <dt>Destinataires</dt>
        <dd>
          Aucun : le modèle est installé sur le matériel du cabinet ; les services d'IA en ligne sont bloqués par le
          logiciel.
        </dd>
        <dt>Décision automatisée</dt>
        <dd>Aucune (art. 22) : le texte proposé n'a aucun effet sans validation humaine.</dd>
      </dl>
    </article>
  );
}

function Information({ c }: { c: Cabinet }) {
  return (
    <article className="card report legal">
      <Disclaimer />
      <PrintButton />
      <h2>Vos données au cabinet de logopédie</h2>
      <p className="muted">Information des patients et de leurs parents (RGPD, art. 13)</p>
      <p>
        <strong>{c.nom}</strong>, {c.titre} (INAMI {c.inami}), {c.adresse}, est responsable du traitement de vos données et de
        celles de votre enfant. Contact : {c.telephone} · {c.email}.
      </p>
      <h3>Pourquoi ?</h3>
      <p>
        Pour réaliser les bilans et la rééducation, tenir le dossier imposé par la loi du 22 avril 2019 relative à la
        qualité de la pratique des soins de santé, et rédiger les rapports destinés au médecin prescripteur et au
        médecin-conseil de votre mutualité.
      </p>
      <h3>Quelles données ?</h3>
      <p>
        Identité et coordonnées, histoire du développement et de la santé utile à la prise en charge, résultats des
        bilans et des exercices, comptes rendus de séance, documents que vous transmettez.
      </p>
      <h3>Qui y a accès ?</h3>
      <p>
        Le logopède, tenu au secret professionnel. Le médecin prescripteur et le médecin-conseil reçoivent les bilans
        nécessaires au remboursement. D'autres professionnels (école, médecin, autre thérapeute) ne reçoivent des
        informations qu'avec votre accord.
      </p>
      <h3>Où sont-elles conservées ?</h3>
      <p>
        Sur l'ordinateur du cabinet, dans un coffre chiffré. Elles ne sont ni envoyées sur Internet ni confiées à un
        prestataire informatique. Si une aide à la rédaction par intelligence artificielle est utilisée, elle
        fonctionne sur le matériel du cabinet, sans le nom de votre enfant, et le logopède relit et valide chaque
        texte.
      </p>
      <h3>Combien de temps ?</h3>
      <p>Au moins 30 ans et au plus 50 ans après le dernier contact, comme la loi l'impose.</p>
      <h3>Vos droits</h3>
      <p>
        Vous pouvez consulter le dossier et en obtenir une copie, le faire corriger, demander la limitation d'un
        traitement ou vous y opposer, et recevoir vos données dans un format lisible par ordinateur. L'effacement
        reste limité par l'obligation légale de conservation. Pour un enfant, ces droits sont exercés par ses parents
        ou représentants légaux ; l'avis de l'enfant est pris en compte selon sa maturité (loi du 22 août 2002
        relative aux droits du patient, art. 12). Adressez votre demande au cabinet : une réponse vous est donnée
        dans le mois.
      </p>
      <p>
        Vous pouvez aussi introduire une plainte auprès de l'Autorité de protection des données, rue de la Presse 35,
        1000 Bruxelles, www.autoriteprotectiondonnees.be.
      </p>
    </article>
  );
}

function Aipd({ c }: { c: Cabinet }) {
  return (
    <article className="card report legal">
      <Disclaimer />
      <PrintButton />
      <h2>Analyse d'impact relative à la protection des données (RGPD, art. 35)</h2>
      <p className="muted">
        {c.nom} · {today()} · trame à compléter. Le traitement porte sur des données de santé de personnes vulnérables
        (enfants) : une analyse est recommandée, même pour un cabinet individuel.
      </p>
      <h3>1. Description du traitement</h3>
      <p>Voir le registre (traitements 1 et 2). Outil : Phonalys, installé sur le matériel du cabinet ; nombre approximatif de patients par an : ……</p>
      <h3>2. Nécessité et proportionnalité</h3>
      <ul>
        <li>Finalités déterminées et légitimes : soins et obligations légales de tenue du dossier.</li>
        <li>Minimisation : seuls les champs utiles à la prise en charge ; aucune donnée transmise à l'éditeur ; IA pseudonymisée.</li>
        <li>Conservation : 30 à 50 ans après le dernier contact (obligation légale).</li>
        <li>Information des personnes : notice remise au premier rendez-vous (onglet « Information des patients »).</li>
        <li>Droits : copie complète du dossier exportable depuis la fiche patient ; rectification ; effacement dans les limites légales.</li>
      </ul>
      <h3>3. Risques et mesures</h3>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Risque</th>
              <th>Mesures en place</th>
              <th>Risque résiduel (à évaluer)</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>Vol ou perte de l'ordinateur ou d'un téléphone</td>
              <td>Coffre chiffré, verrouillage automatique et à la mise en veille, deux facteurs, sauvegardes chiffrées</td>
              <td>……</td>
            </tr>
            <tr>
              <td>Accès par un tiers au poste resté ouvert</td>
              <td>Verrouillage après inactivité et en arrière-plan, réauthentification pour les documents et les exports</td>
              <td>……</td>
            </tr>
            <tr>
              <td>Rançongiciel ou panne du disque</td>
              <td>Sauvegardes quotidiennes testées, copie vers un second support (règle 3-2-1)</td>
              <td>……</td>
            </tr>
            <tr>
              <td>Interception sur le réseau du cabinet</td>
              <td>TLS 1.3, autorité du cabinet limitée au réseau local, empreinte vérifiée à l'appairage</td>
              <td>……</td>
            </tr>
            <tr>
              <td>Fuite vers un service d'IA en ligne</td>
              <td>Adresses publiques et fournisseurs en ligne bloqués par le logiciel ; pseudonymisation</td>
              <td>……</td>
            </tr>
            <tr>
              <td>Erreur dans un texte proposé par l'IA</td>
              <td>Relecture et validation obligatoires par le logopède ; mention de l'origine du texte</td>
              <td>……</td>
            </tr>
            <tr>
              <td>Oubli de la phrase de passe</td>
              <td>Codes de récupération à usage unique, conservés hors ligne</td>
              <td>……</td>
            </tr>
          </tbody>
        </table>
      </div>
      <h3>4. Mesures complémentaires du cabinet</h3>
      <p>Système d'exploitation à jour, chiffrement du disque (BitLocker, FileVault, LUKS), session Windows protégée, réseau Wi-Fi distinct pour les visiteurs, rangement des codes de récupération : ……</p>
      <h3>5. Conclusion</h3>
      <p>Risque résiduel jugé : …… · Consultation préalable de l'APD nécessaire (art. 36) : oui / non · Date de réexamen : ……</p>
    </article>
  );
}

// --- Violations de données ------------------------------------------------------------

export interface Breach {
  id?: string;
  date_constat: string;
  description: string;
  donnees: string;
  personnes: string;
  consequences: string;
  mesures: string;
  risque: "" | "aucun" | "risque" | "eleve";
  date_notification_apd: string;
  personnes_informees: boolean;
}

const EMPTY: Breach = {
  date_constat: "",
  description: "",
  donnees: "",
  personnes: "",
  consequences: "",
  mesures: "",
  risque: "",
  date_notification_apd: "",
  personnes_informees: false,
};

/** Délai de notification à l'APD (art. 33.1) restant, en heures. */
export function hoursLeft(dateConstat: string, now = Date.now()): number | undefined {
  const t = Date.parse(dateConstat);
  if (Number.isNaN(t)) return undefined;
  return Math.floor((t + 72 * 3600 * 1000 - now) / 3600 / 1000);
}

function Violation({ c }: { c: Cabinet }) {
  const list = useLoad(() => api<Breach[]>("GET", "/api/records/violation"));
  const [editing, setEditing] = useState<Breach>();
  return (
    <>
      <article className="card report legal">
        <Disclaimer />
        <h2>En cas de violation de données (RGPD, art. 33 et 34)</h2>
        <p>
          Exemples : vol de l'ordinateur ou d'un téléphone du cabinet, envoi d'un bilan au mauvais destinataire,
          rançongiciel, accès d'une personne non autorisée.
        </p>
        <ol className="steps">
          <li>
            <strong>Limiter les dégâts</strong> : « Verrouiller » (toutes les sessions se ferment), retirer la clé
            biométrique de l'appareil perdu dans « Sécurité », changer la phrase de passe, isoler le poste infecté du
            réseau.
          </li>
          <li>
            <strong>Évaluer</strong> : quelles données, combien de patients, étaient-elles lisibles ? Un appareil
            volé dont le coffre était verrouillé n'expose en principe que des données chiffrées.
          </li>
          <li>
            <strong>Notifier l'APD dans les 72 heures</strong> après la découverte, sauf si la violation n'entraîne
            vraisemblablement aucun risque : formulaire en ligne sur www.autoriteprotectiondonnees.be. Si toutes les
            informations ne sont pas encore connues, notifiez quand même et complétez ensuite.
          </li>
          <li>
            <strong>Informer les patients</strong> concernés (ou leurs parents) sans tarder si le risque est élevé, en
            termes clairs : ce qui s'est passé, les conséquences probables, les mesures prises, le contact au cabinet ({c.telephone}).
          </li>
          <li>
            <strong>Consigner</strong> chaque violation ci-dessous, même non notifiée (art. 33.5) : l'APD peut demander ce registre.
          </li>
        </ol>
      </article>
      <section className="card">
        <div className="toolbar">
          <h2>Registre des violations</h2>
          {!editing && (
            <button type="button" onClick={() => setEditing({ ...EMPTY, date_constat: new Date().toISOString().slice(0, 16) })}>
              Consigner une violation
            </button>
          )}
        </div>
        <ErrorBox error={list.error} />
        {editing && (
          <BreachForm
            breach={editing}
            onDone={() => {
              setEditing(undefined);
              list.reload();
            }}
          />
        )}
        {list.data && list.data.length === 0 && !editing && <Empty>Aucune violation consignée.</Empty>}
        {list.data && list.data.length > 0 && (
          <ul className="list">
            {[...list.data]
              .sort((a, b) => b.date_constat.localeCompare(a.date_constat))
              .map((b) => (
                <li key={b.id}>
                  <button type="button" className="row" onClick={() => setEditing(b)}>
                    {new Date(b.date_constat).toLocaleString("fr-BE")} · {b.description.slice(0, 80) || "(sans description)"}
                  </button>
                  <span className="muted small">
                    {" "}
                    {b.date_notification_apd ? "notifiée à l'APD" : b.risque === "aucun" ? "non notifiée (aucun risque)" : "notification à décider"}
                  </span>
                </li>
              ))}
          </ul>
        )}
      </section>
    </>
  );
}

function BreachForm(props: { breach: Breach; onDone: () => void }) {
  const [b, setB] = useState<Breach>(props.breach);
  const { busy, error, run } = useAction();
  const set = <K extends keyof Breach>(k: K) => (v: Breach[K]) => setB({ ...b, [k]: v });
  const left = hoursLeft(b.date_constat);

  function save(e: FormEvent) {
    e.preventDefault();
    const { id, ...data } = b;
    void run(async () => {
      if (id) await api("PUT", `/api/records/violation/${id}`, data);
      else await api("POST", "/api/records/violation", data);
      props.onDone();
    });
  }

  return (
    <form onSubmit={save} className="breach">
      <div className="grid2">
        <Field label="Découverte le" type="datetime-local" value={b.date_constat} onChange={set("date_constat")} required />
        <label className="field">
          <span>Risque pour les personnes</span>
          <select value={b.risque} onChange={(e) => set("risque")(e.target.value as Breach["risque"])}>
            <option value="">— à évaluer —</option>
            <option value="aucun">Aucun risque vraisemblable (pas de notification)</option>
            <option value="risque">Risque (notification à l'APD)</option>
            <option value="eleve">Risque élevé (APD et personnes concernées)</option>
          </select>
        </label>
      </div>
      {left !== undefined && b.risque !== "aucun" && !b.date_notification_apd && (
        <p className={left <= 24 ? "warning" : "muted"} role="status">
          {left > 0 ? `Délai de notification à l'APD : il reste environ ${left} h.` : "Le délai de 72 heures est dépassé : notifiez sans attendre en expliquant le retard."}
        </p>
      )}
      <TextArea label="Ce qui s'est passé" value={b.description} onChange={set("description")} rows={3} />
      <div className="grid2">
        <Field label="Données concernées" value={b.donnees} onChange={set("donnees")} hint="Ex. dossiers chiffrés, un bilan en clair…" />
        <Field label="Personnes concernées (nombre approximatif)" value={b.personnes} onChange={set("personnes")} />
      </div>
      <TextArea label="Conséquences probables" value={b.consequences} onChange={set("consequences")} rows={2} />
      <TextArea label="Mesures prises" value={b.mesures} onChange={set("mesures")} rows={2} />
      <div className="grid2">
        <Field label="Notifiée à l'APD le" type="date" value={b.date_notification_apd} onChange={set("date_notification_apd")} />
        <label className="check">
          <input type="checkbox" checked={b.personnes_informees} onChange={(e) => set("personnes_informees")(e.target.checked)} />
          Personnes concernées informées
        </label>
      </div>
      <ErrorBox error={error} />
      <div className="actions">
        <button type="submit" disabled={busy}>
          Enregistrer
        </button>
        <button type="button" className="ghost" onClick={props.onDone}>
          Annuler
        </button>
        {b.id && (
          <ConfirmButton
            label="Supprimer"
            confirmLabel="Supprimer cette entrée du registre"
            onConfirm={() =>
              void run(async () => {
                await api("DELETE", `/api/records/violation/${b.id}`);
                props.onDone();
              })
            }
          />
        )}
      </div>
    </form>
  );
}
