import { afterEach, describe, expect, it } from "vitest";
import { DICTIONARIES, setLang, t } from "./i18n";

// Tout le code de l'interface, en texte brut.
const sources = import.meta.glob(["./**/*.tsx", "./**/*.ts", "!./**/*.test.*", "!./i18n.ts"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** Chaînes littérales passées à `t("…")`. */
function calls(): Set<string> {
  const found = new Set<string>();
  const re = /\bt\(\s*"((?:[^"\\]|\\.)*)"/g;
  for (const code of Object.values(sources)) {
    for (const m of code.matchAll(re)) found.add(JSON.parse(`"${m[1]}"`) as string);
  }
  return found;
}

const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("traductions", () => {
  afterEach(() => setLang("fr"));

  it("chaque t(\"…\") du code est traduit en néerlandais et en allemand", () => {
    const keys = calls();
    expect(keys.size).toBeGreaterThan(50);
    for (const [lang, dict] of Object.entries(DICTIONARIES)) {
      const missing = [...keys].filter((k) => !(k in dict));
      expect(missing, `manquant en ${lang}`).toEqual([]);
    }
  });

  it("les deux dictionnaires ont les mêmes clés et les mêmes variables", () => {
    const { nl, de } = DICTIONARIES;
    expect(Object.keys(nl).sort()).toEqual(Object.keys(de).sort());
    for (const [fr, s] of Object.entries(nl)) {
      expect(placeholders(s), fr).toEqual(placeholders(fr));
      expect(placeholders(de[fr] ?? ""), fr).toEqual(placeholders(fr));
    }
  });

  it("aucune entrée inutilisée", () => {
    const keys = calls();
    // Clés passées par variable (menus, constantes).
    const indirect = [
      "Accueil", "Patients", "Agenda", "Bilans", "Exercices", "Documents", "Réglages", "Sauvegardes", "Sécurité",
      "Journal", "Clé biométrique", "Code à 6 chiffres", "{n} minutes", "15 minutes (recommandé)", "1 heure",
      "après 1 minute", "après 5 minutes (recommandé)", "après 15 minutes",
      "jamais (seul le délai d'inactivité s'applique)", "Nom du praticien ou du cabinet", "Titre", "Ex. Logopède",
      "Numéro INAMI", "Adresse", "Téléphone", "E-mail",
    ];
    const unused = Object.keys(DICTIONARIES.nl).filter((k) => !keys.has(k) && !indirect.includes(k) && !sources["./screens/Login.tsx"]?.includes(k));
    expect(unused).toEqual([]);
  });

  it("traduit, remplace les variables et retombe sur le français", () => {
    setLang("nl");
    expect(t("Verrouiller")).toBe("Vergrendelen");
    expect(t("Verrouillage auto : {n} min", { n: 15 })).toBe("Automatische vergrendeling: 15 min");
    expect(t("Texte non traduit")).toBe("Texte non traduit");
    expect(document.documentElement.lang).toBe("nl");
    setLang("de");
    expect(t("Verrouiller")).toBe("Sperren");
    setLang("fr");
    expect(t("Verrouillage auto : {n} min", { n: 5 })).toBe("Verrouillage auto : 5 min");
  });
});
