import { useCallback, useEffect, useRef, useState } from "react";
import { api, ApiError, setApiHooks, type SecurityStatus, type SessionInfo, type Status } from "./api";
import { ErrorBox } from "./components";
import { Agenda } from "./screens/Agenda";
import { Audit } from "./screens/Audit";
import { Backups } from "./screens/Backups";
import { Bilans } from "./screens/Bilans";
import { Documents } from "./screens/Documents";
import { Exercises } from "./screens/Exercises";
import { Home } from "./screens/Home";
import { Login } from "./screens/Login";
import { Pairing } from "./screens/Pairing";
import { Patients } from "./screens/Patients";
import { Security } from "./screens/Security";
import { Settings } from "./screens/Settings";
import { Setup } from "./screens/Setup";
import { webauthnAvailable } from "./webauthn";

type Phase = "loading" | "setup" | "login" | "open" | "offline";
export type Page = "home" | "patients" | "agenda" | "bilans" | "exercices" | "documents" | "settings" | "backups" | "security" | "audit";
export type Navigate = (page: Page, patientId?: string) => void;

const PAGES: readonly (readonly [Page, string])[] = [
  ["home", "Accueil"],
  ["patients", "Patients"],
  ["agenda", "Agenda"],
  ["bilans", "Bilans"],
  ["exercices", "Exercices"],
  ["documents", "Documents"],
  ["settings", "Réglages"],
  ["backups", "Sauvegardes"],
  ["security", "Sécurité"],
  ["audit", "Journal"],
];

/** Le compte à rebours n'apparaît que dans les dernières minutes avant le verrouillage. */
const WARNING_SECS = 120;
/** Au plus un signal d'activité vers le serveur par minute. */
const KEEPALIVE_SECS = 60;

export function App() {
  const [phase, setPhase] = useState<Phase>("loading");
  // Ouverte par le QR code d'appairage, avant toute connexion.
  const [pairing, setPairing] = useState(() => window.location.hash === "#appairage");
  const [page, setPage] = useState<Page>("home");
  const [patientId, setPatientId] = useState<string>();
  const [idle, setIdle] = useState(900);
  const [hiddenLock, setHiddenLock] = useState(300);
  const lastActivity = useRef(Date.now());

  const refresh = useCallback(async () => {
    try {
      const status = await api<Status>("GET", "/api/status");
      if (!status.initialized) return setPhase("setup");
      try {
        const s = await api<SessionInfo>("GET", "/api/session");
        setIdle(s.idle_timeout_secs);
        setHiddenLock(s.hidden_lock_secs ?? 300);
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

  // Taper, cliquer ou faire défiler compte comme une activité : la session est
  // prolongée sur le serveur (au plus une fois par minute), sans quoi le verrouillage
  // surviendrait en pleine saisie d'un compte rendu.
  useEffect(() => {
    if (phase !== "open") return;
    const onInput = () => {
      if (Date.now() - lastActivity.current > KEEPALIVE_SECS * 1000) {
        lastActivity.current = Date.now();
        void api("GET", "/api/session").catch(() => undefined);
      }
    };
    const events = ["pointerdown", "keydown", "wheel", "touchstart"] as const;
    events.forEach((e) => window.addEventListener(e, onInput, { passive: true }));
    return () => events.forEach((e) => window.removeEventListener(e, onInput));
  }, [phase]);

  // Fenêtre réduite, onglet masqué, téléphone mis en veille : au-delà du délai réglé,
  // la session de cet appareil est fermée (les autres appareils restent connectés).
  useEffect(() => {
    if (phase !== "open" || hiddenLock <= 0) return;
    let hiddenAt: number | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const close = () => {
      void api("POST", "/api/logout")
        .catch(() => undefined)
        .finally(() => setPhase("login"));
    };
    const onVisibility = () => {
      clearTimeout(timer);
      if (document.visibilityState === "hidden") {
        hiddenAt = Date.now();
        timer = setTimeout(close, hiddenLock * 1000);
        return;
      }
      // Les minuteurs sont suspendus en arrière-plan sur mobile : contrôle au retour.
      if (hiddenAt !== undefined && Date.now() - hiddenAt >= hiddenLock * 1000) close();
      hiddenAt = undefined;
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [phase, hiddenLock]);

  const navigate: Navigate = useCallback((p, id) => {
    setPage(p);
    setPatientId(id);
    window.scrollTo?.(0, 0);
  }, []);

  if (pairing) return <Pairing onDone={() => setPairing(false)} />;
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
    <div className="shell">
      <header className="topbar">
        <strong className="brand">Phonalys</strong>
        <SessionTimer idleSecs={idle} lastActivity={lastActivity} onExpired={() => setPhase("login")} />
        <LockButton onLocked={() => setPhase("login")} />
      </header>
      <nav className="sidebar" aria-label="Navigation principale">
        {PAGES.map(([p, label]) => (
          <button
            key={p}
            type="button"
            className={page === p ? "nav active" : "nav"}
            aria-current={page === p ? "page" : undefined}
            onClick={() => navigate(p)}
          >
            {label}
          </button>
        ))}
      </nav>
      <main className="content">
        {page === "home" && (
          <>
            <BiometricInvite onActivate={() => navigate("security")} />
            <Home navigate={navigate} />
          </>
        )}
        {page === "patients" && <Patients navigate={navigate} openId={patientId} />}
        {page === "agenda" && <Agenda navigate={navigate} />}
        {page === "bilans" && <Bilans navigate={navigate} />}
        {page === "exercices" && <Exercises patientId={patientId} />}
        {page === "documents" && <Documents />}
        {page === "settings" && <Settings onIdleChanged={setIdle} onHiddenLockChanged={setHiddenLock} />}
        {page === "backups" && <Backups />}
        {page === "security" && <Security />}
        {page === "audit" && <Audit />}
      </main>
    </div>
  );
}

/** Après l'installation : propose la connexion en un geste par biométrie. */
export function BiometricInvite({ onActivate }: { onActivate: () => void }) {
  const [show, setShow] = useState(false);
  useEffect(() => {
    if (!webauthnAvailable()) return;
    api<SecurityStatus>("GET", "/api/security")
      .then((s) => setShow(s.passkeys === 0 || (s.quick_passkeys ?? 0) === 0))
      .catch(() => undefined);
  }, []);
  if (!show) return null;
  return (
    <section className="card invite">
      <h2>Connectez-vous en un geste</h2>
      <p>
        Activez la connexion biométrique : chaque jour, il suffira de poser le doigt, de montrer votre visage ou
        d'utiliser Windows Hello. La phrase de passe ne sera redemandée qu'après trois échecs.
      </p>
      <div className="actions">
        <button type="button" onClick={onActivate}>
          Activer maintenant
        </button>
        <button type="button" className="ghost" onClick={() => setShow(false)}>
          Plus tard
        </button>
      </div>
    </section>
  );
}

function LockButton({ onLocked }: { onLocked: () => void }) {
  const [error, setError] = useState<string>();
  return (
    <>
      <button
        type="button"
        className="lock"
        title="Verrouille immédiatement, sur tous les appareils"
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

/**
 * Verrouillage automatique : invisible tant qu'il reste du temps, puis un compte à
 * rebours avec « Rester connecté » dans les deux dernières minutes.
 */
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

  if (left > WARNING_SECS) {
    return (
      <span className="timer" title="Sans activité, Phonalys se verrouille pour protéger les dossiers. Délai réglable dans « Réglages ».">
        Verrouillage auto : {Math.round(props.idleSecs / 60)} min
      </span>
    );
  }
  const mm = Math.floor(left / 60);
  const ss = String(left % 60).padStart(2, "0");
  return (
    <span className="timer warn" role="status">
      Inactif : verrouillage dans {mm}:{ss}
      <button type="button" className="ghost" onClick={() => void api("GET", "/api/session").catch(() => undefined)}>
        Rester connecté
      </button>
    </span>
  );
}
