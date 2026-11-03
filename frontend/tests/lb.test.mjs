// Leaderboard client rules. Run: node --test frontend/tests/
import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as Lb from '../ui/lb.js';

test('names follow the server rules', () => {
  assert.equal(Lb.cleanName('  Ana   Maria '), 'Ana Maria');
  assert.equal(Lb.cleanName('Ștefan Țăran'), 'Ștefan Țăran');
  assert.equal(Lb.cleanName("o'Neil_99.x-y"), "o'Neil_99.x-y");
  assert.equal(Lb.cleanName('A'), null);
  assert.equal(Lb.cleanName('a'.repeat(17)), null);
  assert.equal(Lb.cleanName('ș'.repeat(16)), 'ș'.repeat(16));
  assert.equal(Lb.cleanName('cube😀'), null);
  assert.equal(Lb.cleanName('abc‮def'), null);
  assert.equal(Lb.cleanName('2×3'), null);
  assert.equal(Lb.cleanName('---'), null);
  assert.equal(Lb.cleanName('tab\there'), null);
  assert.equal(Lb.cleanName(42), null);
});

test('tokens are 32 lowercase hex digits and random', () => {
  const a = Lb.newToken();
  assert.match(a, /^[0-9a-f]{32}$/);
  assert.notEqual(a, Lb.newToken());
});

test('state from storage is validated', () => {
  const today = 20_760;
  const fresh = Lb.loadState(null, today);
  assert.equal(fresh.name, null);
  assert.equal(fresh.fresh, true);
  assert.match(fresh.token, /^[0-9a-f]{32}$/);

  const entry = { id: '0123456789abcdef', rank: 3, total: 10, ms: 25_000, moves: 50 };
  const s = Lb.loadState({
    name: 'Ana', token: 'f'.repeat(32),
    entries: [[today, entry], [today - 1, { ...entry, id: 'bad' }], [today - 400, entry], ['x', entry]],
    pending: { day: today - 1, ms: 30_000, sol: "R U R'" },
  }, today);
  assert.equal(s.fresh, false);
  assert.equal(s.name, 'Ana');
  assert.deepEqual([...s.entries.keys()], [today]);
  assert.deepEqual(s.pending, { day: today - 1, ms: 30_000, sol: "R U R'" });
  // Round trip.
  assert.deepEqual(Lb.loadState(JSON.parse(JSON.stringify(Lb.toJSON(s))), today).entries, s.entries);

  // A pending solve for a closed day, or one already published, is dropped.
  assert.equal(Lb.loadState({ pending: { day: today - 2, ms: 1, sol: 'R' } }, today).pending, null);
  assert.equal(Lb.loadState({ entries: [[today, entry]], pending: { day: today, ms: 1, sol: 'R' } }, today).pending, null);
  assert.equal(Lb.loadState({ name: 'x<script>', token: 'nothex' }, today).name, null);
});

test('boards from the API are validated', () => {
  const row = { rank: 1, name: 'Ana', ms: 20_000, moves: 40 };
  const b = Lb.parseBoard({ day: 5, today: 5, total: 2, top: [row, { rank: 'x' }], me: row });
  assert.deepEqual(b.top, [row]);
  assert.deepEqual(b.me, row);
  assert.equal(Lb.parseBoard({ day: 5 }), null);
  assert.equal(Lb.parseBoard(null), null);
  assert.equal(Lb.parseBoard({ day: 5, today: 5, total: 0, top: Array(80).fill(row), me: null }).top.length, Lb.TOP);
});

test('failed submissions are retried, renamed or dropped', () => {
  assert.equal(Lb.failureAction({ status: 0, code: 'network' }).action, 'retry');
  assert.equal(Lb.failureAction({ status: 502, code: '' }).action, 'retry');
  assert.equal(Lb.failureAction({ status: 429, code: '' }).action, 'retry');
  assert.equal(Lb.failureAction({ status: 429, code: 'limit' }).action, 'drop');
  assert.equal(Lb.failureAction({ status: 422, code: 'name' }).action, 'name');
  assert.equal(Lb.failureAction({ status: 422, code: 'unsolved' }).msg, 'lbRejected');
  assert.equal(Lb.failureAction({ status: 422, code: 'day_closed' }).msg, 'lbClosed');
  assert.equal(Lb.failureAction({ status: 400, code: 'bad_request' }).action, 'drop');
});

test('only today and yesterday are open', () => {
  assert.ok(Lb.isOpen(10, 10));
  assert.ok(Lb.isOpen(9, 10));
  assert.ok(!Lb.isOpen(8, 10));
  assert.ok(!Lb.isOpen(11, 10));
  assert.ok(!Lb.isOpen('10', 10));
});
