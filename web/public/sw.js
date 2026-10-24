// Service worker de Phonalys.
//
// Il ne met en cache QUE l'interface (HTML, JS, CSS, icônes). Les réponses de l'API,
// qui contiennent les données patients, ne sont jamais interceptées ni stockées.

const CACHE = "phonalys-ui-v1";
const SHELL = ["/", "/manifest.webmanifest", "/icon.svg"];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)));
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (event.request.method !== "GET" || url.origin !== self.location.origin) return;
  if (url.pathname.startsWith("/api/")) return; // jamais de données en cache

  if (event.request.mode === "navigate") {
    // Réseau d'abord : l'interface la plus récente ; hors ligne, la copie en cache.
    event.respondWith(fetch(event.request).catch(() => caches.match("/")));
    return;
  }
  if (url.pathname.startsWith("/assets/")) {
    // Fichiers versionnés par leur empreinte : le cache suffit.
    event.respondWith(
      caches.match(event.request).then(
        (hit) =>
          hit ||
          fetch(event.request).then((res) => {
            if (res.ok) {
              const copy = res.clone();
              caches.open(CACHE).then((c) => c.put(event.request, copy));
            }
            return res;
          }),
      ),
    );
  }
});
