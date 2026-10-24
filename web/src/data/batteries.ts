// Catalogue de batteries de référence, pour la saisie des résultats.
//
// Phonalys ne reproduit AUCUN contenu de ces tests (items, consignes, normes) : ils
// appartiennent à leurs éditeurs. Le praticien les fait passer avec son matériel sous
// licence, les cote avec le manuel ou le logiciel de l'éditeur, puis reporte ici les
// scores obtenus. Seuls le nom de la batterie et son éditeur sont repris, à titre de
// référence. Les épreuves sont nommées par le praticien lui-même.

export interface BatteryRef {
  id: string;
  nom: string;
  editeur: string;
  public: "enfant" | "adulte" | "tous";
  domaines: string;
}

export const BATTERIES: readonly BatteryRef[] = [
  { id: "evaleo-6-15", nom: "EVALEO 6-15", editeur: "Ortho Édition", public: "enfant", domaines: "Langage oral et écrit" },
  { id: "evalo-2-6", nom: "EVALO 2-6", editeur: "Ortho Édition", public: "enfant", domaines: "Langage oral, développement" },
  { id: "exalang-3-6", nom: "EXALang 3-6", editeur: "HappyNeuron", public: "enfant", domaines: "Langage oral" },
  { id: "exalang-5-8", nom: "EXALang 5-8", editeur: "HappyNeuron", public: "enfant", domaines: "Langage oral et écrit" },
  { id: "exalang-8-11", nom: "EXALang 8-11", editeur: "HappyNeuron", public: "enfant", domaines: "Langage oral et écrit" },
  { id: "exalang-11-15", nom: "EXALang 11-15", editeur: "HappyNeuron", public: "enfant", domaines: "Langage oral et écrit" },
  { id: "examath-8-15", nom: "Examath 8-15", editeur: "HappyNeuron", public: "enfant", domaines: "Cognition mathématique" },
  { id: "tedi-math", nom: "TEDI-MATH", editeur: "ECPA", public: "enfant", domaines: "Cognition mathématique" },
  { id: "elo", nom: "ELO (Évaluation du langage oral)", editeur: "ECPA", public: "enfant", domaines: "Langage oral" },
  { id: "n-eel", nom: "N-EEL", editeur: "ECPA", public: "enfant", domaines: "Langage oral" },
  { id: "bilo", nom: "BILO", editeur: "ECPA", public: "enfant", domaines: "Langage oral et écrit" },
  { id: "l2ma-2", nom: "L2MA-2", editeur: "ECPA", public: "enfant", domaines: "Langage, mémoire, attention" },
  { id: "alouette-r", nom: "Alouette-R", editeur: "ECPA", public: "tous", domaines: "Lecture" },
  { id: "bale", nom: "BALE", editeur: "Cogni-Sciences", public: "enfant", domaines: "Langage écrit" },
  { id: "odedys-2", nom: "ODEDYS-2", editeur: "Cogni-Sciences", public: "enfant", domaines: "Dépistage des troubles du langage écrit" },
  { id: "belec", nom: "BELEC", editeur: "Mousty, Leybaert et coll. (Belgique)", public: "enfant", domaines: "Lecture et orthographe" },
  { id: "ecosse", nom: "E.CO.S.SE", editeur: "Presses universitaires du Septentrion", public: "enfant", domaines: "Compréhension syntaxico-sémantique" },
  { id: "evip", nom: "EVIP", editeur: "Psycan", public: "enfant", domaines: "Vocabulaire réceptif" },
  { id: "mt-86", nom: "MT-86", editeur: "Ortho Édition", public: "adulte", domaines: "Aphasie" },
  { id: "betl", nom: "BETL", editeur: "Ortho Édition", public: "adulte", domaines: "Lexique (adulte)" },
  { id: "gremots", nom: "GREMOTS", editeur: "De Boeck Supérieur", public: "adulte", domaines: "Langage dans les pathologies neurodégénératives" },
];

export const OTHER_BATTERY = "autre";

export function batteryName(id: string | undefined, free: string | undefined): string {
  if (!id || id === OTHER_BATTERY) return free?.trim() || "Épreuves libres";
  return BATTERIES.find((b) => b.id === id)?.nom ?? (free || id);
}
