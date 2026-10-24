import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Conformite, cabinet, hoursLeft } from "./screens/Conformite";
import { ExportPatient, exportUrl } from "./screens/Export";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const json = (status: number, body?: unknown) => new Response(body === undefined ? null : JSON.stringify(body), { status });

describe("conformité", () => {
  it("préremplit le registre avec les coordonnées du cabinet et signale les manques", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) =>
        url === "/api/settings" ? json(200, { idle_timeout_minutes: 15, practitioner: { nom: "Cabinet Dupont", inami: "6-12345-67-890" } }) : json(200, []),
      ),
    );
    render(<Conformite />);
    await screen.findAllByText(/Cabinet Dupont/);
    expect(screen.getByText(/INAMI 6-12345-67-890/)).toBeTruthy();
    expect(screen.getAllByText(/à compléter dans Réglages/).length).toBeGreaterThan(0);
    expect(screen.getByText(/Aucun sous-traitant informatique/)).toBeTruthy();

    fireEvent.click(screen.getByRole("tab", { name: "Violation de données" }));
    await screen.findByText("Aucune violation consignée.");
  });

  it("compte le délai de 72 heures de notification", () => {
    const t0 = Date.parse("2026-10-20T10:00");
    expect(hoursLeft("2026-10-20T10:00", t0)).toBe(72);
    expect(hoursLeft("2026-10-20T10:00", t0 + 50 * 3600 * 1000)).toBe(22);
    expect(hoursLeft("2026-10-20T10:00", t0 + 80 * 3600 * 1000)).toBeLessThan(0);
    expect(hoursLeft("")).toBeUndefined();
  });

  it("cabinet() remplace les champs vides", () => {
    expect(cabinet(undefined).nom).toMatch(/à compléter/);
    expect(cabinet({ idle_timeout_minutes: 15, practitioner: { nom: "  X  " } } as never).nom).toBe("X");
  });

  it("l'export du dossier demande une réauthentification", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        if (url === "/api/session") return json(200, { elevated: false, idle_timeout_secs: 900 });
        if (url === "/api/security") return json(200, { passkeys: 0 });
        return json(404);
      }),
    );
    render(<ExportPatient patientId="p1" />);
    fireEvent.click(screen.getByRole("button", { name: "Exporter le dossier complet" }));
    await screen.findByText(/L'export du dossier complet demande une vérification/);
    expect(exportUrl("p 1")).toBe("/api/patients/p%201/export");
  });

  it("l'export est proposé directement si la session est déjà élevée", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => json(200, { elevated: true, elevated_secs: 200, idle_timeout_secs: 900 })));
    render(<ExportPatient patientId="p1" />);
    fireEvent.click(screen.getByRole("button", { name: "Exporter le dossier complet" }));
    await waitFor(() =>
      expect(screen.getByRole("link", { name: "Télécharger le dossier (JSON)" }).getAttribute("href")).toBe("/api/patients/p1/export"),
    );
  });
});
