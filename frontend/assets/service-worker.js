// Service worker (served as /service-worker.js, never cached by HTTP).
//
// - Content-hashed assets (wasm, js, the ui-<hash>/ bundle) are immutable:
//   precached on install, then served cache-first. The list and the cache
//   version are injected at build time by tools/postbuild.py.
// - Navigations are network-first (a deploy is picked up immediately) with
//   the cached shell as the offline fallback.
// - A new version waits until the page asks it to take over (the page shows
//   an "Update" toast), so a running solve is never interrupted.
const VERSION = '__CACHE_VERSION__';
const PRECACHE = __PRECACHE__;
const CACHE = `cube-${VERSION}`;
const SHELL = '/';

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) =>
        Promise.all([
          // Hashed files are immutable, so the HTTP-cache copy the page just
          // downloaded is exactly right (no second 4 MB download).
          cache.addAll(PRECACHE),
          cache.add(new Request(SHELL, { cache: 'reload' })),
        ]),
      )
      // Replacing the first-generation worker (/sw.js, cache "cube-v1"): it
      // never asks before updating, so take over right away instead of
      // showing an "Update" toast on the very first visit after the upgrade.
      .then(() => caches.has('cube-v1'))
      .then((legacy) => legacy && self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k.startsWith('cube-') && k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener('message', (event) => {
  if (event.origin && event.origin !== self.location.origin) return;
  if (event.data && event.data.type === 'SKIP_WAITING') self.skipWaiting();
});

const IMMUTABLE = /^\/(ui-[0-9a-f]{12}\/|[\w.-]+-[0-9a-f]{16}(_bg)?\.(wasm|js|mjs|css)$|[0-9a-f]{16}-[\w.-]+\.mjs$)/;

self.addEventListener('fetch', (event) => {
  const req = event.request;
  if (req.method !== 'GET') return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;
  if (url.pathname.startsWith('/cdn-cgi/')) return;

  if (req.mode === 'navigate') {
    event.respondWith(
      fetch(req)
        .then((resp) => {
          if (resp.ok && url.pathname === '/') {
            const copy = resp.clone();
            caches.open(CACHE).then((c) => c.put(SHELL, copy));
          }
          return resp;
        })
        .catch(() => caches.match(SHELL)),
    );
    return;
  }

  if (IMMUTABLE.test(url.pathname)) {
    event.respondWith(
      caches.open(CACHE).then((cache) =>
        cache.match(req).then(
          (hit) =>
            hit ||
            fetch(req).then((resp) => {
              if (resp.ok) cache.put(req, resp.clone());
              return resp;
            }),
        ),
      ),
    );
    return;
  }

  // Icons, manifest: stale-while-revalidate.
  if (/\.(png|svg|webmanifest|ico)$/.test(url.pathname)) {
    event.respondWith(
      caches.open(CACHE).then((cache) =>
        cache.match(req).then((hit) => {
          const net = fetch(req)
            .then((resp) => {
              if (resp.ok) cache.put(req, resp.clone());
              return resp;
            })
            .catch(() => hit);
          return hit || net;
        }),
      ),
    );
  }
});
