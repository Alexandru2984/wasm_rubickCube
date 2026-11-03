//! Kociemba solver integration (the kewb crate): extracts the cube state
//! from the Bevy scene, translates it into the 54-facelet string, and maps
//! the solution back onto world-axis moves.
//!
//! Everything is expressed in world-frame: the solver's "U" face is always the
//! +Y layer, and color letters are assigned from the *current* centers — so
//! slice moves (M/E/S), which move centers, are handled correctly.

use crate::cube::{home_face, CubeMove};
use bevy::prelude::*;
use kewb::{CubieCube, DataTable, FaceCube, Move as KewbMove, Solver};
use std::collections::HashMap;

/// World directions of the faces, in kewb letter order: U R F D L B.
const FACE_DIRS: [IVec3; 6] = [IVec3::Y, IVec3::X, IVec3::Z, IVec3::NEG_Y, IVec3::NEG_X, IVec3::NEG_Z];
const FACE_CHARS: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];

#[derive(Resource, Default)]
pub struct SolverContext {
    /// Two-phase tables; generated lazily on the first press of SOLVE (~1-2s on
    /// wasm, once per session). Serialized they'd be 6.8MB, so it's not
    /// worth embedding them in the binary.
    pub table: Option<DataTable>,
    /// Set by the SOLVE button; the solve runs once the cube is settled.
    pub pending: bool,
    /// Play the solution right away (SOLVE) or step through it (HINT).
    pub autoplay: bool,
    /// One frame of grace before the blocking table generation, so the page
    /// can paint its "working" state.
    pub armed: bool,
}

/// Grid position of the facelet (face, row, col) in Kociemba convention,
/// checked against kewb's CORNER_FACELET/EDGE_FACELET tables.
fn facelet_pos(face: usize, row: i32, col: i32) -> IVec3 {
    match face {
        0 => IVec3::new(col - 1, 1, row - 1),  // U
        1 => IVec3::new(1, 1 - row, 1 - col),  // R
        2 => IVec3::new(col - 1, 1 - row, 1),  // F
        3 => IVec3::new(col - 1, -1, 1 - row), // D
        4 => IVec3::new(-1, 1 - row, col - 1), // L
        _ => IVec3::new(1 - col, 1 - row, -1), // B
    }
}

/// Builds the 54-facelet string from the scene state: for each
/// cubie, the home position follows from its orientation (home = q⁻¹·pos), and
/// each home sticker is projected onto its current world direction.
pub fn scene_to_facelets(cubies: &[(IVec3, Quat)]) -> Option<String> {
    let mut stickers: HashMap<(IVec3, IVec3), usize> = HashMap::with_capacity(54);
    for &(pos, q) in cubies {
        let home = (q.conjugate() * pos.as_vec3()).round().as_ivec3();
        for axis in 0..3 {
            let sign = home[axis];
            if sign == 0 {
                continue;
            }
            let mut home_normal = Vec3::ZERO;
            home_normal[axis] = sign as f32;
            let world_dir = (q * home_normal).round().as_ivec3();
            stickers.insert((pos, world_dir), home_face(axis, sign));
        }
    }
    if stickers.len() != 54 {
        return None;
    }

    // Letters are assigned from the current center colors.
    let mut color_to_char = [None::<char>; 6];
    for (i, d) in FACE_DIRS.iter().enumerate() {
        let color = *stickers.get(&(*d, *d))?;
        color_to_char[color] = Some(FACE_CHARS[i]);
    }

    let mut out = String::with_capacity(54);
    for (face, dir) in FACE_DIRS.iter().enumerate() {
        for i in 0..9i32 {
            let pos = facelet_pos(face, i / 3, i % 3);
            let color = *stickers.get(&(pos, *dir))?;
            out.push(color_to_char[color]?);
        }
    }
    Some(out)
}

/// Solved cube = every face uniform, for any n x n size.
/// `max_c` is the doubled coordinate of the outer layer (n - 1). Unlike
/// comparing orientations directly, this definition ignores the (invisible)
/// twist of centers.
pub fn is_solved(cubies: &[(IVec3, Quat)], max_c: i32) -> bool {
    // Color seen on each face direction; None = not seen yet.
    let mut seen: [Option<usize>; 6] = [None; 6];
    for &(pos, q) in cubies {
        let home = (q.conjugate() * pos.as_vec3()).round().as_ivec3();
        for axis in 0..3 {
            if home[axis].abs() != max_c {
                continue;
            }
            let sign = home[axis].signum();
            let mut home_normal = Vec3::ZERO;
            home_normal[axis] = sign as f32;
            let world_dir = (q * home_normal).round().as_ivec3();
            let Some(face) = FACE_DIRS.iter().position(|d| *d == world_dir) else {
                return false;
            };
            let color = home_face(axis, sign);
            match seen[face] {
                None => seen[face] = Some(color),
                Some(c) if c == color => {}
                Some(_) => return false,
            }
        }
    }
    true
}

/// Translates a kewb move (face letter) into a world-axis move.
/// A clockwise turn of the face with outward direction d is -90° around d.
fn to_cube_move(m: KewbMove) -> CubeMove {
    use KewbMove::*;
    let (face, turns) = match m {
        U => (0, 1),
        U2 => (0, 2),
        U3 => (0, -1),
        R => (1, 1),
        R2 => (1, 2),
        R3 => (1, -1),
        F => (2, 1),
        F2 => (2, 2),
        F3 => (2, -1),
        D => (3, 1),
        D2 => (3, 2),
        D3 => (3, -1),
        L => (4, 1),
        L2 => (4, 2),
        L3 => (4, -1),
        B => (5, 1),
        B2 => (5, 2),
        B3 => (5, -1),
    };
    let dir = FACE_DIRS[face];
    let axis = if dir.x != 0 {
        0
    } else if dir.y != 0 {
        1
    } else {
        2
    };
    let sign = dir[axis];
    let quarters = if turns == 2 { -2 * sign } else { -sign * turns };
    // The app's grid uses doubled coordinates: a 3x3's outer layer is at ±2.
    CubeMove::new(axis as u8, sign * 2, 1, quarters as i8)
}

/// Solves the current state; returns the moves to apply (empty if already
/// solved), or None if the state can't be read (shouldn't happen with a
/// valid cube).
pub fn solve_scene(table: &DataTable, cubies: &[(IVec3, Quat)]) -> Option<Vec<CubeMove>> {
    let facelets = scene_to_facelets(cubies)?;
    let face_cube = FaceCube::try_from(facelets.as_str()).ok()?;
    let state = CubieCube::try_from(&face_cube).ok()?;
    if state == CubieCube::default() {
        return Some(Vec::new());
    }
    let mut solver = Solver::new(table, 23, None);
    let solution = solver.solve(state)?;
    Some(solution.get_all_moves().into_iter().map(to_cube_move).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::{apply_sim, max_coord, solved_scene, CubeMove};
    use crate::notation::parse_alg;
    use std::sync::OnceLock;

    /// Converts doubled 3x3 coordinates (±2) to solver units (±1).
    fn halved(scene: &[(IVec3, Quat)]) -> Vec<(IVec3, Quat)> {
        scene.iter().map(|&(p, q)| (p / 2, q)).collect()
    }

    fn shared_table() -> &'static DataTable {
        static TABLE: OnceLock<DataTable> = OnceLock::new();
        TABLE.get_or_init(DataTable::default)
    }

    fn solve_and_check(scene: &mut [(IVec3, Quat)]) -> usize {
        let moves = solve_scene(shared_table(), &halved(scene)).expect("solver failed");
        apply_sim(scene, &moves);
        assert!(is_solved(scene, 2), "not solved after applying the solution");
        moves.len()
    }

    #[test]
    fn solved_scene_produces_canonical_facelets() {
        let s = scene_to_facelets(&halved(&solved_scene(3))).unwrap();
        assert_eq!(s, "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB");
        assert!(is_solved(&solved_scene(3), 2));
    }

    #[test]
    fn kewb_moves_match_notation() {
        use KewbMove::*;
        let ours = parse_alg("U R F D L B U2 R' B2", 3).unwrap();
        let theirs: Vec<CubeMove> = [U, R, F, D, L, B, U2, R3, B2].into_iter().map(to_cube_move).collect();
        // Half turns may differ in direction; compare effect instead.
        let mut a = solved_scene(3);
        let mut b = solved_scene(3);
        apply_sim(&mut a, &ours);
        apply_sim(&mut b, &theirs);
        assert_eq!(scene_to_facelets(&halved(&a)), scene_to_facelets(&halved(&b)));
    }

    #[test]
    fn single_move_is_not_solved_and_solver_undoes_it() {
        let mut scene = solved_scene(3);
        apply_sim(&mut scene, &parse_alg("R", 3).unwrap());
        assert!(!is_solved(&scene, 2));
        solve_and_check(&mut scene);
    }

    #[test]
    fn center_twist_is_still_solved() {
        // U4 brings the pieces back but leaves the U center with a 360°
        // orientation; invisible center twists must still count as solved.
        let mut scene = solved_scene(3);
        apply_sim(&mut scene, &parse_alg("U U U U", 3).unwrap());
        assert!(is_solved(&scene, 2));
    }

    #[test]
    fn whole_cube_rotation_keeps_solved() {
        let mut scene = solved_scene(3);
        apply_sim(&mut scene, &parse_alg("x y", 3).unwrap());
        assert!(is_solved(&scene, 2));
        apply_sim(&mut scene, &parse_alg("R", 3).unwrap());
        assert!(!is_solved(&scene, 2));
    }

    #[test]
    fn slice_and_wide_moves_are_handled_via_centers() {
        for alg in ["M", "r U f'", "E S' x", "Rw2 Uw"] {
            let mut scene = solved_scene(3);
            apply_sim(&mut scene, &parse_alg(alg, 3).unwrap());
            assert!(!is_solved(&scene, 2), "{alg}");
            solve_and_check(&mut scene);
        }
    }

    #[test]
    fn nxn_scramble_replay_solves() {
        // The REWIND path on NxN: scramble, replay the inverse → solved.
        for n in [2, 3, 4, 5, 6, 9] {
            let max = max_coord(n);
            for seed in [0xABCDEF_u64, 7, 999] {
                let mut scene = solved_scene(n);
                let scramble = crate::scramble::generate(seed, n);
                apply_sim(&mut scene, &scramble);
                let replay: Vec<CubeMove> = scramble.iter().rev().map(|m| m.inverse()).collect();
                apply_sim(&mut scene, &replay);
                assert!(is_solved(&scene, max), "{n}x{n} seed {seed}");
            }
        }
    }

    #[test]
    fn nxn_solved_detection() {
        for n in [2, 4, 5] {
            let max = max_coord(n);
            let mut scene = solved_scene(n);
            assert!(is_solved(&scene, max));
            let r = CubeMove::new(0, max, 1, -1);
            apply_sim(&mut scene, &[r]);
            assert!(!is_solved(&scene, max), "{n}x{n}: R not detected");
            apply_sim(&mut scene, &[r, r, r]);
            assert!(is_solved(&scene, max), "{n}x{n}: R4 isn't identity");
            if n >= 4 {
                apply_sim(&mut scene, &[CubeMove::new(0, max - 2, 1, -1)]);
                assert!(!is_solved(&scene, max), "{n}x{n}: inner layer not detected");
            }
        }
    }

    #[test]
    fn random_states_roundtrip_through_solver() {
        // Random sequences of ALL move kinds → solver → apply → solved.
        let pool = parse_alg("R R' L L' U U' D D' F F' B B' M M' E E' S S' x y z r u' f2", 3).unwrap();
        let mut rng = crate::scramble::Rng::new(0xC0FFEE);
        for round in 0..6 {
            let mut scene = solved_scene(3);
            let seq: Vec<CubeMove> = (0..30).map(|_| pool[rng.range(pool.len())]).collect();
            apply_sim(&mut scene, &seq);
            let len = solve_and_check(&mut scene);
            assert!(len <= 23, "round {round}: solution too long ({len})");
        }
    }
}
