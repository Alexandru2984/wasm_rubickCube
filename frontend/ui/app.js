// Rubik's Cube 3D — page controller.
//
// The WASM engine renders the cube and owns the move logic; this module owns
// every control, the timer, stats, daily challenge, achievements, settings
// and sharing. It talks to the engine through window.wasmBindings.cube_cmd()
// and listens to cube:* events.
//
// Security notes: no innerHTML anywhere — all dynamic text goes through
// textContent (see h()). Storage, URL fragments and imported files are
// validated before use. Engine commands are allow-listed on the Rust side too.

import { t, setLang, getLang, achText } from './i18n.js';
import * as St from './stats.js';
import * as Lb from './lb.js';
import * as Card from './card.js';
import { Sound, vibrate, confetti } from './fx.js';

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const SVGNS = 'http://www.w3.org/2000/svg';
const wideMq = matchMedia('(min-width: 900px)');
const coarse = matchMedia('(pointer: coarse)');
const GITHUB = 'https://github.com/Alexandru2984/wasm_rubickCube';
const NOTATION_RE = /^[RLUDFBMESxyzrludfbw0-9'’′ ()[\],.\n\t]*$/;

// ── DOM builder ─────────────────────────────────────────────────────────────

const ATTRS = new Set([
  'id', 'type', 'role', 'title', 'for', 'name', 'value', 'min', 'max', 'step', 'placeholder', 'rows',
  'href', 'rel', 'target', 'tabindex', 'spellcheck', 'autocomplete', 'inputmode', 'accept', 'viewBox',
  'preserveAspectRatio', 'points', 'd', 'autocapitalize', 'enterkeyhint', 'maxlength', 'download',
]);

/** Element factory. Text only via textContent; attributes from an allow-list. */
function h(tag, props, ...kids) {
  const el = tag.startsWith('svg:') ? document.createElementNS(SVGNS, tag.slice(4)) : document.createElement(tag);
  if (props) {
    for (const [k, v] of Object.entries(props)) {
      if (v == null || v === false) continue;
      if (k === 'class') el.setAttribute('class', v);
      else if (k === 'text') el.textContent = v;
      else if (k === 'on') for (const [ev, fn] of Object.entries(v)) el.addEventListener(ev, fn);
      else if (k === 'data') for (const [dk, dv] of Object.entries(v)) el.dataset[dk] = dv;
      else if (k.startsWith('aria-') || ATTRS.has(k)) el.setAttribute(k, v === true ? '' : String(v));
      else if (k === 'checked' || k === 'disabled' || k === 'hidden') el[k] = Boolean(v);
      else throw new Error(`h(): unsupported prop ${k}`);
    }
  }
  for (const c of kids.flat()) {
    if (c == null || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

function icon(name) {
  const svg = document.createElementNS(SVGNS, 'svg');
  svg.setAttribute('class', 'ic');
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS(SVGNS, 'use');
  use.setAttribute('href', `#i-${name}`);
  svg.append(use);
  return svg;
}

// ── Storage (every read is validated; every access may throw) ───────────────

const KEYS = {
  settings: 'cube.v4.settings', solves: 'cube.v4.solves', daily: 'cube.v4.daily',
  ach: 'cube.v4.ach', trainer: 'cube.v4.trainer', seen: 'cube.v4.seen', lb: 'cube.v4.lb',
};

const store = {
  get(key, fallback = null) {
    try {
      const raw = localStorage.getItem(key);
      return raw == null ? fallback : JSON.parse(raw);
    } catch {
      return fallback;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      /* quota or disabled storage: keep working in memory */
    }
  },
  raw(key) {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  },
  del(key) {
    try {
      localStorage.removeItem(key);
    } catch {
      /* ignore */
    }
  },
};

const isObj = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

// ── Settings ────────────────────────────────────────────────────────────────

const DEFAULTS = {
  lang: (navigator.language || 'en').toLowerCase().startsWith('ro') ? 'ro' : 'en',
  inspection: true, sound: true, haptics: true, hints: true, saver: false,
  scheme: 0, bg: 0, speed: 1,
};

function loadSettings() {
  const raw = store.get(KEYS.settings);
  const s = { ...DEFAULTS };
  if (isObj(raw)) {
    if (raw.lang === 'en' || raw.lang === 'ro') s.lang = raw.lang;
    for (const k of ['inspection', 'sound', 'haptics', 'hints', 'saver']) if (typeof raw[k] === 'boolean') s[k] = raw[k];
    if (Number.isInteger(raw.scheme) && raw.scheme >= 0 && raw.scheme < 5) s.scheme = raw.scheme;
    if (Number.isInteger(raw.bg) && raw.bg >= 0 && raw.bg < 4) s.bg = raw.bg;
    if (Number.isFinite(raw.speed) && raw.speed >= 0.5 && raw.speed <= 3) s.speed = raw.speed;
  }
  return s;
}

let settings = loadSettings();
applyBackground();

/** The canvas is transparent; the page paints the chosen background. */
function applyBackground() {
  document.documentElement.dataset.bg = String(settings.bg);
}
setLang(settings.lang);
const sound = new Sound();
sound.enabled = settings.sound;

function engineSettings() {
  const saver = settings.saver;
  return `speed=${settings.speed};scheme=${settings.scheme};bg=${settings.bg};shadows=${saver ? 0 : 1};dpr=${saver ? 1.25 : 2}`;
}

function saveSettings() {
  store.set(KEYS.settings, settings);
  applyBackground();
  sound.enabled = settings.sound;
  engine.send('settings', engineSettings());
}

// ── Persistent data ─────────────────────────────────────────────────────────

let solves = St.sanitizeSolves(store.get(KEYS.solves, []));
const daily = loadDaily();
const unlocked = loadAch();
const trainerStats = loadTrainer();
migrateV3Bests();
const lb = Lb.loadState(store.get(KEYS.lb), St.utcDay());
if (lb.fresh) saveLb();

function saveLb() {
  store.set(KEYS.lb, Lb.toJSON(lb));
}

function loadDaily() {
  const m = new Map();
  const raw = store.get(KEYS.daily);
  if (isObj(raw)) {
    for (const [k, v] of Object.entries(raw)) {
      if (/^\d{5,6}$/.test(k) && isObj(v) && Number.isFinite(v.t) && v.t >= 0 && Number.isInteger(v.m) && [0, 2000, St.DNF].includes(v.p)) {
        m.set(Number(k), { t: v.t, m: v.m, p: v.p });
      }
    }
  }
  return m;
}

function saveDaily() {
  const keys = [...daily.keys()].sort((a, b) => a - b).slice(-400);
  const out = {};
  for (const k of keys) out[k] = daily.get(k);
  store.set(KEYS.daily, out);
}

function loadAch() {
  const ids = new Set(St.ACHIEVEMENTS.map(([id]) => id));
  const m = new Map();
  const raw = store.get(KEYS.ach);
  if (Array.isArray(raw)) for (const e of raw) if (Array.isArray(e) && ids.has(e[0]) && Number.isFinite(e[1])) m.set(e[0], e[1]);
  return m;
}

function loadTrainer() {
  const m = new Map();
  const raw = store.get(KEYS.trainer);
  if (Array.isArray(raw)) {
    for (const e of raw.slice(0, 200)) {
      if (Array.isArray(e) && typeof e[0] === 'string' && e[0].length <= 40 && isObj(e[1]) && Number.isFinite(e[1].best) && Number.isInteger(e[1].count)) {
        m.set(e[0], { best: e[1].best, count: e[1].count });
      }
    }
  }
  return m;
}

/** v3 kept one best time (seconds) per size; carry them over as solves. */
function migrateV3Bests() {
  let changed = false;
  for (let n = 2; n <= 20; n++) {
    const key = `cube.v3.best.${n}`;
    const raw = store.raw(key);
    if (raw == null) continue;
    const secs = Number(raw);
    if (Number.isFinite(secs) && secs > 0 && secs < 86400 && !solves.some((s) => s.n === n)) {
      solves.push({ t: Math.round(secs * 1000), n, m: 0, d: Date.now(), p: 0, k: 's' });
      changed = true;
    }
    store.del(key);
  }
  if (changed) store.set(KEYS.solves, solves);
}

// ── Engine bridge ───────────────────────────────────────────────────────────

const engine = {
  ready: false,
  queue: [],
  send(name, arg = '') {
    const fn = window.wasmBindings && window.wasmBindings.cube_cmd;
    if (!this.ready || typeof fn !== 'function') {
      if (this.queue.length < 32) this.queue.push([name, String(arg)]);
      return true;
    }
    try {
      return fn(name, String(arg));
    } catch (e) {
      console.error('cube_cmd failed', e);
      return false;
    }
  },
  flush() {
    const q = this.queue.splice(0);
    for (const [n, a] of q) this.send(n, a);
  },
};

function parseDetail(e) {
  try {
    const v = JSON.parse(e.detail);
    return isObj(v) ? v : null;
  } catch {
    return null;
  }
}

let S = {
  n: 3, phase: 'idle', mode: 'free', moves: 0, hist: 0, redo: 0, busy: false, solving: false,
  canUndo: false, canRedo: false, canSolve: false, canRewind: false, scramble: '', trainer: -1,
  player: { active: false, playing: false, idx: 0, len: 0, v: 0 },
};
let catalog = [];
let playerInfo = { v: -1, title: '', kind: 'solution', moves: [] };

// ── Timer ───────────────────────────────────────────────────────────────────

const timerEl = $('#timer');
const timerSub = $('#timerSub');
const T = { phase: 'idle', readyAt: 0, start: 0, penalty: 0, raf: 0, warned: 0, final: null, mode: '' };

function setTimerText(text, cls) {
  timerEl.textContent = text;
  timerEl.className = `timer ${cls}`;
}

function lastTimeFor(n) {
  for (let i = solves.length - 1; i >= 0; i--) if (solves[i].n === n) return St.eff(solves[i]);
  return null;
}

function timerIdle() {
  cancelAnimationFrame(T.raf);
  T.phase = 'idle';
  $('#inspection').hidden = true;
  const last = lastTimeFor(S.n);
  setTimerText(last == null ? '0.00' : St.fmt(last), 'idle');
  updateTimerSub();
}

function updateTimerSub() {
  const list = solves.filter((s) => s.n === S.n);
  const ao5 = St.averageOf(list, 5);
  const best = St.bestSingle(list);
  const parts = [];
  if (best != null) parts.push(`${t('best')} ${St.fmt(best)}`);
  if (ao5 != null) parts.push(`ao5 ${St.fmt(ao5)}`);
  timerSub.textContent = parts.join(' · ');
}

function onReady(mode) {
  T.mode = mode;
  T.penalty = 0;
  T.final = null;
  cancelAnimationFrame(T.raf);
  const inspect = settings.inspection && !mode.startsWith('trainer');
  if (inspect) {
    T.phase = 'inspect';
    T.readyAt = performance.now();
    T.warned = 0;
    $('#inspection').hidden = false;
    setTimerText('0.00', 'ready');
    const tick = () => {
      if (T.phase !== 'inspect') return;
      const el = (performance.now() - T.readyAt) / 1000;
      const num = $('#inspNum');
      if (el < 15) num.textContent = String(Math.ceil(15 - el));
      else if (el < 17) num.textContent = t('plus2');
      else num.textContent = t('dnf');
      num.className = `insp-num${el >= 15 ? ' over' : el >= 8 ? ' warn' : ''}`;
      // WCA judges call out 8 and 12 seconds.
      if (el >= 8 && T.warned < 1) { T.warned = 1; sound.beep(); vibrate(settings.haptics, 30); }
      if (el >= 12 && T.warned < 2) { T.warned = 2; sound.beep(); vibrate(settings.haptics, [30, 60, 30]); }
      T.raf = requestAnimationFrame(tick);
    };
    tick();
  } else {
    T.phase = 'armed';
    setTimerText('0.00', 'ready');
  }
  renderStatus();
}

function onStart() {
  const now = performance.now();
  if (T.phase === 'inspect') {
    const el = (now - T.readyAt) / 1000;
    T.penalty = el > 17 ? St.DNF : el > 15 ? 2000 : 0;
    if (T.penalty) toast(t('inspectionOver'), { em: '⚠️', sub: T.penalty === St.DNF ? t('dnf') : t('plus2') });
  }
  $('#inspection').hidden = true;
  cancelAnimationFrame(T.raf);
  T.phase = 'running';
  T.start = now;
  const tick = () => {
    if (T.phase !== 'running') return;
    setTimerText(St.fmt(performance.now() - T.start), 'running');
    T.raf = requestAnimationFrame(tick);
  };
  tick();
  renderStatus();
}

// ── Solves ──────────────────────────────────────────────────────────────────

function onSolved(ev) {
  cancelAnimationFrame(T.raf);
  const ms = Number.isFinite(ev.ms) ? Math.max(0, ev.ms) : performance.now() - T.start;
  const n = Number.isInteger(ev.n) ? ev.n : S.n;
  const mode = typeof ev.mode === 'string' ? ev.mode : 'scramble';
  const penalty = T.phase === 'running' ? T.penalty : 0;
  T.phase = 'done';

  const prev = solves.filter((s) => s.n === n);
  const prevBest = St.bestSingle(prev);
  const prevBestAo5 = St.bestAverage(prev, 5);

  const solve = { t: Math.round(ms), n, m: Number.isInteger(ev.moves) ? ev.moves : 0, d: Date.now(), p: penalty };
  if (typeof ev.scramble === 'string' && ev.scramble) solve.s = ev.scramble.slice(0, 2000);
  let dailyDay = null;
  let caseName = null;
  if (mode.startsWith('daily:')) {
    solve.k = 'd';
    dailyDay = Number(mode.slice(6));
    if (Number.isInteger(dailyDay)) solve.day = dailyDay;
  } else if (mode.startsWith('trainer:')) {
    solve.k = 't';
    const c = catalog[Number(mode.slice(8))];
    if (c) caseName = solve.c = c.name;
  } else {
    solve.k = 's';
  }
  solves.push(solve);
  if (solves.length > St.MAX_SOLVES) solves = solves.slice(-St.MAX_SOLVES);
  store.set(KEYS.solves, solves);

  let firstDaily = false;
  if (dailyDay !== null && Lb.isOpen(dailyDay, St.utcDay()) && !daily.has(dailyDay)) {
    daily.set(dailyDay, { t: solve.t, m: solve.m, p: solve.p });
    saveDaily();
    firstDaily = true;
  }
  if (caseName) {
    const cur = trainerStats.get(caseName) || { best: Infinity, count: 0 };
    const e = St.eff(solve);
    trainerStats.set(caseName, { best: Math.min(cur.best, Number.isFinite(e) ? e : Infinity), count: cur.count + 1 });
    store.set(KEYS.trainer, [...trainerStats.entries()].map(([k, v]) => [k, { best: Number.isFinite(v.best) ? v.best : 9e9, count: v.count }]));
  }

  const effTime = St.eff(solve);
  // The first attempt at a daily goes to the global leaderboard, with the
  // moves as proof (the server replays them). DNFs and oversized logs don't.
  if (firstDaily && n === 3 && Number.isFinite(effTime) && typeof ev.sol === 'string' && ev.sol && ev.sol.length <= 16_000) {
    lb.pending = { day: dailyDay, ms: Math.round(effTime), sol: ev.sol };
    lbNote = null;
    saveLb();
  }
  setTimerText(St.fmt(effTime), 'done');
  updateTimerSub();

  const list = solves.filter((s) => s.n === n);
  const isPb = Number.isFinite(effTime) && prevBest !== null && effTime < prevBest;
  const ao5 = St.averageOf(list, 5);
  const isPbAo5 = ao5 !== null && Number.isFinite(ao5) && prevBestAo5 !== null && ao5 < prevBestAo5;

  showSolvedModal({ solve, effTime, n, isPb, isPbAo5, ao5, ao12: St.averageOf(list, 12), dailyDay: firstDaily ? dailyDay : null });
  checkAchievements();
  track('solve', { n, mode: solve.k });
  if (currentPanel && ['stats', 'daily', 'trainer'].includes(currentPanel)) renderPanel(currentPanel, true);
}

let lastSolved = null;

function showSolvedModal(info) {
  lastSolved = info;
  const { solve, effTime, n, isPb, isPbAo5, ao5, dailyDay } = info;
  $('#solvedTitle').textContent = dailyDay !== null ? t('dailyDone', { n: St.dailyNumber(dailyDay) }) : t('solved');
  $('#solvedTime').textContent = St.fmt(effTime);
  const tps = solve.t > 0 && solve.m > 0 ? (solve.m / (solve.t / 1000)).toFixed(2) : null;
  const meta = [`${n}×${n}`, t('movesN', { n: solve.m })];
  if (tps) meta.push(t('tps', { v: tps }));
  if (ao5 != null) meta.push(`ao5 ${St.fmt(ao5)}`);
  $('#solvedMeta').textContent = meta.join(' · ');
  const badges = $('#solvedBadges');
  badges.replaceChildren();
  if (solve.p === 2000) badges.append(h('span', { class: 'badge red', text: t('plus2') }));
  if (solve.p === St.DNF) badges.append(h('span', { class: 'badge red', text: t('dnf') }));
  if (isPb) badges.append(h('span', { class: 'badge gold', text: `★ ${t('newPb')}` }));
  if (isPbAo5) badges.append(h('span', { class: 'badge', text: t('newPbAo5') }));
  if (dailyDay !== null) {
    const st = St.streak([...daily.keys()]);
    if (st > 1) badges.append(h('span', { class: 'badge gold', text: `🔥 ${t('streakDays', { n: st })}` }));
  }
  renderSolvedLb();
  if (dailyDay !== null && lb.pending && lb.pending.day === dailyDay && lb.name) publishNow();
  const modal = $('#solvedModal');
  if (!modal.open) modal.showModal();
  // Render the share card once the confetti settles, so Share is instant.
  setTimeout(() => cardFile(dailyDay !== null ? dailyCard(dailyDay) : solvedCard(info)), 1500);
  sound.fanfare();
  vibrate(settings.haptics, [20, 40, 20, 40, 60]);
  confetti($('#confetti'));
}

function shareSolved() {
  if (!lastSolved) return;
  const { solve, effTime, n, dailyDay } = lastSolved;
  if (dailyDay !== null) {
    shareDaily(dailyDay);
    return;
  }
  const text = t('shareSolve', { size: `${n}×${n}`, t: St.fmt(effTime), m: solve.m });
  const url = solve.s ? `${location.origin}/#scramble=${encodeURIComponent(solve.s)}&n=${n}` : location.origin + '/';
  shareText(text, url, solvedCard(lastSolved));
}

function shareDaily(day) {
  const r = daily.get(day);
  if (!r) return;
  const st = St.streak([...daily.keys()]);
  const entry = lb.entries.get(day);
  const text = t('shareDaily', {
    n: St.dailyNumber(day), t: St.fmt(St.eff(r)), m: r.m,
    streak: (st > 1 ? t('shareStreak', { n: st }) : '') + (entry ? t('shareRank', { rank: entry.rank, total: entry.total }) : ''),
  });
  shareText(text, `${location.origin}/#daily`, dailyCard(day));
}

// Result cards ---------------------------------------------------------------

function cardBase(ms, moves, dateMs) {
  const tps = ms > 0 ? (moves / (ms / 1000)).toLocaleString(getLang(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }) : '0';
  return {
    time: St.fmt(ms),
    stats: t('cardStats', { m: moves, tps }),
    date: new Intl.DateTimeFormat(getLang(), { day: 'numeric', month: 'short', year: 'numeric', timeZone: 'UTC' }).format(dateMs),
    colors: SCHEME_PREVIEW[settings.scheme] || SCHEME_PREVIEW[0],
    site: location.host,
  };
}

function dailyCard(day) {
  const r = daily.get(day);
  if (!r) return null;
  const st = St.streak([...daily.keys()]);
  const entry = lb.entries.get(day);
  const pills = [];
  if (entry) pills.push(t('cardRank', { rank: entry.rank, total: entry.total }));
  if (st > 1) pills.push(t('cardStreak', { n: st }));
  return { ...cardBase(St.eff(r), r.m, day * 86_400_000), kicker: t('cardDaily', { n: St.dailyNumber(day) }), pills, name: `cube-daily-${St.dailyNumber(day)}.png` };
}

function solvedCard({ solve, effTime, n, isPb }) {
  if (!Number.isFinite(effTime)) return null;
  return { ...cardBase(effTime, solve.m, Date.now()), kicker: t('cardSolved', { size: `${n}×${n}` }), pills: isPb ? [`★ ${t('newPb')}`] : [], name: `cube-${n}x${n}.png` };
}

// The last card rendered, so sharing right after a solve doesn't wait.
let cardCache = { key: '', file: null };

async function cardFile(card) {
  if (!card) return null;
  const key = JSON.stringify(card);
  if (cardCache.key !== key) cardCache = { key, file: await Card.toFile(card, card.name) };
  return cardCache.file;
}

/** Downloads the card as a PNG (the "Save image" buttons). */
async function saveCard(card) {
  const file = await cardFile(card);
  if (!file) {
    toast(t('shareFail'), { em: '⚠️' });
    return;
  }
  const url = URL.createObjectURL(file);
  // Inside the open dialog when there is one: the page behind it is inert.
  const host = document.querySelector('dialog[open]') || document.body;
  const a = h('a', { href: url, download: file.name, hidden: true });
  host.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
  unlock('sharer');
  track('card_download');
}

async function shareText(text, url, card = null) {
  const file = navigator.share && coarse.matches ? await cardFile(card) : null;
  if (navigator.share && coarse.matches) {
    const withFile = file && navigator.canShare && navigator.canShare({ files: [file] });
    try {
      // Apps that take a file often drop the url field: keep it in the text.
      await navigator.share(withFile ? { files: [file], text: `${text}\n${url}` } : { text, url });
      unlock('sharer');
      track('share', { card: withFile ? 1 : 0 });
      return;
    } catch (e) {
      if (e && e.name === 'AbortError') return;
    }
  }
  try {
    await navigator.clipboard.writeText(url ? `${text}\n${url}` : text);
    toast(t('copied'), { em: '📋' });
    unlock('sharer');
    track('share');
  } catch {
    toast(t('shareFail'), { em: '⚠️' });
  }
}

// ── Achievements ────────────────────────────────────────────────────────────

function unlock(id) {
  if (unlocked.has(id)) return;
  unlocked.set(id, Date.now());
  store.set(KEYS.ach, [...unlocked.entries()]);
  const em = (St.ACHIEVEMENTS.find((a) => a[0] === id) || [])[1] || '🏆';
  const [title, desc] = achText(id);
  toast(`${t('unlocked')}: ${title}`, { em, sub: desc, ms: 4500 });
  if (currentPanel === 'awards') renderPanel('awards', true);
}

function checkAchievements() {
  const pll = new Set(catalog.filter((c) => c.cat === 'pll').map((c) => c.name));
  const tCases = [...trainerStats.keys()];
  const got = St.earned({
    solves,
    dailyDays: [...daily.keys()],
    trainerCases: tCases.length,
    pllCases: tCases.filter((c) => pll.has(c)).length,
  });
  for (const id of got) unlock(id);
}

// ── Toasts ──────────────────────────────────────────────────────────────────

function toast(text, { em, sub, action, ms = 2800 } = {}) {
  const box = $('#toasts');
  while (box.children.length >= 3) box.firstElementChild.remove();
  const el = h('div', { class: 'toast' },
    em ? h('span', { class: 'em', 'aria-hidden': 'true', text: em }) : null,
    h('div', { class: 'grow' }, text, sub ? h('small', { text: sub }) : null),
    action ? h('button', { type: 'button', text: action.label, on: { click: () => { action.fn(); dismiss(); } } }) : null,
  );
  const dismiss = () => {
    el.classList.add('out');
    setTimeout(() => el.remove(), 260);
  };
  box.append(el);
  if (ms > 0) setTimeout(dismiss, ms);
  return dismiss;
}

// ── Engine events ───────────────────────────────────────────────────────────

let started = false;
/** Set (to the browser's reason, maybe empty) when WebGL2 is refused. */
let gpuError = null;

addEventListener('cube:catalog', (e) => {
  const d = parseDetail(e);
  if (d && Array.isArray(d.algs)) {
    catalog = d.algs.filter((a) => isObj(a) && typeof a.name === 'string' && typeof a.moves === 'string' && typeof a.cat === 'string');
  }
  if (!engine.ready) {
    engine.ready = true;
    onEngineReady();
  }
});

addEventListener('cube:state', (e) => {
  const d = parseDetail(e);
  if (!d) return;
  const prev = S;
  S = { ...S, ...d, player: isObj(d.player) ? d.player : S.player };
  if (S.n !== prev.n) {
    renderKeypad();
    if (T.phase === 'idle' || T.phase === 'done') timerIdle();
  }
  // The engine left the timed flow (solve, reset, size change...).
  if ((S.phase === 'idle' || S.phase === 'scrambling') && ['inspect', 'armed', 'running'].includes(T.phase)) timerIdle();
  if (S.phase === 'scrambling' && prev.phase !== 'scrambling') setTimerText('0.00', 'idle');
  renderControls();
  renderStatus();
  renderPlayerState();
  if (currentPanel === 'trainer' && prev.trainer !== S.trainer) renderPanel('trainer', true);
});

addEventListener('cube:player', (e) => {
  const d = parseDetail(e);
  if (!d || !Array.isArray(d.moves)) return;
  playerInfo = { v: d.v, title: String(d.title || ''), kind: d.kind === 'apply' ? 'apply' : 'solution', moves: d.moves.map(String).slice(0, 5000) };
  if (playerInfo.title === 'solver') unlock('solver');
  renderPlayerMoves();
  renderPlayerState();
});

addEventListener('cube:event', (e) => {
  const d = parseDetail(e);
  if (!d) return;
  switch (d.type) {
    case 'turn':
      sound.click();
      vibrate(settings.haptics, 6);
      break;
    case 'ready':
      onReady(typeof d.mode === 'string' ? d.mode : S.mode);
      break;
    case 'start':
      onStart();
      break;
    case 'solved':
      onSolved(d);
      break;
    case 'error':
      showEngineError(String(d.cmd || ''), String(d.code || ''));
      break;
    case 'export':
      shareState(d);
      break;
  }
});

function errorText(code) {
  const [kind, pos] = code.split(':');
  const p = Number(pos) + 1;
  return {
    empty: t('errEmpty'), too_long: t('errTooLong'), too_many: t('errTooMany'), unbalanced: t('errUnbalanced'),
    unexpected: t('errUnexpected', { p }), bad_layer: t('errBadLayer', { n: S.n, p }), slice_even: t('errSliceEven', { p }),
    already_solved: t('errAlreadySolved'),
  }[kind] || t('errGeneric');
}

function showEngineError(cmd, code) {
  const inline = $('#algErr');
  if (inline && currentPanel === 'algs' && cmd !== 'move' && cmd !== 'solve') inline.textContent = errorText(code);
  else if (cmd !== 'move') toast(errorText(code), { em: code === 'already_solved' ? '✅' : '⚠️' });
}

function shareState(d) {
  const alg = typeof d.alg === 'string' ? d.alg : '';
  if (!alg) {
    shareText(t('shareState'), `${location.origin}/`);
    return;
  }
  if (alg.length > 1400) {
    toast(t('tooLongShare'), { em: '⚠️' });
    return;
  }
  shareText(`🧊 ${S.n}×${S.n}`, `${location.origin}/#state=${encodeURIComponent(alg)}&n=${S.n}`);
}

// ── Controls ────────────────────────────────────────────────────────────────

const btn = {
  undo: $('#undoBtn'), redo: $('#redoBtn'), scramble: $('#scrambleBtn'), solve: $('#solveBtn'),
  hint: $('#hintBtn'), rewind: $('#rewindBtn'), keypad: $('#keypadBtn'), more: $('#moreBtn'),
};

function renderControls() {
  btn.undo.disabled = !S.canUndo;
  btn.redo.disabled = !S.canRedo;
  btn.solve.disabled = !S.canSolve;
  btn.solve.classList.toggle('busy', S.solving);
  btn.hint.disabled = !S.canSolve || S.solving;
  btn.rewind.disabled = !S.canRewind;
  $('#sizeBtn').textContent = `${S.n}×${S.n}`;
  $('#sizeDown').disabled = S.n <= 2;
  $('#sizeUp').disabled = S.n >= 20;
}

function modeLabel() {
  if (S.mode.startsWith('daily:')) return t('modeDaily', { n: St.dailyNumber(Number(S.mode.slice(6))) });
  if (S.mode.startsWith('replay:')) return t('modeReplay', { n: St.dailyNumber(Number(S.mode.slice(7))) });
  if (S.mode.startsWith('trainer:')) {
    const c = catalog[Number(S.mode.slice(8))];
    return c ? c.name : '';
  }
  if (S.mode === 'scramble') return t('modeScramble');
  return '';
}

function renderStatus() {
  const chip = $('#modeChip');
  const text = $('#statusText');
  const label = modeLabel();
  chip.hidden = !label;
  chip.textContent = label;
  text.classList.remove('hint');
  if (S.solving) {
    text.textContent = t('statusThinking');
    text.classList.add('hint');
  } else if (S.player.active) {
    text.textContent = '';
  } else if (S.scramble && S.mode !== 'free') {
    const trainerCase = S.mode.startsWith('trainer:');
    text.textContent = trainerCase && T.phase === 'armed' ? t('statusReady') : trainerCase ? '' : S.scramble;
  } else if (S.scramble) {
    text.textContent = S.scramble;
  } else if (T.phase === 'armed') {
    text.textContent = t('statusReady');
    text.classList.add('hint');
  } else if (settings.hints && S.phase === 'idle') {
    text.textContent = wideMq.matches ? `${t('hintIdle')}\n${t('hintKeys')}` : t('hintIdle');
    text.classList.add('hint');
  } else {
    text.textContent = '';
  }
}

function doScramble() {
  sound.unlock();
  engine.send('scramble', '');
  track('scramble', { n: S.n });
}

btn.scramble.addEventListener('click', doScramble);
btn.undo.addEventListener('click', () => engine.send('undo'));
btn.redo.addEventListener('click', () => engine.send('redo'));
btn.solve.addEventListener('click', () => engine.send('solve', 'play'));
btn.hint.addEventListener('click', () => engine.send('solve', 'hint'));
btn.rewind.addEventListener('click', () => engine.send('rewind'));
$('#sizeDown').addEventListener('click', () => engine.send('size', S.n - 1));
$('#sizeUp').addEventListener('click', () => engine.send('size', S.n + 1));
$('#sizeBtn').addEventListener('click', () => openPanel('size'));

btn.keypad.addEventListener('click', () => {
  const kp = $('#keypad');
  kp.hidden = !kp.hidden;
  btn.keypad.setAttribute('aria-pressed', String(!kp.hidden));
  if (!kp.hidden) renderKeypad();
  layoutFloating();
});

function renderKeypad() {
  const kp = $('#keypad');
  if (kp.hidden) return;
  const odd = S.n % 2 === 1;
  const rows = [
    ['R', "R'", 'L', "L'", 'U', "U'"],
    ['D', "D'", 'F', "F'", 'B', "B'"],
    ...(odd ? [['M', "M'", 'E', "E'", 'S', "S'"]] : [['Rw', "Rw'", 'Uw', "Uw'", 'Fw', "Fw'"]]),
    ['x', "x'", 'y', "y'", 'z', "z'"],
  ];
  kp.replaceChildren(
    ...rows.flat().map((m) => {
      const cls = `key${m.includes("'") ? ' prime' : ''}${/^[MES]|w/.test(m) ? ' slice' : ''}${/^[xyz]/.test(m) ? ' rot' : ''}`;
      return h('button', { class: cls, type: 'button', text: m, 'aria-label': m, on: { click: () => { sound.unlock(); engine.send('move', m); } } });
    }),
  );
}

// "More" menu (narrow screens).
const moreMenu = $('#moreMenu');
btn.more.addEventListener('click', (e) => {
  e.stopPropagation();
  moreMenu.hidden = !moreMenu.hidden;
  btn.more.setAttribute('aria-expanded', String(!moreMenu.hidden));
});
document.addEventListener('click', (e) => {
  if (!moreMenu.hidden && !moreMenu.contains(e.target)) {
    moreMenu.hidden = true;
    btn.more.setAttribute('aria-expanded', 'false');
  }
});
moreMenu.addEventListener('click', (e) => {
  const b = e.target.closest('[data-action]');
  if (!b) return;
  moreMenu.hidden = true;
  runAction(b.dataset.action);
});

function runAction(a) {
  switch (a) {
    case 'hint': engine.send('solve', 'hint'); break;
    case 'rewind': engine.send('rewind'); break;
    case 'reset': engine.send('reset'); break;
    case 'view': engine.send('view_reset'); break;
    case 'fullscreen': toggleFullscreen(); break;
    case 'share': engine.send('export'); break;
  }
}

function toggleFullscreen() {
  const d = document;
  if (d.fullscreenElement) d.exitFullscreen?.();
  else d.documentElement.requestFullscreen?.({ navigationUI: 'hide' })?.catch(() => {});
}

// ── Player ──────────────────────────────────────────────────────────────────

const playerEl = $('#player');

function renderPlayerMoves() {
  const box = $('#playerMoves');
  const n = playerInfo.moves.length;
  // Very long sequences (big-cube rewinds) show a window around the cursor.
  box.replaceChildren(...playerInfo.moves.slice(0, 400).map((m, i) => h('span', { class: 'mv', text: m, data: { i: String(i) } })));
  $('#playerTitle').textContent =
    playerInfo.title === 'solver' ? t('solve')
      : playerInfo.title === 'rewind' ? t('rewind')
        : playerInfo.title === 'alg' ? t('algLab')
          : playerInfo.title === 'replay' && replayInfo ? `#${replayInfo.rank} ${replayInfo.name} · ${St.fmt(replayInfo.ms)}`
            : playerInfo.title;
  box.hidden = n === 0;
}

function renderPlayerState() {
  const p = S.player;
  playerEl.hidden = !p.active;
  if (!p.active) {
    layoutFloating();
    return;
  }
  $('#playerCount').textContent = `${p.idx} / ${p.len}`;
  const toggle = $('#playerToggle');
  toggle.replaceChildren(icon(p.playing ? 'pause' : 'play'));
  toggle.setAttribute('aria-label', p.playing ? t('pause') : t('play'));
  const chips = $$('.mv', $('#playerMoves'));
  chips.forEach((c, i) => {
    c.classList.toggle('done', i < p.idx);
    c.classList.toggle('cur', i === p.idx - 1);
  });
  const cur = chips[Math.max(0, p.idx - 1)];
  if (cur) {
    const box = $('#playerMoves');
    const target = cur.offsetLeft - box.clientWidth / 2 + cur.clientWidth / 2;
    box.scrollTo({ left: Math.max(0, target), behavior: 'smooth' });
  }
  layoutFloating();
}

$('#playerToggle').addEventListener('click', () => engine.send('player', S.player.playing ? 'pause' : 'play'));
$('#playerClose').addEventListener('click', () => engine.send('player', 'close'));
$$('[data-player]').forEach((b) => b.addEventListener('click', () => engine.send('player', b.dataset.player)));

/** Keypad and player share the space above the dock; stack them. */
function layoutFloating() {
  const kp = $('#keypad');
  if (!playerEl.hidden && !kp.hidden) {
    kp.style.setProperty('bottom', `calc(var(--sab) + var(--dock-h) + 34px + ${playerEl.offsetHeight}px)`);
  } else {
    kp.style.removeProperty('bottom');
  }
  syncInsets();
}

/**
 * Tells the engine how much of the canvas the overlay covers at the top and
 * bottom, so the cube is framed in the free band between them (it glides up
 * when the keypad or the player opens instead of hiding behind them). Only
 * elements over the middle of the screen count: on wide screens the keypad
 * sits at the side.
 */
let insetKey = '';
let insetRaf = 0;
function syncInsets() {
  if (!engine.ready || insetRaf) return;
  insetRaf = requestAnimationFrame(() => {
    insetRaf = 0;
    const H = innerHeight;
    const W = innerWidth;
    if (!H || !W) return;
    const central = (r) => r.height > 0 && r.left < W * 0.62 && r.right > W * 0.38;
    let top = 0;
    let bottom = H;
    for (const el of [$('.topbar'), $('#status')]) {
      const r = el.getBoundingClientRect();
      if (central(r)) top = Math.max(top, r.bottom);
    }
    for (const el of [$('.dock'), $('#keypad'), playerEl]) {
      if (el.hidden) continue;
      const r = el.getBoundingClientRect();
      if (central(r)) bottom = Math.min(bottom, r.top);
    }
    const key = `${(top / H).toFixed(3)},${(Math.max(0, H - bottom) / H).toFixed(3)}`;
    if (key !== insetKey) {
      insetKey = key;
      engine.send('view', key);
    }
  });
}
if (typeof ResizeObserver === 'function') {
  const ro = new ResizeObserver(() => syncInsets());
  for (const el of [$('.topbar'), $('#status'), $('.dock'), $('#keypad'), playerEl]) ro.observe(el);
}
addEventListener('resize', () => syncInsets());

// ── Panels ──────────────────────────────────────────────────────────────────

const sheet = $('#sheet');
const sheetBody = $('#sheetBody');
let currentPanel = null;
let panelTimer = 0;

const PANELS = {
  menu: { title: () => t('menu'), build: buildMenu },
  size: { title: () => t('sizeTitle'), build: buildSize },
  daily: { title: () => t('daily'), build: buildDaily },
  trainer: { title: () => t('trainer'), build: buildTrainer },
  algs: { title: () => t('algLab'), build: buildAlgs },
  stats: { title: () => t('stats'), build: buildStats },
  awards: { title: () => t('awards'), build: buildAwards },
  settings: { title: () => t('settings'), build: buildSettings },
  help: { title: () => t('help'), build: buildHelp },
};

function openPanel(name) {
  if (!PANELS[name]) return;
  moreMenu.hidden = true;
  renderPanel(name);
  if (!sheet.open) {
    if (wideMq.matches) sheet.show();
    else sheet.showModal();
  }
  sheet.focus?.();
}

function renderPanel(name, keepScroll = false) {
  const top = keepScroll ? sheetBody.scrollTop : 0;
  clearInterval(panelTimer);
  currentPanel = name;
  $('#sheetTitle').textContent = PANELS[name].title();
  sheetBody.replaceChildren(...[PANELS[name].build()].flat());
  sheetBody.scrollTop = top;
}

function closePanel() {
  clearInterval(panelTimer);
  currentPanel = null;
  if (sheet.open) sheet.close();
}

$('#sheetClose').addEventListener('click', closePanel);
sheet.addEventListener('close', () => {
  clearInterval(panelTimer);
  currentPanel = null;
});
// Click on the backdrop (modal sheets) closes.
sheet.addEventListener('click', (e) => {
  if (e.target === sheet) closePanel();
});
$$('[data-panel]').forEach((b) => b.addEventListener('click', () => openPanel(b.dataset.panel)));

const sec = (title, ...kids) => h('section', { class: 'sec' }, title ? h('h3', { class: 'sec-title', text: title }) : null, ...kids);

function buildMenu() {
  const item = (panel, ic, title, sub, fn) =>
    h('button', { class: 'menu-item', type: 'button', on: { click: fn || (() => openPanel(panel)) } }, icon(ic), h('b', { text: title }), h('small', { text: sub }));
  return h('div', { class: 'menu-list' },
    item('daily', 'calendar', t('daily'), t('menuDaily')),
    item('trainer', 'cap', t('trainer'), t('menuTrainer')),
    item('algs', 'code', t('algLab'), t('menuAlgs')),
    item('stats', 'chart', t('stats'), t('menuStats')),
    item('awards', 'trophy', t('awards'), t('menuAwards')),
    item('settings', 'gear', t('settings'), t('menuSettings')),
    item('help', 'help', t('help'), t('menuHelp')),
    item(null, 'share', t('shareState'), t('menuShare'), () => { closePanel(); engine.send('export'); }),
  );
}

function buildSize() {
  const grid = h('div', { class: 'cases' });
  for (let n = 2; n <= 20; n++) {
    const count = solves.filter((s) => s.n === n).length;
    grid.append(h('button', {
      class: `case${count ? ' done' : ''}`, type: 'button', 'aria-current': n === S.n ? 'true' : 'false',
      on: { click: () => { engine.send('size', n); closePanel(); } },
    }, h('b', { text: `${n}×${n}` }), h('small', { text: count ? `${count}` : ' ' })));
  }
  return sec(null, grid);
}

// Daily ----------------------------------------------------------------------

function buildDaily() {
  const today = St.utcDay();
  const num = St.dailyNumber(today);
  const res = daily.get(today);
  const st = St.streak([...daily.keys()], today);
  const countdown = h('div', { class: 'muted small' });
  const tickCountdown = () => {
    const left = (today + 1) * 86_400_000 - Date.now();
    const hh = Math.floor(left / 3_600_000);
    const mm = Math.floor((left % 3_600_000) / 60_000);
    const ss = Math.floor((left % 60_000) / 1000);
    countdown.textContent = t('dailyNext', { t: `${hh}:${String(mm).padStart(2, '0')}:${String(ss).padStart(2, '0')}` });
  };
  tickCountdown();
  panelTimer = setInterval(tickCountdown, 1000);

  const start = () => {
    closePanel();
    sound.unlock();
    engine.send('scramble', `daily:${today}`);
    track('daily_start');
  };
  const hero = h('div', { class: 'daily-hero' },
    h('div', { class: 'daily-num', text: t('dailyTitle', { n: num }) }),
    res
      ? [h('div', { class: 'muted small', text: t('dailyYour') }), h('div', { class: 'daily-big', text: St.fmt(St.eff(res)) }), h('div', { class: 'muted small', text: t('movesN', { n: res.m }) })]
      : h('p', { class: 'muted small', text: t('dailyIntro') }),
    st > 0 ? h('div', { class: 'streak' }, icon('flame'), t('streakDays', { n: st })) : null,
  );
  const actions = res
    ? h('div', { class: 'row wrap daily-actions' },
      h('div', { class: 'split grow' },
        h('button', { class: 'btn primary', type: 'button', on: { click: () => shareDaily(today) } }, icon('share'), t('dailyShare')),
        h('button', { class: 'btn primary', type: 'button', 'aria-label': t('cardSave'), title: t('cardSave'), on: { click: () => saveCard(dailyCard(today)) } }, icon('download'))),
      h('button', { class: 'btn grow', type: 'button', on: { click: start } }, icon('dice'), t('dailyPractice')))
    : h('button', { class: 'btn primary block', type: 'button', on: { click: start } }, icon('play'), t('dailyStart'));

  const week = h('div', { class: 'week' });
  const fmtDay = new Intl.DateTimeFormat(getLang(), { weekday: 'short', timeZone: 'UTC' });
  for (let d = today - 6; d <= today; d++) {
    week.append(h('div', { class: `day${daily.has(d) ? ' ok' : ''}${d === today ? ' today' : ''}`, title: daily.has(d) ? St.fmt(St.eff(daily.get(d))) : '' },
      h('i'), fmtDay.format(new Date(d * 86_400_000))));
  }
  return [sec(null, hero), sec(null, actions, h('div', { class: 'sec' }, countdown)), buildLeaderboard(today), sec(t('dailyHistory'), week)];
}

// Global leaderboard -----------------------------------------------------------

const lbBoards = new Map(); // day → { at, board } (board null = unavailable)
const lbLoading = new Map(); // day → in-flight fetch
let lbTab = 0; // 0 = today, 1 = yesterday
let replayInfo = null; // the recorded solve being watched
let lbBusy = false; // a submission is in flight
let lbNote = null; // { day, msg }: outcome of the last attempt, for the UI
let lbEditing = false;

function loadBoard(day) {
  const hit = lbBoards.get(day);
  if (hit && Date.now() - hit.at < (hit.board ? 30_000 : 5_000)) return Promise.resolve(hit.board);
  if (!lbLoading.has(day)) {
    const mine = lb.entries.get(day);
    lbLoading.set(day, Lb.fetchBoard(day, mine && mine.id).then((board) => {
      lbLoading.delete(day);
      lbBoards.set(day, { at: Date.now(), board });
      return board;
    }));
  }
  return lbLoading.get(day);
}

/** Publishes the pending solve (if there's one and a name). */
async function publishPending() {
  const p = lb.pending;
  if (!p || !lb.name || lbBusy) return null;
  if (!Lb.isOpen(p.day, St.utcDay())) {
    lb.pending = null;
    saveLb();
    return null;
  }
  lbBusy = true;
  const res = await Lb.submit(p.day, { name: lb.name, token: lb.token, ms: p.ms, sol: p.sol });
  lbBusy = false;
  if (res.ok) {
    lb.entries.set(p.day, res.entry);
    lb.pending = null;
    lbNote = null;
    saveLb();
    lbBoards.delete(p.day);
    track('lb_publish');
    return { ok: true, day: p.day, entry: res.entry };
  }
  const f = Lb.failureAction(res);
  if (f.action === 'drop') lb.pending = null;
  if (f.action === 'name') lb.name = null;
  lbNote = { day: p.day, msg: f.msg };
  saveLb();
  return { ok: false, day: p.day, ...f };
}

/** Publishes now and refreshes whatever shows the result. */
function publishNow() {
  const run = publishPending();
  renderLbViews();
  run.then(renderLbViews);
  return run;
}

/** Background retry (startup, back online): only a toast to report. */
function retryPending() {
  if (!lb.pending || !lb.name) return;
  publishPending().then((r) => {
    if (r && r.ok) toast(t('lbPublished', { rank: r.entry.rank, total: r.entry.total }), { em: '🏆', action: { label: t('lbSee'), fn: () => openPanel('daily') } });
    renderLbViews();
  });
}

function renderLbViews() {
  renderSolvedLb();
  if (currentPanel === 'daily') renderPanel('daily', true);
}

function lbNameForm() {
  const input = h('input', {
    type: 'text', class: 'lb-input', placeholder: t('lbNamePh'), 'aria-label': t('lbNamePh'), maxlength: '16',
    autocomplete: 'nickname', autocapitalize: 'off', spellcheck: 'false', enterkeyhint: 'send', value: lb.name || '',
  });
  const err = h('div', { class: 'err', 'aria-live': 'polite', text: lbNote && lbNote.msg === 'lbBadName' ? t('lbBadName') : '' });
  const go = () => {
    const name = Lb.cleanName(input.value);
    if (!name) {
      err.textContent = t('lbNameHint');
      input.focus();
      return;
    }
    lb.name = name;
    lbEditing = false;
    lbNote = null;
    saveLb();
    if (lb.pending) publishNow();
    else renderLbViews();
  };
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      go();
    }
  });
  return h('div', { class: 'lb-form' },
    h('div', { class: 'row' }, input, h('button', { class: 'btn primary', type: 'button', on: { click: go } }, t(lb.pending ? 'lbPublish' : 'lbSave'))),
    err,
    h('p', { class: 'lb-fine', text: t('lbFine') }));
}

/** What the leaderboard knows about `day` for this browser, as UI. */
function lbStatus(day) {
  const entry = lb.entries.get(day);
  if (entry) return h('div', { class: 'lb-result' }, h('span', { class: 'lb-trophy', 'aria-hidden': 'true', text: entry.rank <= 3 ? ['🥇', '🥈', '🥉'][entry.rank - 1] : '🏆' }), t('lbRank', { rank: entry.rank, total: entry.total }));
  if (lb.pending && lb.pending.day === day) {
    if (!lb.name) return h('div', { class: 'lb-join' }, h('b', { text: t('lbJoin') }), lbNameForm());
    if (lbBusy) return h('div', { class: 'lb-result muted' }, t('lbPublishing'));
  }
  if (lbNote && lbNote.day === day) return h('div', { class: 'lb-result muted' }, t(lbNote.msg));
  return null;
}

function renderSolvedLb() {
  const box = $('#solvedLb');
  const day = lastSolved ? lastSolved.dailyDay : null;
  const status = day !== null ? lbStatus(day) : null;
  box.replaceChildren(...(status ? [status] : []));
  if (status && lb.entries.has(day)) {
    box.append(h('button', { class: 'link-btn', type: 'button', text: t('lbSee'), on: { click: () => { $('#solvedModal').close(); lbTab = St.utcDay() - day; openPanel('daily'); } } }));
  }
  box.hidden = !status;
}

function buildLeaderboard(today) {
  if (lbTab !== 0 && lbTab !== 1) lbTab = 0;
  const day = today - lbTab;
  const tabs = h('div', { class: 'tabs', role: 'tablist' },
    ...[[0, t('lbToday')], [1, t('lbYesterday')]].map(([i, label]) =>
      h('button', { class: 'tab', type: 'button', role: 'tab', 'aria-selected': String(lbTab === i), text: label, on: { click: () => { lbTab = i; renderPanel('daily', true); } } })));
  const body = h('div', { class: 'lb-body', 'aria-busy': 'true' }, h('div', { class: 'lb-skel' }, h('i'), h('i'), h('i')));
  const paint = (board) => {
    body.replaceChildren(...lbBoardView(board, day));
    body.setAttribute('aria-busy', 'false');
  };
  const hit = lbBoards.get(day);
  if (hit && Date.now() - hit.at < 30_000) paint(hit.board);
  else loadBoard(day).then((board) => { if (body.isConnected) paint(board); });

  const status = lbStatus(day);
  let footer = null;
  if (lb.name) {
    footer = lbEditing
      ? lbNameForm()
      : h('div', { class: 'row between lb-me' },
        h('span', { class: 'muted small', text: t('lbAs', { name: lb.name }) }),
        h('span', { class: 'row' },
          h('button', { class: 'link-btn', type: 'button', text: t('lbChange'), on: { click: () => { lbEditing = true; renderPanel('daily', true); } } }),
          h('button', { class: 'link-btn', type: 'button', text: t('lbStop'), on: { click: () => { lb.name = null; saveLb(); renderPanel('daily', true); toast(t('lbStopped')); } } })));
  }
  return sec(t('lbTitle'), tabs, status ? h('div', { class: 'card lb-card' }, status) : null, body, footer);
}

function lbBoardView(board, day) {
  if (!board) {
    return [h('div', { class: 'lb-empty' },
      h('p', { class: 'muted small', text: t('lbOffline') }),
      h('button', { class: 'btn', type: 'button', on: { click: () => { lbBoards.delete(day); renderPanel('daily', true); } } }, icon('reset'), t('lbRetry')))];
  }
  if (!board.top.length) return [h('p', { class: 'lb-empty muted small', text: t(day === board.today ? 'lbEmpty' : 'lbEmptyPast') })];
  const meRank = board.me ? board.me.rank : 0;
  const watchable = board.top.some((r) => r.replay);
  const row = (r) => h('li', { class: `lb-row${r.rank === meRank ? ' me' : ''}${r.rank <= 3 ? ` r${r.rank}` : ''}` },
    h('span', { class: 'lb-rank', text: String(r.rank) }),
    h('span', { class: 'lb-name', text: r.name, title: r.name }),
    h('span', { class: 'lb-moves', text: t('lbMv', { n: r.moves }) }),
    h('span', { class: 'lb-time', text: St.fmt(r.ms) }),
    !watchable ? null
      : r.replay ? h('button', { class: 'icon-btn sm lb-play', type: 'button', 'aria-label': t('lbWatch', { rank: r.rank, name: r.name }), on: { click: (e) => watchSolve(day, r.rank, e.currentTarget) } }, icon('play'))
        : h('span'));
  const list = h('ol', { class: `lb-list${watchable ? ' watch' : ''}`, 'aria-label': t('lbTitle') }, ...board.top.map(row));
  if (board.me && board.me.rank > board.top.length) list.append(h('li', { class: 'lb-gap', 'aria-hidden': 'true', text: '⋯' }), row({ ...board.me, replay: false }));
  return [
    watchable ? h('p', { class: 'muted small lb-note', text: t('lbWatchHint') }) : null,
    list,
    h('p', { class: 'muted small lb-total', text: t('lbPlayers', { n: board.total }) }),
    !board.final && day === board.today ? h('p', { class: 'muted small lb-note', text: t('lbWatchLater') }) : null,
  ].filter(Boolean);
}

/** Loads one of a final day's best solves and plays it on the cube. */
async function watchSolve(day, rank, button) {
  button.disabled = true;
  const solve = await Lb.fetchSolve(day, rank);
  button.disabled = false;
  if (!solve) {
    toast(t('lbWatchFail'));
    return;
  }
  sound.unlock();
  replayInfo = solve;
  engine.send('replay', `${solve.day} ${solve.sol}`);
  track('replay', { rank });
  if (!wideMq.matches) closePanel();
}

// Trainer --------------------------------------------------------------------

let trainerTab = 'pll';
let trainerAuf = false;
let trainerHide = false;

function buildTrainer() {
  const tabs = h('div', { class: 'tabs', role: 'tablist' },
    ...[['pll', t('trPll')], ['oll', t('trOll')], ['pattern', t('trPatterns')]].map(([id, label]) =>
      h('button', { class: 'tab', type: 'button', role: 'tab', 'aria-selected': String(trainerTab === id), text: label, on: { click: () => { trainerTab = id; renderPanel('trainer'); } } })));
  const isPattern = trainerTab === 'pattern';
  const grid = h('div', { class: 'cases' });
  catalog.forEach((c, i) => {
    if (c.cat !== trainerTab) return;
    const st = trainerStats.get(c.name);
    grid.append(h('button', {
      class: `case${st ? ' done' : ''}`, type: 'button', 'aria-current': String(S.trainer === i),
      on: { click: () => pickCase(i) },
    }, h('b', { text: c.name.replace('OLL ', '') }), h('small', { text: st && Number.isFinite(st.best) && st.best < 9e9 ? St.fmt(st.best) : ' ' })));
  });

  const parts = [tabs, h('p', { class: 'muted small', text: isPattern ? t('trPatternsIntro') : t('trPick') })];
  if (!isPattern) {
    parts.push(h('div', { class: 'field' },
      h('label', { for: 'aufToggle' }, t('trAuf'), h('small', { text: t('trAufHint') })),
      switchEl('aufToggle', trainerAuf, (v) => { trainerAuf = v; })));
  }
  parts.push(sec(null, grid));

  const cur = catalog[S.trainer];
  if (cur && cur.cat === trainerTab) {
    const alg = h('div', { class: `alg${trainerHide && !isPattern ? ' blur' : ''}`, text: cur.moves });
    parts.unshift(h('div', { class: 'card' },
      h('div', { class: 'row between' }, h('b', { text: cur.name }),
        isPattern ? null : h('button', { class: 'icon-btn sm', type: 'button', 'aria-label': t('trHideAlg'), 'aria-pressed': String(trainerHide), on: { click: () => { trainerHide = !trainerHide; renderPanel('trainer', true); } } }, icon('eye'))),
      alg,
      h('div', { class: 'row wrap sec' },
        h('button', { class: 'btn', type: 'button', on: { click: () => pickCase(S.trainer) } }, icon('reset'), t('trAgain')),
        isPattern ? null : h('button', { class: 'btn', type: 'button', on: { click: randomCase } }, icon('dice'), t('trRandom')),
        isPattern ? null : h('button', { class: 'btn', type: 'button', on: { click: () => { engine.send('rewind'); if (!wideMq.matches) closePanel(); } } }, icon('play'), t('trShowMe')))));
  }
  return parts;
}

function pickCase(i) {
  sound.unlock();
  engine.send('trainer', `${i},${trainerAuf ? 1 : 0}`);
  if (catalog[i] && catalog[i].cat === 'pattern') unlock('pattern');
  if (!wideMq.matches) closePanel();
}

function randomCase() {
  const pool = catalog.map((c, i) => [c, i]).filter(([c, i]) => c.cat === trainerTab && i !== S.trainer);
  if (pool.length) pickCase(pool[(Math.random() * pool.length) | 0][1]);
}

// Algorithm lab ----------------------------------------------------------------

let algDraft = '';
const EXAMPLES = [
  ['Sexy ×6', "(R U R' U')6"], ['Sune', "R U R' U R U2 R'"], ['T-perm', "R U R' U' R' F R2 U' R' U' R U R' F'"],
  ['Checkerboard', 'M2 E2 S2'], ['Superflip', "U R2 F B R B2 R U2 L B2 R U' D' R2 F R' L B2 U2 F2"], ['Sledgehammer', "R' F R F'"],
];

function buildAlgs() {
  const ta = h('textarea', {
    id: 'algInput', rows: '3', spellcheck: 'false', autocomplete: 'off', autocapitalize: 'off',
    placeholder: t('algPlaceholder'), 'aria-label': t('algLab'),
    on: { input: (e) => { algDraft = e.target.value.slice(0, 2000); err.textContent = ''; } },
  });
  ta.value = algDraft;
  const err = h('div', { class: 'err', id: 'algErr', role: 'alert' });
  const val = () => {
    const v = ta.value.trim();
    if (!v) { err.textContent = t('errEmpty'); return null; }
    if (v.length > 2000 || !NOTATION_RE.test(v)) { err.textContent = errorText('unexpected:' + [...v].findIndex((c) => !NOTATION_RE.test(c))); return null; }
    err.textContent = '';
    return v;
  };
  const act = (cmd) => () => {
    const v = val();
    if (v == null) return;
    sound.unlock();
    engine.send(cmd, v);
    if (!wideMq.matches && cmd !== 'share') closePanel();
  };
  const share = () => {
    const v = val();
    if (v) shareText(v, `${location.origin}/#alg=${encodeURIComponent(v)}&n=${S.n}`);
  };

  const scr = h('input', { type: 'text', id: 'scrInput', spellcheck: 'false', autocomplete: 'off', autocapitalize: 'off', placeholder: "D2 F' R2 U ...", 'aria-label': t('algScramble') });
  const scrErr = h('div', { class: 'err', role: 'alert' });
  const scrGo = () => {
    const v = scr.value.trim();
    if (!v || v.length > 2000 || !NOTATION_RE.test(v)) { scrErr.textContent = t('errUnexpected', { p: 1 }); return; }
    sound.unlock();
    engine.send('scramble_alg', v);
    if (!wideMq.matches) closePanel();
  };

  return [
    h('p', { class: 'muted small', text: t('algIntro') }),
    sec(null, ta, err,
      h('div', { class: 'row wrap' },
        h('button', { class: 'btn primary', type: 'button', on: { click: act('play_alg') } }, icon('play'), t('algPlay')),
        h('button', { class: 'btn', type: 'button', on: { click: act('setup_inverse') } }, icon('target'), t('algPractice')),
        h('button', { class: 'btn', type: 'button', on: { click: act('setup') } }, icon('eye'), t('algState')),
        h('button', { class: 'btn', type: 'button', on: { click: share } }, icon('share'), t('algShare')))),
    sec(t('algExamples'), h('div', { class: 'chips' }, ...EXAMPLES.map(([name, alg]) =>
      h('button', { class: 'chip', type: 'button', text: name, on: { click: () => { ta.value = algDraft = alg; err.textContent = ''; } } })))),
    sec(t('algScramble'), h('p', { class: 'muted small', text: t('algScrambleIntro') }), scr, scrErr,
      h('div', { class: 'row sec' }, h('button', { class: 'btn', type: 'button', on: { click: scrGo } }, icon('dice'), t('algScrambleGo')))),
  ];
}

// Stats ------------------------------------------------------------------------

let statsSize = null;

function buildStats() {
  const sizes = [...new Set(solves.map((s) => s.n).concat([S.n]))].sort((a, b) => a - b);
  const n = statsSize && sizes.includes(statsSize) ? statsSize : S.n;
  const list = solves.filter((s) => s.n === n);
  const sum = St.summary(list);

  const tabs = h('div', { class: 'size-tabs', role: 'tablist', 'aria-label': t('stSize') },
    ...sizes.map((sz) => h('button', { type: 'button', role: 'tab', 'aria-selected': String(sz === n), text: `${sz}×${sz}`, on: { click: () => { statsSize = sz; renderPanel('stats', true); } } })));

  if (!list.length) return [tabs, h('p', { class: 'muted sec', text: t('stEmpty', { n }) }), dataActions(n)];

  const stat = (label, v) => h('div', { class: 'stat' }, h('b', { text: v == null ? '—' : St.fmt(v) }), h('small', { text: label }));
  const grid = h('div', { class: 'stat-grid' },
    stat(t('best'), sum.best), stat('ao5', sum.ao5), stat('ao12', sum.ao12),
    stat('ao100', sum.ao100), stat(t('mean'), sum.mean),
    h('div', { class: 'stat' }, h('b', { text: String(sum.count) }), h('small', { text: t('count') })));

  const recent = list.slice(-50);
  const chart = chartEl(recent);
  const best = sum.best;
  const ul = h('ul', { class: 'solves' });
  const startIdx = list.length;
  list.slice(-100).reverse().forEach((s, k) => {
    const e = St.eff(s);
    const tps = s.m > 0 && s.t > 0 ? ` · ${(s.m / (s.t / 1000)).toFixed(1)} tps` : '';
    const date = new Date(s.d).toLocaleDateString(getLang(), { day: 'numeric', month: 'short' });
    const tag = s.k === 'd' ? '📅 ' : s.k === 't' ? `🎓 ${s.c || ''} · ` : '';
    const pen = (p) => () => {
      s.p = s.p === p ? 0 : p;
      store.set(KEYS.solves, solves);
      updateTimerSub();
      renderPanel('stats', true);
    };
    ul.append(h('li', null,
      h('span', { class: 'idx', text: `${startIdx - k}` }),
      h('div', null,
        h('div', { class: `t${s.p === St.DNF ? ' dnf' : e === best ? ' pb' : ''}`, text: St.fmt(e) + (s.p === 2000 ? '+' : '') }),
        h('div', { class: 'meta', text: `${tag}${t('movesN', { n: s.m })}${tps} · ${date}`, title: s.s || '' })),
      h('div', { class: 'acts' },
        h('button', { type: 'button', text: '+2', 'aria-pressed': String(s.p === 2000), on: { click: pen(2000) } }),
        h('button', { type: 'button', text: 'DNF', 'aria-pressed': String(s.p === St.DNF), on: { click: pen(St.DNF) } }),
        h('button', { type: 'button', 'aria-label': t('stDelete'), on: { click: () => { solves = solves.filter((x) => x !== s); store.set(KEYS.solves, solves); updateTimerSub(); renderPanel('stats', true); } } }, icon('trash'))),
    ));
  });

  return [tabs, sec(null, grid), sec(t('stChart', { n: recent.length }), chart), sec(t('stList'), ul), dataActions(n)];
}

function chartEl(list) {
  const svg = h('svg:svg', { class: 'chart', viewBox: '0 0 100 40', preserveAspectRatio: 'none', role: 'img', 'aria-label': t('stChart', { n: list.length }) });
  const vals = list.map(St.eff).filter(Number.isFinite);
  if (vals.length < 2) return svg;
  const lo = Math.min(...vals);
  const hi = Math.max(...vals);
  const span = hi - lo || 1;
  const pts = [];
  let k = 0;
  list.forEach((s) => {
    const e = St.eff(s);
    if (!Number.isFinite(e)) return;
    pts.push([(k++ / (vals.length - 1)) * 100, 37 - ((e - lo) / span) * 34]);
  });
  const line = pts.map((p) => p.map((v) => v.toFixed(2)).join(',')).join(' ');
  svg.append(h('svg:polygon', { class: 'area', points: `0,40 ${line} 100,40` }));
  svg.append(h('svg:polyline', { class: 'line', points: line }));
  const avg = [];
  for (let end = 5; end <= list.length; end++) {
    const a = St.averageOf(list, 5, end);
    if (a != null && Number.isFinite(a)) avg.push([((end - 1) / (list.length - 1)) * 100, 37 - ((a - lo) / span) * 34]);
  }
  if (avg.length > 1) svg.append(h('svg:polyline', { class: 'avg', points: avg.map((p) => p.map((v) => v.toFixed(2)).join(',')).join(' ') }));
  return svg;
}

function download(name, type, text) {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = h('a', { href: url });
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}

function dataActions(n) {
  const file = h('input', { type: 'file', accept: 'application/json,.json', hidden: true });
  file.addEventListener('change', async () => {
    const f = file.files && file.files[0];
    if (!f) return;
    if (f.size > 4 * 1024 * 1024) { toast(t('stImportBad'), { em: '⚠️' }); return; }
    try {
      const data = JSON.parse(await f.text());
      const incoming = Array.isArray(data) ? data : isObj(data) && Array.isArray(data.solves) ? data.solves : null;
      if (!incoming) throw new Error('shape');
      const { merged, added } = St.mergeSolves(solves, incoming);
      solves = merged;
      store.set(KEYS.solves, solves);
      toast(t('stImported', { n: added }), { em: '📥' });
      updateTimerSub();
      checkAchievements();
      renderPanel('stats', true);
    } catch {
      toast(t('stImportBad'), { em: '⚠️' });
    }
  });
  const stamp = new Date().toISOString().slice(0, 10);
  return sec(null, h('div', { class: 'row wrap' },
    h('button', { class: 'btn', type: 'button', on: { click: () => download(`cube-solves-${stamp}.csv`, 'text/csv', St.toCsv(solves)) } }, icon('download'), t('stExportCsv')),
    h('button', { class: 'btn', type: 'button', on: { click: () => download(`cube-backup-${stamp}.json`, 'application/json', JSON.stringify({ app: 'cube.micutu.com', v: 4, solves }, null, 1)) } }, icon('download'), t('stExportJson')),
    h('button', { class: 'btn', type: 'button', on: { click: () => file.click() } }, icon('upload'), t('stImport')),
    h('button', { class: 'btn danger', type: 'button', on: { click: () => {
      if (!confirm(t('stClearConfirm', { n }))) return;
      solves = solves.filter((s) => s.n !== n);
      store.set(KEYS.solves, solves);
      updateTimerSub();
      renderPanel('stats', true);
    } } }, icon('trash'), t('stClear')),
    file));
}

// Achievements ---------------------------------------------------------------

function buildAwards() {
  const total = St.ACHIEVEMENTS.length;
  const got = St.ACHIEVEMENTS.filter(([id]) => unlocked.has(id)).length;
  return [
    h('p', { class: 'muted' }, t('awIntro', { a: got, b: total })),
    h('div', { class: 'progress', role: 'progressbar', 'aria-valuemin': '0', 'aria-valuemax': String(total), 'aria-valuenow': String(got) },
      (() => { const i = h('i'); i.style.width = `${(got / total) * 100}%`; return i; })()),
    sec(null, h('div', { class: 'awards' }, ...St.ACHIEVEMENTS.map(([id, em]) => {
      const [title, desc] = achText(id);
      const on = unlocked.has(id);
      return h('div', { class: `award${on ? ' on' : ''}`, title: on ? new Date(unlocked.get(id)).toLocaleDateString(getLang()) : '' },
        h('span', { class: 'em', 'aria-hidden': 'true', text: em }), h('div', null, h('b', { text: title }), h('small', { text: desc })));
    }))),
  ];
}

// Settings -------------------------------------------------------------------

function switchEl(id, value, onChange) {
  const input = h('input', { type: 'checkbox', id, role: 'switch', checked: value, on: { change: (e) => onChange(e.target.checked) } });
  return h('span', { class: 'switch' }, input, h('span'));
}

const SCHEME_PREVIEW = [
  ['#ffffff', '#008c38', '#b81205'], ['#fafaff', '#1acc59', '#eb1a33'], ['#ffffff', '#009e73', '#d55e00'],
  ['#f7f5f0', '#8cdba8', '#f2858c'], ['#ebebeb', '#d1d1d6', '#b3b3b8'],
];

function buildSettings() {
  const field = (label, hint, control) => h('div', { class: 'field' }, h('div', { class: 'lbl' }, label, hint ? h('small', { text: hint }) : null), control);
  const toggle = (key, label, hint) => {
    const id = `set-${key}`;
    return h('div', { class: 'field' },
      h('label', { for: id }, label, hint ? h('small', { text: hint }) : null),
      switchEl(id, settings[key], (v) => { settings[key] = v; saveSettings(); if (key === 'sound' && v) sound.unlock(); renderStatus(); }));
  };
  const seg = (options, value, onPick) => h('div', { class: 'seg' }, ...options.map(([v, label]) =>
    h('button', { type: 'button', 'aria-pressed': String(v === value), text: label, on: { click: () => onPick(v) } })));

  const speed = h('input', { type: 'range', min: '0.5', max: '3', step: '0.25', value: String(settings.speed), 'aria-label': t('seSpeed') });
  speed.addEventListener('change', () => { settings.speed = Number(speed.value); saveSettings(); });

  const schemes = h('div', { class: 'swatches' }, ...SCHEME_PREVIEW.map((cols, i) => h('button', {
    class: 'swatch', type: 'button', title: t('schemes')[i], 'aria-label': t('schemes')[i], 'aria-pressed': String(settings.scheme === i),
    on: { click: () => { settings.scheme = i; saveSettings(); renderPanel('settings', true); } },
  }, ...cols.flatMap((c) => [c, c, c]).map((c) => { const el = h('i'); el.style.background = c; return el; }))));

  const bgs = h('div', { class: 'swatches' }, ...[0, 1, 2, 3].map((i) => h('button', {
    class: `bg-swatch bg-${i}`, type: 'button', 'aria-label': `${t('seBg')} ${i + 1}`, 'aria-pressed': String(settings.bg === i),
    on: { click: () => { settings.bg = i; saveSettings(); renderPanel('settings', true); } },
  })));

  return [
    sec(null,
      field(t('seLang'), null, seg([['ro', 'Română'], ['en', 'English']], settings.lang, (v) => {
        settings.lang = v; setLang(v); saveSettings(); applyStaticI18n(); renderAll(); renderPanel('settings', true);
      })),
      toggle('inspection', t('seInspection'), t('seInspectionHint')),
      toggle('sound', t('seSound')),
      toggle('haptics', t('seHaptics'), t('seHapticsHint')),
      toggle('hints', t('seHints')),
      toggle('saver', t('seSaver'), t('seSaverHint')),
      field(t('seSpeed'), null, speed)),
    sec(t('seColors'), schemes),
    sec(t('seBg'), bgs),
    sec(t('seData'), h('div', { class: 'row wrap' },
      h('button', { class: 'btn', type: 'button', on: { click: () => engine.send('view_reset') } }, icon('target'), t('resetView')),
      h('button', { class: 'btn', type: 'button', on: { click: () => engine.send('reset') } }, icon('reset'), t('resetCube')),
      h('button', { class: 'btn danger', type: 'button', on: { click: eraseAll } }, icon('trash'), t('seResetAll')))),
  ];
}

function eraseAll() {
  if (!confirm(t('seResetConfirm'))) return;
  try {
    Object.keys(localStorage).filter((k) => k.startsWith('cube.')).forEach((k) => localStorage.removeItem(k));
  } catch {
    /* ignore */
  }
  location.reload();
}

// Help -------------------------------------------------------------------------

function buildHelp() {
  const kbd = (...keys) => h('span', null, ...keys.map((k) => h('span', { class: 'kbd', text: k })));
  const row = (left, right) => h('li', null, left, h('span', { class: 'muted', text: right }));
  return [
    sec(t('hpMouse'), h('ul', { class: 'help-list' },
      row(h('b', { text: t('hpDragSticker') }), t('hpDragSticker2')),
      row(h('b', { text: t('hpDragOut') }), t('hpDragOut2')),
      row(h('b', { text: t('hpZoom') }), t('hpZoom2')))),
    sec(t('hpKeyboard'), h('ul', { class: 'help-list' },
      row(kbd('R', 'L', 'U', 'D', 'F', 'B'), t('hpFaces')),
      row(kbd('M', 'E', 'S'), t('hpSlices')),
      row(kbd('X', 'Y', 'Z'), t('hpRot')),
      row(kbd('Space'), t('hpSpace')),
      row(kbd('⌫'), t('hpBack')),
      row(kbd('Ctrl', 'Z'), t('hpUndo')),
      row(kbd('[', ']'), t('hpBrackets')),
      row(kbd('?'), t('hpQ')))),
    sec(t('hpNotation'), h('p', { class: 'prose', text: t('hpNotationText') })),
    sec(t('hpAbout'), h('div', { class: 'prose' }, h('p', { text: t('hpAboutText') }),
      h('p', null, h('a', { href: GITHUB, target: '_blank', rel: 'noopener noreferrer', text: t('hpSource') })))),
  ];
}

// ── Keyboard ────────────────────────────────────────────────────────────────

addEventListener('keydown', (e) => {
  const target = e.target instanceof Element ? e.target : null;
  if (target && target.closest('input, textarea, select, [contenteditable="true"]')) return;
  if (e.key === 'Escape') {
    if (!moreMenu.hidden) moreMenu.hidden = true;
    else if (sheet.open) closePanel();
    return;
  }
  if (e.ctrlKey || e.metaKey) {
    const k = e.key.toLowerCase();
    if (k === 'z') {
      e.preventDefault();
      engine.send(e.shiftKey ? 'redo' : 'undo');
    } else if (k === 'y') {
      e.preventDefault();
      engine.send('redo');
    }
    return; // never steal browser shortcuts (Ctrl+R, Ctrl+L, ...)
  }
  if ($('#solvedModal').open || (sheet.open && !wideMq.matches)) return;
  if (e.repeat) return;
  const onButton = target && target.closest('button, a');
  if (e.key === ' ' && !onButton) {
    e.preventDefault();
    doScramble();
    return;
  }
  if (e.key === 'Backspace') {
    e.preventDefault();
    engine.send('undo');
    return;
  }
  if (e.key === '?') { openPanel('help'); return; }
  if (e.key === '[') { engine.send('size', S.n - 1); return; }
  if (e.key === ']') { engine.send('size', S.n + 1); return; }
  if (e.altKey && e.code === 'Enter') return;
  // Alt changes e.key on macOS (Alt+R = ®): use the physical key then.
  const letter = (e.altKey && e.code.startsWith('Key') ? e.code.slice(3) : e.key).toUpperCase();
  let tok = null;
  if ('RLUDFB'.includes(letter) && letter.length === 1) tok = letter + (e.altKey ? 'w' : '');
  else if ('MES'.includes(letter) && letter.length === 1 && S.n % 2 === 1) tok = letter;
  else if ('XYZ'.includes(letter) && letter.length === 1) tok = letter.toLowerCase();
  if (!tok) return;
  e.preventDefault();
  sound.unlock();
  engine.send('move', tok + (e.shiftKey ? "'" : ''));
});

// Sound needs a user gesture before the first click can play.
addEventListener('pointerdown', () => { if (settings.sound) sound.unlock(); }, { passive: true });

// ── Static i18n (elements with data-i18n / data-i18n-aria) ──────────────────

function applyStaticI18n() {
  $$('[data-i18n]').forEach((el) => { el.textContent = t(el.dataset.i18n); });
  $$('[data-i18n-aria]').forEach((el) => { el.setAttribute('aria-label', t(el.dataset.i18nAria)); });
  $('#solvedCard').title = t('cardSave');
  $$('[data-panel]').forEach((el) => { if (!el.hasAttribute('title')) el.title = el.getAttribute('aria-label') || ''; });
}

function renderAll() {
  renderControls();
  renderStatus();
  renderPlayerMoves();
  renderPlayerState();
  renderKeypad();
  if (T.phase === 'idle') timerIdle();
  else updateTimerSub();
}

// ── URL fragments: shared links ─────────────────────────────────────────────

const PANEL_LINKS = new Set(['daily', 'trainer', 'algs', 'stats', 'awards', 'settings', 'help']);

function handleHash() {
  const raw = location.hash.slice(1);
  if (!raw) return;
  history.replaceState(null, '', location.pathname + location.search);
  // Panel deep links (#daily, #trainer, ...): used by the PWA shortcuts.
  if (PANEL_LINKS.has(raw)) {
    openPanel(raw);
    return;
  }
  let p;
  try {
    p = new URLSearchParams(raw);
  } catch {
    return;
  }
  const n = Number.parseInt(p.get('n') || '3', 10);
  const size = Number.isInteger(n) && n >= 2 && n <= 20 ? n : 3;
  const ok = (v) => typeof v === 'string' && v.length > 0 && v.length <= 1500 && NOTATION_RE.test(v);
  const pick = [['scramble', 'scramble_alg'], ['alg', 'setup_inverse'], ['state', 'setup']].find(([k]) => ok(p.get(k)));
  if (!pick) return;
  const value = p.get(pick[0]);
  if (size !== S.n) engine.send('size', size);
  engine.send(pick[1], value);
  if (pick[0] === 'alg') {
    algDraft = value;
    openPanel('algs');
  }
}

// Links clicked while the app is open (a shared #scramble=..., #daily).
addEventListener('hashchange', () => {
  if (started) handleHash();
});

// ── Boot ────────────────────────────────────────────────────────────────────

function onEngineReady() {
  started = true;
  hideLoader();
  engine.send('settings', engineSettings());
  engine.flush();
  syncInsets();
  handleHash();
  checkAchievements();
  const today = St.utcDay();
  $('#dailyDot').hidden = daily.has(today);
  retryPending();
  if (!store.raw(KEYS.seen)) {
    store.set(KEYS.seen, 1);
    setTimeout(() => toast(t('welcome'), { em: '👋', sub: t('welcomeSub'), ms: 7000, action: { label: t('tryDaily'), fn: () => openPanel('daily') } }), 900);
  }
}

function hideLoader() {
  const loader = $('#loader');
  if (!loader || loader.classList.contains('done')) return;
  loader.classList.add('done');
  setTimeout(() => loader.remove(), 500);
}

/**
 * Shows a failure on the loader; `detail` is a smaller line under it. A hard
 * failure (not `soft`) also stops the download progress in
 * assets/initializer.mjs from writing over it.
 */
function loaderError(msg, { actions = true, detail = '', soft = false } = {}) {
  if (gpuError !== null) return; // keeps the reason, not the panic that follows
  if (!soft) $('#loader')?.setAttribute('data-failed', '');
  const label = $('#ldr-label');
  if (label) label.textContent = msg;
  $('#ldr-detail')?.remove();
  if (detail && label) label.after(h('p', { id: 'ldr-detail', text: detail }));
  const acts = $('#ldr-actions');
  if (acts && actions) acts.hidden = false;
}

async function hardReload() {
  try {
    if (window.caches) await Promise.all((await caches.keys()).map((k) => caches.delete(k)));
    if (navigator.serviceWorker) await Promise.all((await navigator.serviceWorker.getRegistrations()).map((r) => r.unregister()));
  } catch {
    /* best effort */
  }
  location.reload();
}

$('#ldrReload').addEventListener('click', () => location.reload());
$('#ldrClear').addEventListener('click', hardReload);

const isControlFlow = (x) => {
  const m = String((x && (x.message || x)) || '');
  return m.includes('control flow') || m.includes("isn't actually an error");
};

addEventListener('error', (e) => {
  if (isControlFlow(e.error || e.message)) return;
  if (!started) loaderError(t('unexpected'));
  else if (e.error) toast(t('unexpected'), { em: '⚠️', ms: 0, action: { label: t('reload'), fn: () => location.reload() } });
});
addEventListener('unhandledrejection', (e) => {
  if (isControlFlow(e.reason)) return;
  if (!started) loaderError(t('loadFailed'));
});

// WebGL2 on the engine's own canvas, with the options the engine (wgpu) asks
// for: when it calls getContext it gets this same context back, so this
// answers exactly what the engine will see. If the browser refuses, the
// engine panics a moment later; the loader keeps the reason instead, which
// the browser gives in a webglcontextcreationerror event (Chrome: "blocked
// after a GPU reset", "hardware acceleration disabled", …).
{
  const canvas = $('#bevy');
  let reason = '';
  const onFail = (e) => { reason = String(e.statusMessage || ''); };
  canvas.addEventListener('webglcontextcreationerror', onFail);
  let gl = null;
  try {
    gl = canvas.getContext('webgl2', { antialias: false });
  } catch {
    gl = null;
  }
  canvas.removeEventListener('webglcontextcreationerror', onFail);
  if (!gl) {
    loaderError(t('noWebgl'), { detail: reason ? `${t('noWebglHint')} (${reason.slice(0, 200)})` : t('noWebglHint') });
    gpuError = reason;
    const track = $('.ldr-track');
    if (track) track.hidden = true;
  } else {
    // The engine can't rebuild its GPU resources: a lost context (driver
    // reset, GPU process crash) needs a reload rather than a frozen cube.
    canvas.addEventListener('webglcontextlost', () => {
      if (!started) loaderError(t('gpuLost'), { detail: t('gpuLostSub') });
      else toast(t('gpuLost'), { em: '⚠️', sub: t('gpuLostSub'), ms: 0, action: { label: t('reload'), fn: () => location.reload() } });
    });
  }
}

setTimeout(() => {
  if (!started) loaderError(t('slowLoad'), { soft: true });
}, 30000);

// Service worker + update prompt.
if ('serviceWorker' in navigator && location.protocol === 'https:') {
  addEventListener('load', async () => {
    try {
      const reg = await navigator.serviceWorker.register('/service-worker.js', { updateViaCache: 'none' });
      // Reload only when the user asked for the update. The first-generation
      // worker (/sw.js) is replaced silently: the new one takes over without
      // a reload, since this page already runs the new code.
      let wantReload = false;
      const offer = (w) => {
        const ctl = navigator.serviceWorker.controller;
        if (!w || !ctl || ctl.scriptURL.endsWith('/sw.js')) return;
        toast(t('updateReady'), {
          em: '✨',
          ms: 0,
          action: { label: t('updateNow'), fn: () => { wantReload = true; w.postMessage({ type: 'SKIP_WAITING' }); } },
        });
      };
      if (reg.waiting) offer(reg.waiting);
      reg.addEventListener('updatefound', () => {
        const w = reg.installing;
        if (w) w.addEventListener('statechange', () => { if (w.state === 'installed') offer(w); });
      });
      let reloading = false;
      navigator.serviceWorker.addEventListener('controllerchange', () => {
        if (reloading || !wantReload) return;
        reloading = true;
        location.reload();
      });
      document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible') reg.update().catch(() => {}); });
    } catch {
      /* SW is an enhancement */
    }
  });
}

function track(name, data) {
  try {
    if (window.umami && typeof window.umami.track === 'function') window.umami.track(name, data);
  } catch {
    /* analytics must never break the app */
  }
}

wideMq.addEventListener('change', () => { renderStatus(); if (sheet.open) { const p = currentPanel; closePanel(); if (p) openPanel(p); } });

$('#solvedClose').addEventListener('click', () => $('#solvedModal').close());
$('#solvedAgain').addEventListener('click', () => {
  $('#solvedModal').close();
  if (lastSolved && lastSolved.dailyDay !== null) openPanel('daily');
  else if (S.mode.startsWith('trainer:')) randomCase();
  else doScramble();
});
$('#solvedShare').addEventListener('click', shareSolved);
$('#solvedCard').addEventListener('click', () => {
  if (lastSolved) saveCard(lastSolved.dailyDay !== null ? dailyCard(lastSolved.dailyDay) : solvedCard(lastSolved));
});
addEventListener('online', retryPending);
$('#solvedModal').addEventListener('click', (e) => { if (e.target === e.currentTarget) e.currentTarget.close(); });

applyStaticI18n();
renderAll();
if (!$('#loader')?.hasAttribute('data-failed')) $('#ldr-label').textContent = t('loading');
