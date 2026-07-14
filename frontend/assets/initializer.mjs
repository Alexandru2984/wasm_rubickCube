// Trunk initializer: raporteaza progresul descarcarii WASM in ecranul de
// incarcare din index.html si il ascunde cand aplicatia a pornit.
// v2 — hash nou dupa fixul de MIME .mjs (browserele cachasera octet-stream).
export default function initializer() {
  const bar = () => document.getElementById('ldr-bar');
  const label = () => document.getElementById('ldr-label');

  return {
    onStart: () => {
      const l = label();
      if (l) l.textContent = 'Se descarcă cubul…';
    },
    onProgress: ({ current, total }) => {
      if (!total) return;
      const pct = Math.min(100, Math.round((current / total) * 100));
      const b = bar();
      const l = label();
      if (b) b.style.width = pct + '%';
      if (l) l.textContent = 'Se descarcă cubul… ' + pct + '%';
    },
    onComplete: () => {
      const b = bar();
      const l = label();
      if (b) b.style.width = '100%';
      if (l) l.textContent = 'Pornește motorul 3D…';
    },
    onSuccess: () => {
      const loader = document.getElementById('loader');
      if (loader) {
        loader.classList.add('done');
        setTimeout(() => loader.remove(), 500);
      }
    },
    onFailure: (error) => {
      const l = label();
      if (l) l.textContent = 'Încărcarea a eșuat. Reîncarcă pagina.';
      console.error('WASM load failed:', error);
    },
  };
}
