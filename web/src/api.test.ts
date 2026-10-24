import { api, ApiError, setApiHooks } from "./api";

function mockFetch(status: number, body?: unknown, headers: Record<string, string> = {}) {
  const fn = vi.fn(async (_url: string, _init?: RequestInit) =>
    new Response(body === undefined ? null : JSON.stringify(body), { status, headers }),
  );
  vi.stubGlobal("fetch", fn);
  return fn;
}

afterEach(() => {
  vi.unstubAllGlobals();
  setApiHooks({});
});

describe("api", () => {
  it("ajoute l'en-tête anti-CSRF aux requêtes modifiantes, pas aux lectures", async () => {
    const f = mockFetch(200, { ok: true });
    await api("GET", "/api/patients");
    await api("POST", "/api/patients", { nom: "X" });
    const get = f.mock.calls[0]?.[1]?.headers as Record<string, string>;
    const post = f.mock.calls[1]?.[1]?.headers as Record<string, string>;
    expect(get["x-phonalys-request"]).toBeUndefined();
    expect(post["x-phonalys-request"]).toBe("1");
    expect(post["content-type"]).toBe("application/json");
    expect(f.mock.calls[1]?.[1]?.credentials).toBe("same-origin");
    expect(f.mock.calls[1]?.[1]?.cache).toBe("no-store");
  });

  it("renvoie le message d'erreur du serveur et le délai d'attente", async () => {
    mockFetch(429, { error: "trop de tentatives, réessayez dans 4 s" }, { "retry-after": "4" });
    const err = await api("POST", "/api/login", {}).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).status).toBe(429);
    expect((err as ApiError).retryAfter).toBe(4);
    expect((err as ApiError).message).toMatch(/trop de tentatives/);
  });

  it("signale une session expirée sur une route protégée uniquement", async () => {
    const unauthorized = vi.fn();
    setApiHooks({ unauthorized });
    mockFetch(401, { error: "authentification requise ou invalide" });
    await api("POST", "/api/login", {}).catch(() => undefined);
    expect(unauthorized).not.toHaveBeenCalled();
    await api("GET", "/api/patients").catch(() => undefined);
    expect(unauthorized).toHaveBeenCalledTimes(1);
  });

  it("note l'activité après une requête réussie", async () => {
    const activity = vi.fn();
    setApiHooks({ activity });
    mockFetch(204);
    await expect(api("POST", "/api/lock")).resolves.toBeUndefined();
    expect(activity).toHaveBeenCalledTimes(1);
  });
});
