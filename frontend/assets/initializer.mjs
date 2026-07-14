// Trunk initializer: raporteaza progresul descarcarii WASM in ecranul de
// incarcare din index.html si il ascunde cand aplicatia a pornit.
// v3 — winit iese din bucla de init aruncand o sentinela ("Using exceptions
// for control flow"); NU e o eroare reala, aplicatia ruleaza. O tratam ca
// succes, altfel loader-ul ramane peste cubul care deja se randeaza.
export default function initializer() {
  const bar = () => document.getElementById('ldr-bar');
  const label = () => document.getElementById('ldr-label');

  let removed = false;
  const removeLoader = () => {
    if (removed) return;
    removed = true;
    const loader = document.getElementById('loader');
    if (loader) {
      loader.classList.add('done');
      setTimeout(() => loader.remove(), 500);
    }
  };

  // Sentinela winit prin care cedeaza controlul buclei de evenimente a
  // browserului — nu e un esec de incarcare.
  const isControlFlow = (e) => {
    const m = (e && (e.message || e.toString())) || '';
    return m.includes('control flow') || m.includes("isn't actually an error");
  };

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
      removeLoader();
    },
    onFailure: (error) => {
      if (isControlFlow(error)) {
        // Aplicatia ruleaza deja; ascunde loader-ul.
        removeLoader();
        return;
      }
      const l = label();
      if (l) l.textContent = 'Încărcarea a eșuat. Reîncarcă pagina.';
      console.error('WASM load failed:', error);
    },
  };
}
