// Daily-challenge leaderboard client: local state, input rules and the two
// API calls. The server (backend/, Gleam) replays every solve before ranking
// it; the checks here only spare the user a round trip.

export const NAME_MIN = 2;
export const NAME_MAX = 16;
export const TOP = 50;
const KEEP_DAYS = 60;
const TIMEOUT_MS = 10_000;

// Same set as backend/src/cube_leaderboard/validate.gleam: ASCII letters and
// digits, space . _ - ' and Latin letters with diacritics (U+00C0–U+024F
// minus × and ÷). No emoji, symbols, control or bidi characters.
const LETTER = 'A-Za-z0-9\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u024F';
const NAME_RE = new RegExp(`^[${LETTER} ._'\\-]+$`, 'u');
const HAS_LETTER_RE = new RegExp(`[${LETTER}]`, 'u');
const TOKEN_RE = /^[0-9a-f]{32}$/;
const ID_RE = /^[0-9a-f]{16}$/;

const isObj = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const isInt = (v, min, max) => Number.isInteger(v) && v >= min && v <= max;

/** Trimmed name with inner spaces collapsed, or null if the server would refuse it. */
export function cleanName(raw) {
  if (typeof raw !== 'string') return null;
  const name = raw.split(' ').filter(Boolean).join(' ');
  const len = [...name].length;
  if (len < NAME_MIN || len > NAME_MAX) return null;
  if (!NAME_RE.test(name) || !HAS_LETTER_RE.test(name)) return null;
  return name;
}

/** 128 random bits as hex: this browser's secret for "one entry a day". */
export function newToken() {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return [...bytes].map((b) => b.toString(16).padStart(2, '0')).join('');
}

const cleanEntry = (e) =>
  isObj(e) && ID_RE.test(e.id) && isInt(e.rank, 1, 1e6) && isInt(e.total, 1, 1e6) && isInt(e.ms, 0, 3_600_000) && isInt(e.moves, 0, 5000)
    ? { id: e.id, rank: e.rank, total: e.total, ms: e.ms, moves: e.moves }
    : null;

/**
 * Validated state from storage. `name` null = not opted in.
 * `entries`: day → the entry the server returned. `pending`: a solve waiting
 * to be published (no name yet, or offline). `fresh` = a token was minted
 * and the state should be saved.
 */
export function loadState(raw, today) {
  const src = isObj(raw) ? raw : {};
  const state = {
    name: cleanName(src.name),
    token: typeof src.token === 'string' && TOKEN_RE.test(src.token) ? src.token : null,
    entries: new Map(),
    pending: null,
    fresh: false,
  };
  if (!state.token) {
    state.token = newToken();
    state.fresh = true;
  }
  if (Array.isArray(src.entries)) {
    for (const pair of src.entries.slice(-KEEP_DAYS * 2)) {
      if (!Array.isArray(pair) || !isInt(pair[0], today - KEEP_DAYS, today + 1)) continue;
      const e = cleanEntry(pair[1]);
      if (e) state.entries.set(pair[0], e);
    }
  }
  const p = src.pending;
  if (isObj(p) && isOpen(p.day, today) && isInt(p.ms, 0, 3_600_000) && typeof p.sol === 'string' && p.sol.length > 0 && p.sol.length <= 16_000 && !state.entries.has(p.day)) {
    state.pending = { day: p.day, ms: p.ms, sol: p.sol };
  }
  return state;
}

export function toJSON(state) {
  return { name: state.name, token: state.token, entries: [...state.entries], pending: state.pending };
}

/** The server takes today's and yesterday's challenge (a solve may cross midnight). */
export function isOpen(day, today) {
  return Number.isInteger(day) && (day === today || day === today - 1);
}

function withTimeout() {
  if (typeof AbortSignal !== 'undefined' && AbortSignal.timeout) return AbortSignal.timeout(TIMEOUT_MS);
  const ctl = new AbortController();
  setTimeout(() => ctl.abort(), TIMEOUT_MS);
  return ctl.signal;
}

const cleanRow = (r) =>
  isObj(r) && isInt(r.rank, 1, 1e6) && typeof r.name === 'string' && r.name.length <= 64 && isInt(r.ms, 0, 3_600_000) && isInt(r.moves, 0, 5000)
    ? { rank: r.rank, name: r.name, ms: r.ms, moves: r.moves }
    : null;

/** Validated board from the API's JSON, or null. */
export function parseBoard(d) {
  if (!isObj(d) || !Number.isInteger(d.day) || !Number.isInteger(d.today) || !isInt(d.total, 0, 1e6) || !Array.isArray(d.top)) return null;
  const top = d.top.slice(0, TOP).map(cleanRow).filter(Boolean);
  return { day: d.day, today: d.today, total: d.total, top, me: d.me == null ? null : cleanRow(d.me) };
}

/** GET a day's board. Resolves to a board, or null when unavailable. */
export async function fetchBoard(day, meId) {
  const q = meId && ID_RE.test(meId) ? `?me=${meId}` : '';
  try {
    const resp = await fetch(`/api/daily/${day}${q}`, { signal: withTimeout(), cache: 'no-store', credentials: 'omit' });
    if (!resp.ok) return null;
    return parseBoard(await resp.json());
  } catch {
    return null;
  }
}

/**
 * POST a solve. Resolves to { ok: true, entry } or { ok: false, status, code }
 * (status 0 = network).
 */
export async function submit(day, { name, token, ms, sol }) {
  let resp;
  try {
    resp = await fetch(`/api/daily/${day}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, token, ms: Math.round(ms), sol }),
      signal: withTimeout(),
      cache: 'no-store',
      credentials: 'omit',
    });
  } catch {
    return { ok: false, status: 0, code: 'network' };
  }
  let data = null;
  try {
    data = await resp.json();
  } catch {
    /* nginx error pages (413, 429, 502) aren't JSON */
  }
  if (resp.ok) {
    const entry = cleanEntry(data);
    return entry ? { ok: true, entry, name: typeof data.name === 'string' ? data.name : name } : { ok: false, status: resp.status, code: 'bad_response' };
  }
  return { ok: false, status: resp.status, code: isObj(data) && typeof data.error === 'string' ? data.error : '' };
}

/**
 * What to do after a failed submit:
 * - 'retry': keep it pending (offline, rate limited, server trouble);
 * - 'name': the name was refused, ask for another and keep it pending;
 * - 'drop': it will never be accepted (with an i18n key for why).
 */
export function failureAction({ status, code }) {
  if (status === 0 || status >= 500 || (status === 429 && code !== 'limit')) return { action: 'retry', msg: status === 0 ? 'lbQueued' : 'lbFailed' };
  if (status === 422 && code === 'name') return { action: 'name', msg: 'lbBadName' };
  if (status === 422 && code === 'day_closed') return { action: 'drop', msg: 'lbClosed' };
  if (status === 429) return { action: 'drop', msg: 'lbLimit' };
  if (status === 422) return { action: 'drop', msg: 'lbRejected' };
  return { action: 'drop', msg: 'lbFailed' };
}
