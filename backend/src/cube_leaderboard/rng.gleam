//// SplitMix64, bit-for-bit identical to the engine's `scramble::Rng`
//// (frontend/src/scramble.rs). Erlang integers are unbounded, so every
//// step masks back to 64 bits to reproduce Rust's wrapping arithmetic.

import gleam/int

const mask = 0xFFFFFFFFFFFFFFFF

const golden = 0x9E3779B97F4A7C15

pub opaque type Rng {
  Rng(state: Int)
}

pub fn new(seed: Int) -> Rng {
  Rng(int.bitwise_exclusive_or(int.bitwise_and(seed, mask), golden))
}

pub fn next(rng: Rng) -> #(Int, Rng) {
  let s = int.bitwise_and(rng.state + golden, mask)
  let z = wrapping_mul(xor_shift(s, 30), 0xBF58476D1CE4E5B9)
  let z = wrapping_mul(xor_shift(z, 27), 0x94D049BB133111EB)
  #(xor_shift(z, 31), Rng(s))
}

/// Uniform-ish value in 0..n-1, as Rust's `next_u64() % n`.
pub fn range(rng: Rng, n: Int) -> #(Int, Rng) {
  let #(value, rng) = next(rng)
  #(value % n, rng)
}

/// Seed of the daily challenge, as the engine's `bridge::daily_seed`.
pub fn daily_seed(day: Int) -> Int {
  int.bitwise_exclusive_or(
    wrapping_mul(day, 0x2545F4914F6CDD1D),
    0xC0BED41170000000,
  )
}

fn xor_shift(z: Int, by: Int) -> Int {
  int.bitwise_exclusive_or(z, int.bitwise_shift_right(z, by))
}

fn wrapping_mul(a: Int, b: Int) -> Int {
  int.bitwise_and(a * b, mask)
}
