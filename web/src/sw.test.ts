// Vérifie que le service worker ne met jamais en cache les réponses de l'API.
import source from "../public/sw.js?raw";

type Listener = (event: { request: Request; respondWith: (r: unknown) => void; waitUntil: (p: unknown) => void }) => void;

function loadWorker() {
  const listeners: Record<string, Listener> = {};
  const self = {
    location: { origin: "https://phonalys.lan:8443" },
    addEventListener: (type: string, fn: Listener) => (listeners[type] = fn),
    skipWaiting: () => undefined,
    clients: { claim: async () => undefined },
  };
  new Function("self", "caches", "fetch", source)(self, { match: async () => undefined, open: async () => ({ put: async () => undefined }) }, () => Promise.reject(new Error("réseau")));
  return listeners;
}

function dispatch(listeners: Record<string, Listener>, url: string, init?: RequestInit & { mode?: RequestMode }) {
  const responded = vi.fn();
  const request = { url, method: init?.method ?? "GET", mode: init?.mode ?? "cors" } as Request;
  listeners.fetch?.({ request, respondWith: responded, waitUntil: () => undefined });
  return responded;
}

describe("service worker", () => {
  it("n'intercepte aucune requête d'API", () => {
    const l = loadWorker();
    expect(dispatch(l, "https://phonalys.lan:8443/api/patients")).not.toHaveBeenCalled();
    expect(dispatch(l, "https://phonalys.lan:8443/api/patients/1", { mode: "navigate" })).not.toHaveBeenCalled();
    expect(dispatch(l, "https://phonalys.lan:8443/api/session")).not.toHaveBeenCalled();
  });

  it("ignore les écritures et les autres origines", () => {
    const l = loadWorker();
    expect(dispatch(l, "https://phonalys.lan:8443/assets/a.js", { method: "POST" })).not.toHaveBeenCalled();
    expect(dispatch(l, "https://ailleurs.example/assets/a.js")).not.toHaveBeenCalled();
  });

  it("sert l'interface", () => {
    const l = loadWorker();
    expect(dispatch(l, "https://phonalys.lan:8443/patients", { mode: "navigate" })).toHaveBeenCalled();
  });
});
