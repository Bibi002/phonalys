// Listes de mots originales des exercices Phonalys (aucune liste issue d'un test édité).
// Pour le comptage syllabique, seuls des mots sans « e » muet final sont retenus :
// leur nombre de syllabes orales ne prête pas à discussion.

export const SYLLABES: readonly (readonly [string, number])[] = [
  ["chat", 1], ["pain", 1], ["train", 1], ["loup", 1], ["sac", 1], ["lit", 1], ["bus", 1], ["nez", 1], ["four", 1], ["jus", 1], ["bol", 1], ["mur", 1],
  ["lapin", 2], ["maison", 2], ["bateau", 2], ["gâteau", 2], ["ballon", 2], ["vélo", 2], ["moto", 2], ["robot", 2], ["citron", 2], ["oiseau", 2], ["soleil", 2], ["tambour", 2], ["cadeau", 2], ["chapeau", 2], ["mouton", 2], ["poisson", 2],
  ["éléphant", 3], ["chocolat", 3], ["kangourou", 3], ["domino", 3], ["papillon", 3], ["pantalon", 3], ["ananas", 3], ["abricot", 3], ["escargot", 3], ["canapé", 3], ["hérisson", 3], ["pyjama", 3], ["animal", 3],
  ["ordinateur", 4], ["alligator", 4], ["macaroni", 4], ["harmonica", 4], ["caméléon", 4], ["aspirateur", 4],
  ["réfrigérateur", 5], ["électricité", 5], ["université", 5],
];

/** Paires de mots et leur statut de rime (même son final). */
export const RIMES: readonly (readonly [string, string, boolean])[] = [
  ["chat", "rat", true], ["bateau", "gâteau", true], ["lapin", "sapin", true], ["ballon", "bouton", true],
  ["souris", "radis", true], ["soleil", "réveil", true], ["moto", "vélo", true], ["poisson", "maison", true],
  ["chapeau", "râteau", true], ["genou", "hibou", true], ["canard", "foulard", true], ["mouton", "melon", true],
  ["bonbon", "ballon", true], ["matin", "jardin", true],
  ["chat", "loup", false], ["bateau", "lapin", false], ["soleil", "poisson", false], ["souris", "canard", false],
  ["genou", "maison", false], ["vélo", "sapin", false], ["chapeau", "radis", false], ["mouton", "réveil", false],
  ["hibou", "foulard", false], ["melon", "radis", false], ["jardin", "bouton", false], ["matin", "moto", false],
];

export const CATEGORIES: Readonly<Record<string, readonly string[]>> = {
  Fruits: ["pomme", "banane", "poire", "cerise", "fraise", "orange", "kiwi", "ananas", "raisin", "prune"],
  Animaux: ["chien", "lapin", "cheval", "vache", "lion", "girafe", "souris", "cochon", "tigre", "mouton"],
  Vêtements: ["pull", "pantalon", "chemise", "robe", "chaussette", "manteau", "écharpe", "bonnet", "jupe", "veste"],
  Transports: ["voiture", "vélo", "train", "avion", "bateau", "bus", "moto", "camion", "tram", "fusée"],
  Meubles: ["table", "chaise", "lit", "armoire", "canapé", "bureau", "étagère", "fauteuil", "commode", "tabouret"],
};

/** Mots fréquents pour la lecture flash, du plus court au plus long. */
export const MOTS_FLASH: readonly (readonly string[])[] = [
  ["lit", "sac", "bol", "mur", "jus", "nid", "car", "vélo", "lune", "rose", "page", "lama"],
  ["table", "pomme", "lapin", "école", "porte", "livre", "tasse", "jardin", "cheval", "bateau", "chemin", "nuage"],
  ["maison", "fenêtre", "voiture", "poisson", "cadeau", "soleil", "montagne", "oiseau", "crayon", "tortue", "dimanche", "semaine"],
  ["cartable", "papillon", "chocolat", "histoire", "trottinette", "boulangerie", "parapluie", "anniversaire", "dinosaure", "bibliothèque", "ordinateur", "téléphone"],
];

/** Lettres visuellement proches, sources des pseudo-mots distracteurs. */
const VOISINES: Readonly<Record<string, string>> = {
  b: "dpq", d: "bqp", p: "qbd", q: "pdb", m: "n", n: "mu", u: "nv", a: "oe", o: "ae", e: "ca", c: "eo",
  l: "ti", t: "lf", f: "t", i: "l", r: "n", s: "z", v: "u", g: "q", h: "n",
};

/** Pseudo-mots proches de `mot` (substitution, inversion, omission), tous distincts. */
export function distracteurs(mot: string, n: number, rand: () => number = Math.random): string[] {
  const out = new Set<string>();
  const letters = [...mot];
  for (let guard = 0; out.size < n && guard < 200; guard++) {
    const i = Math.floor(rand() * letters.length);
    const kind = Math.floor(rand() * 3);
    const copy = [...letters];
    const ch = copy[i] ?? "";
    if (kind === 0 && VOISINES[ch]) {
      const opts = VOISINES[ch] ?? "";
      copy[i] = opts[Math.floor(rand() * opts.length)] ?? ch;
    } else if (kind === 1 && i < copy.length - 1) {
      copy[i] = copy[i + 1] ?? "";
      copy[i + 1] = ch;
    } else if (kind === 2 && copy.length > 3) {
      copy.splice(i, 1);
    }
    const w = copy.join("");
    if (w !== mot) out.add(w);
  }
  return [...out];
}

export function shuffle<T>(list: readonly T[], rand: () => number = Math.random): T[] {
  const a = [...list];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [a[i], a[j]] = [a[j] as T, a[i] as T];
  }
  return a;
}
