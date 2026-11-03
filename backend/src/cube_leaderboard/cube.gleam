//// A 3x3 cube as 54 stickers in the world frame, enough to replay a
//// scramble and a solution and tell whether the result is solved.
////
//// Coordinates: cubie centres in -1..1 on each axis. A turn rotates every
//// sticker whose cubie lies in one of the turned layers by quarter turns
//// around +axis (right-hand rule), the same convention as the engine's
//// `CubeMove::quarters`.

import gleam/dict
import gleam/list
import gleam/option

pub type Turn {
  /// `axis`: 0 = x, 1 = y, 2 = z. `layers`: cubie coordinates along the
  /// axis that turn. `quarters`: quarter turns around +axis, -1, 1 or 2.
  Turn(axis: Int, layers: List(Int), quarters: Int)
}

type Sticker {
  Sticker(x: Int, y: Int, z: Int, nx: Int, ny: Int, nz: Int, color: Int)
}

pub opaque type Cube {
  Cube(stickers: List(Sticker))
}

const normals = [
  #(1, 0, 0),
  #(-1, 0, 0),
  #(0, 1, 0),
  #(0, -1, 0),
  #(0, 0, 1),
  #(0, 0, -1),
]

const coords = [-1, 0, 1]

pub fn solved() -> Cube {
  let stickers =
    list.index_map(normals, fn(n, color) {
      let #(nx, ny, nz) = n
      use a <- list.flat_map(coords)
      use b <- list.map(coords)
      // The cubie sits on the face (coordinate = normal) and spans the
      // other two axes with a and b.
      case nx, ny {
        0, 0 -> Sticker(a, b, nz, nx, ny, nz, color)
        0, _ -> Sticker(a, ny, b, nx, ny, nz, color)
        _, _ -> Sticker(nx, a, b, nx, ny, nz, color)
      }
    })
    |> list.flatten
  Cube(stickers)
}

pub fn apply(cube: Cube, turn: Turn) -> Cube {
  let times = { turn.quarters % 4 + 4 } % 4
  Cube(
    list.map(cube.stickers, fn(s) {
      case list.contains(turn.layers, along(s, turn.axis)) {
        True -> rotate_times(s, turn.axis, times)
        False -> s
      }
    }),
  )
}

pub fn apply_all(cube: Cube, turns: List(Turn)) -> Cube {
  list.fold(turns, cube, apply)
}

/// Solved = every face shows one color. Orientation doesn't matter, so a
/// solve that ends with the cube rotated (x, y, z) still counts.
pub fn is_solved(cube: Cube) -> Bool {
  let faces =
    list.fold(cube.stickers, dict.new(), fn(acc, s) {
      dict.upsert(acc, #(s.nx, s.ny, s.nz), fn(seen) {
        [s.color, ..option.unwrap(seen, [])]
      })
    })
  dict.size(faces) == 6
  && list.all(dict.values(faces), fn(colors) {
    case colors {
      [first, ..rest] ->
        list.length(colors) == 9 && list.all(rest, fn(c) { c == first })
      [] -> False
    }
  })
}

fn along(s: Sticker, axis: Int) -> Int {
  case axis {
    0 -> s.x
    1 -> s.y
    _ -> s.z
  }
}

fn rotate_times(s: Sticker, axis: Int, times: Int) -> Sticker {
  case times {
    0 -> s
    _ -> rotate_times(rotate(s, axis), axis, times - 1)
  }
}

/// +90° around +axis: x: (x,y,z) → (x,−z,y); y: → (z,y,−x); z: → (−y,x,z).
fn rotate(s: Sticker, axis: Int) -> Sticker {
  let #(x, y, z) = rot(axis, s.x, s.y, s.z)
  let #(nx, ny, nz) = rot(axis, s.nx, s.ny, s.nz)
  Sticker(x, y, z, nx, ny, nz, s.color)
}

fn rot(axis: Int, x: Int, y: Int, z: Int) -> #(Int, Int, Int) {
  case axis {
    0 -> #(x, -z, y)
    1 -> #(z, y, -x)
    _ -> #(-y, x, z)
  }
}
