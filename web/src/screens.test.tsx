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

describe("connexion", () => {
  it("envoie la phrase de passe et le code TOTP, puis efface les champs", async () => {
    const fetch = vi.fn(async () => jsonResponse(204));
    vi.stubGlobal("fetch", fetch);
    const onLoggedIn = vi.fn();
    render(<Login onLoggedIn={onLoggedIn} />);

    fireEvent.click(screen.getByRole("tab", { name: "Code à 6 chiffres" }));
    fireEvent.change(screen.getByLabelText("Phrase de passe"), { target: { value: "ma phrase de passe" } });
    fireEvent.change(screen.getByLabelText(/Code de l'application/), { target: { value: "123 456" } });
    fireEvent.click(screen.getByRole("button", { name: "Déverrouiller" }));

    await waitFor(() => expect(onLoggedIn).toHaveBeenCalled());
    const [url, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("/api/login");
    expect(JSON.parse(init.body as string)).toEqual({ passphrase: "ma phrase de passe", code: "123456" });
    expect((screen.getByLabelText("Phrase de passe") as HTMLInputElement).value).toBe("");
  });

  it("affiche une erreur générique en cas d'échec", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse(401, { error: "authentification requise ou invalide" })));
    render(<Login onLoggedIn={() => undefined} />);
    fireEvent.click(screen.getByRole("tab", { name: "Code de récupération" }));
    fireEvent.change(screen.getByLabelText("Phrase de passe"), { target: { value: "x" } });
    fireEvent.change(screen.getByLabelText(/^Code de récupération/), { target: { value: "AAAA-BBBB-CCCC-DDDD" } });
    fireEvent.click(screen.getByRole("button", { name: "Déverrouiller" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Identifiants ou code invalides.");
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
    render(<SessionTimer idleSecs={90} lastActivity={last} onExpired={onExpired} now={() => now} />);
    screen.getByText(/Verrouillage dans 1:30/);

    now = 40_000;
    act(() => void vi.advanceTimersByTime(1000));
    screen.getByRole("button", { name: "Rester connecté" });

    now = 91_000;
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
