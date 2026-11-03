//// The daily 3x3 scramble, regenerated exactly as the engine does
//// (frontend/src/scramble.rs, `generate(daily_seed(day), 3)`). The draw
//// order matters: every candidate move draws axis, side and depth, and a
//// rejected candidate skips the quarter-turn draw.

import cube_leaderboard/cube.{type Turn, Turn}
import cube_leaderboard/rng.{type Rng}
import gleam/list
import gleam/string

/// WCA random-move length for 3x3.
const length = 25

pub type FaceTurn {
  /// `sign`: which outer layer (+1 = R/U/F side). `quarters`: around +axis.
  FaceTurn(axis: Int, sign: Int, quarters: Int)
}

pub fn daily(day: Int) -> List(FaceTurn) {
  generate(rng.new(rng.daily_seed(day)), length, -1, [], [])
}

pub fn to_turns(moves: List(FaceTurn)) -> List(Turn) {
  list.map(moves, fn(m) { Turn(m.axis, [m.sign], m.quarters) })
}

pub fn format(moves: List(FaceTurn)) -> String {
  moves
  |> list.map(fn(m) {
    let letter = case m.axis, m.sign {
      0, 1 -> "R"
      0, _ -> "L"
      1, 1 -> "U"
      1, _ -> "D"
      _, 1 -> "F"
      _, _ -> "B"
    }
    let turns = case m.quarters {
      2 | -2 -> 2
      q -> -m.sign * q
    }
    letter
    <> case turns {
      1 -> ""
      -1 -> "'"
      _ -> "2"
    }
  })
  |> string.join(" ")
}

fn generate(
  r: Rng,
  left: Int,
  run_axis: Int,
  run_used: List(Int),
  acc: List(FaceTurn),
) -> List(FaceTurn) {
  case left {
    0 -> list.reverse(acc)
    _ -> {
      let #(axis, r) = rng.range(r, 3)
      let #(side, r) = rng.range(r, 2)
      let sign = case side {
        0 -> 1
        _ -> -1
      }
      // Depth is always 1 on a 3x3, but the engine still draws it.
      let #(_depth, r) = rng.range(r, 1)
      let same_axis = run_axis == axis
      case same_axis && list.contains(run_used, sign) {
        True -> generate(r, left, run_axis, run_used, acc)
        False -> {
          let used = case same_axis {
            True -> [sign, ..run_used]
            False -> [sign]
          }
          let #(q, r) = rng.range(r, 3)
          let quarters = case q {
            0 -> 1
            1 -> -1
            _ -> 2
          }
          generate(r, left - 1, axis, used, [
            FaceTurn(axis, sign, quarters),
            ..acc
          ])
        }
      }
    }
  }
}
