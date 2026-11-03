//! Standard cube notation for any size: parsing and formatting.
//!
//! Supported (WCA + SiGN):
//! - faces `R L U D F B`, with `'`, `2`, `2'`, `3` suffixes
//! - wide moves `Rw` / `r` (two layers), `3Rw` / `3r` (three layers), ...
//! - inner slices `2R`, `3U'` (a single inner layer, counted from the face)
//! - middle slices `M E S` (odd cubes only)
//! - whole-cube rotations `x y z`
//! - grouping with repetition: `(R U R' U')3`
//!
//! Input comes from the page (typed algorithms, share links), so the parser
//! is strictly bounded: input length, nesting depth and the expanded move
//! count are all capped.

use crate::cube::{max_coord, CubeMove, LAYER_ALL};

pub const MAX_INPUT_LEN: usize = 2000;
pub const MAX_MOVES: usize = 1000;
const MAX_NESTING: usize = 4;
const MAX_REPEAT: usize = 99;

/// Face letter → (axis, outward sign).
const FACES: [(char, u8, i32); 6] = [('R', 0, 1), ('L', 0, -1), ('U', 1, 1), ('D', 1, -1), ('F', 2, 1), ('B', 2, -1)];

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    TooLong,
    TooManyMoves,
    Unexpected(usize),
    BadLayer(usize),
    SliceOnEvenCube(usize),
    Unbalanced,
}

impl ParseError {
    /// Short machine-readable code, translated by the page.
    pub fn code(&self) -> String {
        match self {
            ParseError::TooLong => "too_long".into(),
            ParseError::TooManyMoves => "too_many".into(),
            ParseError::Unexpected(p) => format!("unexpected:{p}"),
            ParseError::BadLayer(p) => format!("bad_layer:{p}"),
            ParseError::SliceOnEvenCube(p) => format!("slice_even:{p}"),
            ParseError::Unbalanced => "unbalanced".into(),
        }
    }
}

/// A move on face `f` of an n-cube: `turns` clockwise quarter turns as seen
/// from outside that face (1, -1 or 2), `slice` = 1 for the outer layer,
/// `depth` = layers turned together.
fn face_move(axis: u8, sign: i32, n: i32, slice: i32, depth: i32, turns: i32) -> CubeMove {
    let layer = sign * (max_coord(n) - 2 * (slice - 1));
    // Clockwise seen from the face = -90° around its outward normal.
    let quarters = if turns.abs() == 2 { -2 * sign } else { -sign * turns };
    CubeMove::new(axis, layer, depth as u8, quarters as i8)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    n: i32,
    max_moves: usize,
}

impl Parser<'_> {
    fn skip_separators(&mut self) {
        while self.i < self.s.len()
            && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r' | b',' | b'.' | b'[' | b']')
        {
            self.i += 1;
        }
    }

    fn number(&mut self) -> Option<i32> {
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i].is_ascii_digit() && self.i - start < 3 {
            self.i += 1;
        }
        if self.i == start {
            return None;
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }

    fn sequence(&mut self, depth: usize, out: &mut Vec<CubeMove>) -> Result<(), ParseError> {
        loop {
            self.skip_separators();
            if self.i >= self.s.len() {
                return if depth == 0 { Ok(()) } else { Err(ParseError::Unbalanced) };
            }
            match self.s[self.i] {
                b'(' => {
                    if depth >= MAX_NESTING {
                        return Err(ParseError::Unexpected(self.i));
                    }
                    self.i += 1;
                    let mut inner = Vec::new();
                    self.sequence(depth + 1, &mut inner)?;
                    // Closing paren consumed by the inner call; optional repeat count.
                    let times = self.number().unwrap_or(1).clamp(0, MAX_REPEAT as i32) as usize;
                    for _ in 0..times {
                        if out.len() + inner.len() > self.max_moves {
                            return Err(ParseError::TooManyMoves);
                        }
                        out.extend_from_slice(&inner);
                    }
                }
                b')' => {
                    if depth == 0 {
                        return Err(ParseError::Unbalanced);
                    }
                    self.i += 1;
                    return Ok(());
                }
                _ => {
                    let mv = self.one_move()?;
                    if out.len() >= self.max_moves {
                        return Err(ParseError::TooManyMoves);
                    }
                    out.push(mv);
                }
            }
        }
    }

    fn one_move(&mut self) -> Result<CubeMove, ParseError> {
        let at = self.i;
        let n = self.n;
        let prefix = self.number();
        let Some(&c) = self.s.get(self.i) else {
            return Err(ParseError::Unexpected(at));
        };
        self.i += 1;
        let mut wide = false;
        if self.s.get(self.i) == Some(&b'w') {
            wide = true;
            self.i += 1;
        }
        // Suffix: 2, 3 (= prime), ' ; accept "2'" and "'2".
        let mut turns = 1;
        let mut seen_digit = false;
        for _ in 0..2 {
            match self.s.get(self.i) {
                Some(b'2') if !seen_digit => {
                    turns = 2;
                    seen_digit = true;
                    self.i += 1;
                }
                Some(b'3') if !seen_digit => {
                    turns = -turns;
                    seen_digit = true;
                    self.i += 1;
                }
                Some(b'\'') => {
                    if turns != 2 {
                        turns = -turns;
                    }
                    self.i += 1;
                }
                _ => break,
            }
        }

        let upper = c.to_ascii_uppercase() as char;
        if let Some(&(_, axis, sign)) = FACES.iter().find(|f| f.0 == upper) {
            let lower = c.is_ascii_lowercase();
            if lower && wide {
                return Err(ParseError::Unexpected(at));
            }
            let (slice, depth) = match (lower || wide, prefix) {
                // R, 3R (inner slice)
                (false, None) => (1, 1),
                (false, Some(k)) => (k, 1),
                // r / Rw = two layers; 3r / 3Rw = three layers
                (true, None) => (1, 2),
                (true, Some(k)) => (1, k),
            };
            if slice < 1 || slice > n || depth < 1 || depth > n {
                return Err(ParseError::BadLayer(at));
            }
            let mv = face_move(axis, sign, n, slice, depth, turns);
            if !mv.valid_for(n) {
                return Err(ParseError::BadLayer(at));
            }
            return Ok(mv);
        }

        if prefix.is_some() || wide {
            return Err(ParseError::Unexpected(at));
        }
        let quarters = |follow_sign: i32| -> i8 {
            if turns == 2 {
                (-2 * follow_sign) as i8
            } else {
                (-follow_sign * turns) as i8
            }
        };
        match c {
            // Slices follow L, D and F respectively.
            b'M' | b'E' | b'S' => {
                if n % 2 == 0 {
                    return Err(ParseError::SliceOnEvenCube(at));
                }
                let (axis, follow) = match c {
                    b'M' => (0, -1),
                    b'E' => (1, -1),
                    _ => (2, 1),
                };
                Ok(CubeMove::new(axis, 0, 1, quarters(follow)))
            }
            // Rotations follow R, U and F.
            b'x' => Ok(CubeMove::rotation(0, quarters(1))),
            b'y' => Ok(CubeMove::rotation(1, quarters(1))),
            b'z' => Ok(CubeMove::rotation(2, quarters(1))),
            _ => Err(ParseError::Unexpected(at)),
        }
    }
}

/// Parses an algorithm for an n x n cube.
pub fn parse_alg(input: &str, n: i32) -> Result<Vec<CubeMove>, ParseError> {
    parse_alg_limited(input, n, MAX_INPUT_LEN, MAX_MOVES)
}

/// [`parse_alg`] with other bounds, for whole recorded solves.
pub fn parse_alg_limited(input: &str, n: i32, max_len: usize, max_moves: usize) -> Result<Vec<CubeMove>, ParseError> {
    if input.len() > max_len {
        return Err(ParseError::TooLong);
    }
    // Typographic primes from phones and copy-pasted docs.
    let normalized: String = input
        .chars()
        .map(|c| match c {
            '’' | '′' | '‘' | '`' | '´' => '\'',
            c => c,
        })
        .collect();
    if !normalized.is_ascii() {
        let pos = normalized.char_indices().find(|(_, c)| !c.is_ascii()).map(|(i, _)| i).unwrap_or(0);
        return Err(ParseError::Unexpected(pos));
    }
    let mut p = Parser { s: normalized.as_bytes(), i: 0, n, max_moves };
    let mut out = Vec::new();
    p.sequence(0, &mut out)?;
    Ok(out)
}

fn suffix(turns: i32) -> &'static str {
    match turns {
        1 => "",
        -1 => "'",
        _ => "2",
    }
}

/// Formats one move in notation for an n x n cube.
pub fn format_move(mv: &CubeMove, n: i32) -> String {
    let q = mv.quarters as i32;
    let axis = mv.axis as usize;
    if mv.layer == LAYER_ALL {
        let turns = if q.abs() == 2 { 2 } else { -q };
        return format!("{}{}", ['x', 'y', 'z'][axis], suffix(turns));
    }
    if mv.layer == 0 {
        // M follows L, E follows D, S follows F.
        let follow = [-1, -1, 1][axis];
        let turns = if q.abs() == 2 { 2 } else { -follow * q };
        return format!("{}{}", ['M', 'E', 'S'][axis], suffix(turns));
    }
    let sign = mv.layer.signum();
    let letter = FACES.iter().find(|f| f.1 == mv.axis && f.2 == sign).map(|f| f.0).unwrap_or('?');
    let turns = if q.abs() == 2 { 2 } else { -sign * q };
    let slice = (max_coord(n) - mv.layer.abs()) / 2 + 1;
    let depth = mv.depth as i32;
    let body = match (slice, depth) {
        (1, 1) => letter.to_string(),
        (k, 1) => format!("{k}{letter}"),
        (1, 2) => format!("{letter}w"),
        (1, d) => format!("{d}{letter}w"),
        // Not expressible in WCA notation; SiGN range form.
        (k, d) => format!("{k}-{}{letter}w", k + d - 1),
    };
    format!("{body}{}", suffix(turns))
}

pub fn format_alg(moves: &[CubeMove], n: i32) -> String {
    moves.iter().map(|m| format_move(m, n)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::{apply_sim, solved_scene};

    #[test]
    fn roundtrip_basic_3x3() {
        let s = "R U R' U' F2 B' L D2 M E' S2 x y' z2";
        let moves = parse_alg(s, 3).unwrap();
        assert_eq!(format_alg(&moves, 3), s);
    }

    #[test]
    fn roundtrip_big_cube() {
        let s = "Rw U' 3Fw2 2R 3L' Dw' 4Bw";
        let moves = parse_alg(s, 7).unwrap();
        assert_eq!(format_alg(&moves, 7), s);
    }

    #[test]
    fn lowercase_wide_equals_w() {
        assert_eq!(parse_alg("r u' 3f2", 5), parse_alg("Rw Uw' 3Fw2", 5));
    }

    #[test]
    fn no_spaces_and_typographic_primes() {
        assert_eq!(parse_alg("RUR’U′", 3).unwrap(), parse_alg("R U R' U'", 3).unwrap());
        assert_eq!(parse_alg("R2'", 3).unwrap(), parse_alg("R2", 3).unwrap());
        assert_eq!(parse_alg("R3", 3).unwrap(), parse_alg("R'", 3).unwrap());
    }

    #[test]
    fn repetition_groups() {
        let six = parse_alg("(R U R' U')6", 3).unwrap();
        assert_eq!(six.len(), 24);
        // The sexy move has order 6.
        let mut scene = solved_scene(3);
        let start = scene.clone();
        apply_sim(&mut scene, &six);
        assert_eq!(scene.iter().map(|c| c.0).collect::<Vec<_>>(), start.iter().map(|c| c.0).collect::<Vec<_>>());
    }

    #[test]
    fn notation_directions_match_the_classic_constructors() {
        // R = -90° around X on the +X layer; L = +90° on the -X layer, etc.
        let m = parse_alg("R L U D F B M E S x y z", 3).unwrap();
        let expect = [
            CubeMove::new(0, 2, 1, -1),
            CubeMove::new(0, -2, 1, 1),
            CubeMove::new(1, 2, 1, -1),
            CubeMove::new(1, -2, 1, 1),
            CubeMove::new(2, 2, 1, -1),
            CubeMove::new(2, -2, 1, 1),
            CubeMove::new(0, 0, 1, 1),
            CubeMove::new(1, 0, 1, 1),
            CubeMove::new(2, 0, 1, -1),
            CubeMove::rotation(0, -1),
            CubeMove::rotation(1, -1),
            CubeMove::rotation(2, -1),
        ];
        assert_eq!(m, expect);
    }

    #[test]
    fn wide_equals_face_plus_slice_on_3x3() {
        let mut a = solved_scene(3);
        let mut b = solved_scene(3);
        apply_sim(&mut a, &parse_alg("r U' f2", 3).unwrap());
        apply_sim(&mut b, &parse_alg("R M' U' F2 S2", 3).unwrap());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.0, y.0);
        }
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(parse_alg("M", 4), Err(ParseError::SliceOnEvenCube(0)));
        assert!(matches!(parse_alg("4R", 3), Err(ParseError::BadLayer(_))));
        assert!(matches!(parse_alg("R Q", 3), Err(ParseError::Unexpected(2))));
        assert!(matches!(parse_alg("(R U", 3), Err(ParseError::Unbalanced)));
        assert!(matches!(parse_alg("R)", 3), Err(ParseError::Unbalanced)));
        assert!(matches!(parse_alg("2x", 3), Err(ParseError::Unexpected(0))));
        assert!(matches!(parse_alg("<script>", 3), Err(ParseError::Unexpected(0))));
        assert_eq!(parse_alg(&"R ".repeat(1500), 3), Err(ParseError::TooLong));
        assert_eq!(parse_alg("(((R U R' U')99)99)99", 3), Err(ParseError::TooManyMoves));
        assert!(parse_alg("", 3).unwrap().is_empty());
    }
}
