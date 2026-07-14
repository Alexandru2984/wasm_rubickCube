# 🧊 Rubik's Cube — Rust + Bevy + WebAssembly

[![CI](https://github.com/Alexandru2984/wasm_rubickCube/actions/workflows/ci.yml/badge.svg)](https://github.com/Alexandru2984/wasm_rubickCube/actions/workflows/ci.yml)

An interactive 3D Rubik's Cube that runs entirely in the browser — no backend, no frameworks on the page, just a Bevy game engine scene compiled to WebAssembly.

**▶ Play it live: [cube.micutu.com](https://cube.micutu.com)**

Works on desktop (mouse + keyboard) and mobile (touch-first controls), installs as a PWA, and keeps working offline after the first visit.

## Features

- **2×2 up to 20×20 cubes** — one integer-lattice move engine drives every size; the grid uses doubled coordinates so even cubes (no middle layer) stay exact.
- **Live layer dragging** — grab any sticker and the layer follows your finger in real time, snapping to the nearest 90° on release. Drag outside the cube to orbit the camera; pinch to zoom.
- **Kociemba two-phase solver** (3×3) — the ✨ SOLVE button computes a ~20-move solution from *any* reachable state (via [kewb](https://crates.io/crates/kewb)) and plays it back move by move.
- **OLL / PLL algorithm trainer** (3×3) — pick any of the 78 last-layer cases; the cube sets itself into that case (setup by inverse) and the algorithm solves it, with an optional random recognition angle.
- **Rewind** — alternatively, watch the cube retrace your entire move history in reverse (any size).
- **Speedcubing timer** — arms after a scramble, starts on your first move, stops on solve detection, and tracks a personal best per cube size.
- **Undo / redo** — buttons or `Ctrl+Z` / `Ctrl+Shift+Z`.
- **Full persistence** — cube size, state, history and records survive page reloads (localStorage).
- **Keyboard notation** — `R L U D F B` faces, `M E S` slices, `x y z` whole-cube rotations, `Shift` for prime moves.

## Engineering highlights

**25 MB → 11 MB WASM (4.1 MB on the wire).** Bevy's default feature set ships audio, glTF, animation, UI, and text stacks this project never touches. Trimming to `bevy_winit + bevy_pbr + webgl2`, building with full LTO at `opt-level = "z"`, and serving precompressed gzip through nginx `gzip_static` cut first-load size by ~84%. A loading screen with real download progress (Trunk initializer hooks) covers the rest.

**Drag detection with real geometry.** A screen-space ray is cast against the cubies' rotated AABBs; the hit face's normal restricts candidate rotation axes to the two lying in the face plane — the same constraint a physical cube gives your hand. The chosen axis is the one whose screen-projected tangent best matches the drag direction, and the layer angle then tracks the pointer directly (radians per pixel), so the cube feels held rather than clicked.

**State extraction for the solver.** The renderer is the source of truth: each cubie's home position is recovered from its orientation quaternion (`home = q⁻¹ · pos`), stickers are projected to their current world directions, and the 54-facelet string is assembled with face letters assigned from the *current* centers — which is what makes slice moves (they relocate centers) solve correctly. The solution comes back in face letters and is mapped onto world-axis layer rotations.

**Solved detection that survives center twists.** Comparing piece orientations naively fails on a real solve: face centers accumulate invisible twists around their own normals. Solved is therefore defined on facelets — every face uniform — which is the physical definition.

**Setup-by-inverse trainer.** The OLL/PLL trainer doesn't hardcode per-case scrambles. It parses each algorithm from standard notation, runs it backwards to place the cube into exactly the state that algorithm resolves, then hands control back. This makes correctness independent of labels — and a unit test proves it for all 78 cases at once: every algorithm parses, its inverse-then-forward is identity, it preserves the first two layers, and it matches its declared category (PLL leaves the top face one color, OLL doesn't).

**No float drift, ever.** Grid positions are integers; after every animation the orientation quaternion is snapped back to the nearest 90°-multiple rotation matrix. A thousand moves later the cube is still exact, which is also what lets the persistence layer store a whole session as 3 digits per move.

All of the above is covered by native unit tests (`cargo test`), including scramble → solver → apply → solved round-trips over random states with slice moves.

## Tech stack

| | |
|---|---|
| Language | Rust |
| Engine | [Bevy 0.14](https://bevyengine.org/) (trimmed features, WebGL2) |
| UI overlay | `bevy_egui` |
| Solver | [kewb](https://crates.io/crates/kewb) (Kociemba two-phase) |
| Build | [Trunk](https://trunkrs.dev/) → `wasm32-unknown-unknown`, `wasm-opt -Oz` |
| Hosting | nginx, static files + `gzip_static`, PWA service worker |

## Controls

| Input | Action |
|---|---|
| Drag on a sticker | Rotate that layer (follows the pointer, snaps on release) |
| Drag on background | Orbit camera |
| Scroll / pinch | Zoom |
| `R L U D F B` / `M E S` | Face / slice moves (`Shift` = counter-clockwise) |
| `x y z` | Whole-cube rotations (`Shift` = counter-clockwise) |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| 2×2 … 5×5 selector | Switch cube size (top-right) |

## Development

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk

cd frontend
trunk serve            # dev server at http://127.0.0.1:8080
cargo test             # native unit tests (cube algebra, codec, solver round-trips)
```

`./deploy.sh` builds the release bundle, precompresses it, and copies it to the web root.

## Project layout

```
frontend/
  src/main.rs      # scene, input (mouse/touch), move engine, UI, persistence
  src/solver.rs    # scene → facelets → Kociemba solver → world moves
  src/trainer.rs   # OLL/PLL algorithm library, notation parser, setup-by-inverse
  assets/          # PWA manifest, icons, service worker, loading screen
backend/           # experimental Gleam stub — not deployed
```
