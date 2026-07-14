//! Trainer de algoritmi OLL/PLL (3x3).
//!
//! Design "setup prin invers": cubul se aseaza in starea `alg⁻¹(rezolvat)`,
//! deci executarea algoritmului il rezolva garantat — corectitudinea drill-ului
//! nu depinde de etichete. Testele verifica structural fiecare algoritm:
//! pastreaza F2L (primele doua straturi + centrele) si apartine categoriei.

use crate::{CubeMove, LAYER_ALL};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Pll,
    Oll,
}

pub struct Alg {
    pub name: &'static str,
    pub category: Category,
    pub moves: &'static str,
}

#[derive(Resource, Default)]
pub struct Trainer {
    pub open: bool,
    pub random_auf: bool,
    /// Cazul curent (index in ALGS) — afisat in HUD si fereastra.
    pub current: Option<usize>,
    /// Caz ales, de aplicat cand scena e asezata (si pe 3x3).
    pub pending: Option<usize>,
}

/// Parseaza notatia standard: fete (R U F...), felii (M E S), rotatii (x y z),
/// wide moves (r u f... = fata + felia adiacenta), sufixe ' si 2.
pub fn parse_alg(s: &str) -> Option<Vec<CubeMove>> {
    let max = 2; // 3x3 in coordonate dublate

    let face = |axis: Vec3, layer_axis: u8, layer: i32, cw_sign: f32| CubeMove {
        rotation_axis: axis,
        layer_axis,
        layer_value: layer,
        angle: cw_sign * FRAC_PI_2,
    };

    let mut out = Vec::new();
    for token in s.replace(['(', ')'], " ").split_whitespace() {
        let mut chars = token.chars();
        let base = chars.next()?;
        let rest: String = chars.collect();
        let (double, prime) = match rest.as_str() {
            "" => (false, false),
            "'" => (false, true),
            "2" | "2'" => (true, false),
            _ => return None,
        };

        // (mutare de baza, felia atasata pentru wide)
        let (main, wide): (CubeMove, Option<CubeMove>) = match base {
            'R' => (face(Vec3::X, 0, max, -1.0), None),
            'L' => (face(Vec3::X, 0, -max, 1.0), None),
            'U' => (face(Vec3::Y, 1, max, -1.0), None),
            'D' => (face(Vec3::Y, 1, -max, 1.0), None),
            'F' => (face(Vec3::Z, 2, max, -1.0), None),
            'B' => (face(Vec3::Z, 2, -max, 1.0), None),
            'M' => (face(Vec3::X, 0, 0, 1.0), None),
            'E' => (face(Vec3::Y, 1, 0, 1.0), None),
            'S' => (face(Vec3::Z, 2, 0, -1.0), None),
            'x' => (face(Vec3::X, 0, LAYER_ALL, -1.0), None),
            'y' => (face(Vec3::Y, 1, LAYER_ALL, -1.0), None),
            'z' => (face(Vec3::Z, 2, LAYER_ALL, -1.0), None),
            // wide: fata + felia din mijloc care se misca odata cu ea
            'r' => (face(Vec3::X, 0, max, -1.0), Some(face(Vec3::X, 0, 0, -1.0))),
            'l' => (face(Vec3::X, 0, -max, 1.0), Some(face(Vec3::X, 0, 0, 1.0))),
            'u' => (face(Vec3::Y, 1, max, -1.0), Some(face(Vec3::Y, 1, 0, -1.0))),
            'd' => (face(Vec3::Y, 1, -max, 1.0), Some(face(Vec3::Y, 1, 0, 1.0))),
            'f' => (face(Vec3::Z, 2, max, -1.0), Some(face(Vec3::Z, 2, 0, -1.0))),
            'b' => (face(Vec3::Z, 2, -max, 1.0), Some(face(Vec3::Z, 2, 0, 1.0))),
            _ => return None,
        };

        for mv in std::iter::once(main).chain(wide) {
            let mut mv = mv;
            if prime {
                mv.angle = -mv.angle;
            }
            if double {
                mv.angle *= 2.0;
                // ±180° sunt echivalente; pastram unghiul in [-PI, PI]
                if mv.angle > PI + 1e-3 || mv.angle < -PI - 1e-3 {
                    return None;
                }
            }
            out.push(mv);
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// Inversul unui algoritm: mutarile inversate, in ordine inversa.
pub fn inverse_alg(moves: &[CubeMove]) -> Vec<CubeMove> {
    moves.iter().rev().map(|m| m.inverse()).collect()
}

#[rustfmt::skip]
pub const ALGS: &[Alg] = &[
    // ── PLL (21) ──────────────────────────────────────────────────────────
    Alg { name: "Aa", category: Category::Pll, moves: "x R' U R' D2 R U' R' D2 R2 x'" },
    Alg { name: "Ab", category: Category::Pll, moves: "x R2 D2 R U R' D2 R U' R x'" },
    Alg { name: "E",  category: Category::Pll, moves: "x' R U' R' D R U R' D' R U R' D R U' R' D' x" },
    Alg { name: "F",  category: Category::Pll, moves: "R' U' F' R U R' U' R' F R2 U' R' U' R U R' U R" },
    Alg { name: "Ga", category: Category::Pll, moves: "R2 U R' U R' U' R U' R2 U' D R' U R D'" },
    Alg { name: "Gb", category: Category::Pll, moves: "R' U' R U D' R2 U R' U R U' R U' R2 D" },
    Alg { name: "Gc", category: Category::Pll, moves: "R2 U' R U' R U R' U R2 U D' R U' R' D" },
    Alg { name: "Gd", category: Category::Pll, moves: "R U R' U' D R2 U' R U' R' U R' U R2 D'" },
    Alg { name: "H",  category: Category::Pll, moves: "M2 U M2 U2 M2 U M2" },
    Alg { name: "Ja", category: Category::Pll, moves: "R' U L' U2 R U' R' U2 R L" },
    Alg { name: "Jb", category: Category::Pll, moves: "R U R' F' R U R' U' R' F R2 U' R'" },
    Alg { name: "Na", category: Category::Pll, moves: "R U R' U R U R' F' R U R' U' R' F R2 U' R' U2 R U' R'" },
    Alg { name: "Nb", category: Category::Pll, moves: "R' U R U' R' F' U' F R U R' F R' F' R U' R" },
    Alg { name: "Ra", category: Category::Pll, moves: "R U' R' U' R U R D R' U' R D' R' U2 R'" },
    Alg { name: "Rb", category: Category::Pll, moves: "R2 F R U R U' R' F' R U2 R' U2 R" },
    Alg { name: "T",  category: Category::Pll, moves: "R U R' U' R' F R2 U' R' U' R U R' F'" },
    Alg { name: "Ua", category: Category::Pll, moves: "R U' R U R U R U' R' U' R2" },
    Alg { name: "Ub", category: Category::Pll, moves: "R2 U R U R' U' R' U' R' U R'" },
    Alg { name: "V",  category: Category::Pll, moves: "R' U R' U' y R' F' R2 U' R' U R' F R F" },
    Alg { name: "Y",  category: Category::Pll, moves: "F R U' R' U' R U R' F' R U R' U' R' F R F'" },
    Alg { name: "Z",  category: Category::Pll, moves: "M' U M2 U M2 U M' U2 M2" },

    // ── OLL (57) ──────────────────────────────────────────────────────────
    Alg { name: "OLL 1",  category: Category::Oll, moves: "R U2 R2 F R F' U2 R' F R F'" },
    Alg { name: "OLL 2",  category: Category::Oll, moves: "F R U R' U' F' f R U R' U' f'" },
    Alg { name: "OLL 3",  category: Category::Oll, moves: "f R U R' U' f' U' F R U R' U' F'" },
    Alg { name: "OLL 4",  category: Category::Oll, moves: "f R U R' U' f' U F R U R' U' F'" },
    Alg { name: "OLL 5",  category: Category::Oll, moves: "r' U2 R U R' U r" },
    Alg { name: "OLL 6",  category: Category::Oll, moves: "r U2 R' U' R U' r'" },
    Alg { name: "OLL 7",  category: Category::Oll, moves: "r U R' U R U2 r'" },
    Alg { name: "OLL 8",  category: Category::Oll, moves: "r' U' R U' R' U2 r" },
    Alg { name: "OLL 9",  category: Category::Oll, moves: "R U R' U' R' F R2 U R' U' F'" },
    Alg { name: "OLL 10", category: Category::Oll, moves: "R U R' U R' F R F' R U2 R'" },
    Alg { name: "OLL 11", category: Category::Oll, moves: "r U R' U R' F R F' R U2 r'" },
    Alg { name: "OLL 12", category: Category::Oll, moves: "F R U R' U' F' U F R U R' U' F'" },
    Alg { name: "OLL 13", category: Category::Oll, moves: "F U R U' R2 F' R U R U' R'" },
    Alg { name: "OLL 14", category: Category::Oll, moves: "R' F R U R' F' R F U' F'" },
    Alg { name: "OLL 15", category: Category::Oll, moves: "r' U' r R' U' R U r' U r" },
    Alg { name: "OLL 16", category: Category::Oll, moves: "r U r' R U R' U' r U' r'" },
    Alg { name: "OLL 17", category: Category::Oll, moves: "R U R' U R' F R F' U2 R' F R F'" },
    Alg { name: "OLL 18", category: Category::Oll, moves: "r U R' U R U2 r' r' U' R U' R' U2 r" },
    Alg { name: "OLL 19", category: Category::Oll, moves: "M U R U R' U' M' R' F R F'" },
    Alg { name: "OLL 20", category: Category::Oll, moves: "M U R U R' U' M2 U R U' r'" },
    Alg { name: "OLL 21", category: Category::Oll, moves: "R U2 R' U' R U R' U' R U' R'" },
    Alg { name: "OLL 22", category: Category::Oll, moves: "R U2 R2 U' R2 U' R2 U2 R" },
    Alg { name: "OLL 23", category: Category::Oll, moves: "R2 D R' U2 R D' R' U2 R'" },
    Alg { name: "OLL 24", category: Category::Oll, moves: "r U R' U' r' F R F'" },
    Alg { name: "OLL 25", category: Category::Oll, moves: "F' r U R' U' r' F R" },
    Alg { name: "OLL 26", category: Category::Oll, moves: "R U2 R' U' R U' R'" },
    Alg { name: "OLL 27", category: Category::Oll, moves: "R U R' U R U2 R'" },
    Alg { name: "OLL 28", category: Category::Oll, moves: "r U R' U' r' R U R U' R'" },
    Alg { name: "OLL 29", category: Category::Oll, moves: "R U R' U' R U' R' F' U' F R U R'" },
    Alg { name: "OLL 30", category: Category::Oll, moves: "F U R U2 R' U' R U2 R' U' F'" },
    Alg { name: "OLL 31", category: Category::Oll, moves: "R' U' F U R U' R' F' R" },
    Alg { name: "OLL 32", category: Category::Oll, moves: "L U F' U' L' U L F L'" },
    Alg { name: "OLL 33", category: Category::Oll, moves: "R U R' U' R' F R F'" },
    Alg { name: "OLL 34", category: Category::Oll, moves: "R U R2 U' R' F R U R U' F'" },
    Alg { name: "OLL 35", category: Category::Oll, moves: "R U2 R2 F R F' R U2 R'" },
    Alg { name: "OLL 36", category: Category::Oll, moves: "L' U' L U' L' U L U L F' L' F" },
    Alg { name: "OLL 37", category: Category::Oll, moves: "F R' F' R U R U' R'" },
    Alg { name: "OLL 38", category: Category::Oll, moves: "R U R' U R U' R' U' R' F R F'" },
    Alg { name: "OLL 39", category: Category::Oll, moves: "L F' L' U' L U F U' L'" },
    Alg { name: "OLL 40", category: Category::Oll, moves: "R' F R U R' U' F' U R" },
    Alg { name: "OLL 41", category: Category::Oll, moves: "R U R' U R U2 R' F R U R' U' F'" },
    Alg { name: "OLL 42", category: Category::Oll, moves: "R' U' R U' R' U2 R F R U R' U' F'" },
    Alg { name: "OLL 43", category: Category::Oll, moves: "F' U' L' U L F" },
    Alg { name: "OLL 44", category: Category::Oll, moves: "f R U R' U' f'" },
    Alg { name: "OLL 45", category: Category::Oll, moves: "F R U R' U' F'" },
    Alg { name: "OLL 46", category: Category::Oll, moves: "R' U' R' F R F' U R" },
    Alg { name: "OLL 47", category: Category::Oll, moves: "F' L' U' L U L' U' L U F" },
    Alg { name: "OLL 48", category: Category::Oll, moves: "F R U R' U' R U R' U' F'" },
    Alg { name: "OLL 49", category: Category::Oll, moves: "r U' r2 U r2 U r2 U' r" },
    Alg { name: "OLL 50", category: Category::Oll, moves: "r' U r2 U' r2 U' r2 U r'" },
    Alg { name: "OLL 51", category: Category::Oll, moves: "f R U R' U' R U R' U' f'" },
    Alg { name: "OLL 52", category: Category::Oll, moves: "R U R' U R U' B U' B' R'" },
    Alg { name: "OLL 53", category: Category::Oll, moves: "r' U' R U' R' U R U' R' U2 r" },
    Alg { name: "OLL 54", category: Category::Oll, moves: "r U R' U R U' R' U R U2 r'" },
    Alg { name: "OLL 55", category: Category::Oll, moves: "R' F R U R U' R2 F' R2 U' R' U R U R'" },
    Alg { name: "OLL 56", category: Category::Oll, moves: "r U r' U R U' R' U R U' R' r U' r'" },
    Alg { name: "OLL 57", category: Category::Oll, moves: "R U R' U' M' U R U' r'" },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::scene_to_facelets;
    use crate::{max_coord, rotate_grid_pos};

    const SOLVED: &str = "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB";

    fn solved_scene() -> Vec<(IVec3, Quat)> {
        let max = max_coord(3);
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

    fn apply(scene: &mut [(IVec3, Quat)], moves: &[CubeMove]) {
        for mv in moves {
            let q = Quat::from_axis_angle(mv.rotation_axis, mv.angle);
            for (pos, rot) in scene.iter_mut() {
                let layer_val = match mv.layer_axis { 0 => pos.x, 1 => pos.y, _ => pos.z };
                if !mv.affects(layer_val) { continue; }
                let (nx, ny, nz) = rotate_grid_pos(pos.x, pos.y, pos.z, mv.rotation_axis, mv.angle);
                *pos = IVec3::new(nx, ny, nz);
                *rot = (q * *rot).normalize();
            }
        }
    }

    fn facelets(scene: &[(IVec3, Quat)]) -> String {
        let halved: Vec<_> = scene.iter().map(|&(p, q)| (p / 2, q)).collect();
        scene_to_facelets(&halved).expect("facelets")
    }

    /// Indecsii care nu apartin ultimului strat (F2L + centrele laterale + D).
    fn f2l_indices() -> Vec<usize> {
        let last_layer: Vec<usize> = (0..9)
            .chain(9..12)   // randul de sus al fetei R
            .chain(18..21)  // F
            .chain(36..39)  // L
            .chain(45..48)  // B
            .collect();
        (0..54).filter(|i| !last_layer.contains(i)).collect()
    }

    #[test]
    fn all_algs_parse() {
        for alg in ALGS {
            let moves = parse_alg(alg.moves)
                .unwrap_or_else(|| panic!("algoritmul {} nu se parseaza", alg.name));
            assert!(!moves.is_empty());
        }
    }

    #[test]
    fn alg_then_inverse_is_identity() {
        for alg in ALGS {
            let moves = parse_alg(alg.moves).unwrap();
            let mut scene = solved_scene();
            apply(&mut scene, &moves);
            apply(&mut scene, &inverse_alg(&moves));
            assert_eq!(facelets(&scene), SOLVED, "{}: alg + invers ≠ identitate", alg.name);
        }
    }

    /// Orice algoritm de last layer trebuie sa lase F2L neatins.
    #[test]
    fn all_algs_preserve_f2l() {
        let solved: Vec<char> = SOLVED.chars().collect();
        for alg in ALGS {
            let moves = parse_alg(alg.moves).unwrap();
            let mut scene = solved_scene();
            apply(&mut scene, &moves);
            let after: Vec<char> = facelets(&scene).chars().collect();
            for &i in &f2l_indices() {
                assert_eq!(
                    after[i], solved[i],
                    "{}: strica F2L la facelet {i}", alg.name
                );
            }
        }
    }

    /// PLL: aplicat pe rezolvat, lasa fata de sus uniforma (doar permuta) si
    /// nu e identitate. OLL: inversul (starea de caz) are fata de sus
    /// ne-uniforma (e un caz de orientare).
    #[test]
    fn algs_match_their_category() {
        for alg in ALGS {
            let moves = parse_alg(alg.moves).unwrap();
            match alg.category {
                Category::Pll => {
                    let mut scene = solved_scene();
                    apply(&mut scene, &moves);
                    let f = facelets(&scene);
                    let top: Vec<char> = f.chars().take(9).collect();
                    assert!(
                        top.iter().all(|&c| c == top[0]),
                        "{}: PLL cu fata de sus ne-uniforma", alg.name
                    );
                    assert_ne!(f, SOLVED, "{}: PLL care nu face nimic", alg.name);
                }
                Category::Oll => {
                    let mut scene = solved_scene();
                    apply(&mut scene, &inverse_alg(&moves));
                    let f = facelets(&scene);
                    let top: Vec<char> = f.chars().take(9).collect();
                    assert!(
                        !top.iter().all(|&c| c == top[0]),
                        "{}: cazul OLL are deja orientarea rezolvata", alg.name
                    );
                }
            }
        }
    }
}
