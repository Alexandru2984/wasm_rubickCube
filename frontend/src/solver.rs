//! Kociemba solver integration (the kewb crate): extracts the cube state
//! from the Bevy scene, translates it into the 54-facelet string, and maps
//! the solution back onto world-axis moves.
//!
//! Everything is expressed in world-frame: the solver's "U" face is always the
//! +Y layer, and color letters are assigned from the *current* centers — so
//! slice moves (M/E/S), which move centers, are handled correctly.

use crate::CubeMove;
use bevy::prelude::*;
use kewb::{CubieCube, DataTable, FaceCube, Move as KewbMove, Solver};
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};

/// World directions of the faces, in kewb letter order: U R F D L B.
const FACE_DIRS: [IVec3; 6] = [
    IVec3::Y,
    IVec3::X,
    IVec3::Z,
    IVec3::NEG_Y,
    IVec3::NEG_X,
    IVec3::NEG_Z,
];
const FACE_CHARS: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];

#[derive(Resource, Default)]
pub struct SolverContext {
    /// Two-phase tables; generated lazily on the first press of SOLVE (~1-2s on
    /// wasm, once per session). Serialized they'd be 6.8MB, so it's not
    /// worth embedding them in the binary.
    pub table: Option<DataTable>,
    /// Set by the SOLVE button; the solve runs on the next frame, so the UI
    /// gets a chance to show the working state before the brief block.
    pub pending: bool,
}

/// Home face index of a sticker (same order as face_color).
fn home_color(axis: usize, sign: i32) -> usize {
    match (axis, sign > 0) {
        (1, true) => 0,  // +Y white
        (1, false) => 1, // -Y yellow
        (0, true) => 2,  // +X red
        (0, false) => 3, // -X orange
        (2, true) => 4,  // +Z blue
        _ => 5,          // -Z green
    }
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
            stickers.insert((pos, world_dir), home_color(axis, sign));
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
            let color = home_color(axis, sign);
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
/// A clockwise turn of the face with direction d = -sign(d)·90° around axis |d|.
fn to_cube_move(m: KewbMove) -> CubeMove {
    use KewbMove::*;
    let (face, quarters) = match m {
        U => (0, 1), U2 => (0, 2), U3 => (0, 3),
        R => (1, 1), R2 => (1, 2), R3 => (1, 3),
        F => (2, 1), F2 => (2, 2), F3 => (2, 3),
        D => (3, 1), D2 => (3, 2), D3 => (3, 3),
        L => (4, 1), L2 => (4, 2), L3 => (4, 3),
        B => (5, 1), B2 => (5, 2), B3 => (5, 3),
    };
    let dir = FACE_DIRS[face];
    let axis_idx = if dir.x != 0 { 0 } else if dir.y != 0 { 1 } else { 2 };
    let sign = dir[axis_idx] as f32;
    let angle = match quarters {
        1 => -sign * FRAC_PI_2,
        2 => -sign * PI,
        _ => sign * FRAC_PI_2,
    };
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    CubeMove {
        rotation_axis: axes[axis_idx],
        layer_axis: axis_idx as u8,
        // The app's grid uses doubled coordinates: the outer layer of
        // a 3x3 cube sits at ±2.
        layer_value: dir[axis_idx] * 2,
        angle,
    }
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
    use crate::{max_coord, rotate_grid_pos};
    use std::sync::OnceLock;

    /// Solved scene for an n x n cube, in doubled coordinates (as in the app):
    /// only the surface cubies, identity orientation.
    fn solved_scene(n: i32) -> Vec<(IVec3, Quat)> {
        let max = max_coord(n);
        let mut v = Vec::new();
        for x in (-max..=max).step_by(2) {
            for y in (-max..=max).step_by(2) {
                for z in (-max..=max).step_by(2) {
                    if x.abs() != max && y.abs() != max && z.abs() != max {
                        continue;
                    }
                    v.push((IVec3::new(x, y, z), Quat::IDENTITY));
                }
            }
        }
        v
    }

    /// Applies a move to the simulated scene, the same way process_rotation's
    /// finalize does: permutes positions and composes orientations.
    fn apply_move_sim(cubies: &mut [(IVec3, Quat)], mv: &CubeMove) {
        let q = Quat::from_axis_angle(mv.rotation_axis, mv.angle);
        for (pos, rot) in cubies.iter_mut() {
            let layer_val = match mv.layer_axis {
                0 => pos.x,
                1 => pos.y,
                _ => pos.z,
            };
            if !mv.affects(layer_val) {
                continue;
            }
            let (nx, ny, nz) = rotate_grid_pos(pos.x, pos.y, pos.z, mv.rotation_axis, mv.angle);
            *pos = IVec3::new(nx, ny, nz);
            *rot = (q * *rot).normalize();
        }
    }

    /// Converts doubled 3x3 coordinates (±2) to solver units (±1).
    fn halved(scene: &[(IVec3, Quat)]) -> Vec<(IVec3, Quat)> {
        scene.iter().map(|&(p, q)| (p / 2, q)).collect()
    }

    fn shared_table() -> &'static DataTable {
        static TABLE: OnceLock<DataTable> = OnceLock::new();
        TABLE.get_or_init(DataTable::default)
    }

    #[test]
    fn solved_scene_produces_canonical_facelets() {
        let s = scene_to_facelets(&halved(&solved_scene(3))).unwrap();
        assert_eq!(s, "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB");
        assert!(is_solved(&solved_scene(3), 2));
    }

    #[test]
    fn single_move_is_not_solved_and_solver_undoes_it() {
        let mut scene = solved_scene(3);
        apply_move_sim(&mut scene, &CubeMove::r(2));
        assert!(!is_solved(&scene, 2));

        let moves = solve_scene(shared_table(), &halved(&scene)).unwrap();
        for mv in &moves {
            apply_move_sim(&mut scene, mv);
        }
        assert!(is_solved(&scene, 2));
    }

    #[test]
    fn center_twist_is_still_solved() {
        // U U U U brings the pieces back, but the U center keeps a -360°
        // orientation (equivalent to identity); moves that only twist centers must
        // still count as "solved".
        let mut scene = solved_scene(3);
        for _ in 0..4 {
            apply_move_sim(&mut scene, &CubeMove::u(2));
        }
        assert!(is_solved(&scene, 2));
    }

    #[test]
    fn whole_cube_rotation_keeps_solved() {
        let mut scene = solved_scene(3);
        apply_move_sim(&mut scene, &CubeMove::x());
        apply_move_sim(&mut scene, &CubeMove::y());
        assert!(is_solved(&scene, 2));
        // ...and after a whole-cube rotation, a face move still breaks it.
        apply_move_sim(&mut scene, &CubeMove::r(2));
        assert!(!is_solved(&scene, 2));
    }

    #[test]
    fn slice_moves_are_handled_via_centers() {
        // M moves centers: the state stays solvable and detectable.
        let mut scene = solved_scene(3);
        apply_move_sim(&mut scene, &CubeMove::m());
        assert!(!is_solved(&scene, 2));
        let moves = solve_scene(shared_table(), &halved(&scene)).unwrap();
        for mv in &moves {
            apply_move_sim(&mut scene, mv);
        }
        assert!(is_solved(&scene, 2));
    }

    #[test]
    fn nxn_scramble_replay_solves() {
        // Reproduces the SOLVE/REWIND button on NxN: scramble → replay the inverse in
        // reverse order → must end up solved.
        for n in [2, 3, 4, 5, 6] {
            let max = max_coord(n);
            for seed in [0xABCDEF_u64, 7, 999] {
                let mut scene = solved_scene(n);
                let scramble = crate::generate_scramble(seed, crate::scramble_len(n), n);
                for mv in &scramble {
                    apply_move_sim(&mut scene, mv);
                }
                let replay: Vec<CubeMove> = scramble.iter().rev().map(|m| m.inverse()).collect();
                for mv in &replay {
                    apply_move_sim(&mut scene, mv);
                }
                assert!(
                    is_solved(&scene, max),
                    "{n}x{n} seed {seed}: not solved after replaying the inverse"
                );
            }
        }
    }

    #[test]
    fn nxn_solved_detection() {
        for n in [2, 4, 5] {
            let max = max_coord(n);
            let mut scene = solved_scene(n);
            assert!(is_solved(&scene, max), "{n}x{n} solved cube not detected");

            apply_move_sim(&mut scene, &CubeMove::r(max));
            assert!(!is_solved(&scene, max), "{n}x{n}: R not detected as unsolved");
            for _ in 0..3 {
                apply_move_sim(&mut scene, &CubeMove::r(max));
            }
            assert!(is_solved(&scene, max), "{n}x{n}: R four times isn't identity");

            // Inner layer (exists from 4x4 upward).
            if n >= 4 {
                let inner = CubeMove { layer_value: max - 2, ..CubeMove::r(max) };
                apply_move_sim(&mut scene, &inner);
                assert!(!is_solved(&scene, max), "{n}x{n}: inner layer not detected");
            }
        }
    }

    #[test]
    fn random_states_roundtrip_through_solver() {
        // Key test: random sequences from ALL our moves
        // (faces, slices, primes, whole cube) → solver → apply → solved.
        let all_moves = [
            CubeMove::r(2), CubeMove::ri(2), CubeMove::l(2), CubeMove::li(2),
            CubeMove::u(2), CubeMove::ui(2), CubeMove::d(2), CubeMove::di(2),
            CubeMove::f(2), CubeMove::fi(2), CubeMove::b(2), CubeMove::bi(2),
            CubeMove::m(), CubeMove::mi(), CubeMove::e(), CubeMove::ei(),
            CubeMove::s(), CubeMove::si(),
            CubeMove::x(), CubeMove::y(), CubeMove::z(),
        ];
        let mut seed = 0xC0FFEE_u64;
        for round in 0..5 {
            let mut scene = solved_scene(3);
            for _ in 0..30 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let mv = all_moves[(seed as usize) % all_moves.len()];
                apply_move_sim(&mut scene, &mv);
            }
            let moves = solve_scene(shared_table(), &halved(&scene))
                .unwrap_or_else(|| panic!("solver failed at round {round}"));
            assert!(moves.len() <= 23, "solution too long: {}", moves.len());
            for mv in &moves {
                apply_move_sim(&mut scene, mv);
            }
            assert!(is_solved(&scene, 2), "not solved at round {round}");
        }
    }
}
