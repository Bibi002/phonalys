import { useSyncExternalStore } from "react";

/**
 * Traduction de l'interface (français, néerlandais, allemand : les trois langues
 * officielles de Belgique). La clé est le texte français lui-même : une chaîne absente
 * du dictionnaire s'affiche en français, jamais vide. `i18n.test.ts` vérifie que chaque
 * appel `t("…")` du code a sa traduction dans les deux autres langues.
 *
 * Les contenus cliniques (exercices, lexiques, batteries) sont propres au français et
 * ne sont pas traduits.
 */
export type Lang = "fr" | "nl" | "de";

export const LANGS: readonly (readonly [Lang, string])[] = [
  ["fr", "Français"],
  ["nl", "Nederlands"],
  ["de", "Deutsch"],
];

// Préférence d'affichage propre à l'appareil (aucune donnée sensible).
const KEY = "phonalys.lang";

function detect(): Lang {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved === "fr" || saved === "nl" || saved === "de") return saved;
  } catch {
    // stockage indisponible
  }
  const nav = (typeof navigator !== "undefined" ? navigator.language : "fr").slice(0, 2).toLowerCase();
  return nav === "nl" || nav === "de" ? nav : "fr";
}

let current: Lang = detect();
const listeners = new Set<() => void>();

function applyDocumentLang() {
  if (typeof document !== "undefined") document.documentElement.lang = current;
}
applyDocumentLang();

export function getLang(): Lang {
  return current;
}

export function setLang(lang: Lang) {
  current = lang;
  try {
    localStorage.setItem(KEY, lang);
  } catch {
    // stockage indisponible
  }
  applyDocumentLang();
  listeners.forEach((l) => l());
}

/** Re-rend le composant (et ses enfants) quand la langue change. */
export function useLang(): Lang {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
    () => current,
  );
}

/** Traduit `fr` dans la langue courante ; `{nom}` est remplacé par `vars.nom`. */
export function t(fr: string, vars?: Record<string, string | number>): string {
  const s = current === "fr" ? fr : (DICT[current][fr] ?? fr);
  return vars ? s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : s;
}

type Dict = Record<string, string>;

const NL: Dict = {
  // Structure
  "Aller au contenu": "Naar de inhoud",
  "Navigation principale": "Hoofdnavigatie",
  Accueil: "Start",
  Patients: "Patiënten",
  Agenda: "Agenda",
  Bilans: "Bilans",
  Exercices: "Oefeningen",
  Documents: "Documenten",
  "Réglages": "Instellingen",
  Sauvegardes: "Back-ups",
  "Sécurité": "Beveiliging",
  Journal: "Logboek",
  "Conformité": "Privacy",
  "Connexion au serveur…": "Verbinding met de server…",
  "Serveur injoignable": "Server onbereikbaar",
  "Vérifiez que le serveur Phonalys du cabinet est démarré et que cet appareil est sur le même réseau.":
    "Controleer of de Phonalys-server van de praktijk draait en of dit toestel op hetzelfde netwerk zit.",
  "Réessayer": "Opnieuw proberen",
  "Connectez-vous en un geste": "Aanmelden met één gebaar",
  "Activez la connexion biométrique : chaque jour, il suffira de poser le doigt, de montrer votre visage ou d'utiliser Windows Hello. La phrase de passe ne sera redemandée qu'après trois échecs.":
    "Schakel biometrisch aanmelden in: elke dag volstaat dan uw vinger, uw gezicht of Windows Hello. De wachtzin wordt pas na drie mislukte pogingen opnieuw gevraagd.",
  "Activer maintenant": "Nu inschakelen",
  "Plus tard": "Later",
  "Verrouille immédiatement, sur tous les appareils": "Vergrendelt onmiddellijk, op alle toestellen",
  Verrouiller: "Vergrendelen",
  "Sans activité, Phonalys se verrouille pour protéger les dossiers. Délai réglable dans « Réglages ».":
    "Zonder activiteit vergrendelt Phonalys zich om de dossiers te beschermen. Termijn instelbaar in « Instellingen ».",
  "Verrouillage auto : {n} min": "Automatische vergrendeling: {n} min",
  "Inactif : verrouillage dans {t}": "Inactief: vergrendeling over {t}",
  "Phonalys se verrouillera dans deux minutes faute d'activité.":
    "Phonalys vergrendelt over twee minuten bij gebrek aan activiteit.",
  "Rester connecté": "Aangemeld blijven",
  // Composants communs
  Masquer: "Verbergen",
  Afficher: "Tonen",
  Annuler: "Annuleren",
  Continuer: "Doorgaan",
  "Chargement…": "Laden…",
  Enregistrer: "Opslaan",
  "Codes de récupération": "Herstelcodes",
  "Ces codes ne seront plus jamais affichés.": "Deze codes worden nooit meer getoond.",
  "Chacun permet une seule connexion si vous perdez votre appareil. Imprimez-les ou notez-les et rangez-les hors ligne, à l'écart de votre ordinateur.":
    "Elke code geeft één keer toegang als u uw toestel verliest. Druk ze af of schrijf ze op en bewaar ze offline, uit de buurt van uw computer.",
  "J'ai mis ces codes en lieu sûr.": "Ik heb deze codes veilig opgeborgen.",
  // Connexion
  "Vos données sont chiffrées. Identifiez-vous pour les ouvrir.":
    "Uw gegevens zijn versleuteld. Meld u aan om ze te openen.",
  "Vérification…": "Controle…",
  "Se connecter avec ma clé biométrique": "Aanmelden met mijn biometrische sleutel",
  "Empreinte, visage ou Windows Hello : un seul geste suffit.":
    "Vingerafdruk, gezicht of Windows Hello: één gebaar volstaat.",
  "Utiliser une autre méthode": "Een andere methode gebruiken",
  "Second facteur": "Tweede factor",
  "Clé biométrique": "Biometrische sleutel",
  "Code à 6 chiffres": "Code van 6 cijfers",
  "Phrase de passe": "Wachtzin",
  "Celle choisie à l'installation, exactement (majuscules, accents et espaces comptent).":
    "Precies die van de installatie (hoofdletters, accenten en spaties tellen mee).",
  "Code de l'application d'authentification": "Code van de authenticatie-app",
  "Code de récupération": "Herstelcode",
  "Format XXXX-XXXX-XXXX-XXXX. Chaque code ne sert qu'une fois et débloque aussi une connexion bloquée.":
    "Formaat XXXX-XXXX-XXXX-XXXX. Elke code dient maar één keer en deblokkeert ook een geblokkeerde aanmelding.",
  "Après la phrase de passe, votre appareil demande l'empreinte, le visage ou le code Windows Hello. La clé sert à chaque connexion, sans limite.":
    "Na de wachtzin vraagt uw toestel uw vingerafdruk, gezicht of Windows Hello-code. De sleutel dient bij elke aanmelding, zonder limiet.",
  "Les clés biométriques exigent une connexion HTTPS avec un nom de domaine et un certificat reconnu.":
    "Biometrische sleutels vereisen een HTTPS-verbinding met een domeinnaam en een erkend certificaat.",
  "Débloquer avec un code de récupération": "Deblokkeren met een herstelcode",
  "Continuer avec ma clé": "Doorgaan met mijn sleutel",
  "Déverrouiller": "Ontgrendelen",
  "Revenir à la connexion en un geste": "Terug naar aanmelden met één gebaar",
  "Aucune clé biométrique n'est encore enregistrée. Connectez-vous avec le code à 6 chiffres de votre application d'authentification ; vous pourrez ensuite activer l'empreinte, le visage ou Windows Hello dans « Sécurité ».":
    "Er is nog geen biometrische sleutel geregistreerd. Meld u aan met de code van 6 cijfers uit uw authenticatie-app; daarna kunt u vingerafdruk, gezicht of Windows Hello inschakelen in « Beveiliging ».",
  "Après 3 échecs, deux vérifications sont demandées pour protéger les dossiers : votre phrase de passe, puis votre clé biométrique ou le code à 6 chiffres.":
    "Na 3 mislukte pogingen zijn twee controles nodig om de dossiers te beschermen: uw wachtzin, en daarna uw biometrische sleutel of de code van 6 cijfers.",
  "Cet appareil ne fournit pas l'extension PRF : utilisez une autre méthode.":
    "Dit toestel ondersteunt de PRF-extensie niet: gebruik een andere methode.",
  "Ce coffre est lié au matériel, mais cet appareil ou ce navigateur ne fournit pas l'extension PRF. Utilisez une clé liée ou un code de récupération.":
    "Deze kluis is aan hardware gekoppeld, maar dit toestel of deze browser ondersteunt de PRF-extensie niet. Gebruik een gekoppelde sleutel of een herstelcode.",
  // Installation
  "Installation de Phonalys": "Installatie van Phonalys",
  "Vos données restent sur cet appareil, chiffrées. Personne d'autre, pas même l'éditeur, ne peut les lire.":
    "Uw gegevens blijven versleuteld op dit toestel. Niemand anders, zelfs de uitgever niet, kan ze lezen.",
  "Jeton d'installation": "Installatietoken",
  "Affiché dans la console au démarrage du serveur.": "Getoond in de console bij het starten van de server.",
  "Votre nom (facultatif)": "Uw naam (facultatief)",
  "Au moins 14 caractères ; idéalement 5 mots ou plus. Un gestionnaire de mots de passe (Bitwarden, KeePassXC) peut la générer et la retenir.":
    "Minstens 14 tekens; bij voorkeur 5 woorden of meer. Een wachtwoordbeheerder (Bitwarden, KeePassXC) kan ze genereren en onthouden.",
  Confirmation: "Bevestiging",
  "Si votre navigateur propose de générer ou d'enregistrer un mot de passe, vérifiez avec « Afficher » que c'est bien celui que vous retiendrez.":
    "Als uw browser voorstelt een wachtwoord te genereren of op te slaan, controleer dan met « Tonen » dat het wel degelijk het wachtwoord is dat u zult onthouden.",
  "Sans cette phrase de passe, le coffre est définitivement illisible. Il n'existe aucune réinitialisation.":
    "Zonder deze wachtzin is de kluis definitief onleesbaar. Er bestaat geen herstelprocedure.",
  "Création du coffre…": "Kluis wordt aangemaakt…",
  "Créer le coffre": "Kluis aanmaken",
  "Les deux phrases de passe sont différentes.": "De twee wachtzinnen verschillen.",
  "Scannez ce code avec une application d'authentification hors ligne (Aegis, 2FAS, Ente Auth) ou avec votre gestionnaire de mots de passe (Bitwarden, KeePassXC).":
    "Scan deze code met een offline authenticatie-app (Aegis, 2FAS, Ente Auth) of met uw wachtwoordbeheerder (Bitwarden, KeePassXC).",
  "QR code du second facteur": "QR-code van de tweede factor",
  "Saisie manuelle": "Handmatige invoer",
  "Après la première connexion, vous pourrez ajouter une clé biométrique.":
    "Na de eerste aanmelding kunt u een biometrische sleutel toevoegen.",
  "C'est fait": "Klaar",
  "Vous avez déjà une sauvegarde ?": "Hebt u al een back-up?",
  "Sur un nouvel ordinateur, restaurez votre fichier .phonalys-backup au lieu de créer un coffre vide. Vous vous connecterez ensuite avec la phrase de passe et le second facteur en vigueur lors de la sauvegarde.":
    "Herstel op een nieuwe computer uw .phonalys-backup-bestand in plaats van een lege kluis aan te maken. U meldt zich daarna aan met de wachtzin en de tweede factor die golden bij de back-up.",
  "Fichier de sauvegarde": "Back-upbestand",
  "Restauration…": "Herstellen…",
  "Restaurer cette sauvegarde": "Deze back-up herstellen",
  // Réglages
  "Langue de l'interface": "Taal van de interface",
  "Propre à cet appareil. Les exercices et les lexiques restent en français.":
    "Eigen aan dit toestel. De oefeningen en woordenlijsten blijven in het Frans.",
  "Verrouillage automatique": "Automatische vergrendeling",
  "Sans activité (clavier, souris, écran tactile) pendant ce délai, Phonalys se verrouille et efface les clés de la mémoire. Un compte à rebours apparaît deux minutes avant.":
    "Zonder activiteit (toetsenbord, muis, aanraakscherm) gedurende deze termijn vergrendelt Phonalys zich en wist het de sleutels uit het geheugen. Twee minuten vooraf verschijnt een aftelling.",
  "Délai d'inactivité": "Inactiviteitstermijn",
  "{n} minutes": "{n} minuten",
  "15 minutes (recommandé)": "15 minuten (aanbevolen)",
  "1 heure": "1 uur",
  "Application en arrière-plan": "Toepassing op de achtergrond",
  "après 1 minute": "na 1 minuut",
  "après 5 minutes (recommandé)": "na 5 minuten (aanbevolen)",
  "après 15 minutes": "na 15 minuten",
  "jamais (seul le délai d'inactivité s'applique)": "nooit (alleen de inactiviteitstermijn geldt)",
  "Fenêtre réduite, autre onglet, téléphone en veille : l'appareil se déconnecte. La mise en veille de l'ordinateur verrouille toujours tout.":
    "Geminimaliseerd venster, ander tabblad, telefoon in slaapstand: het toestel meldt zich af. De slaapstand van de computer vergrendelt altijd alles.",
  Cabinet: "Praktijk",
  "Repris en en-tête des bilans imprimés.": "Opgenomen in de koptekst van afgedrukte bilans.",
  "Nom du praticien ou du cabinet": "Naam van de zorgverlener of praktijk",
  Titre: "Titel",
  "Ex. Logopède": "Bv. Logopedist",
  "Numéro INAMI": "RIZIV-nummer",
  Adresse: "Adres",
  "Téléphone": "Telefoon",
  "E-mail": "E-mail",
  "Réglages enregistrés.": "Instellingen opgeslagen.",
  // Appairage
  "Téléphones et tablettes": "Telefoons en tablets",
  "Cette installation n'est accessible que depuis cet ordinateur. Pour connecter un téléphone, lancez le serveur du cabinet (phonalys-server) sur le réseau local : voir le guide d'installation.":
    "Deze installatie is alleen vanaf deze computer bereikbaar. Om een telefoon te verbinden, start u de praktijkserver (phonalys-server) op het lokale netwerk: zie de installatiegids.",
  "Scannez ce code avec l'appareil photo du téléphone, puis suivez les étapes affichées. L'appareil installe une seule fois l'autorité du cabinet ; elle ne vaut que pour les adresses du réseau local.":
    "Scan deze code met de camera van de telefoon en volg de getoonde stappen. Het toestel installeert eenmalig de certificaatautoriteit van de praktijk; die geldt alleen voor adressen van het lokale netwerk.",
  "QR code vers {url}": "QR-code naar {url}",
  "Adresse : {url}": "Adres: {url}",
  "Empreinte à comparer sur le téléphone :": "Vingerafdruk om op de telefoon te vergelijken:",
  "Connecter cet appareil au cabinet": "Dit toestel met de praktijk verbinden",
  "Serveur injoignable : vérifiez que l'appareil est sur le réseau du cabinet.":
    "Server onbereikbaar: controleer of het toestel op het netwerk van de praktijk zit.",
  "Ce serveur n'accepte pas d'autres appareils (installation limitée à l'ordinateur).":
    "Deze server aanvaardt geen andere toestellen (installatie beperkt tot de computer).",
  "Étape 1 : vérifiez que l'empreinte ci-dessous est identique à celle affichée sur l'ordinateur du cabinet (écran Sécurité). Si elle diffère, arrêtez-vous : quelqu'un s'interpose.":
    "Stap 1: controleer of de vingerafdruk hieronder identiek is aan die op de computer van de praktijk (scherm Beveiliging). Als ze verschilt, stop dan: iemand onderschept de verbinding.",
  "En abrégé : {fp}": "Verkort: {fp}",
  "Télécharger le profil": "Profiel downloaden",
  "Puis touchez « Autoriser ».": "Tik daarna op « Sta toe ».",
  "Réglages › Profil téléchargé › Installer (code de l'appareil demandé).":
    "Instellingen › Profiel gedownload › Installeer (toegangscode van het toestel gevraagd).",
  "Réglages › Général › Informations › Réglages des certificats : activez « Phonalys - autorité locale du cabinet ». Vérifiez-y l'empreinte SHA-256.":
    "Instellingen › Algemeen › Info › Certificaatvertrouwen: schakel « Phonalys - autorité locale du cabinet » in. Controleer er de SHA-256-vingerafdruk.",
  "Revenez ici et touchez « Continuer ».": "Kom hier terug en tik op « Doorgaan ».",
  "Télécharger le certificat": "Certificaat downloaden",
  "Paramètres › Sécurité › Chiffrement et identifiants › Installer un certificat › Certificat CA, puis choisissez « phonalys-cabinet.crt ». (Le chemin varie selon le fabricant : cherchez « certificat CA » dans les paramètres.)":
    "Instellingen › Beveiliging › Versleuteling en inloggegevens › Certificaat installeren › CA-certificaat, en kies « phonalys-cabinet.crt ». (Het pad verschilt per fabrikant: zoek « CA-certificaat » in de instellingen.)",
  "Profil pour Mac": "Profiel voor Mac",
  "Installez-le comme autorité racine de confiance du système ou du navigateur.":
    "Installeer het als vertrouwde basiscertificaatautoriteit van het systeem of de browser.",
  "Rechargez cette page : le navigateur ne doit plus afficher d'avertissement.":
    "Herlaad deze pagina: de browser mag geen waarschuwing meer tonen.",
  "Ensuite, ajoutez Phonalys à l'écran d'accueil (Partager › Sur l'écran d'accueil sur iPhone, menu ⋮ › Installer l'application sur Android) et activez la connexion biométrique dans « Sécurité ».":
    "Voeg Phonalys daarna toe aan het beginscherm (Deel › Zet op beginscherm op iPhone, menu ⋮ › App installeren op Android) en schakel biometrisch aanmelden in bij « Beveiliging ».",
};

const DE: Dict = {
  // Structure
  "Aller au contenu": "Zum Inhalt",
  "Navigation principale": "Hauptnavigation",
  Accueil: "Startseite",
  Patients: "Patienten",
  Agenda: "Terminkalender",
  Bilans: "Bilanzen",
  Exercices: "Übungen",
  Documents: "Dokumente",
  "Réglages": "Einstellungen",
  Sauvegardes: "Sicherungen",
  "Sécurité": "Sicherheit",
  Journal: "Protokoll",
  "Conformité": "Datenschutz",
  "Connexion au serveur…": "Verbindung zum Server…",
  "Serveur injoignable": "Server nicht erreichbar",
  "Vérifiez que le serveur Phonalys du cabinet est démarré et que cet appareil est sur le même réseau.":
    "Prüfen Sie, ob der Phonalys-Server der Praxis läuft und dieses Gerät im selben Netzwerk ist.",
  "Réessayer": "Erneut versuchen",
  "Connectez-vous en un geste": "Mit einer Geste anmelden",
  "Activez la connexion biométrique : chaque jour, il suffira de poser le doigt, de montrer votre visage ou d'utiliser Windows Hello. La phrase de passe ne sera redemandée qu'après trois échecs.":
    "Aktivieren Sie die biometrische Anmeldung: Täglich genügt dann Ihr Finger, Ihr Gesicht oder Windows Hello. Die Passphrase wird erst nach drei Fehlversuchen erneut verlangt.",
  "Activer maintenant": "Jetzt aktivieren",
  "Plus tard": "Später",
  "Verrouille immédiatement, sur tous les appareils": "Sperrt sofort, auf allen Geräten",
  Verrouiller: "Sperren",
  "Sans activité, Phonalys se verrouille pour protéger les dossiers. Délai réglable dans « Réglages ».":
    "Ohne Aktivität sperrt sich Phonalys, um die Akten zu schützen. Frist einstellbar unter « Einstellungen ».",
  "Verrouillage auto : {n} min": "Automatische Sperre: {n} Min.",
  "Inactif : verrouillage dans {t}": "Inaktiv: Sperre in {t}",
  "Phonalys se verrouillera dans deux minutes faute d'activité.":
    "Phonalys sperrt sich mangels Aktivität in zwei Minuten.",
  "Rester connecté": "Angemeldet bleiben",
  // Composants communs
  Masquer: "Verbergen",
  Afficher: "Anzeigen",
  Annuler: "Abbrechen",
  Continuer: "Weiter",
  "Chargement…": "Wird geladen…",
  Enregistrer: "Speichern",
  "Codes de récupération": "Wiederherstellungscodes",
  "Ces codes ne seront plus jamais affichés.": "Diese Codes werden nie wieder angezeigt.",
  "Chacun permet une seule connexion si vous perdez votre appareil. Imprimez-les ou notez-les et rangez-les hors ligne, à l'écart de votre ordinateur.":
    "Jeder Code erlaubt eine einzige Anmeldung, falls Sie Ihr Gerät verlieren. Drucken oder notieren Sie sie und bewahren Sie sie offline auf, getrennt von Ihrem Computer.",
  "J'ai mis ces codes en lieu sûr.": "Ich habe diese Codes sicher aufbewahrt.",
  // Connexion
  "Vos données sont chiffrées. Identifiez-vous pour les ouvrir.":
    "Ihre Daten sind verschlüsselt. Melden Sie sich an, um sie zu öffnen.",
  "Vérification…": "Prüfung…",
  "Se connecter avec ma clé biométrique": "Mit meinem biometrischen Schlüssel anmelden",
  "Empreinte, visage ou Windows Hello : un seul geste suffit.":
    "Fingerabdruck, Gesicht oder Windows Hello: eine Geste genügt.",
  "Utiliser une autre méthode": "Andere Methode verwenden",
  "Second facteur": "Zweiter Faktor",
  "Clé biométrique": "Biometrischer Schlüssel",
  "Code à 6 chiffres": "6-stelliger Code",
  "Phrase de passe": "Passphrase",
  "Celle choisie à l'installation, exactement (majuscules, accents et espaces comptent).":
    "Genau die bei der Installation gewählte (Groß- und Kleinschreibung, Akzente und Leerzeichen zählen).",
  "Code de l'application d'authentification": "Code der Authentifizierungs-App",
  "Code de récupération": "Wiederherstellungscode",
  "Format XXXX-XXXX-XXXX-XXXX. Chaque code ne sert qu'une fois et débloque aussi une connexion bloquée.":
    "Format XXXX-XXXX-XXXX-XXXX. Jeder Code gilt nur einmal und entsperrt auch eine gesperrte Anmeldung.",
  "Après la phrase de passe, votre appareil demande l'empreinte, le visage ou le code Windows Hello. La clé sert à chaque connexion, sans limite.":
    "Nach der Passphrase verlangt Ihr Gerät Fingerabdruck, Gesicht oder Windows-Hello-PIN. Der Schlüssel dient bei jeder Anmeldung, ohne Begrenzung.",
  "Les clés biométriques exigent une connexion HTTPS avec un nom de domaine et un certificat reconnu.":
    "Biometrische Schlüssel erfordern eine HTTPS-Verbindung mit Domainnamen und anerkanntem Zertifikat.",
  "Débloquer avec un code de récupération": "Mit einem Wiederherstellungscode entsperren",
  "Continuer avec ma clé": "Mit meinem Schlüssel fortfahren",
  "Déverrouiller": "Entsperren",
  "Revenir à la connexion en un geste": "Zurück zur Anmeldung mit einer Geste",
  "Aucune clé biométrique n'est encore enregistrée. Connectez-vous avec le code à 6 chiffres de votre application d'authentification ; vous pourrez ensuite activer l'empreinte, le visage ou Windows Hello dans « Sécurité ».":
    "Es ist noch kein biometrischer Schlüssel registriert. Melden Sie sich mit dem 6-stelligen Code Ihrer Authentifizierungs-App an; danach können Sie Fingerabdruck, Gesicht oder Windows Hello unter « Sicherheit » aktivieren.",
  "Après 3 échecs, deux vérifications sont demandées pour protéger les dossiers : votre phrase de passe, puis votre clé biométrique ou le code à 6 chiffres.":
    "Nach 3 Fehlversuchen sind zum Schutz der Akten zwei Prüfungen nötig: Ihre Passphrase, dann Ihr biometrischer Schlüssel oder der 6-stellige Code.",
  "Cet appareil ne fournit pas l'extension PRF : utilisez une autre méthode.":
    "Dieses Gerät unterstützt die PRF-Erweiterung nicht: Verwenden Sie eine andere Methode.",
  "Ce coffre est lié au matériel, mais cet appareil ou ce navigateur ne fournit pas l'extension PRF. Utilisez une clé liée ou un code de récupération.":
    "Dieser Tresor ist an Hardware gebunden, aber dieses Gerät oder dieser Browser unterstützt die PRF-Erweiterung nicht. Verwenden Sie einen gebundenen Schlüssel oder einen Wiederherstellungscode.",
  // Installation
  "Installation de Phonalys": "Installation von Phonalys",
  "Vos données restent sur cet appareil, chiffrées. Personne d'autre, pas même l'éditeur, ne peut les lire.":
    "Ihre Daten bleiben verschlüsselt auf diesem Gerät. Niemand sonst, nicht einmal der Hersteller, kann sie lesen.",
  "Jeton d'installation": "Installationstoken",
  "Affiché dans la console au démarrage du serveur.": "Wird beim Serverstart in der Konsole angezeigt.",
  "Votre nom (facultatif)": "Ihr Name (optional)",
  "Au moins 14 caractères ; idéalement 5 mots ou plus. Un gestionnaire de mots de passe (Bitwarden, KeePassXC) peut la générer et la retenir.":
    "Mindestens 14 Zeichen; idealerweise 5 Wörter oder mehr. Ein Passwortmanager (Bitwarden, KeePassXC) kann sie erzeugen und speichern.",
  Confirmation: "Bestätigung",
  "Si votre navigateur propose de générer ou d'enregistrer un mot de passe, vérifiez avec « Afficher » que c'est bien celui que vous retiendrez.":
    "Wenn Ihr Browser anbietet, ein Passwort zu erzeugen oder zu speichern, prüfen Sie mit « Anzeigen », dass es das ist, das Sie sich merken werden.",
  "Sans cette phrase de passe, le coffre est définitivement illisible. Il n'existe aucune réinitialisation.":
    "Ohne diese Passphrase ist der Tresor endgültig unlesbar. Es gibt keine Zurücksetzung.",
  "Création du coffre…": "Tresor wird erstellt…",
  "Créer le coffre": "Tresor erstellen",
  "Les deux phrases de passe sont différentes.": "Die beiden Passphrasen sind verschieden.",
  "Scannez ce code avec une application d'authentification hors ligne (Aegis, 2FAS, Ente Auth) ou avec votre gestionnaire de mots de passe (Bitwarden, KeePassXC).":
    "Scannen Sie diesen Code mit einer Offline-Authentifizierungs-App (Aegis, 2FAS, Ente Auth) oder mit Ihrem Passwortmanager (Bitwarden, KeePassXC).",
  "QR code du second facteur": "QR-Code des zweiten Faktors",
  "Saisie manuelle": "Manuelle Eingabe",
  "Après la première connexion, vous pourrez ajouter une clé biométrique.":
    "Nach der ersten Anmeldung können Sie einen biometrischen Schlüssel hinzufügen.",
  "C'est fait": "Erledigt",
  "Vous avez déjà une sauvegarde ?": "Haben Sie bereits eine Sicherung?",
  "Sur un nouvel ordinateur, restaurez votre fichier .phonalys-backup au lieu de créer un coffre vide. Vous vous connecterez ensuite avec la phrase de passe et le second facteur en vigueur lors de la sauvegarde.":
    "Stellen Sie auf einem neuen Computer Ihre .phonalys-backup-Datei wieder her, statt einen leeren Tresor zu erstellen. Sie melden sich danach mit der Passphrase und dem zweiten Faktor an, die zum Zeitpunkt der Sicherung galten.",
  "Fichier de sauvegarde": "Sicherungsdatei",
  "Restauration…": "Wiederherstellung…",
  "Restaurer cette sauvegarde": "Diese Sicherung wiederherstellen",
  // Réglages
  "Langue de l'interface": "Sprache der Oberfläche",
  "Propre à cet appareil. Les exercices et les lexiques restent en français.":
    "Gilt für dieses Gerät. Übungen und Wortlisten bleiben auf Französisch.",
  "Verrouillage automatique": "Automatische Sperre",
  "Sans activité (clavier, souris, écran tactile) pendant ce délai, Phonalys se verrouille et efface les clés de la mémoire. Un compte à rebours apparaît deux minutes avant.":
    "Ohne Aktivität (Tastatur, Maus, Touchscreen) während dieser Frist sperrt sich Phonalys und löscht die Schlüssel aus dem Speicher. Zwei Minuten vorher erscheint ein Countdown.",
  "Délai d'inactivité": "Inaktivitätsfrist",
  "{n} minutes": "{n} Minuten",
  "15 minutes (recommandé)": "15 Minuten (empfohlen)",
  "1 heure": "1 Stunde",
  "Application en arrière-plan": "Anwendung im Hintergrund",
  "après 1 minute": "nach 1 Minute",
  "après 5 minutes (recommandé)": "nach 5 Minuten (empfohlen)",
  "après 15 minutes": "nach 15 Minuten",
  "jamais (seul le délai d'inactivité s'applique)": "nie (nur die Inaktivitätsfrist gilt)",
  "Fenêtre réduite, autre onglet, téléphone en veille : l'appareil se déconnecte. La mise en veille de l'ordinateur verrouille toujours tout.":
    "Minimiertes Fenster, anderer Tab, Telefon im Ruhezustand: Das Gerät meldet sich ab. Der Ruhezustand des Computers sperrt immer alles.",
  Cabinet: "Praxis",
  "Repris en en-tête des bilans imprimés.": "Erscheint in der Kopfzeile gedruckter Bilanzen.",
  "Nom du praticien ou du cabinet": "Name der Fachkraft oder der Praxis",
  Titre: "Titel",
  "Ex. Logopède": "z. B. Logopäde",
  "Numéro INAMI": "LIKIV-Nummer",
  Adresse: "Adresse",
  "Téléphone": "Telefon",
  "E-mail": "E-Mail",
  "Réglages enregistrés.": "Einstellungen gespeichert.",
  // Appairage
  "Téléphones et tablettes": "Telefone und Tablets",
  "Cette installation n'est accessible que depuis cet ordinateur. Pour connecter un téléphone, lancez le serveur du cabinet (phonalys-server) sur le réseau local : voir le guide d'installation.":
    "Diese Installation ist nur von diesem Computer aus erreichbar. Um ein Telefon zu verbinden, starten Sie den Praxisserver (phonalys-server) im lokalen Netzwerk: siehe Installationsanleitung.",
  "Scannez ce code avec l'appareil photo du téléphone, puis suivez les étapes affichées. L'appareil installe une seule fois l'autorité du cabinet ; elle ne vaut que pour les adresses du réseau local.":
    "Scannen Sie diesen Code mit der Kamera des Telefons und folgen Sie den angezeigten Schritten. Das Gerät installiert einmalig die Zertifizierungsstelle der Praxis; sie gilt nur für Adressen im lokalen Netzwerk.",
  "QR code vers {url}": "QR-Code zu {url}",
  "Adresse : {url}": "Adresse: {url}",
  "Empreinte à comparer sur le téléphone :": "Auf dem Telefon zu vergleichender Fingerabdruck:",
  "Connecter cet appareil au cabinet": "Dieses Gerät mit der Praxis verbinden",
  "Serveur injoignable : vérifiez que l'appareil est sur le réseau du cabinet.":
    "Server nicht erreichbar: Prüfen Sie, ob das Gerät im Netzwerk der Praxis ist.",
  "Ce serveur n'accepte pas d'autres appareils (installation limitée à l'ordinateur).":
    "Dieser Server akzeptiert keine weiteren Geräte (Installation auf den Computer beschränkt).",
  "Étape 1 : vérifiez que l'empreinte ci-dessous est identique à celle affichée sur l'ordinateur du cabinet (écran Sécurité). Si elle diffère, arrêtez-vous : quelqu'un s'interpose.":
    "Schritt 1: Prüfen Sie, ob der Fingerabdruck unten mit dem auf dem Praxiscomputer (Bildschirm Sicherheit) identisch ist. Weicht er ab, brechen Sie ab: Jemand schaltet sich dazwischen.",
  "En abrégé : {fp}": "Kurzform: {fp}",
  "Télécharger le profil": "Profil herunterladen",
  "Puis touchez « Autoriser ».": "Tippen Sie dann auf « Zulassen ».",
  "Réglages › Profil téléchargé › Installer (code de l'appareil demandé).":
    "Einstellungen › Profil geladen › Installieren (Gerätecode wird verlangt).",
  "Réglages › Général › Informations › Réglages des certificats : activez « Phonalys - autorité locale du cabinet ». Vérifiez-y l'empreinte SHA-256.":
    "Einstellungen › Allgemein › Info › Zertifikatsvertrauenseinstellungen: Aktivieren Sie « Phonalys - autorité locale du cabinet ». Prüfen Sie dort den SHA-256-Fingerabdruck.",
  "Revenez ici et touchez « Continuer ».": "Kehren Sie hierher zurück und tippen Sie auf « Weiter ».",
  "Télécharger le certificat": "Zertifikat herunterladen",
  "Paramètres › Sécurité › Chiffrement et identifiants › Installer un certificat › Certificat CA, puis choisissez « phonalys-cabinet.crt ». (Le chemin varie selon le fabricant : cherchez « certificat CA » dans les paramètres.)":
    "Einstellungen › Sicherheit › Verschlüsselung und Anmeldedaten › Zertifikat installieren › CA-Zertifikat, dann « phonalys-cabinet.crt » wählen. (Der Pfad variiert je nach Hersteller: Suchen Sie in den Einstellungen nach « CA-Zertifikat ».)",
  "Profil pour Mac": "Profil für Mac",
  "Installez-le comme autorité racine de confiance du système ou du navigateur.":
    "Installieren Sie es als vertrauenswürdige Stammzertifizierungsstelle des Systems oder Browsers.",
  "Rechargez cette page : le navigateur ne doit plus afficher d'avertissement.":
    "Laden Sie diese Seite neu: Der Browser darf keine Warnung mehr anzeigen.",
  "Ensuite, ajoutez Phonalys à l'écran d'accueil (Partager › Sur l'écran d'accueil sur iPhone, menu ⋮ › Installer l'application sur Android) et activez la connexion biométrique dans « Sécurité ».":
    "Fügen Sie Phonalys danach zum Startbildschirm hinzu (Teilen › Zum Home-Bildschirm auf dem iPhone, Menü ⋮ › App installieren auf Android) und aktivieren Sie die biometrische Anmeldung unter « Sicherheit ».",
};

const DICT: Record<Exclude<Lang, "fr">, Dict> = { nl: NL, de: DE };

/** Pour les tests : les dictionnaires bruts. */
export const DICTIONARIES = DICT;
