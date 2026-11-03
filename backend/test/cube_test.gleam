//// The engine (frontend/src/scramble.rs, test
//// `daily_scrambles_match_the_server`) pins the same vectors: if the two
//// generators ever disagree, every daily submission would be rejected.

import cube_leaderboard/cube
import cube_leaderboard/notation
import cube_leaderboard/rng
import cube_leaderboard/scramble
import gleam/list

/// #(day, seed, scramble, a Kociemba solution of that scramble).
const vectors = [
  #(
    0,
    13_888_771_472_215_769_088,
    "F B' R L U F' U' R2 U F' L' B2 U' D B F L2 R B D U' R' F B' R2",
    "F L2 U' F L2 U L F2 R' F R2 B2 D2 F2 D L2 D B2 D R2 U' L2",
  ),
  #(
    1,
    16_571_874_989_031_873_821,
    "L2 F' D R' U' B' D' R2 U B2 F' R U' D B' U' B D' U B F2 U' B R2 L",
    "R B' U D2 B2 D' R' F U2 F2 B R2 U' F2 D L2 U2 R2 B2 D B2",
  ),
  #(
    20_395,
    13_586_762_507_681_506_655,
    "R D R2 L2 U' D L D B L R U' D2 L2 D2 L U D2 R L U' L' D2 L2 D",
    "R F2 D' R' B' R' U' B' R B' L2 D2 F2 U R2 U' F2 L2 U' B2 L2 U",
  ),
  #(
    20_396,
    7_045_878_535_472_317_052,
    "D2 B F2 U F D R B R2 B2 R2 B2 U D' L B' D R2 F U' D' B' U' F2 D'",
    "B R B' U' D2 R L' D F U F2 U2 L2 B2 U' R2 D' B2 U' F2 U' L2",
  ),
  #(
    20_500,
    385_708_942_251_677_252,
    "U2 F D B2 D2 F L D' R' U' D2 B' F R' D R2 B' R U' R2 U2 R' L' D' R'",
    "D2 R2 B' R U D' F R2 L F2 R L2 U2 F2 R2 B2 D R2 F2 D' B2 U",
  ),
  #(
    21_000,
    5_924_762_292_955_263_720,
    "U2 F' U2 R' U' D' B F L' D' L2 D R' B2 U2 B' D' R' B' U F R' F2 L D",
    "D' L' D L2 U R F2 D' B U2 B2 R2 U' R2 U B2 U R2 U2 F2 D' L2",
  ),
]

pub fn daily_seed_matches_engine_test() {
  list.each(vectors, fn(v) {
    let #(day, seed, _, _) = v
    assert rng.daily_seed(day) == seed
  })
}

pub fn daily_scramble_matches_engine_test() {
  list.each(vectors, fn(v) {
    let #(day, _, text, _) = v
    assert scramble.format(scramble.daily(day)) == text
  })
}

pub fn known_solutions_solve_the_daily_test() {
  list.each(vectors, fn(v) {
    let #(day, _, _, solution) = v
    let assert Ok(parsed) = notation.parse(solution)
    let scrambled =
      cube.solved() |> cube.apply_all(scramble.to_turns(scramble.daily(day)))
    assert !cube.is_solved(scrambled)
    assert cube.is_solved(cube.apply_all(scrambled, parsed.turns))
    assert parsed.moves == list.length(parsed.turns)
  })
}

pub fn scramble_parses_like_it_generates_test() {
  // The formatted scramble, parsed back, is the same cube state.
  let moves = scramble.daily(20_760)
  let assert Ok(parsed) = notation.parse(scramble.format(moves))
  assert parsed.turns == scramble.to_turns(moves)
}

fn same(a: String, b: String) -> Bool {
  let assert Ok(pa) = notation.parse(a)
  let assert Ok(pb) = notation.parse(b)
  let start =
    cube.solved() |> cube.apply_all(scramble.to_turns(scramble.daily(7)))
  cube.apply_all(start, pa.turns) == cube.apply_all(start, pb.turns)
}

pub fn notation_identities_match_the_engine_test() {
  // The same identities the engine's notation tests pin.
  assert same("M", "R L' x'")
  assert same("E", "U D' y'")
  assert same("S", "F' B z")
  assert same("Rw", "L x")
  assert same("x", "R M' L'")
  assert same("Uw", "D y")
  assert same("Fw", "B z")
  assert same("3Rw", "x")
  assert same("3Lw", "x'")
  assert same("R2", "R R")
  assert same("R'", "R R R")
  assert !same("R", "R'")
}

pub fn rotations_are_free_but_still_applied_test() {
  let assert Ok(p) = notation.parse("x y' z2 R U M2")
  assert p.moves == 3
  assert list.length(p.turns) == 6
}

pub fn solved_ignores_orientation_test() {
  let assert Ok(p) = notation.parse("x y2 z'")
  assert cube.is_solved(cube.apply_all(cube.solved(), p.turns))
  let assert Ok(p) = notation.parse("R U R' U'")
  assert !cube.is_solved(cube.apply_all(cube.solved(), p.turns))
  // The sexy move has order 6.
  let assert Ok(p) =
    notation.parse(
      "R U R' U' R U R' U' R U R' U' R U R' U' R U R' U' R U R' U'",
    )
  assert cube.is_solved(cube.apply_all(cube.solved(), p.turns))
}

pub fn notation_rejects_junk_test() {
  assert notation.parse("") == Error(Nil)
  assert notation.parse("   ") == Error(Nil)
  assert notation.parse("R2'") == Error(Nil)
  assert notation.parse("R3") == Error(Nil)
  assert notation.parse("2R") == Error(Nil)
  assert notation.parse("r") == Error(Nil)
  assert notation.parse("(R U)2") == Error(Nil)
  assert notation.parse("R\tU") == Error(Nil)
  assert notation.parse("R U\u{2019}") == Error(Nil)
  let too_long = list.repeat("R", notation.max_moves + 1)
  assert notation.parse(join(too_long)) == Error(Nil)
  let just_fits = list.repeat("R", notation.max_moves)
  let assert Ok(_) = notation.parse(join(just_fits))
}

fn join(parts: List(String)) -> String {
  list.fold(parts, "", fn(acc, p) { acc <> " " <> p })
}
