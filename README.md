# 🧊 3D Rubik's Cube (Rust + Bevy + WebAssembly)

A fully functional, interactive 3D Rubik's Cube built from scratch using [Rust](https://www.rust-lang.org/) and the [Bevy Engine](https://bevyengine.org/). The project is compiled to WebAssembly (WASM) to run natively and smoothly directly in the browser.

## ✨ Features

* **Authentic 3D Rendering:** Renders 26 individual cubies (ignoring the invisible center core for optimization) with physically accurate face spacing.
* **Advanced Orbit Camera:** Quaternion-based trackball camera implementation. Rotate the cube seamlessly without experiencing gimbal lock or 90-degree constraints.
* **Smooth Animations:** Moves are queued and animated using a `smoothstep` function for natural acceleration/deceleration.
* **Mathematical Precision:** Automatic integer snapping (`rotate_grid_pos`) post-animation prevents floating-point drift, ensuring the cube never deforms over time.
* **Full Control Set:** * Standard moves: `R`, `L`, `U`, `D`, `F`, `B`
  * Slice moves: `M` (Middle), `E` (Equator), `S` (Standing)
  * Prime moves (Counter-clockwise): Hold `Shift` + Move Key
* **Interactive UI:** Built with `bevy_egui`. Includes a randomized `Scramble` generator and an auto-`Solve` function that reverses the move history dynamically.

## 🛠️ Tech Stack

* **Language:** Rust
* **Game Engine:** Bevy (v0.14)
* **UI Framework:** `bevy_egui`
* **Target:** WebAssembly (`wasm32-unknown-unknown`)
* **Build Tool:** Trunk

## 🚀 How to Run (Locally)

1. Ensure you have the Rust toolchain installed.
2. Install the `wasm32-unknown-unknown` target:
   `rustup target add wasm32-unknown-unknown`
3. Install [Trunk](https://trunkrs.dev/) (the WASM web application bundler for Rust):
   `cargo install trunk`
4. Clone the repository and run the development server:
   `trunk serve`
5. Open your browser and navigate to `http://127.0.0.1:8080`.

## 🎮 Controls

### Camera
* **Left Click + Drag:** Rotate the camera around the cube.
* **Scroll Wheel:** Zoom in and out.

### Cube Movements
* **Standard Faces:** `R` (Right), `L` (Left), `U` (Up), `D` (Down), `F` (Front), `B` (Back)
* **Middle Slices:** `M` (Middle), `E` (Equator), `S` (Standing)
* **Invert Move:** Hold `Shift` while pressing any of the keys above.

### UI Panel
* **Scramble:** Applies 20 random moves to shuffle the cube.
* **Solve:** Automatically calculates and executes the reverse moves to solve the cube based on the session's history.