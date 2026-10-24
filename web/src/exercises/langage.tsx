// Exercices de langage : métaphonologie, lexique, lecture.

import { useEffect, useState } from "react";
import { CATEGORIES, distracteurs, MOTS_FLASH, RIMES, shuffle, SYLLABES } from "../data/lexique";
import { Quiz, SpeakButton, type ExerciseProps, type QuizItem } from "./common";

export function Syllabes({ niveau, onFinish }: ExerciseProps) {
  const [items] = useState<QuizItem[]>(() => {
    const max = niveau === 1 ? 2 : niveau === 2 ? 3 : 5;
    const pool = SYLLABES.filter(([, n]) => n <= max);
    const choices = Array.from({ length: Math.max(3, max) }, (_, i) => String(i + 1));
    return shuffle(pool)
      .slice(0, 10)
      .map(([mot, n]) => ({
        key: mot,
        prompt: (
          <>
            <p className="big-word">{mot}</p>
            <p className="muted">Combien de syllabes entends-tu ? Tape dans tes mains pour compter.</p>
            <SpeakButton text={mot} />
          </>
        ),
        choices,
        answer: String(n),
      }));
  });
  return <Quiz items={items} onFinish={onFinish} label="Comptage syllabique" />;
}

export function Rimes({ niveau, onFinish }: ExerciseProps) {
  const [items] = useState<QuizItem[]>(() => {
    const count = niveau === 1 ? 8 : niveau === 2 ? 12 : 16;
    const yes = shuffle(RIMES.filter((r) => r[2]));
    const no = shuffle(RIMES.filter((r) => !r[2]));
    const picked = shuffle([...yes.slice(0, count / 2), ...no.slice(0, count / 2)]);
    return picked.map(([a, b, rime]) => ({
      key: `${a}/${b}`,
      prompt: (
        <>
          <p className="big-word">
            {a} · {b}
          </p>
          <p className="muted">Est-ce que ces deux mots riment (finissent par le même son) ?</p>
          <SpeakButton text={`${a}, ${b}`} />
        </>
      ),
      choices: ["Ça rime", "Ça ne rime pas"],
      answer: rime ? "Ça rime" : "Ça ne rime pas",
    }));
  });
  return <Quiz items={items} onFinish={onFinish} label="Jugement de rimes" />;
}

export function Categories({ niveau, onFinish }: ExerciseProps) {
  const [items] = useState<QuizItem[]>(() => {
    const nCat = niveau + 1;
    const cats = shuffle(Object.keys(CATEGORIES)).slice(0, nCat);
    const words = shuffle(
      cats.flatMap((c) => shuffle(CATEGORIES[c] ?? []).slice(0, 4).map((w) => [w, c] as const)),
    );
    return words.map(([w, c]) => ({
      key: w,
      prompt: (
        <>
          <p className="big-word">{w}</p>
          <p className="muted">Dans quelle famille range-t-on ce mot ?</p>
          <SpeakButton text={w} />
        </>
      ),
      choices: cats,
      answer: c,
    }));
  });
  return <Quiz items={items} onFinish={onFinish} label="Catégorisation sémantique" />;
}

function Flash({ word, ms, done }: { word: string; ms: number; done: () => void }) {
  const [phase, setPhase] = useState<"ready" | "show" | "mask">("ready");
  useEffect(() => {
    if (phase === "show") {
      const t = setTimeout(() => setPhase("mask"), ms);
      return () => clearTimeout(t);
    }
    if (phase === "mask") {
      const t = setTimeout(done, 400);
      return () => clearTimeout(t);
    }
    return undefined;
  }, [phase, ms, done]);
  if (phase === "ready")
    return (
      <div className="quiz-prompt">
        <p className="muted">Regarde bien le centre de l'écran : le mot va apparaître très vite.</p>
        <button type="button" onClick={() => setPhase("show")}>
          Je suis prêt
        </button>
      </div>
    );
  return (
    <div className="quiz-prompt">
      <p className="big-word flash" aria-live="off">
        {phase === "show" ? word : "#".repeat(word.length)}
      </p>
    </div>
  );
}

export function LectureFlash({ niveau, onFinish }: ExerciseProps) {
  const [items] = useState<QuizItem[]>(() => {
    const words = MOTS_FLASH[Math.min(niveau, MOTS_FLASH.length) - 1] ?? [];
    const ms = [900, 700, 500, 350][niveau - 1] ?? 500;
    return shuffle(words)
      .slice(0, 10)
      .map((w) => ({
        key: w,
        before: (done: () => void) => <Flash word={w} ms={ms} done={done} />,
        prompt: <p className="muted">Quel mot as-tu vu ?</p>,
        choices: shuffle([w, ...distracteurs(w, 3)]),
        answer: w,
      }));
  });
  return <Quiz items={items} onFinish={onFinish} label="Lecture flash" />;
}
