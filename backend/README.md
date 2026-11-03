# cube-leaderboard

The global leaderboard for the daily challenge on
[cube.micutu.com](https://cube.micutu.com). It's written in Gleam, runs on the BEAM and uses no
web framework: OTP's own `inets` httpd listens on loopback behind nginx.

## How a solve is checked

Times can't be trusted (the clock runs in the browser), but the solve itself can. When the
engine solves the daily, it reports the moves made since the scramble finished. For each
submission the server:

1. regenerates that day's scramble. `src/cube_leaderboard/rng.gleam` and `scramble.gleam` port
   the engine's SplitMix64 generator bit for bit, and both test suites pin the same vectors;
2. replays the scramble and the reported moves on a 54-sticker cube;
3. ranks the entry only if every face ends up one color.

The move count is the server's own (whole-cube rotations are free). The time must be at least 3 s
and no faster than 15 turns per second.

## API

| | |
|---|---|
| `GET /api/health` | `{"ok":true}` |
| `GET /api/daily/<day>[?me=<id>]` | `{day, today, final, total, top: [{rank,name,ms,moves,replay}] (50), me}` |
| `POST /api/daily/<day>` | body `{name, token, ms, sol}` → `201 {id, rank, total, name, ms, moves, fresh}` |
| `GET /api/daily/<day>/solve/<rank>` | `{day, rank, name, ms, moves, sol}` for ranks 1–10 of a final day |

`<day>` is the UTC day number (`Date.now() / 86400000`, floored). Today's challenge accepts
submissions, and so does yesterday's for the first hour after midnight UTC, so a solve that
crosses midnight still counts. After that the day is `final`, and its best solves can be fetched
and watched (`replay: true` on those rows). GET serves the last 60 days.

**Errors** come back as `{"error": code}`:

| Status | Codes |
|---|---|
| 400 | `bad_request` |
| 403 | `origin` |
| 404 | `not_found`; for a solve: `not_final`, `no_solution` |
| 405 | |
| 413 | body over 16 KB |
| 415 | `content_type` |
| 422 | `day_closed`, `name`, `time`, `solution`, `unsolved` |
| 429 | `limit`, five entries per address a day |
| 503 | `full`, `storage` |

## Abuse limits and privacy

- **One entry per browser per day.** `token` is a random 128-bit secret kept in the browser. Only
  its SHA-256 is stored, and a resubmission returns the first entry unchanged (status 200,
  `fresh: false`).
- **Five accepted entries per IPv4 address or IPv6 /64 per day.** This count is held in memory
  only. **No IP address is ever written to disk.**
- **Names**:
  - 2–16 characters: letters (including Latin diacritics such as ă â î ș ț), digits, spaces and
    `. _ - '`;
  - no emoji, symbols, control or bidi characters;
  - a best-effort filter for Romanian and English slurs.
- **nginx** rate-limits reads and writes per address before anything reaches the service.

## Storage

`$CUBE_LB_DATA/entries.jsonl` is append-only. Each line is one entry, synced to disk before the
reply. Moderation appends a tombstone (`{"del":"<id>"}`), as `deploy/backend/cube-lb-moderate`
does, and the next restart drops the entry. The file is replayed at startup, keeping the last 60
days in memory.

Solutions are kept only for replay of the top 10. A line carries its `sol` only if the entry
ranked in the day's top 10 when it arrived, with at most 200 such lines a day. In memory, an
entry drops its solution once it is pushed out of the top 10.

A daily timer backs the file up: a local snapshot, plus an encrypted copy on Google Drive that is
read back and verified (`deploy/backend/cube-lb-backup`).

## Development

```sh
gleam test                                 # 36 tests: vectors, replay, validation, store, HTTP
CUBE_LB_PORT=18737 CUBE_LB_DATA=/tmp/lb gleam run
curl -s localhost:18737/api/daily/$(( $(date +%s) / 86400 ))
```

| Variable | Default |
|---|---|
| `CUBE_LB_PORT` | `8737` (loopback only) |
| `CUBE_LB_DATA` | `./data` |
| `CUBE_LB_ORIGIN` | `https://cube.micutu.com` (required `Origin` on POST) |

Deployment, from the repo root: `deploy/backend/deploy-backend.sh` builds an Erlang shipment,
installs it in `/opt/cube-leaderboard` and restarts the hardened systemd unit.
