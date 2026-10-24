// Modèle du dossier clinique. Tout est chiffré par le serveur, enregistrement par
// enregistrement ; rien n'est conservé dans le navigateur.

export interface Contact {
  lien: string;
  nom: string;
  telephone: string;
  email: string;
}

export interface PriseEnCharge {
  prescripteur?: string;
  date_prescription?: string;
  date_accord?: string;
  seances_accordees?: string;
  mutuelle?: string;
  remarques?: string;
}

export type PatientStatut = "actif" | "attente" | "termine";

export interface Patient {
  id: string;
  nom?: string;
  prenom?: string;
  naissance?: string;
  sexe?: string;
  langues?: string;
  ecole?: string;
  classe?: string;
  medecin?: string;
  motif?: string;
  anamnese?: string;
  notes?: string;
  statut?: PatientStatut;
  contacts?: Contact[];
  prise_en_charge?: PriseEnCharge;
}

export type SeanceStatut = "prevue" | "realisee" | "annulee" | "absent";

export interface Seance {
  id?: string;
  patient_id: string;
  date: string;
  heure?: string;
  duree?: number;
  lieu?: string;
  statut: SeanceStatut;
  contenu?: string;
  observations?: string;
  suite?: string;
}

export type Mesure = "z" | "percentile" | "note_standard" | "note_composite" | "aucune";

export interface Resultat {
  epreuve: string;
  domaine?: string;
  brut?: string;
  mesure: Mesure;
  valeur?: string;
  commentaire?: string;
}

export type BilanType = "initial" | "evolution" | "fin";

export interface Bilan {
  id?: string;
  patient_id: string;
  date: string;
  type: BilanType;
  batterie?: string;
  batterie_nom?: string;
  demande?: string;
  observations?: string;
  resultats: Resultat[];
  conclusion?: string;
  projet?: string;
}

export type ObjectifStatut = "en_cours" | "atteint" | "suspendu";

export interface Objectif {
  id?: string;
  patient_id: string;
  domaine: string;
  intitule: string;
  critere?: string;
  debut?: string;
  echeance?: string;
  statut: ObjectifStatut;
  progression: number;
}

export interface ExerciceResultat {
  id?: string;
  patient_id: string;
  exercice: string;
  date: string;
  niveau: number;
  score: number;
  total: number;
  duree_s: number;
  details?: string;
}

// --- Libellés --------------------------------------------------------------------------

export const PATIENT_STATUT: Record<PatientStatut, string> = {
  actif: "En suivi",
  attente: "Liste d'attente",
  termine: "Suivi terminé",
};

export const SEANCE_STATUT: Record<SeanceStatut, string> = {
  prevue: "Prévue",
  realisee: "Réalisée",
  annulee: "Annulée",
  absent: "Absence",
};

export const BILAN_TYPE: Record<BilanType, string> = {
  initial: "Bilan initial",
  evolution: "Bilan d'évolution",
  fin: "Bilan de fin de prise en charge",
};

export const MESURE: Record<Mesure, string> = {
  z: "Écart-type (z)",
  percentile: "Percentile",
  note_standard: "Note standard (moyenne 10, ET 3)",
  note_composite: "Score composite (moyenne 100, ET 15)",
  aucune: "Pas de score normé",
};

export const OBJECTIF_STATUT: Record<ObjectifStatut, string> = {
  en_cours: "En cours",
  atteint: "Atteint",
  suspendu: "Suspendu",
};

export const DOMAINES = [
  "Langage oral : phonologie",
  "Langage oral : lexique",
  "Langage oral : morphosyntaxe",
  "Langage oral : pragmatique",
  "Métaphonologie",
  "Lecture",
  "Orthographe",
  "Production écrite",
  "Mémoire",
  "Attention",
  "Fonctions exécutives",
  "Cognition mathématique",
  "Fluence (bégaiement)",
  "Voix",
  "Déglutition / oromyofonctionnel",
  "Autre",
] as const;

// --- Utilitaires ------------------------------------------------------------------------

export function fullName(p: Pick<Patient, "nom" | "prenom"> | undefined): string {
  if (!p) return "(patient effacé)";
  return [p.nom?.toUpperCase(), p.prenom].filter(Boolean).join(" ") || "(sans nom)";
}

/** Date du jour au format AAAA-MM-JJ, en heure locale. */
export function today(d = new Date()): string {
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${m}-${day}`;
}

export function formatDate(iso: string | undefined): string {
  if (!iso) return "";
  const [y, m, d] = iso.split("-").map(Number);
  if (!y || !m || !d) return iso;
  return new Date(y, m - 1, d).toLocaleDateString("fr-BE", { day: "numeric", month: "long", year: "numeric" });
}

/** Âge en années et mois à une date donnée, ex. « 8 ans 4 mois ». */
export function age(naissance: string | undefined, at: string = today()): string {
  const months = ageMonths(naissance, at);
  if (months === undefined) return "";
  const y = Math.floor(months / 12);
  const m = months % 12;
  return `${y} an${y > 1 ? "s" : ""}${m ? ` ${m} mois` : ""}`;
}

export function ageMonths(naissance: string | undefined, at: string = today()): number | undefined {
  if (!naissance) return undefined;
  const [by, bm, bd] = naissance.split("-").map(Number);
  const [ay, am, ad] = at.split("-").map(Number);
  if (!by || !bm || !bd || !ay || !am || !ad) return undefined;
  let months = (ay - by) * 12 + (am - bm);
  if (ad < bd) months -= 1;
  return months >= 0 ? months : undefined;
}

// --- Scores normés : conversions purement mathématiques -----------------------------------

/** Fonction de répartition de la loi normale centrée réduite (Abramowitz et Stegun 7.1.26). */
export function normalCdf(z: number): number {
  const t = 1 / (1 + 0.3275911 * Math.abs(z) / Math.SQRT2);
  const poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
  const erf = 1 - poly * Math.exp(-(z * z) / 2);
  return z >= 0 ? (1 + erf) / 2 : (1 - erf) / 2;
}

/** Inverse de la loi normale (algorithme d'Acklam), pour 0 < p < 1. */
export function normalInv(p: number): number {
  const a = [-39.69683028665376, 220.9460984245205, -275.9285104469687, 138.357751867269, -30.66479806614716, 2.506628277459239];
  const b = [-54.47609879822406, 161.5858368580409, -155.6989798598866, 66.80131188771972, -13.28068155288572];
  const c = [-0.007784894002430293, -0.3223964580411365, -2.400758277161838, -2.549732539343734, 4.374664141464968, 2.938163982698783];
  const d = [0.007784695709041462, 0.3224671290700398, 2.445134137142996, 3.754408661907416];
  const [a0, a1, a2, a3, a4, a5] = a as [number, number, number, number, number, number];
  const [b0, b1, b2, b3, b4] = b as [number, number, number, number, number];
  const [c0, c1, c2, c3, c4, c5] = c as [number, number, number, number, number, number];
  const [d0, d1, d2, d3] = d as [number, number, number, number];
  const low = 0.02425;
  if (p < low) {
    const q = Math.sqrt(-2 * Math.log(p));
    return (((((c0 * q + c1) * q + c2) * q + c3) * q + c4) * q + c5) / ((((d0 * q + d1) * q + d2) * q + d3) * q + 1);
  }
  if (p > 1 - low) {
    const q = Math.sqrt(-2 * Math.log(1 - p));
    return -(((((c0 * q + c1) * q + c2) * q + c3) * q + c4) * q + c5) / ((((d0 * q + d1) * q + d2) * q + d3) * q + 1);
  }
  const q = p - 0.5;
  const r = q * q;
  return ((((((a0 * r + a1) * r + a2) * r + a3) * r + a4) * r + a5) * q) /
    (((((b0 * r + b1) * r + b2) * r + b3) * r + b4) * r + 1);
}

/**
 * Score exprimé en écart-type (z), à partir de la mesure saisie par le praticien.
 * Phonalys ne calcule aucune norme : il convertit seulement les échelles usuelles.
 */
export function toZ(mesure: Mesure, valeur: string | undefined): number | undefined {
  const v = Number(String(valeur ?? "").replace(",", "."));
  if (valeur === undefined || valeur.trim() === "" || !Number.isFinite(v)) return undefined;
  switch (mesure) {
    case "z":
      return v;
    case "note_standard":
      return (v - 10) / 3;
    case "note_composite":
      return (v - 100) / 15;
    case "percentile":
      return v > 0 && v < 100 ? normalInv(v / 100) : undefined;
    case "aucune":
      return undefined;
  }
}
