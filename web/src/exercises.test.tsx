import { cleanup, render, screen } from "@testing-library/react";
import { CONTRAIRES, nearbyAnswers, PHRASES, SONS_INITIAUX, SONS_PROCHES, SUPPRESSIONS } from "./data/exercices";
import { EXERCISES } from "./exercises";
import { calculItem } from "./exercises/plus";
import { Progression, percent, series } from "./screens/Progression";

afterEach(cleanup);

describe("contenus des exercices", () => {
  it("chaque mot commence par le son annoncé, et les sons proches existent", () => {
    for (const [son, mots] of Object.entries(SONS_INITIAUX)) {
      for (const m of mots) expect(m.startsWith(son), `${m} / ${son}`).toBe(true);
      for (const proche of SONS_PROCHES[son] ?? []) expect(SONS_INITIAUX[proche]).toBeDefined();
    }
  });

  it("la syllabe enlevée est bien au début (niveau 1) ou à la fin (niveau 2)", () => {
    for (const [mot, s] of SUPPRESSIONS[0] ?? []) expect(mot.startsWith(s), mot).toBe(true);
    for (const [mot, s] of SUPPRESSIONS[1] ?? []) expect(mot.endsWith(s), mot).toBe(true);
  });

  it("aucun mot n'apparaît deux fois dans les contraires, phrases variées", () => {
    const words = CONTRAIRES.flat().flat();
    expect(new Set(words).size).toBe(words.length);
    for (const level of PHRASES) {
      expect(level.some(([, v]) => v)).toBe(true);
      expect(level.some(([, v]) => !v)).toBe(true);
    }
  });

  it("les calculs restent dans les bornes de chaque niveau", () => {
    for (let i = 0; i < 300; i++) {
      expect(calculItem(2).answer).toBeLessThanOrEqual(10);
      const n3 = calculItem(3).answer;
      expect(n3).toBeGreaterThanOrEqual(0);
      expect(n3).toBeLessThanOrEqual(20);
      expect(calculItem(4).answer).toBeLessThanOrEqual(50);
      expect(calculItem(5).answer).toBeGreaterThanOrEqual(6);
      const [a, b] = calculItem(1).question.replace(" ?", "").split(" ou ");
      expect(a).not.toBe(b);
    }
    const near = nearbyAnswers(0, 3);
    expect(near).toHaveLength(3);
    expect(near.every((n) => n > 0)).toBe(true);
  });

  it("identifiants d'exercices uniques, au moins treize exercices", () => {
    const ids = EXERCISES.map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.length).toBeGreaterThanOrEqual(13);
  });
});

describe("chaque exercice démarre à chaque niveau", () => {
  for (const e of EXERCISES) {
    e.niveaux.forEach((label, i) => {
      it(`${e.titre} — ${label}`, () => {
        const { container } = render(<e.Component niveau={i + 1} onFinish={() => undefined} />);
        expect(container.textContent?.length).toBeGreaterThan(0);
        cleanup();
      });
    });
  }
});

describe("progression", () => {
  const base = { patient_id: "p", niveau: 1, duree_s: 30 };
  const results = [
    { ...base, id: "a", exercice: "rimes", date: "2026-10-20", score: 4, total: 8 },
    { ...base, id: "b", exercice: "rimes", date: "2026-10-01", score: 2, total: 8 },
    { ...base, id: "c", exercice: "rimes", date: "2026-10-25", score: 7, total: 8 },
    { ...base, id: "d", exercice: "calcul", date: "2026-10-25", score: 5, total: 10 },
  ];

  it("ne trace que les exercices passés au moins deux fois, dans l'ordre chronologique", () => {
    const s = series(results);
    expect(s.map(([id]) => id)).toEqual(["rimes"]);
    expect(s[0]?.[1].map((r) => r.id)).toEqual(["b", "a", "c"]);
    expect(percent({ score: 7, total: 8 })).toBe(88);
    expect(percent({ score: 1, total: 0 })).toBe(0);
  });

  it("décrit la courbe pour les lecteurs d'écran", () => {
    render(<Progression results={results} />);
    const chart = screen.getByRole("img");
    expect(chart.getAttribute("aria-label")).toMatch(/25 %.*88 %.*\+63 points.*3 passations/);
  });
});