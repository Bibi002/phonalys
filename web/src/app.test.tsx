import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
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
      if (url === "/api/session") return json({ idle_timeout_secs: 900 });
      if (url === "/api/patients") return json([PATIENT]);
      if (url.startsWith("/api/records/bilan"))
        return json([{ id: "b1", patient_id: PATIENT.id, date: "2026-10-01", type: "initial", resultats: [{ epreuve: "Répétition", mesure: "z", valeur: "-1" }] }]);
      if (url.startsWith("/api/records/")) return json([]);
      if (url === "/api/security")
        return json({ require_passkey: false, hardware_bound: false, passkeys: 0, bound_passkeys: 0, quick_passkeys: 0, recovery_codes_remaining: 10 });
      if (url === "/api/passkeys") return json([]);
      if (url === "/api/settings") return json({ idle_timeout_minutes: 15, practitioner: {} });
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
    for (const tab of ["Anamnèse", "Prise en charge", "Séances", "Bilans", "Objectifs", "Exercices", "Identité"]) {
      fireEvent.click(await screen.findByRole("tab", { name: tab }));
      await screen.findByRole("heading", { level: 2 });
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