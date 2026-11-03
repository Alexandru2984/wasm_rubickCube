# 🧊 Rubik's Cube 3D — Rust + Bevy + WebAssembly

[![CI](https://github.com/Alexandru2984/wasm_rubickCube/actions/workflows/ci.yml/badge.svg)](https://github.com/Alexandru2984/wasm_rubickCube/actions/workflows/ci.yml)

A 3D Rubik's Cube you can play, time and learn in the browser. The cube is a Bevy scene compiled to WebAssembly. Around it is a hand-written HTML interface built for phones first. There's no backend and no account, and after the first visit it works offline.

**▶ Play: [cube.micutu.com](https://cube.micutu.com)**

## Features

**Play**
- **2×2 up to 20×20.** One integer-lattice move engine drives every size; doubled coordinates keep even cubes exact.
- **Live layer dragging.** Grab a sticker and the layer follows your finger, snapping to 90° on release. Drag the background to orbit, pinch or scroll to zoom.
- **Move keypad** for full notation (`R U' M2 x …`) and a **keyboard**: face letters, `Shift` for prime, `Alt` for wide moves, `Space` to scramble, `Ctrl+Z / Ctrl+Y`.
- **Undo / redo / rewind**, and a **move player** that steps through solutions and algorithms (play, pause, step, jump to start or end).

**Speedcubing**
- WCA-style random-move scrambles (deterministic per seed), a timer that starts on your first turn, and optional **WCA inspection** (15 s, warnings at 8 s and 12 s, +2 and DNF).
- **Statistics per size**: ao5 / ao12 / ao100 with WCA trimming, best single and averages, a progress chart, +2/DNF/delete per solve, plus CSV and JSON export and a validated import.
- **Kociemba two-phase solver** (3×3, via [kewb](https://crates.io/crates/kewb)). It plays its ~20-move solution step by step, or gives a single-move **hint**.

**Coming back every day**
- **Daily challenge**: the same scramble for everyone each day, a countdown, the last 7 days and a streak.
- **Algorithm trainer**: all 21 PLL and 57 OLL cases plus patterns, with a random AUF for recognition, per-case stats and "show me".
- **Algorithm lab**: write any sequence, then run it, set it up, invert it or share it as a link.
- **23 achievements**, a solved screen with your PB and ao5, confetti, sound and haptics.
- **Share links** for a scramble, an algorithm or the current state (`#scramble=…`, `#alg=…`, `#state=…`, `#daily`), with Web Share on phones.

**Comfort**
- Romanian and English, five color schemes (including a colorblind-friendly one), backgrounds, animation speed and a battery saver.
- Installable PWA with shortcuts to the daily challenge, the trainer and stats. The engine renders on demand, so an idle cube costs no battery.

## Security

The site is static, but it's still treated as an attack surface:

- **Strict CSP**: `default-src 'none'`, with no `'unsafe-inline'` for scripts or styles. Only `'wasm-unsafe-eval'` is needed for WebAssembly. The build moves Trunk's inline bootstrap into a hashed file and **fails** if any inline script, event handler or `style=` attribute appears.
- **Subresource Integrity** on every first-party script and stylesheet. UI files are served from content-hashed URLs.
- The page never uses `innerHTML`. Every DOM node goes through a builder with an attribute allow-list. Everything read from storage, imports or the URL is validated and size-capped. CSV export defuses spreadsheet formulas.
- The engine accepts only allow-listed commands through a bounded queue. The notation parser limits input length, move count, nesting and repetition.
- nginx re-applies the security headers in every location. Responses carry HSTS, COOP/CORP, `X-Frame-Options: DENY` and a locked-down Permissions-Policy. Only GET and HEAD are allowed, real 404s replace the old SPA fallback, and dotfiles are denied.
- Source paths are remapped at build time, so the binary doesn't reveal the build machine's layout. CI pins every third-party action to a commit SHA, verifies the Trunk download's checksum and runs a strict `cargo audit` weekly.

## Engineering notes

- **Integer move engine.** Grid positions are integers and every quarter turn is an exact lattice rotation. Orientations are snapped back to 90° multiples after each animation, so a thousand moves later the cube is still exact, and the history can be stored as compact integers.
- **Drag detection with real geometry.** A screen ray is cast against the cubies' AABBs. The hit face's normal limits the candidate axes to the two lying in that face, and the drag direction picks between them.
- **State extraction for the solver.** Each cubie's home position is recovered from its orientation (`home = q⁻¹ · pos`). The 54-facelet string uses the *current* centers, which is why slice moves solve correctly.
- **Framing.** The page reports how much of the screen its UI covers, and the camera frames the cube in the free band, easing over when the keypad or the player opens. Lights are attached to the camera, so every angle shows three clearly shaded faces.
- **Size.** Trimmed Bevy features, full LTO, `opt-level = "z"` and `wasm-opt -Oz` produce 7.7 MB of WASM, about 2.5 MB gzipped, served precompressed through nginx `gzip_static`.

Native unit tests cover the move algebra, notation parser, scrambles, persistence codec, bridge parsing, camera fit and solver round-trips (`cargo test`). Node tests cover statistics, averages, validation and CSV escaping (`node --test`).

## Tech stack

| | |
|---|---|
| Engine | Rust, [Bevy 0.14](https://bevyengine.org/) (trimmed, WebGL2) |
| Interface | Plain ES modules + CSS, no framework (`frontend/ui/`) |
| Solver | [kewb](https://crates.io/crates/kewb) (Kociemba two-phase) |
| Build | [Trunk](https://trunkrs.dev/) → `wasm32-unknown-unknown`, then `tools/postbuild.py` |
| Hosting | nginx (static files, `gzip_static`) behind Cloudflare, atomic releases |

## Development

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked

cd frontend
trunk serve                      # dev server at http://127.0.0.1:8080
cargo test                       # engine unit tests
node --test tests/*.test.mjs     # page logic tests
./build.sh                       # release build into frontend/dist
```

## Deployment

```bash
deploy/install-nginx.sh    # install the vhost + header snippet (backs up, tests, reloads)
deploy/deploy.sh           # build, publish a new release, switch atomically
deploy/deploy.sh --rollback
```

Releases live in `/srv/canary/cube.micutu.com/releases/<UTC stamp>-<commit>/`. The site root is a symlink, swapped with a single `rename(2)`, and the last five releases are kept.

## Project layout

```
frontend/
  src/main.rs        app setup, plugins, system order
  src/cube.rs        cubies, integer moves, color schemes
  src/game.rs        session state, commands, animation, solve detection, state to the page
  src/input.rs       pointer drag/orbit/pinch, camera framing
  src/notation.rs    WCA/SiGN notation parser and formatter
  src/scramble.rs    seeded random-move scrambles
  src/solver.rs      scene → facelets → Kociemba → moves
  src/trainer.rs     PLL/OLL/pattern library
  src/persist.rs     localStorage codec (with migration from v3)
  src/bridge.rs      page ⇄ engine commands and events
  ui/                the interface (app.js, stats.js, i18n.js, fx.js, app.css)
  assets/            manifest, icons, service worker, loader, 404, OG image
  tools/postbuild.py CSP/SRI/hashing/precache/sitemap post-processing
deploy/              nginx vhost + headers, install and deploy scripts
backend/             Gleam placeholder, not deployed (a future home for leaderboards)
```
