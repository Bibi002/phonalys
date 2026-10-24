import { distracteurs, RIMES, shuffle, SYLLABES } from "./data/lexique";
import { age, ageMonths, normalCdf, normalInv, toZ } from "./model";
import { mondayOf } from "./screens/Agenda";

describe("modèle clinique", () => {
  it("calcule l'âge en années et mois", () => {
    expect(age("2018-03-15", "2026-10-24")).toBe("8 ans 7 mois");
    expect(ageMonths("2018-03-25", "2026-10-24")).toBe(102);
    expect(age("2027-01-01", "2026-10-24")).toBe("");
  });

  it("convertit les échelles usuelles en écarts-types", () => {
    expect(toZ("z", "-1,5")).toBe(-1.5);
    expect(toZ("note_standard", "7")).toBeCloseTo(-1);
    expect(toZ("note_composite", "130")).toBeCloseTo(2);
    expect(toZ("percentile", "50")).toBeCloseTo(0, 6);
    expect(toZ("percentile", "2.5")).toBeCloseTo(-1.96, 2);
    expect(toZ("percentile", "100")).toBeUndefined();
    expect(toZ("aucune", "3")).toBeUndefined();
    expect(toZ("z", "")).toBeUndefined();
  });

  it("la loi normale et son inverse se répondent", () => {
    for (const z of [-2.5, -1, 0, 0.7, 2]) expect(normalInv(normalCdf(z))).toBeCloseTo(z, 3);
    expect(normalCdf(-1.645)).toBeCloseTo(0.05, 3);
  });

  it("trouve le lundi de la semaine", () => {
    expect(mondayOf("2026-10-24")).toBe("2026-10-19");
    expect(mondayOf("2026-10-19")).toBe("2026-10-19");
    expect(mondayOf("2026-10-25")).toBe("2026-10-19");
  });
});

describe("contenu des exercices", () => {
  it("les distracteurs sont distincts du mot et entre eux", () => {
    for (const w of ["maison", "lit", "bibliothèque"]) {
      const d = distracteurs(w, 3);
      expect(d).not.toContain(w);
      expect(new Set(d).size).toBe(d.length);
      expect(d.length).toBeGreaterThan(0);
    }
  });

  it("les listes sont cohérentes", () => {
    expect(SYLLABES.every(([w, n]) => n >= 1 && n <= 5 && !w.endsWith("e"))).toBe(true);
    expect(RIMES.filter((r) => r[2]).length).toBeGreaterThanOrEqual(8);
    expect(RIMES.filter((r) => !r[2]).length).toBeGreaterThanOrEqual(8);
    expect(shuffle([1, 2, 3]).sort()).toEqual([1, 2, 3]);
  });
});
