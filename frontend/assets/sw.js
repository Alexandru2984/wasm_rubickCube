// Service worker: activele cu hash in nume (wasm/js) sunt imuabile → cache-first,
// deci a doua vizita porneste instant si merge offline. index.html ramane
// network-first ca un deploy nou sa fie preluat imediat.
const CACHE = 'cube-v1';

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  if (url.origin !== location.origin || event.request.method !== 'GET') return;

  const hashed = /-[0-9a-f]{8,}(_bg)?\.(wasm|js)$/.test(url.pathname)
    || /\.(png|svg|webmanifest)$/.test(url.pathname);

  if (hashed) {
    event.respondWith(
      caches.open(CACHE).then((cache) =>
        cache.match(event.request).then(
          (cached) =>
            cached ||
            fetch(event.request).then((resp) => {
              if (resp.ok) cache.put(event.request, resp.clone());
              return resp;
            })
        )
      )
    );
  } else if (event.request.mode === 'navigate') {
    event.respondWith(
      fetch(event.request)
        .then((resp) => {
          const copy = resp.clone();
          caches.open(CACHE).then((cache) => cache.put(event.request, copy));
          return resp;
        })
        .catch(() => caches.match(event.request))
    );
  }
});
