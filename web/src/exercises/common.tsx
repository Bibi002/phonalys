import { useEffect, useRef, useState, type ReactNode } from "react";

export interface Outcome {
  score: number;
  total: number;
  details?: string;
}

export interface ExerciseProps {
  niveau: number;
  onFinish: (r: Outcome) => void;
}

// --- Synthèse vocale strictement locale ------------------------------------------------
//
// Certaines voix des navigateurs sont « en ligne » : le texte part chez un fournisseur
// de synthèse. Phonalys n'utilise que les voix installées sur l'appareil
// (`localService`), pour qu'aucun mot ne quitte le cabinet.

export function localFrenchVoice(): SpeechSynthesisVoice | undefined {
  if (typeof window === "undefined" || !("speechSynthesis" in window)) return undefined;
  return window.speechSynthesis
    .getVoices()
    .find((v) => v.localService && v.lang.toLowerCase().startsWith("fr"));
}

export function speak(text: string): boolean {
  const voice = localFrenchVoice();
  if (!voice) return false;
  const u = new SpeechSynthesisUtterance(text);
  u.voice = voice;
  u.lang = voice.lang;
  u.rate = 0.85;
  window.speechSynthesis.cancel();
  window.speechSynthesis.speak(u);
  return true;
}

/** Les voix se chargent de façon asynchrone dans certains navigateurs. */
export function useLocalVoice(): boolean {
  const [ok, setOk] = useState(() => Boolean(localFrenchVoice()));
  useEffect(() => {
    if (typeof window === "undefined" || !("speechSynthesis" in window)) return;
    const update = () => setOk(Boolean(localFrenchVoice()));
    window.speechSynthesis.addEventListener("voiceschanged", update);
    return () => window.speechSynthesis.removeEventListener("voiceschanged", update);
  }, []);
  return ok;
}

export function SpeakButton({ text }: { text: string }) {
  const ok = useLocalVoice();
  if (!ok) return null;
  return (
    <button type="button" className="ghost" onClick={() => speak(text)}>
      Écouter
    </button>
  );
}

// --- Questionnaire à choix ---------------------------------------------------------------

export interface QuizItem {
  key: string;
  prompt: ReactNode;
  choices: readonly string[];
  answer: string;
  /** Étape préalable (ex. mot flashé) ; la question s'affiche quand elle se termine. */
  before?: (done: () => void) => ReactNode;
}

/** Enchaîne des items à choix, avec un retour immédiat et bienveillant. */
export function Quiz({ items, onFinish, label }: { items: readonly QuizItem[]; onFinish: (r: Outcome) => void; label: string }) {
  const [index, setIndex] = useState(0);
  const [score, setScore] = useState(0);
  const [feedback, setFeedback] = useState<{ ok: boolean; answer: string }>();
  const [ready, setReady] = useState(false);
  const errors = useRef<string[]>([]);
  const item = items[index];

  useEffect(() => setReady(!item?.before), [item]);

  if (!item) return null;

  function choose(choice: string) {
    if (feedback || !item) return;
    const ok = choice === item.answer;
    if (ok) setScore((s) => s + 1);
    else errors.current.push(item.key);
    setFeedback({ ok, answer: item.answer });
    setTimeout(() => {
      setFeedback(undefined);
      if (index + 1 >= items.length) {
        const total = items.length;
        const final = score + (ok ? 1 : 0);
        onFinish({
          score: final,
          total,
          details: errors.current.length ? `À revoir : ${errors.current.join(", ")}` : "Aucune erreur",
        });
      } else {
        setIndex(index + 1);
      }
    }, ok ? 700 : 1600);
  }

  return (
    <div className="quiz" aria-label={label}>
      <p className="progress-text">
        {index + 1} / {items.length}
      </p>
      {!ready && item.before ? (
        item.before(() => setReady(true))
      ) : (
        <>
          <div className="quiz-prompt">{item.prompt}</div>
          <div className="choices">
            {item.choices.map((c) => (
              <button
                key={c}
                type="button"
                className={
                  feedback && c === feedback.answer ? "choice right" : feedback && !feedback.ok ? "choice dim" : "choice"
                }
                onClick={() => choose(c)}
                disabled={Boolean(feedback)}
              >
                {c}
              </button>
            ))}
          </div>
          <p className="feedback" role="status">
            {feedback ? (feedback.ok ? "Bravo !" : `La bonne réponse était : ${feedback.answer}`) : " "}
          </p>
        </>
      )}
    </div>
  );
}

/** Chronomètre qui démarre au montage (secondes écoulées). */
export function useElapsed(running = true): number {
  const start = useRef(Date.now());
  const [secs, setSecs] = useState(0);
  useEffect(() => {
    if (!running) return;
    const t = setInterval(() => setSecs(Math.round((Date.now() - start.current) / 1000)), 500);
    return () => clearInterval(t);
  }, [running]);
  return secs;
}

export function formatSecs(s: number): string {
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
