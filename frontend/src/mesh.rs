//! Procedural meshes for a modern-looking cube: cubies with rounded edges
//! and corners, stickers with rounded corners. Built at startup from a few
//! numbers, so the look costs no asset download.
//!
//! Rounded box: every face is a grid whose outer rows sit on the bevel. A
//! grid point `p` on the sharp box is clamped into the inner box
//! (`size/2 - radius`); the vertex goes to `inner + normalize(p - inner) *
//! radius`, which is the flat face, a quarter cylinder along an edge, or an
//! eighth of a sphere at a corner. Bevel rows are placed at `tan` steps so
//! the normals turn in even angles, and both faces meeting at an edge produce
//! the same vertex there (45°), so shading is seamless.

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

/// Box of edge `size`, edges rounded with `radius` in `segments` steps per
/// 45° (0 = sharp box).
pub fn rounded_box(size: f32, radius: f32, segments: u32) -> Mesh {
    let half = size / 2.0;
    let (radius, segments) = if segments == 0 || radius <= 0.0 { (0.0, 0) } else { (radius.min(half), segments) };
    let inner = half - radius;

    // Grid coordinates along a face edge: bevel rows, the flat part, bevel rows.
    let mut coords = Vec::new();
    for i in (1..=segments).rev() {
        coords.push(-(inner + radius * (i as f32 / segments as f32 * FRAC_PI_4).tan()));
    }
    coords.push(-inner);
    coords.push(inner);
    for i in 1..=segments {
        coords.push(inner + radius * (i as f32 / segments as f32 * FRAC_PI_4).tan());
    }
    if segments == 0 {
        coords = vec![-half, half];
    }
    let m = coords.len();

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(6 * m * m);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(6 * m * m);
    let mut indices: Vec<u32> = Vec::with_capacity(6 * (m - 1) * (m - 1) * 6);

    for axis in 0..3 {
        // u x v = +axis, so counter-clockwise in (u, v) faces outward on +axis.
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for sign in [1.0f32, -1.0] {
            let base = positions.len() as u32;
            for &cu in &coords {
                for &cv in &coords {
                    let mut p = Vec3::ZERO;
                    p[axis] = sign * half;
                    p[u] = cu;
                    p[v] = cv;
                    let (pos, normal) = if segments == 0 {
                        let mut n = Vec3::ZERO;
                        n[axis] = sign;
                        (p, n)
                    } else {
                        let core = p.clamp(Vec3::splat(-inner), Vec3::splat(inner));
                        let n = (p - core).normalize();
                        (core + n * radius, n)
                    };
                    positions.push(pos.to_array());
                    normals.push(normal.to_array());
                }
            }
            for i in 0..m - 1 {
                for j in 0..m - 1 {
                    let a = base + (i * m + j) as u32;
                    let b = base + ((i + 1) * m + j) as u32;
                    let c = base + ((i + 1) * m + j + 1) as u32;
                    let d = base + (i * m + j + 1) as u32;
                    if sign > 0.0 {
                        indices.extend_from_slice(&[a, b, c, a, c, d]);
                    } else {
                        indices.extend_from_slice(&[a, c, b, a, d, c]);
                    }
                }
            }
        }
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}

/// Square of side `size` in the XY plane facing +Z (like `Rectangle`), with
/// corners rounded by `radius` in `segments` steps per corner.
pub fn rounded_square(size: f32, radius: f32, segments: u32) -> Mesh {
    let half = size / 2.0;
    let radius = radius.clamp(0.0, half);
    let segments = if radius == 0.0 { 0 } else { segments };
    let inner = half - radius;

    let mut positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]];
    // Corners counter-clockwise from the +X+Y one; each arc turns 90°.
    for (k, (cx, cy)) in [(inner, inner), (-inner, inner), (-inner, -inner), (inner, -inner)].into_iter().enumerate() {
        if segments == 0 {
            positions.push([half.copysign(cx), half.copysign(cy), 0.0]);
            continue;
        }
        let start = k as f32 * FRAC_PI_2;
        for s in 0..=segments {
            let angle = start + s as f32 / segments as f32 * FRAC_PI_2;
            positions.push([cx + radius * angle.cos(), cy + radius * angle.sin(), 0.0]);
        }
    }
    let rim = positions.len() as u32 - 1;
    let mut indices = Vec::with_capacity(rim as usize * 3);
    for i in 1..=rim {
        indices.extend_from_slice(&[0, i, if i == rim { 1 } else { i + 1 }]);
    }
    let normals = vec![[0.0, 0.0, 1.0]; positions.len()];

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::mesh::VertexAttributeValues;

    fn data(mesh: &Mesh) -> (Vec<Vec3>, Vec<Vec3>, Vec<u32>) {
        let get = |attr| match mesh.attribute(attr) {
            Some(VertexAttributeValues::Float32x3(v)) => v.iter().map(|&a| Vec3::from_array(a)).collect::<Vec<_>>(),
            _ => panic!("missing attribute"),
        };
        let idx = match mesh.indices() {
            Some(Indices::U32(i)) => i.clone(),
            _ => panic!("missing indices"),
        };
        (get(Mesh::ATTRIBUTE_POSITION), get(Mesh::ATTRIBUTE_NORMAL), idx)
    }

    #[test]
    fn rounded_box_is_closed_outward_and_bounded() {
        for segments in [0, 1, 2, 3, 4] {
            let (pos, nor, idx) = data(&rounded_box(0.92, 0.08, segments));
            let half = 0.46 + 1e-5;
            assert!(pos.iter().all(|p| p.abs().max_element() <= half), "segments {segments}: vertex outside the box");
            assert!(nor.iter().all(|n| (n.length() - 1.0).abs() < 1e-4));
            // Every face of the six points away from the center.
            for t in idx.chunks(3) {
                let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
                let cross = (b - a).cross(c - a);
                if cross.length() < 1e-9 {
                    continue; // degenerate sliver at a corner row
                }
                assert!(cross.dot((a + b + c) / 3.0) > 0.0, "segments {segments}: inward triangle");
            }
            // The extremes reach the full size on every axis.
            for axis in 0..3 {
                let max = pos.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
                assert!((max - 0.46).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn rounded_box_shading_is_seamless_across_edges() {
        // Vertices shared by two faces (same position) carry the same normal.
        let (pos, nor, _) = data(&rounded_box(1.0, 0.1, 3));
        for i in 0..pos.len() {
            for j in i + 1..pos.len() {
                if pos[i].distance(pos[j]) < 1e-6 {
                    assert!(nor[i].distance(nor[j]) < 1e-5, "crease at {:?}", pos[i]);
                }
            }
        }
    }

    #[test]
    fn rounded_box_normals_are_radial_on_the_bevel() {
        let (pos, nor, _) = data(&rounded_box(1.0, 0.1, 3));
        let inner = 0.4;
        for (p, n) in pos.iter().zip(&nor) {
            let core = p.clamp(Vec3::splat(-inner), Vec3::splat(inner));
            if p.distance(core) > 1e-4 {
                assert!((p.distance(core) - 0.1).abs() < 1e-4);
                assert!(((*p - core).normalize() - *n).length() < 1e-4);
            }
        }
    }

    #[test]
    fn rounded_square_is_a_closed_fan_facing_z() {
        for segments in [0, 1, 4] {
            let (pos, nor, idx) = data(&rounded_square(0.8, 0.12, segments));
            assert!(nor.iter().all(|n| *n == Vec3::Z));
            assert!(pos.iter().all(|p| p.x.abs() <= 0.4 + 1e-6 && p.y.abs() <= 0.4 + 1e-6 && p.z == 0.0));
            for t in idx.chunks(3) {
                let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
                assert!((b - a).cross(c - a).z >= 0.0, "clockwise triangle");
            }
            let area: f32 = idx
                .chunks(3)
                .map(|t| {
                    (pos[t[1] as usize] - pos[t[0] as usize]).cross(pos[t[2] as usize] - pos[t[0] as usize]).z / 2.0
                })
                .sum();
            // Between the chamfered square and the true rounded one.
            let (low, high) = if segments == 0 {
                (0.64, 0.64)
            } else {
                (0.64 - 2.0 * 0.12 * 0.12, 0.64 - (4.0 - std::f32::consts::PI) * 0.12 * 0.12)
            };
            assert!(area >= low - 1e-4 && area <= high + 1e-4, "segments {segments}: area {area} not in {low}..{high}");
        }
    }
}
