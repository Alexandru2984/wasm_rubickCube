//// Parser for the solution log the engine reports with a solved daily
//// (`format_alg(.., 3)` in frontend/src/notation.rs). Only the tokens that
//// formatter can produce for a 3x3 are accepted, separated by spaces:
////
//// - faces `R L U D F B`, wide `Rw ..`, whole-cube wide `3Rw ..`
//// - middle slices `M E S` (M follows L, E follows D, S follows F)
//// - rotations `x y z` (x follows R, y follows U, z follows F)
//// - each with no suffix, `'` or `2`.

import cube_leaderboard/cube.{type Turn, Turn}
import gleam/list
import gleam/result
import gleam/string

/// The engine stops logging past 2000 turns; anything longer is refused.
pub const max_moves = 2000

pub type Parsed {
  /// `moves` counts every turn except whole-cube rotations, like the
  /// engine's move counter.
  Parsed(turns: List(Turn), moves: Int)
}

pub fn parse(text: String) -> Result(Parsed, Nil) {
  let tokens =
    text
    |> string.split(" ")
    |> list.filter(fn(t) { t != "" })
  use <- guard(tokens == [] || list.length(tokens) > max_moves)
  use parsed <- result.try(list.try_map(tokens, token))
  let moves = list.count(parsed, fn(p) { p.1 })
  Ok(Parsed(list.map(parsed, fn(p) { p.0 }), moves))
}

fn guard(fails: Bool, next: fn() -> Result(a, Nil)) -> Result(a, Nil) {
  case fails {
    True -> Error(Nil)
    False -> next()
  }
}

fn token(tok: String) -> Result(#(Turn, Bool), Nil) {
  let #(body, turns) = case string.ends_with(tok, "'") {
    True -> #(string.drop_end(tok, 1), -1)
    False ->
      case string.ends_with(tok, "2") {
        True -> #(string.drop_end(tok, 1), 2)
        False -> #(tok, 1)
      }
  }
  use #(axis, layers, dir, counted) <- result.map(base(body))
  let quarters = case turns {
    2 -> 2
    t -> dir * t
  }
  #(Turn(axis, layers, quarters), counted)
}

/// Token body → (axis, layers, quarters of one clockwise turn, counted).
fn base(body: String) -> Result(#(Int, List(Int), Int, Bool), Nil) {
  let all = [-1, 0, 1]
  case body {
    "R" -> Ok(#(0, [1], -1, True))
    "L" -> Ok(#(0, [-1], 1, True))
    "U" -> Ok(#(1, [1], -1, True))
    "D" -> Ok(#(1, [-1], 1, True))
    "F" -> Ok(#(2, [1], -1, True))
    "B" -> Ok(#(2, [-1], 1, True))
    "Rw" -> Ok(#(0, [0, 1], -1, True))
    "Lw" -> Ok(#(0, [-1, 0], 1, True))
    "Uw" -> Ok(#(1, [0, 1], -1, True))
    "Dw" -> Ok(#(1, [-1, 0], 1, True))
    "Fw" -> Ok(#(2, [0, 1], -1, True))
    "Bw" -> Ok(#(2, [-1, 0], 1, True))
    "3Rw" -> Ok(#(0, all, -1, True))
    "3Lw" -> Ok(#(0, all, 1, True))
    "3Uw" -> Ok(#(1, all, -1, True))
    "3Dw" -> Ok(#(1, all, 1, True))
    "3Fw" -> Ok(#(2, all, -1, True))
    "3Bw" -> Ok(#(2, all, 1, True))
    "M" -> Ok(#(0, [0], 1, True))
    "E" -> Ok(#(1, [0], 1, True))
    "S" -> Ok(#(2, [0], -1, True))
    "x" -> Ok(#(0, all, -1, False))
    "y" -> Ok(#(1, all, -1, False))
    "z" -> Ok(#(2, all, -1, False))
    _ -> Error(Nil)
  }
}
