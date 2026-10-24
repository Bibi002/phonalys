// Contenus originaux des exercices Phonalys, rédigés pour l'application (aucun item
// repris d'un test ou d'un logiciel édité).

/** Mots classés par son initial, écrit avec sa graphie la plus simple. */
export const SONS_INITIAUX: Readonly<Record<string, readonly string[]>> = {
  p: ["pomme", "poisson", "pantalon", "pirate"],
  b: ["banane", "bateau", "ballon", "bonbon"],
  t: ["tortue", "table", "tapis", "tomate"],
  d: ["domino", "dent", "dauphin", "dragon"],
  f: ["fusée", "fleur", "four", "fourmi"],
  v: ["vélo", "vache", "valise", "voiture"],
  s: ["sapin", "soleil", "souris", "salade"],
  ch: ["cheval", "chapeau", "chocolat", "chemise"],
  j: ["jardin", "jupe", "journal", "judo"],
  l: ["lapin", "lune", "lion", "livre"],
  m: ["maison", "mouton", "melon", "moto"],
  n: ["nuage", "nid", "neige", "navet"],
  r: ["robot", "radis", "renard", "raisin"],
};

/** Sons proches, souvent confondus (sourde et sonore, lieu d'articulation voisin). */
export const SONS_PROCHES: Readonly<Record<string, readonly string[]>> = {
  p: ["b", "t"],
  b: ["p", "d"],
  t: ["d", "p"],
  d: ["t", "b"],
  f: ["v", "s"],
  v: ["f", "b"],
  s: ["ch", "f"],
  ch: ["j", "s"],
  j: ["ch", "s"],
  l: ["r", "n"],
  m: ["n", "b"],
  n: ["m", "l"],
  r: ["l", "j"],
};

/** Suppression syllabique : [mot, syllabe enlevée, ce qui reste à l'oral]. */
export const SUPPRESSIONS: readonly (readonly (readonly [string, string, string])[])[] = [
  // Niveau 1 : on enlève le début du mot.
  [
    ["lapin", "la", "pin"],
    ["chapeau", "cha", "peau"],
    ["poisson", "pois", "son"],
    ["souris", "sou", "riz"],
    ["cadeau", "ca", "dos"],
    ["bonjour", "bon", "jour"],
    ["pinceau", "pin", "seau"],
    ["parapluie", "para", "pluie"],
    ["tournevis", "tourne", "vis"],
    ["chameau", "cha", "mot"],
  ],
  // Niveau 2 : on enlève la fin du mot.
  [
    ["lapin", "pin", "la"],
    ["chapeau", "peau", "chat"],
    ["poisson", "son", "pois"],
    ["maison", "son", "mai"],
    ["souris", "ris", "sous"],
    ["bateau", "teau", "bas"],
    ["radis", "dis", "rat"],
    ["pinceau", "ceau", "pin"],
    ["bonjour", "jour", "bon"],
    ["mouton", "ton", "mou"],
    ["couteau", "teau", "cou"],
  ],
];

/** Contraires, du plus concret au plus abstrait. */
export const CONTRAIRES: readonly (readonly (readonly [string, string])[])[] = [
  [
    ["grand", "petit"], ["chaud", "froid"], ["jour", "nuit"], ["haut", "bas"], ["ouvert", "fermé"], ["plein", "vide"],
    ["propre", "sale"], ["long", "court"], ["dur", "mou"], ["jeune", "vieux"], ["rapide", "lent"], ["lourd", "léger"],
  ],
  [
    ["monter", "descendre"], ["entrer", "sortir"], ["gagner", "perdre"], ["allumer", "éteindre"], ["devant", "derrière"],
    ["dessus", "dessous"], ["toujours", "jamais"], ["rire", "pleurer"], ["acheter", "vendre"], ["mouillé", "sec"],
    ["facile", "difficile"], ["gentil", "méchant"], ["avant", "après"], ["remplir", "vider"],
  ],
];

/** Compréhension : phrases à juger vraies ou fausses, de difficulté croissante. */
export const PHRASES: readonly (readonly (readonly [string, boolean])[])[] = [
  [
    ["Le poisson nage dans l'eau.", true],
    ["La vache vole dans le ciel.", false],
    ["On met des chaussettes aux pieds.", true],
    ["Le soleil brille la nuit.", false],
    ["On boit dans un verre.", true],
    ["Un chat a quatre pattes.", true],
    ["La neige est chaude.", false],
    ["On dort dans un lit.", true],
    ["Les oiseaux ont des roues.", false],
    ["Une pomme est un fruit.", true],
    ["Le feu est froid.", false],
    ["On lit un livre.", true],
  ],
  [
    ["Un éléphant n'est pas plus petit qu'une souris.", true],
    ["Une souris est plus grande qu'un cheval.", false],
    ["Le lundi vient après le dimanche.", true],
    ["On ne peut pas manger une chaise.", true],
    ["Il ne fait jamais froid en hiver.", false],
    ["Une heure est plus longue qu'une minute.", true],
    ["Un bébé est plus âgé que son grand-père.", false],
    ["Une semaine compte sept jours.", true],
    ["Une voiture roule moins vite qu'un escargot.", false],
    ["On n'a pas besoin d'eau pour nager.", false],
  ],
  [
    ["La souris est mangée par le chat : c'est le chat qui mange.", true],
    ["Le garçon que la fille pousse tombe : c'est la fille qui tombe.", false],
    ["Avant de se coucher, Léo se brosse les dents : il se les brosse après s'être couché.", false],
    ["Le livre que Paul prête à Marie est rouge : c'est Marie qui prête le livre.", false],
    ["Le chien est lavé par Tom : c'est Tom qui lave.", true],
    ["Sophie mange après avoir joué : elle joue d'abord.", true],
    ["Le ballon que le chien rapporte est vert : c'est le chien qui rapporte le ballon.", true],
    ["S'il pleut, Zoé prend son parapluie. Il pleut : Zoé prend son parapluie.", true],
    ["Pierre est plus grand que Max, et Max est plus grand que Lou : Lou est le plus grand.", false],
    ["Ce n'est pas faux de dire que la glace est froide.", true],
  ],
];

/** Choix proposés pour un résultat de calcul : la réponse et des erreurs plausibles. */
export function nearbyAnswers(answer: number, count: number, rand: () => number = Math.random): number[] {
  const candidates = [answer + 1, answer - 1, answer + 2, answer - 2, answer + 10, answer - 10, answer + 3];
  const out = new Set<number>();
  for (const c of candidates.sort(() => rand() - 0.5)) {
    if (out.size >= count) break;
    if (c >= 0 && c !== answer) out.add(c);
  }
  return [...out];
}
