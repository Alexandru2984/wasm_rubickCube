// Result card: a square PNG of a solve, attached when sharing. Drawn with
// canvas 2D from a handful of values, so it needs no assets, works offline
// and stays within the page's CSP.

export const SIZE = 1080;

const FONT = 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';
const MONO = 'ui-monospace, "SF Mono", "Cascadia Mono", "Roboto Mono", Menlo, Consolas, monospace';
const ACCENT = '#ffd60a';
const MUTED = 'rgba(214, 220, 245, 0.66)';
const BODY = '#0b0b0f';

/**
 * Draws a card onto a 2D context (SIZE × SIZE).
 * @param {CanvasRenderingContext2D} ctx
 * @param {{ kicker: string, time: string, stats: string, date: string,
 *           pills: string[], colors: string[], site: string }} card
 *   `colors`: the cube's [top, left, right] face colors.
 */
export function draw(ctx, card) {
  const S = SIZE;
  const bg = ctx.createRadialGradient(S / 2, S * 0.42, 0, S / 2, S * 0.42, S * 0.78);
  bg.addColorStop(0, '#262b55');
  bg.addColorStop(0.5, '#151830');
  bg.addColorStop(1, '#0a0b16');
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, S, S);

  ctx.textBaseline = 'alphabetic';
  ctx.textAlign = 'center';

  // Brand: a tiny 3x3 face and the name.
  const brandY = 128;
  ctx.font = `700 46px ${FONT}`;
  const nameW = ctx.measureText('Cube').width;
  const logo = 50;
  const x0 = S / 2 - (logo + 18 + nameW) / 2;
  const tiles = ['#00a03c', '#ffd500', '#c4001b', '#ffffff', '#0046ad', '#ff5900', '#ffd500', '#ffffff', '#00a03c'];
  roundRect(ctx, x0, brandY - 40, logo, logo, 10);
  ctx.fillStyle = BODY;
  ctx.fill();
  tiles.forEach((c, i) => {
    roundRect(ctx, x0 + 4 + (i % 3) * 14.3, brandY - 36 + Math.floor(i / 3) * 14.3, 12.3, 12.3, 3);
    ctx.fillStyle = c;
    ctx.fill();
  });
  ctx.textAlign = 'left';
  ctx.fillStyle = '#eceefb';
  ctx.fillText('Cube', x0 + logo + 18, brandY);
  ctx.textAlign = 'center';

  // Kicker, time, stats.
  ctx.fillStyle = ACCENT;
  ctx.font = `800 42px ${FONT}`;
  spaced(ctx, card.kicker.toUpperCase(), S / 2, 262, 5);

  ctx.fillStyle = '#ffffff';
  ctx.font = `700 ${fitSize(ctx, card.time, 240, S - 140, MONO)}px ${MONO}`;
  ctx.shadowColor = 'rgba(0, 0, 0, 0.45)';
  ctx.shadowBlur = 30;
  ctx.shadowOffsetY = 8;
  ctx.fillText(card.time, S / 2, 488);
  ctx.shadowColor = 'transparent';

  ctx.fillStyle = MUTED;
  ctx.font = `500 42px ${FONT}`;
  ctx.fillText(card.stats, S / 2, 566);

  // Pills: rank, streak, personal best.
  if (card.pills.length) {
    ctx.font = `700 36px ${FONT}`;
    const padX = 30;
    const h = 66;
    const gap = 18;
    const widths = card.pills.map((p) => ctx.measureText(p).width + padX * 2);
    let x = S / 2 - (widths.reduce((a, b) => a + b, 0) + gap * (widths.length - 1)) / 2;
    card.pills.forEach((p, i) => {
      roundRect(ctx, x, 618, widths[i], h, h / 2);
      ctx.fillStyle = i === 0 ? 'rgba(255, 214, 10, 0.14)' : 'rgba(255, 255, 255, 0.08)';
      ctx.fill();
      ctx.lineWidth = 2;
      ctx.strokeStyle = i === 0 ? 'rgba(255, 214, 10, 0.5)' : 'rgba(255, 255, 255, 0.16)';
      ctx.stroke();
      ctx.fillStyle = i === 0 ? ACCENT : '#eceefb';
      ctx.fillText(p, x + widths[i] / 2, 618 + h / 2 + 13);
      x += widths[i] + gap;
    });
  }

  isoCube(ctx, S / 2, 848, 140, card.colors);

  ctx.fillStyle = MUTED;
  ctx.font = `500 32px ${FONT}`;
  ctx.textAlign = 'left';
  ctx.fillText(card.date, 56, S - 52);
  ctx.textAlign = 'right';
  ctx.fillStyle = '#eceefb';
  ctx.font = `700 32px ${FONT}`;
  ctx.fillText(card.site, S - 56, S - 52);
  ctx.textAlign = 'center';
}

/** Renders the card to a PNG File, or null where canvas export fails. */
export async function toFile(card, name = 'cube-result.png') {
  try {
    const canvas = document.createElement('canvas');
    canvas.width = SIZE;
    canvas.height = SIZE;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    draw(ctx, card);
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'));
    return blob ? new File([blob], name, { type: 'image/png' }) : null;
  } catch {
    return null;
  }
}

/** Largest font size ≤ max at which `text` fits in `width`. */
function fitSize(ctx, text, max, width, family) {
  let size = max;
  ctx.font = `700 ${size}px ${family}`;
  const w = ctx.measureText(text).width;
  if (w > width) size = Math.floor(size * (width / w));
  return size;
}

/** Centered text with letter spacing (canvas letterSpacing isn't everywhere). */
function spaced(ctx, text, cx, y, spacing) {
  const chars = [...text];
  const widths = chars.map((c) => ctx.measureText(c).width);
  const total = widths.reduce((a, b) => a + b, 0) + spacing * (chars.length - 1);
  let x = cx - total / 2;
  const align = ctx.textAlign;
  ctx.textAlign = 'left';
  chars.forEach((c, i) => {
    ctx.fillText(c, x, y);
    x += widths[i] + spacing;
  });
  ctx.textAlign = align;
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

/** A closed polygon with rounded corners. */
function roundPoly(ctx, pts, r) {
  const n = pts.length;
  ctx.beginPath();
  const mid = (a, b) => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
  const start = mid(pts[n - 1], pts[0]);
  ctx.moveTo(start[0], start[1]);
  for (let i = 0; i < n; i++) {
    const p = pts[i];
    const q = pts[(i + 1) % n];
    ctx.arcTo(p[0], p[1], q[0], q[1], r);
  }
  ctx.closePath();
}

const mix = (hex, k) => {
  const v = parseInt(hex.slice(1), 16);
  const c = [(v >> 16) & 255, (v >> 8) & 255, v & 255].map((x) => Math.round(x * k));
  return `rgb(${c[0]}, ${c[1]}, ${c[2]})`;
};

/**
 * An isometric solved 3x3 seen from a corner: (cx, cy) is the front corner
 * where the three faces meet, `h` the on-screen edge length.
 */
function isoCube(ctx, cx, cy, h, [top, left, right]) {
  const k = Math.cos(Math.PI / 6) * h;
  const C = [cx, cy];
  const R = [cx + k, cy - h / 2];
  const L = [cx - k, cy - h / 2];
  const B = [cx, cy + h];
  const BL = [cx - k, cy + h / 2];
  const BR = [cx + k, cy + h / 2];
  const TT = [cx, cy - h];

  ctx.save();
  ctx.shadowColor = 'rgba(0, 0, 0, 0.55)';
  ctx.shadowBlur = 50;
  ctx.shadowOffsetY = 24;
  roundPoly(ctx, [TT, R, BR, B, BL, L], 22);
  ctx.fillStyle = BODY;
  ctx.fill();
  ctx.restore();

  // face: origin + u, v edge vectors (each one cube edge long).
  const faces = [
    { o: L, u: [TT[0] - L[0], TT[1] - L[1]], v: [C[0] - L[0], C[1] - L[1]], color: top, shade: 1 },
    { o: L, u: [C[0] - L[0], C[1] - L[1]], v: [BL[0] - L[0], BL[1] - L[1]], color: left, shade: 0.8 },
    { o: C, u: [R[0] - C[0], R[1] - C[1]], v: [B[0] - C[0], B[1] - C[1]], color: right, shade: 0.64 },
  ];
  const at = (f, s, t) => [f.o[0] + f.u[0] * s + f.v[0] * t, f.o[1] + f.u[1] * s + f.v[1] * t];
  const gap = 0.032;
  for (const f of faces) {
    ctx.fillStyle = mix(f.color, f.shade);
    for (let i = 0; i < 3; i++) {
      for (let j = 0; j < 3; j++) {
        const s0 = i / 3 + gap;
        const s1 = (i + 1) / 3 - gap;
        const t0 = j / 3 + gap;
        const t1 = (j + 1) / 3 - gap;
        roundPoly(ctx, [at(f, s0, t0), at(f, s1, t0), at(f, s1, t1), at(f, s0, t1)], 6);
        ctx.fill();
      }
    }
  }
  // A soft highlight along the top face.
  const g = ctx.createLinearGradient(L[0], L[1], R[0], R[1]);
  g.addColorStop(0, 'rgba(255, 255, 255, 0.10)');
  g.addColorStop(1, 'rgba(255, 255, 255, 0)');
  roundPoly(ctx, [TT, R, C, L], 16);
  ctx.fillStyle = g;
  ctx.fill();
}
