//! WCA-style random-move scrambles for every size, deterministic per seed
//! (the daily challenge depends on everyone getting the same one).

use crate::cube::{max_coord, CubeMove};

/// SplitMix64: tiny, fast, and well distributed even for sequential seeds
/// (the daily challenge seeds with consecutive day numbers).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn range(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// Scramble length per size, following WCA random-move lengths.
pub fn scramble_len(n: i32) -> usize {
    match n {
        2 => 11,
        3 => 25,
        4 => 40,
        5 => 60,
        6 => 80,
        7 => 100,
        _ => (n as usize * 15).min(200),
    }
}

/// Random-move scramble:
/// - 2x2 uses only R, U, F (the other three faces are redundant);
/// - 3x3 uses the six outer faces;
/// - bigger cubes add wide moves up to half the cube deep.
///
/// No two consecutive moves turn the same layer set, and within a run of
/// moves on the same axis no layer set repeats (so `R L R` can't occur).
pub fn generate(seed: u64, n: i32) -> Vec<CubeMove> {
    let mut rng = Rng::new(seed);
    let max = max_coord(n);
    let count = scramble_len(n);
    let max_depth = if n >= 4 { (n / 2) as u8 } else { 1 };

    let mut out: Vec<CubeMove> = Vec::with_capacity(count);
    let mut run_axis: Option<u8> = None;
    let mut run_used: Vec<(i32, u8)> = Vec::new();

    while out.len() < count {
        let (axis, sign) = if n == 2 {
            ([0u8, 1, 2][rng.range(3)], 1)
        } else {
            (rng.range(3) as u8, if rng.range(2) == 0 { 1 } else { -1 })
        };
        let depth = 1 + rng.range(max_depth as usize) as u8;
        let key = (sign, depth);
        if run_axis == Some(axis) {
            if run_used.contains(&key) {
                continue;
            }
        } else {
            run_axis = Some(axis);
            run_used.clear();
        }
        run_used.push(key);

        let quarters: i8 = match rng.range(3) {
            0 => 1,
            1 => -1,
            _ => 2,
        };
        out.push(CubeMove::new(axis, sign * max, depth, quarters));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The leaderboard server (backend/, Gleam) regenerates the daily
    /// scramble to verify submitted solves. Both test suites pin the same
    /// strings: change the generator here and backend/test/cube_test.gleam
    /// must change with it, or every daily submission gets rejected.
    #[test]
    fn daily_scrambles_match_the_server() {
        use crate::bridge::daily_seed;
        use crate::notation::format_alg;
        let cases = [
            (0, 13_888_771_472_215_769_088u64, "F B' R L U F' U' R2 U F' L' B2 U' D B F L2 R B D U' R' F B' R2"),
            (20_395, 13_586_762_507_681_506_655, "R D R2 L2 U' D L D B L R U' D2 L2 D2 L U D2 R L U' L' D2 L2 D"),
            (21_000, 5_924_762_292_955_263_720, "U2 F' U2 R' U' D' B F L' D' L2 D R' B2 U2 B' D' R' B' U F R' F2 L D"),
        ];
        for (day, seed, scramble) in cases {
            assert_eq!(daily_seed(day), seed, "day {day}");
            assert_eq!(format_alg(&generate(seed, 3), 3), scramble, "day {day}");
        }
    }
    use crate::cube::{apply_sim, solved_scene};
    use crate::solver::is_solved;

    #[test]
    fn scrambles_are_valid_and_deterministic() {
        for n in [2, 3, 4, 5, 7, 10, 20] {
            for seed in [0u64, 1, 42, 0xDEAD_BEEF, u64::MAX] {
                let a = generate(seed, n);
                assert_eq!(a.len(), scramble_len(n));
                assert_eq!(a, generate(seed, n), "{n}x{n}: not deterministic");
                for mv in &a {
                    assert!(mv.valid_for(n), "{n}x{n}: invalid move {mv:?}");
                }
                for w in a.windows(2) {
                    let same = w[0].axis == w[1].axis && w[0].layer == w[1].layer && w[0].depth == w[1].depth;
                    assert!(!same, "{n}x{n} seed {seed}: repeated layer");
                }
            }
        }
    }

    #[test]
    fn no_redundant_same_axis_runs() {
        for seed in 0..200u64 {
            let s = generate(seed, 3);
            for w in s.windows(3) {
                let all_same_axis = w[0].axis == w[1].axis && w[1].axis == w[2].axis;
                assert!(!(all_same_axis && w[0].layer == w[2].layer), "seed {seed}: R L R pattern");
            }
        }
    }

    #[test]
    fn scramble_actually_scrambles() {
        for n in [2, 3, 4, 6] {
            let mut scene = solved_scene(n);
            apply_sim(&mut scene, &generate(7, n));
            assert!(!is_solved(&scene, max_coord(n)), "{n}x{n}: scramble left the cube solved");
        }
    }

    #[test]
    fn consecutive_seeds_differ() {
        assert_ne!(generate(20_000, 3), generate(20_001, 3));
    }
}
