//! localStorage persistence of the cube itself (size, history, redo stack).
//! Stats, settings and achievements are owned by the page.
//!
//! Storage is untrusted input too (extensions, other tabs, old versions):
//! everything is validated against the current cube size, and anything that
//! doesn't decode cleanly is ignored rather than half-applied.

use crate::cube::{CubeMove, LAYER_ALL, MAX_SIZE, MIN_SIZE};

pub const KEY_SIZE: &str = "cube.v4.size";
pub const KEY_HISTORY: &str = "cube.v4.history";
pub const KEY_REDO: &str = "cube.v4.redo";
pub const KEY_ENGINE: &str = "cube.v4.engine";

const V3_SIZE: &str = "cube.v3.size";
const V3_HISTORY: &str = "cube.v3.history";
const V3_REDO: &str = "cube.v3.redo";

/// History beyond this is trimmed from the oldest end (keeps writes and
/// startup replay bounded; ~120 KB of storage).
pub const MAX_HISTORY: usize = 20_000;

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(target_arch = "wasm32")]
pub fn set(key: &str, value: &str) {
    if let Some(s) = local_storage() {
        let _ = s.set_item(key, value);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn get(key: &str) -> Option<String> {
    local_storage()?.get_item(key).ok().flatten()
}

#[cfg(target_arch = "wasm32")]
fn remove(key: &str) {
    if let Some(s) = local_storage() {
        let _ = s.remove_item(key);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set(_key: &str, _value: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn get(_key: &str) -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn remove(_key: &str) {}

fn parse_size(s: &str) -> Option<i32> {
    s.trim().parse::<i32>().ok().filter(|n| (MIN_SIZE..=MAX_SIZE).contains(n))
}

/// v4 codec — 6 digits per move: axis, layer + 40 (2 digits; 99 = whole
/// cube), depth (2 digits), quarters + 2.
pub fn encode(moves: &[CubeMove]) -> String {
    let mut s = String::with_capacity(moves.len() * 6);
    for m in moves {
        let layer = if m.layer == LAYER_ALL { 99 } else { m.layer + 40 };
        s.push_str(&format!("{}{:02}{:02}{}", m.axis, layer, m.depth, m.quarters + 2));
    }
    s
}

fn digits(c: &[u8]) -> Option<i32> {
    c.iter().try_fold(0i32, |acc, &b| (b as char).to_digit(10).map(|d| acc * 10 + d as i32))
}

pub fn decode(s: &str, n: i32) -> Option<Vec<CubeMove>> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(6) || b.len() / 6 > MAX_HISTORY {
        return None;
    }
    b.chunks(6)
        .map(|c| {
            let axis = digits(&c[0..1])?;
            let raw = digits(&c[1..3])?;
            let depth = digits(&c[3..5])?;
            let quarters = digits(&c[5..6])? - 2;
            let layer = if raw == 99 { LAYER_ALL } else { raw - 40 };
            let mv = CubeMove::new(axis as u8, layer, depth as u8, quarters as i8);
            mv.valid_for(n).then_some(mv)
        })
        .collect()
}

/// v3 codec (previous release) — 4 digits per move, single layers only.
fn decode_v3(s: &str, n: i32) -> Option<Vec<CubeMove>> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(4) || b.len() / 4 > MAX_HISTORY {
        return None;
    }
    b.chunks(4)
        .map(|c| {
            let axis = digits(&c[0..1])?;
            let raw = digits(&c[1..3])?;
            let quarters = digits(&c[3..4])? - 2;
            let layer = if raw == 99 { LAYER_ALL } else { raw - 40 };
            let mv = CubeMove::new(axis as u8, layer, 1, quarters as i8);
            mv.valid_for(n).then_some(mv)
        })
        .collect()
}

pub fn load_size() -> i32 {
    get(KEY_SIZE).as_deref().and_then(parse_size).or_else(|| get(V3_SIZE).as_deref().and_then(parse_size)).unwrap_or(3)
}

/// Saved (history, redo) for an n-cube; migrates v3 data once.
pub fn load_moves(n: i32) -> (Vec<CubeMove>, Vec<CubeMove>) {
    if let Some(h) = get(KEY_HISTORY) {
        let history = decode(&h, n).unwrap_or_default();
        let redo = get(KEY_REDO).and_then(|r| decode(&r, n)).unwrap_or_default();
        return (history, redo);
    }
    let history = get(V3_HISTORY).and_then(|h| decode_v3(&h, n)).unwrap_or_default();
    let redo = get(V3_REDO).and_then(|r| decode_v3(&r, n)).unwrap_or_default();
    if get(V3_HISTORY).is_some() {
        set(KEY_SIZE, &n.to_string());
        set(KEY_HISTORY, &encode(&history));
        set(KEY_REDO, &encode(&redo));
        for k in [V3_SIZE, V3_HISTORY, V3_REDO] {
            remove(k);
        }
    }
    (history, redo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::parse_alg;

    #[test]
    fn codec_roundtrip() {
        let moves = parse_alg("R U' Rw2 3Lw' 2F M2 x y' z2 Bw", 7).unwrap();
        assert_eq!(decode(&encode(&moves), 7), Some(moves));
    }

    #[test]
    fn decode_rejects_garbage_and_wrong_size() {
        assert_eq!(decode("", 3), Some(vec![]));
        assert_eq!(decode("12345", 3), None, "wrong length");
        assert_eq!(decode("9420 1", 3), None, "invalid axis / non-digit");
        assert_eq!(decode("042012", 3), None, "zero turn");
        // 5x5 inner layer (4-2=2 → +42) isn't valid parity on a 4x4.
        assert_eq!(decode("042011", 4), None);
        assert!(decode("042011", 5).is_some());
        // Too deep for a 3x3.
        assert_eq!(decode("042041", 3), None);
    }

    #[test]
    fn v3_migration_decodes() {
        // R on a 3x3 in v3 = axis 0, layer 2+40, quarters -1+2.
        assert_eq!(decode_v3("0421", 3), Some(vec![CubeMove::new(0, 2, 1, -1)]));
        assert_eq!(decode_v3("0991", 3), Some(vec![CubeMove::rotation(0, -1)]));
        assert_eq!(decode_v3("0401", 4), None, "middle layer on an even cube");
    }
}
