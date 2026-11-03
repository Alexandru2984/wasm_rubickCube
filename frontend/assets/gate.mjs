// WebGL gate: the first module script on the page (tools/postbuild.py places
// it before ui/app.js and fills in the engine's URL).
//
// It asks for WebGL2 on the engine's own canvas with the engine's options
// (wgpu asks for antialias: false; when it calls getContext later it gets
// this same context back), so the answer is exactly what the engine will
// see. Only when that works does the 1.8 MB engine download start: without
// WebGL2 the engine can't run, so there is no point downloading it, and it
// would only panic. The bootstrap checks the result before starting it.
//
// The result is left on the canvas for ui/app.js, which explains it:
//   data-webgl="ok" | "no", data-webgl-reason = the browser's own words
//   (Chrome sends them in a webglcontextcreationerror event).
const WASM = '__WASM_URL__';

const canvas = document.getElementById('bevy');
if (canvas) {
  let reason = '';
  const onFail = (e) => {
    reason = String(e.statusMessage || '');
  };
  canvas.addEventListener('webglcontextcreationerror', onFail);
  let gl = null;
  try {
    gl = canvas.getContext('webgl2', { antialias: false });
  } catch {
    gl = null;
  }
  canvas.removeEventListener('webglcontextcreationerror', onFail);
  canvas.dataset.webgl = gl ? 'ok' : 'no';
  if (gl) {
    // What Trunk's static preload did, now only when it will be used. Same
    // request as the bootstrap's fetch(), so the browser reuses it.
    const link = document.createElement('link');
    link.rel = 'preload';
    link.as = 'fetch';
    link.type = 'application/wasm';
    link.crossOrigin = 'anonymous';
    link.href = WASM;
    document.head.append(link);
  } else {
    canvas.dataset.webglReason = reason.slice(0, 300);
  }
}
