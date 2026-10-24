import { useCallback, useEffect, useRef, useState } from "react";
import { api, ApiError, setApiHooks, type SessionInfo, type Status } from "./api";
import { ErrorBox } from "./components";
import { Login } from "./screens/Login";
import { Patients } from "./screens/Patients";
import { Security } from "./screens/Security";
import { Setup } from "./screens/Setup";

type Phase = "loading" | "setup" | "login" | "open" | "offline";
type Page = "patients" | "security";

/** Délai avant le verrouillage automatique à partir duquel on prévient. */
const WARNING_SECS = 60;

export function App() {
  const [phase, setPhase] = useState<Phase>("loading");
  const [page, setPage] = useState<Page>("patients");
  const [idle, setIdle] = useState(300);
  const lastActivity = useRef(Date.now());

  const refresh = useCallback(async () => {
    try {
      const status = await api<Status>("GET", "/api/status");
      if (!status.initialized) return setPhase("setup");
      try {
        const s = await api<SessionInfo>("GET", "/api/session");
        setIdle(s.idle_timeout_secs);
        setPhase("open");
      } catch {
        setPhase("login");
      }
    } catch {
      setPhase("offline");
    }
  }, []);

  useEffect(() => {
    setApiHooks({
      activity: () => {
        lastActivity.current = Date.now();
      },
      unauthorized: () => setPhase("login"),
    });
    void refresh();
  }, [refresh]);

  if (phase === "loading") return <main className="narrow"><p className="muted">Connexion au serveur…</p></main>;
  if (phase === "offline")
    return (
      <main className="narrow">
        <section className="card">
          <h1>Serveur injoignable</h1>
          <p>Vérifiez que le serveur Phonalys du cabinet est démarré et que cet appareil est sur le même réseau.</p>
          <button type="button" onClick={() => void refresh()}>Réessayer</button>
        </section>
      </main>
    );
  if (phase === "setup") return <Setup onDone={() => setPhase("login")} />;
  if (phase === "login") return <Login onLoggedIn={() => void refresh()} />;

  return (
    <>
      <header className="topbar">
        <strong className="brand">Phonalys</strong>
        <nav>
          <button type="button" className={page === "patients" ? "tab active" : "tab"} onClick={() => setPage("patients")}>
            Patients
          </button>
          <button type="button" className={page === "security" ? "tab active" : "tab"} onClick={() => setPage("security")}>
            Sécurité
          </button>
        </nav>
        <SessionTimer idleSecs={idle} lastActivity={lastActivity} onExpired={() => setPhase("login")} />
        <LockButton onLocked={() => setPhase("login")} />
      </header>
      <main>{page === "patients" ? <Patients /> : <Security />}</main>
    </>
  );
}

function LockButton({ onLocked }: { onLocked: () => void }) {
  const [error, setError] = useState<string>();
  return (
    <>
      <button
        type="button"
        className="lock"
        title="Verrouille le coffre sur tous les appareils"
        onClick={() =>
          api("POST", "/api/lock")
            .then(onLocked)
            .catch((e: unknown) => setError(e instanceof ApiError ? e.message : String(e)))
        }
      >
        Verrouiller
      </button>
      <ErrorBox error={error} />
    </>
  );
}

/** Compte à rebours du verrouillage automatique (calé sur la dernière requête). */
export function SessionTimer(props: {
  idleSecs: number;
  lastActivity: { current: number };
  onExpired: () => void;
  now?: () => number;
}) {
  const now = props.now ?? Date.now;
  const remaining = () => Math.max(0, Math.ceil(props.idleSecs - (now() - props.lastActivity.current) / 1000));
  const [left, setLeft] = useState(remaining);
  const { onExpired } = props;

  useEffect(() => {
    const t = setInterval(() => {
      const r = remaining();
      setLeft(r);
      if (r === 0) onExpired();
    }, 1000);
    return () => clearInterval(t);
  });

  const mm = Math.floor(left / 60);
  const ss = String(left % 60).padStart(2, "0");
  return (
    <span className={left <= WARNING_SECS ? "timer warn" : "timer"} aria-live={left <= WARNING_SECS ? "polite" : "off"}>
      Verrouillage dans {mm}:{ss}
      {left <= WARNING_SECS && (
        <button type="button" className="ghost" onClick={() => void api("GET", "/api/session").catch(() => undefined)}>
          Rester connecté
        </button>
      )}
    </span>
  );
}
