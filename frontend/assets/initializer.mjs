// Trunk initializer: reports WASM download progress on the loading screen and
// hides it once the engine runs.
//
// winit leaves its init loop by throwing a sentinel ("Using exceptions for
// control flow"); that is NOT a load failure — the app is already running —
// so it's treated as success, otherwise the loader would cover the cube.
//
// The page language is set by ui/app.js before this runs; strings here stay
// self-contained because this file isn't part of the hashed ui/ bundle.
const STR = {
  en: { dl: 'Downloading the cube…', start: 'Starting the 3D engine…', fail: 'Loading failed. Please reload the page.' },
  ro: { dl: 'Se descarcă cubul…', start: 'Pornește motorul 3D…', fail: 'Încărcarea a eșuat. Reîncarcă pagina.' },
};

export default function initializer() {
  const s = STR[document.documentElement.lang] || STR.en;
  const bar = () => document.getElementById('ldr-bar');
  const label = () => document.getElementById('ldr-label');
  const loader = () => document.getElementById('loader');

  let removed = false;
  const removeLoader = () => {
    if (removed) return;
    removed = true;
    const el = loader();
    if (el) {
      el.classList.add('done');
      setTimeout(() => el.remove(), 500);
    }
  };

  const isControlFlow = (e) => {
    const m = (e && (e.message || String(e))) || '';
    return m.includes('control flow') || m.includes("isn't actually an error");
  };

  const progress = (pct) => {
    const b = bar();
    if (b) b.style.width = `${pct}%`;
    const el = loader();
    if (el) el.setAttribute('aria-valuenow', String(pct));
  };

  return {
    onStart: () => {
      const l = label();
      if (l) l.textContent = s.dl;
    },
    onProgress: ({ current, total }) => {
      if (!total) return;
      const pct = Math.min(100, Math.round((current / total) * 100));
      progress(pct);
      const l = label();
      if (l) l.textContent = `${s.dl} ${pct}%`;
    },
    onComplete: () => {
      progress(100);
      const l = label();
      if (l) l.textContent = s.start;
    },
    onSuccess: () => removeLoader(),
    onFailure: (error) => {
      if (isControlFlow(error)) {
        removeLoader();
        return;
      }
      const l = label();
      if (l) l.textContent = s.fail;
      const acts = document.getElementById('ldr-actions');
      if (acts) acts.hidden = false;
      console.error('WASM load failed:', error);
    },
  };
}
