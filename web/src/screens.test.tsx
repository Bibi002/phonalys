import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SessionTimer } from "./App";
import { Login } from "./screens/Login";
import { RecoveryCodes } from "./components";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

function jsonResponse(status: number, body?: unknown) {
  return new Response(body === undefined ? null : JSON.stringify(body), { status });
}

/** Simule le serveur : `/api/status` répond toujours, le reste passe par `handler`. */
function server(handler: (url: string, init?: RequestInit) => Response, status: Record<string, unknown> = {}) {
  const fn = vi.fn(async (url: string, init?: RequestInit) =>
    url === "/api/status" ? jsonResponse(200, { initialized: true, ...status }) : handler(url, init),
  );
  vi.stubGlobal("fetch", fn);
  return fn;
}

function callsTo(fn: ReturnType<typeof server>, path: string) {
  return (fn.mock.calls as unknown as [string, RequestInit][]).filter(([u]) => u === path);
}

describe("connexion", () => {
  it("envoie la phrase de passe et le code TOTP, puis efface les champs", async () => {
    const fetch = server(() => jsonResponse(204));
    const onLoggedIn = vi.fn();
    render(<Login onLoggedIn={onLoggedIn} />);

    fireEvent.click(screen.getByRole("tab", { name: "Code à 6 chiffres" }));
    fireEvent.change(screen.getByLabelText("Phrase de passe"), { target: { value: "ma phrase de passe" } });
    fireEvent.change(screen.getByLabelText(/Code de l'application/), { target: { value: "123 456" } });
    fireEvent.click(screen.getByRole("button", { name: "Déverrouiller" }));

    await waitFor(() => expect(onLoggedIn).toHaveBeenCalled());
    const [[, init]] = callsTo(fetch, "/api/login") as [[string, RequestInit]];
    expect(JSON.parse(init.body as string)).toEqual({ passphrase: "ma phrase de passe", code: "123456" });
    expect((screen.getByLabelText("Phrase de passe") as HTMLInputElement).value).toBe("");
  });

  it("affiche le message du serveur en cas d'échec de récupération", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse(401, { error: "phrase de passe ou code de récupération incorrect" })));
    render(<Login onLoggedIn={() => undefined} />);
    fireEvent.click(screen.getByRole("tab", { name: "Code de récupération" }));
    fireEvent.change(screen.getByLabelText("Phrase de passe"), { target: { value: "x" } });
    fireEvent.change(screen.getByLabelText(/^Code de récupération/), { target: { value: "AAAA-BBBB-CCCC-DDDD" } });
    fireEvent.click(screen.getByRole("button", { name: "Déverrouiller" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Phrase de passe ou code de récupération incorrect.");
  });

  it("les champs de phrase de passe sont compatibles avec les gestionnaires de mots de passe", () => {
    render(<Login onLoggedIn={() => undefined} />);
    expect(screen.getByLabelText("Phrase de passe").getAttribute("autocomplete")).toBe("current-password");
  });
});

describe("codes de récupération", () => {
  it("exige une confirmation avant de continuer", () => {
    const onDone = vi.fn();
    render(<RecoveryCodes codes={["AAAA-BBBB-CCCC-DDDD"]} onDone={onDone} />);
    const next = screen.getByRole("button", { name: "Continuer" }) as HTMLButtonElement;
    expect(next.disabled).toBe(true);
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(next);
    expect(onDone).toHaveBeenCalled();
  });
});

describe("verrouillage automatique", () => {
  it("décompte, prévient, puis signale l'expiration", () => {
    vi.useFakeTimers();
    let now = 0;
    const last = { current: 0 };
    const onExpired = vi.fn();
    render(<SessionTimer idleSecs={300} lastActivity={last} onExpired={onExpired} now={() => now} />);
    // Discret tant qu'il reste du temps.
    screen.getByText(/Verrouillage auto : 5 min/);
    expect(screen.queryByRole("button", { name: "Rester connecté" })).toBeNull();

    now = 210_000;
    act(() => void vi.advanceTimersByTime(1000));
    screen.getByText(/verrouillage dans 1:30/);
    screen.getByRole("button", { name: "Rester connecté" });

    now = 301_000;
    act(() => void vi.advanceTimersByTime(1000));
    expect(onExpired).toHaveBeenCalled();
  });
});

describe("installation depuis l'application de bureau", () => {
  it("lit le jeton dans le fragment puis l'efface de l'adresse", async () => {
    const { takeSetupToken } = await import("./screens/Setup");
    history.replaceState(null, "", "/#setup=ABCDEF234567");
    expect(takeSetupToken()).toBe("ABCDEF234567");
    expect(window.location.hash).toBe("");
    expect(takeSetupToken()).toBe("");
    history.replaceState(null, "", "/#setup=<script>");
    expect(takeSetupToken()).toBe("");
  });
});

describe("connexion : accompagnement", () => {
  beforeEach(() => {
    try {
      localStorage.clear();
    } catch {
      // stockage indisponible
    }
  });

  it("propose le code à 6 chiffres par défaut tant qu'aucune clé n'a servi", () => {
    render(<Login onLoggedIn={() => undefined} />);
    expect(screen.getByRole("tab", { name: "Code à 6 chiffres" }).getAttribute("aria-selected")).toBe("true");
  });

  it("bascule vers le code à 6 chiffres si aucune clé biométrique n'existe", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse(400, { error: "aucune clé biométrique enregistrée" })));
    render(<Login onLoggedIn={() => undefined} />);
    fireEvent.click(screen.getByRole("tab", { name: "Clé biométrique" }));
    fireEvent.change(screen.getByLabelText("Phrase de passe"), { target: { value: "x" } });
    fireEvent.click(screen.getByRole("button", { name: "Continuer avec ma clé" }));
    expect((await screen.findByRole("alert")).textContent).toMatch(/Aucune clé biométrique n'est encore enregistrée/);
    expect(screen.getByRole("tab", { name: "Code à 6 chiffres" }).getAttribute("aria-selected")).toBe("true");
  });

  it("indique les essais restants, puis propose le déblocage par code de récupération", async () => {
    const replies = [
      jsonResponse(401, { error: "identifiants incorrects : encore 1 essai avant le blocage", remaining_attempts: 1 }),
      jsonResponse(429, { error: "connexion bloquée après 4 échecs : réessayez dans 15 min, ou utilisez un code de récupération pour débloquer tout de suite", retry_after_secs: 900 }),
    ];
    server(() => replies.shift() as Response);
    render(<Login onLoggedIn={() => undefined} />);
    fireEvent.change(screen.getByLabelText("Phrase de passe"), { target: { value: "x" } });
    fireEvent.change(screen.getByLabelText(/Code de l'application/), { target: { value: "000000" } });
    fireEvent.click(screen.getByRole("button", { name: "Déverrouiller" }));
    expect((await screen.findByRole("alert")).textContent).toMatch(/encore 1 essai/);
    expect(screen.queryByRole("button", { name: /Débloquer/ })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Déverrouiller" }));
    const unblock = await screen.findByRole("button", { name: "Débloquer avec un code de récupération" });
    fireEvent.click(unblock);
    expect(screen.getByRole("tab", { name: "Code de récupération" }).getAttribute("aria-selected")).toBe("true");
    screen.getByLabelText(/^Code de récupération/);
  });

  it("permet d'afficher la phrase de passe pour vérifier la saisie", () => {
    render(<Login onLoggedIn={() => undefined} />);
    const input = screen.getByLabelText("Phrase de passe") as HTMLInputElement;
    expect(input.type).toBe("password");
    fireEvent.click(screen.getByRole("button", { name: "Afficher" }));
    expect(input.type).toBe("text");
  });
});
describe("connexion : double vérification", () => {
  it("après 3 échecs, explique que deux vérifications sont demandées", async () => {
    server(() => jsonResponse(204), { quick_unlock: true, step_up: true });
    render(<Login onLoggedIn={() => undefined} />);
    expect((await screen.findByText(/Après 3 échecs, deux vérifications/)).className).toBe("warning");
    screen.getByLabelText("Phrase de passe");
  });
});