// Trunk initializer: reports WASM download progress on the loading screen.
//
// The loader is NOT hidden here: init() returns (or winit throws its "Using
// exceptions for control flow" sentinel, which is not a failure) before the
// renderer exists — wgpu is set up afterwards, asynchronously, and can still
// fail. ui/app.js hides the loader when the engine reports it is ready, or
// shows what went wrong.
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
  // Progress text, unless ui/app.js already reported a failure there (e.g.
  // no WebGL2), which must stay readable.
  const say = (text) => {
    const l = label();
    if (l && !loader()?.hasAttribute('data-failed')) l.textContent = text;
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
    onStart: () => say(s.dl),
    onProgress: ({ current, total }) => {
      if (!total) return;
      const pct = Math.min(100, Math.round((current / total) * 100));
      progress(pct);
      say(`${s.dl} ${pct}%`);
    },
    onComplete: () => {
      progress(100);
      say(s.start);
    },
    onSuccess: () => {},
    onFailure: (error) => {
      if (isControlFlow(error)) return;
      say(s.fail);
      const acts = document.getElementById('ldr-actions');
      if (acts) acts.hidden = false;
      console.error('WASM load failed:', error);
    },
  };
}
