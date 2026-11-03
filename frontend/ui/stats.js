// Pure logic: solve records, WCA averages, streaks, achievements, validation.
// No DOM here, so it runs under `node --test` too.

export const DNF = -1;
export const MAX_SOLVES = 5000;
const DAY_MS = 86_400_000;
/** Day #1 of the daily challenge (UTC). */
export const LAUNCH_DAY = Date.UTC(2026, 10, 3) / DAY_MS;

/** UTC day number for a timestamp. */
export function utcDay(ms = Date.now()) {
  return Math.floor(ms / DAY_MS);
}

export function dailyNumber(day) {
  return day - LAUNCH_DAY + 1;
}

/** Effective time of a solve: +2 adds 2 s, DNF is infinite. */
export function eff(s) {
  if (s.p === DNF) return Infinity;
  return s.t + (s.p > 0 ? s.p : 0);
}

/** WCA-style display: truncated to centiseconds, m:ss.cc past a minute. */
export function fmt(ms) {
  if (ms == null || Number.isNaN(ms)) return '—';
  if (!Number.isFinite(ms)) return 'DNF';
  const cs = Math.floor(ms / 10);
  const s = Math.floor(cs / 100);
  const c = String(cs % 100).padStart(2, '0');
  if (s < 60) return `${s}.${c}`;
  const m = Math.floor(s / 60);
  return `${m}:${String(s % 60).padStart(2, '0')}.${c}`;
}

/**
 * Average of the last k solves with WCA trimming (1 best + 1 worst for ao5 /
 * ao12; 5 % each side for longer). More DNFs than the trim → DNF (Infinity).
 */
export function averageOf(solves, k, end = solves.length) {
  if (end < k || k < 3) return null;
  const times = solves.slice(end - k, end).map(eff).sort((a, b) => a - b);
  const trim = k <= 12 ? 1 : Math.ceil(k * 0.05);
  const kept = times.slice(trim, k - trim);
  if (kept.some((x) => !Number.isFinite(x))) return Infinity;
  return kept.reduce((a, b) => a + b, 0) / kept.length;
}

export function mean(solves) {
  const ok = solves.map(eff).filter(Number.isFinite);
  return ok.length ? ok.reduce((a, b) => a + b, 0) / ok.length : null;
}

export function bestSingle(solves) {
  let best = null;
  for (const s of solves) {
    const e = eff(s);
    if (Number.isFinite(e) && (best === null || e < best)) best = e;
  }
  return best;
}

/** Best rolling average of k (null if never reached). */
export function bestAverage(solves, k) {
  let best = null;
  for (let end = k; end <= solves.length; end++) {
    const a = averageOf(solves, k, end);
    if (a !== null && Number.isFinite(a) && (best === null || a < best)) best = a;
  }
  return best;
}

export function summary(solves) {
  return {
    count: solves.length,
    best: bestSingle(solves),
    ao5: averageOf(solves, 5),
    ao12: averageOf(solves, 12),
    ao100: averageOf(solves, 100),
    mean: mean(solves),
    bestAo5: bestAverage(solves, 5),
  };
}

/** Consecutive completed days ending today (or yesterday, if today isn't done yet). */
export function streak(dailyDays, today = utcDay()) {
  const done = new Set(dailyDays);
  let d = done.has(today) ? today : today - 1;
  let n = 0;
  while (done.has(d)) {
    n++;
    d--;
  }
  return n;
}

// ── Validation of anything read back from storage or an imported file ──────

const KINDS = new Set(['s', 'd', 't', 'x']);

export function isSolve(s) {
  return (
    s !== null && typeof s === 'object' && !Array.isArray(s) &&
    Number.isFinite(s.t) && s.t >= 0 && s.t < 7 * DAY_MS &&
    Number.isInteger(s.n) && s.n >= 2 && s.n <= 20 &&
    Number.isInteger(s.m) && s.m >= 0 && s.m < 1e6 &&
    Number.isFinite(s.d) && s.d > 0 && s.d < 4e12 &&
    (s.p === 0 || s.p === 2000 || s.p === DNF) &&
    (s.s === undefined || (typeof s.s === 'string' && s.s.length <= 2000)) &&
    (s.k === undefined || KINDS.has(s.k)) &&
    (s.c === undefined || (typeof s.c === 'string' && s.c.length <= 40)) &&
    (s.day === undefined || Number.isInteger(s.day))
  );
}

/** Copies only known fields, so nothing unexpected rides along. */
export function cleanSolve(s) {
  const out = { t: Math.round(s.t), n: s.n, m: s.m, d: s.d, p: s.p };
  if (s.s !== undefined) out.s = s.s;
  if (s.k !== undefined) out.k = s.k;
  if (s.c !== undefined) out.c = s.c;
  if (s.day !== undefined) out.day = s.day;
  return out;
}

export function sanitizeSolves(list) {
  if (!Array.isArray(list)) return [];
  return list.filter(isSolve).map(cleanSolve).slice(-MAX_SOLVES);
}

/** Merges imported solves, skipping duplicates (same date, size and time). */
export function mergeSolves(current, incoming) {
  const key = (s) => `${s.d}|${s.n}|${s.t}`;
  const seen = new Set(current.map(key));
  const added = [];
  for (const s of sanitizeSolves(incoming)) {
    if (!seen.has(key(s))) {
      seen.add(key(s));
      added.push(s);
    }
  }
  const merged = current.concat(added).sort((a, b) => a.d - b.d).slice(-MAX_SOLVES);
  return { merged, added: added.length };
}

/** CSV export; cells that a spreadsheet would treat as formulas are defused. */
export function toCsv(solves) {
  const cell = (v) => {
    let s = String(v ?? '');
    if (/^[=+\-@\t\r]/.test(s)) s = `'${s}`;
    return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  const rows = [['#', 'size', 'time', 'time_ms', 'penalty', 'moves', 'date', 'mode', 'case', 'scramble']];
  solves.forEach((s, i) =>
    rows.push([
      i + 1, `${s.n}x${s.n}`, fmt(eff(s)), s.t, s.p === DNF ? 'DNF' : s.p ? '+2' : '', s.m,
      new Date(s.d).toISOString(), { s: 'scramble', d: 'daily', t: 'trainer', x: 'setup' }[s.k] || '', s.c || '', s.s || '',
    ]),
  );
  return rows.map((r) => r.map(cell).join(',')).join('\n');
}

// ── Achievements ───────────────────────────────────────────────────────────

export const ACHIEVEMENTS = [
  ['first', '🧊'], ['ten', '🔟'], ['fifty', '🏅'], ['hundred', '💯'],
  ['sub60', '⏱️'], ['sub30', '⚡'], ['sub20', '🚀'], ['sub10', '👑'],
  ['pocket', '🎲'], ['big', '🏔️'], ['giant', '🗻'],
  ['daily', '📅'], ['streak3', '🔥'], ['streak7', '🌟'], ['streak30', '🏆'],
  ['trainer10', '🎓'], ['pllAll', '🧠'], ['efficient', '🎯'], ['tps', '💨'], ['owl', '🦉'],
  ['solver', '✨'], ['pattern', '🎨'], ['sharer', '📣'],
];

/** Achievements earned by the solve history (flag-style ones are unlocked directly). */
export function earned({ solves, dailyDays, trainerCases, pllCases, today = utcDay() }) {
  const out = new Set();
  const timed = solves.filter((s) => s.p !== DNF);
  const total = timed.length;
  const three = timed.filter((s) => s.n === 3);
  const best3 = bestSingle(three);
  if (total >= 1) out.add('first');
  if (total >= 10) out.add('ten');
  if (total >= 50) out.add('fifty');
  if (total >= 100) out.add('hundred');
  if (best3 !== null) {
    if (best3 < 60000) out.add('sub60');
    if (best3 < 30000) out.add('sub30');
    if (best3 < 20000) out.add('sub20');
    if (best3 < 10000) out.add('sub10');
  }
  if (timed.some((s) => s.n === 2)) out.add('pocket');
  if (timed.some((s) => s.n >= 5)) out.add('big');
  if (timed.some((s) => s.n >= 7)) out.add('giant');
  if (dailyDays.length >= 1) out.add('daily');
  const st = streak(dailyDays, today);
  if (st >= 3) out.add('streak3');
  if (st >= 7) out.add('streak7');
  if (st >= 30) out.add('streak30');
  if (trainerCases >= 10) out.add('trainer10');
  if (pllCases >= 21) out.add('pllAll');
  if (three.some((s) => s.k !== 't' && s.m > 0 && s.m < 60)) out.add('efficient');
  if (three.some((s) => s.m >= 20 && s.t > 0 && s.m / (s.t / 1000) >= 3)) out.add('tps');
  if (timed.some((s) => new Date(s.d).getHours() < 5)) out.add('owl');
  return out;
}
