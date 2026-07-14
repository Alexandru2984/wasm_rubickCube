//! Integrarea solver-ului Kociemba (crate-ul kewb): extrage starea cubului
//! din scena Bevy, o traduce in sirul de 54 de facelets, si mapeaza solutia
//! inapoi pe mutari pe axele world.
//!
//! Totul e exprimat in world-frame: fata "U" a solver-ului e mereu stratul
//! +Y, iar literele culorilor se aloca dupa centrele *curente* — asa ca
//! mutarile de felii (M/E/S), care muta centrele, sunt tratate corect.

use crate::CubeMove;
use bevy::prelude::*;
use kewb::{CubieCube, DataTable, FaceCube, Move as KewbMove, Solver};
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};

/// Directiile world ale fetelor, in ordinea literelor kewb: U R F D L B.
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
    /// Tabelele two-phase; generate lazy la prima apasare pe SOLVE (~1-2s pe
    /// wasm, o singura data pe sesiune). Serializate ar avea 6.8MB, deci nu
    /// merita inglobate in binar.
    pub table: Option<DataTable>,
    /// Setat de butonul SOLVE; rezolvarea ruleaza in frame-ul urmator, ca UI-ul
    /// sa apuce sa afiseze starea de lucru inainte de blocarea scurta.
    pub pending: bool,
}

/// Indexul fetei de origine a unui sticker (aceeasi ordine ca face_color).
fn home_color(axis: usize, sign: i32) -> usize {
    match (axis, sign > 0) {
        (1, true) => 0,  // +Y alb
        (1, false) => 1, // -Y galben
        (0, true) => 2,  // +X rosu
        (0, false) => 3, // -X portocaliu
        (2, true) => 4,  // +Z albastru
        _ => 5,          // -Z verde
    }
}

/// Pozitia in grila a facelet-ului (face, row, col) in conventia Kociemba,
/// verificata contra tabelelor CORNER_FACELET/EDGE_FACELET din kewb.
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

/// Construieste sirul de 54 de facelets din starea scenei: pentru fiecare
/// cubie, pozitia de origine rezulta din orientare (home = q⁻¹·pos), iar
/// fiecare sticker de origine e proiectat pe directia lui world curenta.
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

    // Literele se aloca dupa culorile centrelor curente.
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

/// Cub rezolvat = fiecare fata uniforma. Spre deosebire de compararea
/// orientarilor, definitia asta ignora rasucirea (invizibila) a centrelor.
pub fn is_solved(cubies: &[(IVec3, Quat)]) -> bool {
    scene_to_facelets(cubies).is_some_and(|s| {
        s.as_bytes().chunks(9).all(|face| face.iter().all(|&b| b == face[0]))
    })
}

/// Traduce o mutare kewb (litera de fata) intr-o mutare pe axe world.
/// O tura in sens orar a fetei cu directia d = -sign(d)·90° in jurul axei |d|.
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
        layer_value: dir[axis_idx],
        angle,
    }
}

/// Rezolva starea curenta; intoarce mutarile de aplicat (goale daca e deja
/// rezolvat) sau None daca starea nu poate fi citita (nu ar trebui sa se
/// intample cu un cub valid).
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
    use crate::rotate_grid_pos;
    use std::sync::OnceLock;

    /// Scena rezolvata: 26 de cubie-uri cu orientare identitate.
    fn solved_scene() -> Vec<(IVec3, Quat)> {
        let mut v = Vec::new();
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if x == 0 && y == 0 && z == 0 {
                        continue;
                    }
                    v.push((IVec3::new(x, y, z), Quat::IDENTITY));
                }
            }
        }
        v
    }

    /// Aplica o mutare pe scena-simulare, la fel ca finalize-ul din
    /// process_rotation: permuta pozitiile si compune orientarile.
    fn apply_move_sim(cubies: &mut [(IVec3, Quat)], mv: &CubeMove) {
        let q = Quat::from_axis_angle(mv.rotation_axis, mv.angle);
        for (pos, rot) in cubies.iter_mut() {
            let layer_val = match mv.layer_axis {
                0 => pos.x,
                1 => pos.y,
                _ => pos.z,
            };
            if layer_val != mv.layer_value {
                continue;
            }
            let (nx, ny, nz) = rotate_grid_pos(pos.x, pos.y, pos.z, mv.rotation_axis, mv.angle);
            *pos = IVec3::new(nx, ny, nz);
            *rot = (q * *rot).normalize();
        }
    }

    fn shared_table() -> &'static DataTable {
        static TABLE: OnceLock<DataTable> = OnceLock::new();
        TABLE.get_or_init(DataTable::default)
    }

    #[test]
    fn solved_scene_produces_canonical_facelets() {
        let s = scene_to_facelets(&solved_scene()).unwrap();
        assert_eq!(s, "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB");
        assert!(is_solved(&solved_scene()));
    }

    #[test]
    fn single_move_is_not_solved_and_solver_undoes_it() {
        let mut scene = solved_scene();
        apply_move_sim(&mut scene, &CubeMove::r());
        assert!(!is_solved(&scene));

        let moves = solve_scene(shared_table(), &scene).unwrap();
        for mv in &moves {
            apply_move_sim(&mut scene, mv);
        }
        assert!(is_solved(&scene));
    }

    #[test]
    fn center_twist_is_still_solved() {
        // U U U U readuce piesele, dar centrul U ramane cu orientare -360°
        // (echivalenta); si mutari care rasucesc doar centre trebuie sa
        // ramana "rezolvat".
        let mut scene = solved_scene();
        for _ in 0..4 {
            apply_move_sim(&mut scene, &CubeMove::u());
        }
        assert!(is_solved(&scene));
    }

    #[test]
    fn slice_moves_are_handled_via_centers() {
        // M muta centrele: starea ramane rezolvabila si detectabila.
        let mut scene = solved_scene();
        apply_move_sim(&mut scene, &CubeMove::m());
        assert!(!is_solved(&scene));
        let moves = solve_scene(shared_table(), &scene).unwrap();
        for mv in &moves {
            apply_move_sim(&mut scene, mv);
        }
        assert!(is_solved(&scene));
    }

    #[test]
    fn random_states_roundtrip_through_solver() {
        // Testul-cheie: secvente aleatoare din TOATE mutarile noastre
        // (fete, felii, prime) → solver → aplicare → rezolvat.
        let all_moves = [
            CubeMove::r(), CubeMove::ri(), CubeMove::l(), CubeMove::li(),
            CubeMove::u(), CubeMove::ui(), CubeMove::d(), CubeMove::di(),
            CubeMove::f(), CubeMove::fi(), CubeMove::b(), CubeMove::bi(),
            CubeMove::m(), CubeMove::mi(), CubeMove::e(), CubeMove::ei(),
            CubeMove::s(), CubeMove::si(),
        ];
        let mut seed = 0xC0FFEE_u64;
        for round in 0..5 {
            let mut scene = solved_scene();
            for _ in 0..30 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let mv = all_moves[(seed as usize) % all_moves.len()];
                apply_move_sim(&mut scene, &mv);
            }
            let moves = solve_scene(shared_table(), &scene)
                .unwrap_or_else(|| panic!("solver failed at round {round}"));
            assert!(moves.len() <= 23, "solution too long: {}", moves.len());
            for mv in &moves {
                apply_move_sim(&mut scene, mv);
            }
            assert!(is_solved(&scene), "not solved at round {round}");
        }
    }
}
