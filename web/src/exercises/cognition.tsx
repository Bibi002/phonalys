// Exercices de mémoire, d'attention et d'évocation lexicale.
//
// Ce sont des paradigmes classiques et libres (empan, séquences spatiales, barrage,
// fluence) réalisés par Phonalys. Ils servent à l'entraînement et au suivi : ils ne
// sont pas étalonnés et ne remplacent pas un test normé.

import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { shuffle } from "../data/lexique";
import { formatSecs, speak, useElapsed, useLocalVoice, type ExerciseProps } from "./common";

const MAX_SPAN = 9;

/**
 * Procédure adaptative commune : deux essais par longueur ; on monte d'un cran dès
 * qu'un essai est réussi, on s'arrête après deux échecs à la même longueur.
 */
function useSpan(start: number) {
  const [length, setLength] = useState(start);
  const [trial, setTrial] = useState(0);
  const [failures, setFailures] = useState(0);
  const [best, setBest] = useState(0);
  const [done, setDone] = useState(false);

  function record(ok: boolean) {
    if (ok) {
      setBest(Math.max(best, length));
      if (length >= MAX_SPAN) return setDone(true);
      setLength(length + 1);
      setTrial(0);
      setFailures(0);
    } else if (failures + 1 >= 2) {
      setDone(true);
    } else {
      setFailures(failures + 1);
      setTrial(trial + 1);
    }
  }
  return { length, trial, best, done, record };
}

function randomDigits(n: number): number[] {
  const out: number[] = [];
  while (out.length < n) {
    const d = 1 + Math.floor(Math.random() * 9);
    if (d !== out[out.length - 1]) out.push(d);
  }
  return out;
}

export function EmpanChiffres({ niveau, onFinish }: ExerciseProps) {
  const backward = niveau >= 2;
  const span = useSpan(backward ? 2 : 3);
  const [seq, setSeq] = useState<number[]>();
  const [shown, setShown] = useState<number | null>(null);
  const [answer, setAnswer] = useState("");
  const [audio, setAudio] = useState(false);
  const voice = useLocalVoice();
  const log = useRef<string[]>([]);

  useEffect(() => {
    if (span.done) onFinish({ score: span.best, total: MAX_SPAN, details: log.current.join(" · ") });
  }, [span.done, span.best, onFinish]);

  function present() {
    const s = randomDigits(span.length);
    setSeq(undefined);
    setAnswer("");
    let i = 0;
    const step = () => {
      if (i >= s.length) {
        setShown(null);
        setSeq(s);
        return;
      }
      const d = s[i] ?? 0;
      setShown(d);
      if (audio) speak(String(d));
      i += 1;
      setTimeout(() => {
        setShown(null);
        setTimeout(step, 300);
      }, 900);
    };
    step();
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    if (!seq) return;
    const expected = (backward ? [...seq].reverse() : seq).join("");
    const ok = answer.replace(/\D/g, "") === expected;
    log.current.push(`${span.length} : ${ok ? "réussi" : "échoué"}`);
    setSeq(undefined);
    span.record(ok);
  }

  if (span.done) return null;
  return (
    <div className="quiz">
      <p className="progress-text">
        Série de {span.length} chiffres · {backward ? "à rebours" : "dans l'ordre"}
      </p>
      {shown !== null ? (
        <p className="big-word digit">{shown}</p>
      ) : seq ? (
        <form onSubmit={submit} className="quiz-prompt">
          <label className="field">
            <span>{backward ? "Tape les chiffres en commençant par le dernier" : "Tape les chiffres dans l'ordre"}</span>
            <input inputMode="numeric" autoFocus value={answer} onChange={(e) => setAnswer(e.target.value)} />
          </label>
          <button type="submit">Valider</button>
        </form>
      ) : (
        <div className="quiz-prompt">
          {voice && (
            <label className="check">
              <input type="checkbox" checked={audio} onChange={(e) => setAudio(e.target.checked)} />
              Dire les chiffres à voix haute (voix de l'appareil)
            </label>
          )}
          <button type="button" onClick={present}>
            {span.length === (backward ? 2 : 3) && span.trial === 0 ? "Commencer" : "Série suivante"}
          </button>
        </div>
      )}
    </div>
  );
}

export function SequencesSpatiales({ niveau, onFinish }: ExerciseProps) {
  const size = niveau >= 2 ? 4 : 3;
  const span = useSpan(2);
  const [seq, setSeq] = useState<number[]>();
  const [lit, setLit] = useState<number | null>(null);
  const [input, setInput] = useState<number[]>([]);
  const [playing, setPlaying] = useState(false);
  const log = useRef<string[]>([]);

  useEffect(() => {
    if (span.done) onFinish({ score: span.best, total: MAX_SPAN, details: log.current.join(" · ") });
  }, [span.done, span.best, onFinish]);

  function play() {
    const cells = shuffle(Array.from({ length: size * size }, (_, i) => i)).slice(0, span.length);
    setSeq(cells);
    setInput([]);
    setPlaying(true);
    cells.forEach((c, i) => {
      setTimeout(() => setLit(c), i * 1000);
      setTimeout(() => setLit(null), i * 1000 + 700);
    });
    setTimeout(() => setPlaying(false), cells.length * 1000);
  }

  function tap(i: number) {
    if (!seq || playing) return;
    const next = [...input, i];
    setInput(next);
    if (next.length === seq.length) {
      const ok = next.every((v, k) => v === seq[k]);
      log.current.push(`${span.length} : ${ok ? "réussi" : "échoué"}`);
      setSeq(undefined);
      span.record(ok);
    }
  }

  if (span.done) return null;
  return (
    <div className="quiz">
      <p className="progress-text">Séquence de {span.length} cases</p>
      <div className={`grid-cells g${size}`}>
        {Array.from({ length: size * size }, (_, i) => (
          <button
            key={i}
            type="button"
            aria-label={`case ${i + 1}`}
            className={lit === i ? "cell lit" : input.includes(i) && seq ? "cell picked" : "cell"}
            onClick={() => tap(i)}
          />
        ))}
      </div>
      {!seq && (
        <button type="button" onClick={play}>
          {span.length === 2 && span.trial === 0 ? "Commencer" : "Séquence suivante"}
        </button>
      )}
      {seq && !playing && <p className="muted">Touche les cases dans le même ordre.</p>}
    </div>
  );
}

const BARRAGE_LEVELS = [
  { target: "b", pool: ["b", "d"], rows: 6, cols: 8 },
  { target: "b", pool: ["b", "d", "p", "q"], rows: 8, cols: 10 },
  { target: "q", pool: ["b", "d", "p", "q"], rows: 10, cols: 12 },
] as const;

export function Barrage({ niveau, onFinish }: ExerciseProps) {
  const level = BARRAGE_LEVELS[Math.min(niveau, BARRAGE_LEVELS.length) - 1] ?? BARRAGE_LEVELS[0];
  const letters = useMemo(() => {
    const n = level.rows * level.cols;
    const targets = Math.round(n * 0.25);
    const others = level.pool.filter((l) => l !== level.target);
    return shuffle([
      ...Array.from({ length: targets }, () => level.target as string),
      ...Array.from({ length: n - targets }, (_, i) => others[i % others.length] ?? "d"),
    ]);
  }, [level]);
  const [marked, setMarked] = useState<Set<number>>(new Set());
  const secs = useElapsed();

  function finish() {
    const total = letters.filter((l) => l === level.target).length;
    let hits = 0;
    let false_alarms = 0;
    marked.forEach((i) => (letters[i] === level.target ? hits++ : false_alarms++));
    onFinish({
      score: Math.max(0, hits - false_alarms),
      total,
      details: `Cibles barrées : ${hits}/${total} · erreurs : ${false_alarms} · oublis : ${total - hits} · temps : ${formatSecs(secs)}`,
    });
  }

  return (
    <div className="quiz">
      <p className="progress-text">
        Barre toutes les lettres <strong className="target">{level.target}</strong> · {formatSecs(secs)}
      </p>
      <div className="barrage" role="grid" aria-label="Lettres à barrer">
        {letters.map((l, i) => (
          <button
            key={i}
            type="button"
            className={marked.has(i) ? "letter struck" : "letter"}
            aria-pressed={marked.has(i)}
            onClick={() => {
              const next = new Set(marked);
              if (next.has(i)) next.delete(i);
              else next.add(i);
              setMarked(next);
            }}
          >
            {l}
          </button>
        ))}
      </div>
      <button type="button" onClick={finish}>
        J'ai fini
      </button>
    </div>
  );
}

const FLUENCE = [
  { consigne: "Des noms d'animaux", type: "catégorielle" },
  { consigne: "Des mots qui commencent par « p »", type: "phonologique" },
  { consigne: "Des mots qui commencent par « f »", type: "phonologique" },
] as const;

export function Fluence({ niveau, onFinish }: ExerciseProps) {
  const task = FLUENCE[Math.min(niveau, FLUENCE.length) - 1] ?? FLUENCE[0];
  const [running, setRunning] = useState(false);
  const [left, setLeft] = useState(60);
  const [count, setCount] = useState(0);
  const [errors, setErrors] = useState(0);
  const [words, setWords] = useState("");

  useEffect(() => {
    if (!running) return;
    if (left <= 0) {
      setRunning(false);
      return;
    }
    const t = setTimeout(() => setLeft(left - 1), 1000);
    return () => clearTimeout(t);
  }, [running, left]);

  const over = !running && left <= 0;
  return (
    <div className="quiz">
      <p className="progress-text">Fluence {task.type} · {formatSecs(left)}</p>
      <div className="quiz-prompt">
        <p className="big-word small">{task.consigne}</p>
        <p className="muted">
          Le patient donne le plus de mots possible en une minute. Le praticien compte les réponses correctes (sans
          répétition ni dérivé).
        </p>
      </div>
      {!running && !over && (
        <button type="button" onClick={() => setRunning(true)}>
          Démarrer la minute
        </button>
      )}
      {(running || over) && (
        <div className="actions">
          <button type="button" className="choice" onClick={() => setCount(count + 1)}>
            + 1 mot correct ({count})
          </button>
          <button type="button" className="ghost" onClick={() => setErrors(errors + 1)}>
            + 1 erreur ou répétition ({errors})
          </button>
        </div>
      )}
      <label className="field">
        <span>Mots produits (facultatif)</span>
        <textarea rows={3} value={words} onChange={(e) => setWords(e.target.value)} />
      </label>
      {over && (
        <button
          type="button"
          onClick={() =>
            onFinish({
              score: count,
              total: count + errors,
              details: `${task.consigne} : ${count} mot(s), ${errors} erreur(s)${words.trim() ? ` — ${words.trim()}` : ""}`,
            })
          }
        >
          Terminer
        </button>
      )}
    </div>
  );
}
