// Exercices complémentaires : son initial, suppression syllabique, contraires,
// compréhension de phrases, nombres et calcul.

import { useState } from "react";
import { CONTRAIRES, nearbyAnswers, PHRASES, SONS_INITIAUX, SONS_PROCHES, SUPPRESSIONS } from "../data/exercices";
import { shuffle } from "../data/lexique";
import { Quiz, SpeakButton, type ExerciseProps, type QuizItem } from "./common";

export function SonInitial({ niveau, onFinish }: ExerciseProps) {
  const [items] = useState<QuizItem[]>(() => {
    const sons = Object.keys(SONS_INITIAUX);
    const words = shuffle(sons.flatMap((s) => (SONS_INITIAUX[s] ?? []).map((w) => [w, s] as const))).slice(0, 10);
    return words.map(([mot, son]) => {
      // Niveau 2 : les sons voisins, qui se confondent à l'oreille.
      const pool = niveau >= 2 ? (SONS_PROCHES[son] ?? []) : sons.filter((s) => s !== son && !(SONS_PROCHES[son] ?? []).includes(s));
      return {
        key: mot,
        prompt: (
          <>
            <p className="big-word">{mot}</p>
            <p className="muted">Par quel son commence ce mot ?</p>
            <SpeakButton text={mot} />
          </>
        ),
        choices: shuffle([son, ...shuffle(pool).slice(0, 2)]),
        answer: son,
      };
    });
  });
  return <Quiz items={items} onFinish={onFinish} label="Identification du son initial" />;
}

export function Suppression({ niveau, onFinish }: ExerciseProps) {
  const [items] = useState<QuizItem[]>(() => {
    const list = SUPPRESSIONS[Math.min(niveau, SUPPRESSIONS.length) - 1] ?? [];
    const restes = [...new Set(list.map(([, , r]) => r))];
    return shuffle(list)
      .slice(0, 8)
      .map(([mot, enleve, reste]) => ({
        key: `${mot}-${enleve}`,
        prompt: (
          <>
            <p className="big-word">{mot}</p>
            <p className="muted">
              Dis « {mot} » sans « {enleve} ». Qu'est-ce qu'on entend ?
            </p>
            <SpeakButton text={`${mot}, sans ${enleve}`} />
          </>
        ),
        choices: shuffle([reste, ...shuffle(restes.filter((r) => r !== reste)).slice(0, 2)]),
        answer: reste,
      }));
  });
  return <Quiz items={items} onFinish={onFinish} label="Suppression syllabique" />;
}

export function Contraires({ niveau, onFinish }: ExerciseProps) {
  const [items] = useState<QuizItem[]>(() => {
    const list = CONTRAIRES.slice(0, Math.min(niveau, CONTRAIRES.length)).flat();
    const all = list.flatMap(([a, b]) => [a, b]);
    return shuffle(list)
      .slice(0, 10)
      .map(([a, b]) => {
        const [mot, contraire] = Math.random() < 0.5 ? [a, b] : [b, a];
        return {
          key: mot,
          prompt: (
            <>
              <p className="big-word">{mot}</p>
              <p className="muted">Quel est le contraire de ce mot ?</p>
              <SpeakButton text={mot} />
            </>
          ),
          choices: shuffle([contraire, ...shuffle(all.filter((w) => w !== mot && w !== contraire)).slice(0, 2)]),
          answer: contraire,
        };
      });
  });
  return <Quiz items={items} onFinish={onFinish} label="Contraires" />;
}

export function Phrases({ niveau, onFinish }: ExerciseProps) {
  const [items] = useState<QuizItem[]>(() => {
    const list = PHRASES[Math.min(niveau, PHRASES.length) - 1] ?? [];
    return shuffle(list)
      .slice(0, 8)
      .map(([phrase, vrai], i) => ({
        key: `phrase ${i + 1}`,
        prompt: (
          <>
            <p className="big-sentence">{phrase}</p>
            <p className="muted">Cette phrase est-elle vraie ou fausse ?</p>
            <SpeakButton text={phrase} />
          </>
        ),
        choices: ["Vrai", "Faux"],
        answer: vrai ? "Vrai" : "Faux",
      }));
  });
  return <Quiz items={items} onFinish={onFinish} label="Compréhension de phrases" />;
}

const int = (min: number, max: number) => min + Math.floor(Math.random() * (max - min + 1));

/** Un item de calcul ; exporté pour les tests. */
export function calculItem(niveau: number): { question: string; answer: number } {
  switch (niveau) {
    case 1: {
      const a = int(0, 20);
      let b = int(0, 20);
      while (b === a) b = int(0, 20);
      return { question: `${a} ou ${b} ?`, answer: Math.max(a, b) };
    }
    case 2: {
      const a = int(0, 10);
      const b = int(0, 10 - a);
      return { question: `${a} + ${b}`, answer: a + b };
    }
    case 3: {
      const a = int(0, 20);
      if (Math.random() < 0.5) {
        const b = int(0, 20 - a);
        return { question: `${a} + ${b}`, answer: a + b };
      }
      const b = int(0, a);
      return { question: `${a} − ${b}`, answer: a - b };
    }
    case 4: {
      const a = int(2, 5);
      const b = int(1, 10);
      return { question: `${a} × ${b}`, answer: a * b };
    }
    default: {
      const a = int(6, 9);
      const b = int(1, 10);
      return { question: `${a} × ${b}`, answer: a * b };
    }
  }
}

export function Calcul({ niveau, onFinish }: ExerciseProps) {
  const [items] = useState<QuizItem[]>(() =>
    Array.from({ length: 10 }, (_, i) => {
      const { question, answer } = calculItem(niveau);
      const choices =
        niveau === 1
          ? question.replace(" ?", "").split(" ou ")
          : shuffle([answer, ...nearbyAnswers(answer, 3)].map(String));
      return {
        key: `${question} (${i + 1})`,
        prompt: (
          <>
            <p className="big-word">{question}</p>
            <p className="muted">{niveau === 1 ? "Quel est le plus grand nombre ?" : "Combien ça fait ?"}</p>
          </>
        ),
        choices,
        answer: String(answer),
      };
    }),
  );
  return <Quiz items={items} onFinish={onFinish} label="Nombres et calcul" />;
}
