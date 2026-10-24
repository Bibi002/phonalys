import type { ComponentType } from "react";
import { Barrage, EmpanChiffres, Fluence, SequencesSpatiales } from "./cognition";
import type { ExerciseProps } from "./common";
import { Categories, LectureFlash, Rimes, Syllabes } from "./langage";

export interface ExerciseDef {
  id: string;
  titre: string;
  domaine: string;
  description: string;
  niveaux: readonly string[];
  /** Le score est une longueur d'empan (et non un nombre de bonnes réponses). */
  empan?: boolean;
  Component: ComponentType<ExerciseProps>;
}

export const EXERCISES: readonly ExerciseDef[] = [
  {
    id: "syllabes",
    titre: "Compter les syllabes",
    domaine: "Métaphonologie",
    description: "Segmentation syllabique de mots entendus ou lus.",
    niveaux: ["1 à 2 syllabes", "1 à 3 syllabes", "1 à 5 syllabes"],
    Component: Syllabes,
  },
  {
    id: "rimes",
    titre: "Ça rime ?",
    domaine: "Métaphonologie",
    description: "Jugement de rimes sur des paires de mots.",
    niveaux: ["8 paires", "12 paires", "16 paires"],
    Component: Rimes,
  },
  {
    id: "categories",
    titre: "Ranger les mots",
    domaine: "Lexique et sémantique",
    description: "Catégorisation sémantique (fruits, animaux, vêtements…).",
    niveaux: ["2 familles", "3 familles", "4 familles"],
    Component: Categories,
  },
  {
    id: "lecture-flash",
    titre: "Lecture flash",
    domaine: "Lecture",
    description: "Reconnaissance rapide de mots parmi des pseudo-mots visuellement proches.",
    niveaux: ["Mots courts, 0,9 s", "Mots moyens, 0,7 s", "Mots longs, 0,5 s", "Mots très longs, 0,35 s"],
    Component: LectureFlash,
  },
  {
    id: "empan-chiffres",
    titre: "Empan de chiffres",
    domaine: "Mémoire",
    description: "Mémoire à court terme (ordre direct) et mémoire de travail (ordre inverse).",
    niveaux: ["Ordre direct", "Ordre inverse"],
    empan: true,
    Component: EmpanChiffres,
  },
  {
    id: "sequences",
    titre: "Séquences spatiales",
    domaine: "Mémoire",
    description: "Mémoire visuo-spatiale : reproduire l'ordre des cases allumées.",
    niveaux: ["Grille 3 × 3", "Grille 4 × 4"],
    empan: true,
    Component: SequencesSpatiales,
  },
  {
    id: "barrage",
    titre: "Barrage de lettres",
    domaine: "Attention",
    description: "Attention sélective et discrimination visuelle (b, d, p, q).",
    niveaux: ["b parmi d", "b parmi d, p, q", "q parmi b, d, p"],
    Component: Barrage,
  },
  {
    id: "fluence",
    titre: "Fluence verbale",
    domaine: "Lexique et sémantique",
    description: "Évocation lexicale en une minute, comptée par le praticien.",
    niveaux: ["Catégorie : animaux", "Lettre « p »", "Lettre « f »"],
    Component: Fluence,
  },
];

export function exerciseTitle(id: string): string {
  return EXERCISES.find((e) => e.id === id)?.titre ?? id;
}
