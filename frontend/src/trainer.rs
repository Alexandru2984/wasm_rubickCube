//! Algorithm trainer (3x3): OLL / PLL drills plus a few classic patterns.
//!
//! "Setup by inverse" design: for a drill the cube is placed into the state
//! `alg⁻¹(solved)`, so executing the algorithm is guaranteed to solve it —
//! correctness doesn't depend on labels. Patterns are the opposite: the
//! algorithm itself is applied to a solved cube. Tests verify every entry
//! structurally (F2L preserved, category matches, pattern looks right).

use crate::cube::CubeMove;
use crate::notation::parse_alg;
use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Pll,
    Oll,
    Pattern,
}

impl Category {
    pub fn code(self) -> &'static str {
        match self {
            Category::Pll => "pll",
            Category::Oll => "oll",
            Category::Pattern => "pattern",
        }
    }
}

pub struct Alg {
    pub name: &'static str,
    pub category: Category,
    pub moves: &'static str,
}

#[derive(Resource, Default)]
pub struct Trainer {
    /// Current case (index into ALGS).
    pub current: Option<usize>,
}

pub fn alg_moves(i: usize) -> Vec<CubeMove> {
    parse_alg(ALGS[i].moves, 3).expect("valid algorithm (checked in tests)")
}

/// Inverse of an algorithm: the moves inverted, in reverse order.
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

    // ── Patterns (applied from solved) ────────────────────────────────────
    Alg { name: "Checkerboard", category: Category::Pattern, moves: "M2 E2 S2" },
    Alg { name: "Dots",         category: Category::Pattern, moves: "U D' R L' F B' U D'" },
    Alg { name: "Superflip",    category: Category::Pattern, moves: "U R2 F B R B2 R U2 L B2 R U' D' R2 F R' L B2 U2 F2" },
    Alg { name: "Cube in cube", category: Category::Pattern, moves: "F L F U' R U F2 L2 U' L' B D' B' L2 U" },
    Alg { name: "Stripes",      category: Category::Pattern, moves: "R2 L2 U2 D2" },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::{apply_sim, solved_scene};
    use crate::solver::scene_to_facelets;

    const SOLVED: &str = "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB";

    fn facelets_after(moves: &[CubeMove]) -> String {
        let mut scene = solved_scene(3);
        apply_sim(&mut scene, moves);
        let halved: Vec<_> = scene.iter().map(|&(p, q)| (p / 2, q)).collect();
        scene_to_facelets(&halved).expect("facelets")
    }

    /// Indices that don't belong to the last layer (F2L + side centers + D).
    fn f2l_indices() -> Vec<usize> {
        let last_layer: Vec<usize> = (0..9).chain(9..12).chain(18..21).chain(36..39).chain(45..48).collect();
        (0..54).filter(|i| !last_layer.contains(i)).collect()
    }

    #[test]
    fn all_algs_parse() {
        for (i, alg) in ALGS.iter().enumerate() {
            assert!(!alg_moves(i).is_empty(), "{} is empty", alg.name);
        }
    }

    #[test]
    fn alg_then_inverse_is_identity() {
        for (i, alg) in ALGS.iter().enumerate() {
            let m = alg_moves(i);
            let both: Vec<_> = m.iter().copied().chain(inverse_alg(&m)).collect();
            assert_eq!(facelets_after(&both), SOLVED, "{}", alg.name);
        }
    }

    #[test]
    fn last_layer_algs_preserve_f2l() {
        let solved: Vec<char> = SOLVED.chars().collect();
        for (i, alg) in ALGS.iter().enumerate() {
            if alg.category == Category::Pattern {
                continue;
            }
            let after: Vec<char> = facelets_after(&alg_moves(i)).chars().collect();
            for &k in &f2l_indices() {
                assert_eq!(after[k], solved[k], "{}: breaks F2L at facelet {k}", alg.name);
            }
        }
    }

    /// PLL: leaves the top face uniform and isn't the identity. OLL: the case
    /// (inverse applied to solved) has a non-uniform top face.
    #[test]
    fn algs_match_their_category() {
        for (i, alg) in ALGS.iter().enumerate() {
            let m = alg_moves(i);
            match alg.category {
                Category::Pll => {
                    let f = facelets_after(&m);
                    let top: Vec<char> = f.chars().take(9).collect();
                    assert!(top.iter().all(|&c| c == top[0]), "{}: PLL with a non-uniform top", alg.name);
                    assert_ne!(f, SOLVED, "{}: PLL that does nothing", alg.name);
                }
                Category::Oll => {
                    let f = facelets_after(&inverse_alg(&m));
                    let top: Vec<char> = f.chars().take(9).collect();
                    assert!(!top.iter().all(|&c| c == top[0]), "{}: OLL already oriented", alg.name);
                }
                Category::Pattern => assert_ne!(facelets_after(&m), SOLVED, "{}", alg.name),
            }
        }
    }

    fn pattern(name: &str) -> Vec<char> {
        let i = ALGS.iter().position(|a| a.name == name).unwrap();
        facelets_after(&alg_moves(i)).chars().collect()
    }

    #[test]
    fn patterns_look_right() {
        // Checkerboard: on every face corners+center share one color, edges another.
        let f = pattern("Checkerboard");
        for face in f.chunks(9) {
            assert!([0, 2, 6, 8].iter().all(|&k| face[k] == face[4]));
            assert!([1, 3, 5, 7].iter().all(|&k| face[k] == face[1] && face[k] != face[4]));
        }
        // Dots: every face uniform except its center.
        let f = pattern("Dots");
        for face in f.chunks(9) {
            assert!((0..9).filter(|&k| k != 4).all(|k| face[k] == face[0]) && face[4] != face[0]);
        }
        // Superflip: corners and centers solved, every edge sticker flipped.
        let f = pattern("Superflip");
        let s: Vec<char> = SOLVED.chars().collect();
        for (k, (a, b)) in f.iter().zip(&s).enumerate() {
            match k % 9 {
                1 | 3 | 5 | 7 => assert_ne!(a, b, "superflip edge {k}"),
                _ => assert_eq!(a, b, "superflip corner/center {k}"),
            }
        }
    }
}
