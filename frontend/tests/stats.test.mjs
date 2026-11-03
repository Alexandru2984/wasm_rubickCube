// Unit tests for the page's pure logic. Run: node --test frontend/tests/
import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as St from '../ui/stats.js';

const solve = (t, extra = {}) => ({ t, n: 3, m: 40, d: 1_790_000_000_000, p: 0, ...extra });

test('fmt truncates to centiseconds and switches to minutes', () => {
  assert.equal(St.fmt(12_349), '12.34');
  assert.equal(St.fmt(5_000), '5.00');
  assert.equal(St.fmt(65_430), '1:05.43');
  assert.equal(St.fmt(3_600_000), '60:00.00');
  assert.equal(St.fmt(Infinity), 'DNF');
  assert.equal(St.fmt(null), '—');
});

test('ao5 trims best and worst', () => {
  const s = [10, 12, 14, 16, 50].map((x) => solve(x * 1000));
  assert.equal(St.averageOf(s, 5), 14_000);
  assert.equal(St.averageOf(s.slice(0, 4), 5), null);
});

test('one DNF in ao5 is trimmed, two make it DNF', () => {
  const one = [10, 12, 14, 16].map((x) => solve(x * 1000)).concat([solve(9000, { p: St.DNF })]);
  assert.equal(St.averageOf(one, 5), 14_000);
  const two = one.slice(0, 3).concat([solve(1, { p: St.DNF }), solve(1, { p: St.DNF })]);
  assert.equal(St.averageOf(two, 5), Infinity);
});

test('+2 counts in the effective time', () => {
  assert.equal(St.eff(solve(10_000, { p: 2000 })), 12_000);
  assert.equal(St.bestSingle([solve(10_000, { p: 2000 }), solve(11_000)]), 11_000);
});

test('ao100 trims 5 on each side', () => {
  const s = Array.from({ length: 100 }, (_, i) => solve((i + 1) * 100));
  // Keeps 6..95 → mean of 600..9500 step 100.
  assert.equal(St.averageOf(s, 100), 5050);
});

test('best rolling average', () => {
  const s = [20, 20, 20, 20, 20, 10, 10, 10, 10, 10].map((x) => solve(x * 1000));
  assert.equal(St.bestAverage(s, 5), 10_000);
});

test('streak counts back from today or yesterday', () => {
  assert.equal(St.streak([100, 101, 102], 102), 3);
  assert.equal(St.streak([100, 101, 102], 103), 3, 'today not done yet');
  assert.equal(St.streak([100, 101, 102], 104), 0);
  assert.equal(St.streak([], 5), 0);
});

test('validation drops malformed and hostile records', () => {
  const good = solve(1234, { s: "R U R'", k: 'd', day: 20_000 });
  const list = [
    good,
    { ...good, t: -1 },
    { ...good, n: 21 },
    { ...good, p: 5 },
    { ...good, k: 'evil' },
    { ...good, s: 'x'.repeat(5000) },
    { ...good, __proto__: { polluted: true }, extra: '<img onerror>' },
    null,
    'string',
    [1, 2, 3],
  ];
  const out = St.sanitizeSolves(list);
  assert.equal(out.length, 2);
  assert.deepEqual(Object.keys(out[1]).sort(), ['d', 'day', 'k', 'm', 'n', 'p', 's', 't']);
  assert.equal(({}).polluted, undefined);
  assert.deepEqual(St.sanitizeSolves({ not: 'an array' }), []);
});

test('merge skips duplicates', () => {
  const a = [solve(1000, { d: 1 }), solve(2000, { d: 2 })];
  const { merged, added } = St.mergeSolves(a, [solve(2000, { d: 2 }), solve(3000, { d: 3 })]);
  assert.equal(added, 1);
  assert.equal(merged.length, 3);
});

test('CSV defuses formula injection and quotes', () => {
  const csv = St.toCsv([solve(1000, { s: '=HYPERLINK("x")', c: 'a,b' })]);
  const row = csv.split('\n')[1];
  assert.match(row, /"'=HYPERLINK\(""x""\)"/);
  assert.match(row, /"a,b"/);
});

test('achievements follow the history', () => {
  const s = [solve(25_000, { m: 55 }), solve(70_000, { n: 2 })];
  const got = St.earned({ solves: s, dailyDays: [10, 11, 12], trainerCases: 0, pllCases: 0, today: 12 });
  for (const id of ['first', 'sub60', 'sub30', 'pocket', 'daily', 'streak3', 'efficient']) assert.ok(got.has(id), id);
  for (const id of ['sub20', 'big', 'ten', 'streak7']) assert.ok(!got.has(id), id);
});

test('daily numbering starts at #1 on launch day', () => {
  assert.equal(St.dailyNumber(St.LAUNCH_DAY), 1);
  assert.equal(St.utcDay(Date.UTC(2026, 10, 3, 23, 59)), St.LAUNCH_DAY);
});
