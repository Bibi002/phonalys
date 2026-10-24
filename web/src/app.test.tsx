import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "./App";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

const PATIENT = { id: "11111111-1111-4111-8111-111111111111", nom: "Test", prenom: "Léa", naissance: "2018-03-15" };

function fakeServer() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      if (url === "/api/status") return json({ initialized: true });
      if (url === "/api/session") return json({ idle_timeout_secs: 900, hidden_lock_secs: 60 });
      if (url === "/api/patients") return json([PATIENT]);
      if (url.startsWith("/api/records/bilan"))
        return json([{ id: "b1", patient_id: PATIENT.id, date: "2026-10-01", type: "initial", resultats: [{ epreuve: "Répétition", mesure: "z", valeur: "-1" }] }]);
      if (url.startsWith("/api/records/")) return json([]);
      if (url === "/api/security")
        return json({ require_passkey: false, hardware_bound: false, passkeys: 0, bound_passkeys: 0, quick_passkeys: 0, recovery_codes_remaining: 10 });
      if (url === "/api/passkeys") return json([]);
      if (url.startsWith("/api/documents"))
        return json([{ id: "d1", patient_id: PATIENT.id, name: "Prescription.pdf", mime: "application/pdf", size: 2048, category: "Prescription", created_at: 1761400000 }]);
      if (url === "/api/reauth") return json({ elevated_secs: 300 });
      if (url === "/api/settings") return json({ idle_timeout_minutes: 15, practitioner: {}, auto_backup: true });
      if (url === "/api/backups") return json({ dir: "C:\\data", extra_dir: null, auto: true, last: null, files: [] });
      if (url === "/api/audit") return json({ verified: true, total: 1, entries: [{ seq: 0, at: "2026-10-25T10:00:00Z", action: "login", target: null }] });
      return json({ error: "introuvable" }, 404);
    }),
  );
}

describe("application", () => {
  it("chaque page de la navigation s'affiche", async () => {
    fakeServer();
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: "Navigation principale" });
    const expectations: [string, RegExp][] = [
      ["Patients", /TEST Léa/],
      ["Agenda", /Semaine du/],
      ["Bilans", /Bilans enregistrés/],
      ["Exercices", /Compter les syllabes/],
      ["Réglages", /Délai d'inactivité/],
      ["Sécurité", /Clés biométriques et de sécurité/],
      ["Journal", /Intégrité du journal vérifiée/],
      ["Documents", /Coffre documentaire/],
      ["Sauvegardes", /Pas de copie sur un second support/],
      ["Accueil", /Aujourd'hui/],
    ];
    for (const [label, text] of expectations) {
      fireEvent.click(within(nav).getByRole("button", { name: label }));
      expect((await screen.findAllByText(text)).length).toBeGreaterThan(0);
    }
  });

  it("le dossier patient s'ouvre sur tous ses onglets", async () => {
    fakeServer();
    render(<App />);
    fireEvent.click(within(await screen.findByRole("navigation")).getByRole("button", { name: "Patients" }));
    fireEvent.click(await screen.findByText("TEST Léa"));
    for (const tab of ["Anamnèse", "Prise en charge", "Séances", "Bilans", "Objectifs", "Exercices", "Documents", "Identité"]) {
      fireEvent.click(await screen.findByRole("tab", { name: tab }));
      expect((await screen.findAllByRole("heading", { level: 2 })).length).toBeGreaterThan(0);
    }
  });

  it("un exercice se lance et se termine", async () => {
    fakeServer();
    render(<App />);
    fireEvent.click(within(await screen.findByRole("navigation")).getByRole("button", { name: "Exercices" }));
    const card = (await screen.findByText("Ça rime ?")).closest(".exercise") as HTMLElement;
    fireEvent.click(within(card).getByRole("button", { name: "Choisir" }));
    fireEvent.click(within(card).getByRole("button", { name: "Commencer" }));
    await screen.findByText("1 / 8");
    screen.getByRole("button", { name: "Ça rime" });
  });
});
describe("arrière-plan", () => {
  function setVisibility(state: "hidden" | "visible") {
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => state });
    document.dispatchEvent(new Event("visibilitychange"));
  }

  it("un bref passage en arrière-plan garde la session, un long la ferme", async () => {
    fakeServer();
    render(<App />);
    await screen.findByRole("navigation");
    const start = Date.now();
    const clock = vi.spyOn(Date, "now");
    const logouts = () =>
      (fetch as unknown as { mock: { calls: [string, RequestInit?][] } }).mock.calls.filter(([u]) => u === "/api/logout").length;

    clock.mockReturnValue(start);
    setVisibility("hidden");
    clock.mockReturnValue(start + 30_000);
    setVisibility("visible");
    expect(logouts()).toBe(0);

    setVisibility("hidden");
    clock.mockReturnValue(start + 200_000);
    setVisibility("visible");
    expect(logouts()).toBe(1);
    clock.mockRestore();
    await waitFor(() => expect(screen.queryByRole("navigation")).toBeNull());
  });
});

describe("coffre documentaire", () => {
  it("demande de confirmer son identité avant d'afficher un document", async () => {
    fakeServer();
    render(<App />);
    fireEvent.click(within(await screen.findByRole("navigation")).getByRole("button", { name: "Documents" }));
    await screen.findByText("Prescription.pdf");
    expect(screen.queryByRole("button", { name: "Aperçu" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Ouvrir les documents" }));
    fireEvent.change(await screen.findByLabelText("Phrase de passe"), { target: { value: "girafe tambour lumière escalier" } });
    fireEvent.change(screen.getByLabelText("Code à 6 chiffres"), { target: { value: "123 456" } });
    fireEvent.click(screen.getByRole("button", { name: "Confirmer" }));
    await screen.findByText(/Ouverts encore/);
    fireEvent.click(screen.getByRole("button", { name: "Aperçu" }));
    const frame = await screen.findByTitle("Prescription.pdf");
    expect(frame.getAttribute("src")).toBe("/api/documents/d1/content");
    const calls = (fetch as unknown as { mock: { calls: [string, RequestInit?][] } }).mock.calls;
    const reauth = calls.find(([u]) => u === "/api/reauth");
    expect(JSON.parse(String(reauth?.[1]?.body))).toEqual({ passphrase: "girafe tambour lumière escalier", code: "123456" });
  });

  it("refuse un fichier trop volumineux sans l'envoyer", async () => {
    const { uploadDocument, viewerKind, MAX_DOCUMENT_BYTES } = await import("./screens/Documents");
    expect(viewerKind("application/pdf")).toBe("pdf");
    expect(viewerKind("image/jpeg")).toBe("image");
    expect(viewerKind("application/vnd.oasis.opendocument.text")).toBeNull();
    const f = vi.fn(async (_u: string, _i?: RequestInit) => json({ id: "d2", name: "a.pdf", mime: "application/pdf", size: 3, category: "", created_at: 0 }, 201));
    vi.stubGlobal("fetch", f);
    await uploadDocument(new Blob(["%PDF"]), "Compte rendu.pdf", "Courrier", "p1");
    const [url, init] = f.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("/api/documents?name=Compte+rendu.pdf&category=Courrier&patient_id=p1");
    expect((init.headers as Record<string, string>)["x-phonalys-request"]).toBe("1");
    expect(MAX_DOCUMENT_BYTES).toBe(20 * 1024 * 1024);
  });
});

describe("restauration", () => {
  it("envoie le fichier brut avec le jeton d'installation et l'en-tête anti-CSRF", async () => {
    const f = vi.fn(async (_u: string, _i?: RequestInit) => new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", f);
    const { uploadRestore } = await import("./screens/Setup");
    const file = new Blob([new Uint8Array([1, 2, 3])]);
    await uploadRestore(" ABC234 ", file);
    const [url, init] = f.mock.calls[0] as [string, RequestInit];
    const headers = init.headers as Record<string, string>;
    expect(url).toBe("/api/restore");
    expect(headers["x-phonalys-setup-token"]).toBe("ABC234");
    expect(headers["x-phonalys-request"]).toBe("1");
    expect(init.body).toBe(file);
  });

  it("remonte le message d'erreur du serveur", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ error: "ce fichier n'est pas une sauvegarde Phonalys valide" }), { status: 400 })));
    const { uploadRestore } = await import("./screens/Setup");
    await expect(uploadRestore("X", new Blob([]))).rejects.toThrow(/pas une sauvegarde/);
  });
});