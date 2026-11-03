// Feedback effects: synthesized sounds (no audio files), vibration, confetti.

const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

export class Sound {
  constructor() {
    this.ctx = null;
    this.noise = null;
    this.last = 0;
    this.enabled = true;
  }

  /** Must first run inside a user gesture (autoplay policies). */
  unlock() {
    if (!this.enabled) return null;
    if (!this.ctx) {
      const AC = window.AudioContext || window.webkitAudioContext;
      if (!AC) return null;
      try {
        this.ctx = new AC();
      } catch {
        return null;
      }
      const len = Math.floor(this.ctx.sampleRate * 0.05);
      this.noise = this.ctx.createBuffer(1, len, this.ctx.sampleRate);
      const data = this.noise.getChannelData(0);
      for (let i = 0; i < len; i++) data[i] = (Math.random() * 2 - 1) * Math.pow(1 - i / len, 3);
    }
    if (this.ctx.state === 'suspended') this.ctx.resume().catch(() => {});
    return this.ctx;
  }

  /** A short plastic click for a layer turn (rate-limited for fast sequences). */
  click() {
    if (!this.enabled || !this.ctx || this.ctx.state !== 'running') return;
    const now = performance.now();
    if (now - this.last < 40) return;
    this.last = now;
    const ctx = this.ctx;
    const src = ctx.createBufferSource();
    src.buffer = this.noise;
    src.playbackRate.value = 0.85 + Math.random() * 0.3;
    const bp = ctx.createBiquadFilter();
    bp.type = 'bandpass';
    bp.frequency.value = 1700 + Math.random() * 900;
    bp.Q.value = 1.4;
    const g = ctx.createGain();
    g.gain.value = 0.55;
    src.connect(bp).connect(g).connect(ctx.destination);
    src.start();
  }

  tone(freq, at, dur = 0.12, vol = 0.12, type = 'sine') {
    if (!this.enabled || !this.ctx || this.ctx.state !== 'running') return;
    const ctx = this.ctx;
    const o = ctx.createOscillator();
    const g = ctx.createGain();
    o.type = type;
    o.frequency.value = freq;
    const t0 = ctx.currentTime + at;
    g.gain.setValueAtTime(0.0001, t0);
    g.gain.exponentialRampToValueAtTime(vol, t0 + 0.012);
    g.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
    o.connect(g).connect(ctx.destination);
    o.start(t0);
    o.stop(t0 + dur + 0.02);
  }

  beep() {
    this.tone(880, 0, 0.15, 0.1, 'triangle');
  }

  fanfare() {
    [523.25, 659.25, 783.99, 1046.5].forEach((f, i) => this.tone(f, i * 0.09, 0.28, 0.1, 'triangle'));
  }
}

export function vibrate(enabled, pattern) {
  if (enabled && navigator.vibrate && matchMedia('(pointer: coarse)').matches) {
    try {
      navigator.vibrate(pattern);
    } catch {
      /* not allowed */
    }
  }
}

const COLORS = ['#ffffff', '#ffd600', '#b81205', '#ff5900', '#008c38', '#0045ad', '#ffd60a'];

export function confetti(canvas) {
  if (reducedMotion()) return;
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const w = (canvas.width = Math.floor(innerWidth * dpr));
  const h = (canvas.height = Math.floor(innerHeight * dpr));
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const parts = Array.from({ length: 160 }, () => ({
    x: w / 2 + (Math.random() - 0.5) * w * 0.3,
    y: h * 0.45,
    vx: (Math.random() - 0.5) * 18 * dpr,
    vy: (-Math.random() * 17 - 6) * dpr,
    s: (5 + Math.random() * 6) * dpr,
    r: Math.random() * Math.PI,
    vr: (Math.random() - 0.5) * 0.4,
    c: COLORS[(Math.random() * COLORS.length) | 0],
  }));
  const start = performance.now();
  const frame = (now) => {
    const el = now - start;
    ctx.clearRect(0, 0, w, h);
    for (const p of parts) {
      p.vy += 0.45 * dpr;
      p.vx *= 0.99;
      p.x += p.vx;
      p.y += p.vy;
      p.r += p.vr;
      ctx.save();
      ctx.globalAlpha = Math.max(0, 1 - el / 2600);
      ctx.translate(p.x, p.y);
      ctx.rotate(p.r);
      ctx.fillStyle = p.c;
      ctx.fillRect(-p.s / 2, -p.s / 3, p.s, (p.s * 2) / 3);
      ctx.restore();
    }
    if (el < 2600) requestAnimationFrame(frame);
    else ctx.clearRect(0, 0, w, h);
  };
  requestAnimationFrame(frame);
}
