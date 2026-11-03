//! Cube model: the integer move representation, grid math and scene
//! construction. Everything here is exact — positions are integers and
//! orientations are snapped back to 90° multiples after every move — so a
//! session of thousands of moves never drifts.

use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

pub const CUBIE_SIZE: f32 = 0.92;
/// Rounded edges of the plastic body (see [`crate::mesh`]).
const BEVEL: f32 = 0.08;
/// Stickers cover the flat part of a face, with rounded corners.
const STICKER_SIZE: f32 = 0.78;
const STICKER_RADIUS: f32 = 0.1;
const FACE_OFFSET: f32 = CUBIE_SIZE / 2.0 + 0.004;

/// Geometry detail by cube size: (bevel segments per 45°, segments per
/// sticker corner). A cubie costs 12·(2s+1)² triangles, so detail drops as
/// the count grows and every size stays under [`TRIANGLE_BUDGET`]; from 9x9
/// up a bevel is a couple of pixels wide and cubies are plain boxes.
fn detail_level(n: i32) -> usize {
    match n {
        ..=3 => 0,
        4..=5 => 1,
        6..=8 => 2,
        9..=13 => 3,
        _ => 4,
    }
}
const DETAIL: [(u32, u32); 5] = [(3, 6), (2, 5), (1, 4), (0, 3), (0, 0)];
#[cfg(test)]
const TRIANGLE_BUDGET: usize = 40_000;

pub const MIN_SIZE: i32 = 2;
pub const MAX_SIZE: i32 = 20;

pub const AXES: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];

/// The grid uses "doubled coordinates" (step 2), so even-sized cubes (2x2,
/// 4x4) also have integer positions: n=3 → layers -2,0,2; n=4 → -3,-1,1,3.
/// The outer layer has value `max_coord(n) = n - 1`; world = grid * 0.5.
pub fn max_coord(n: i32) -> i32 {
    n - 1
}

/// `layer` sentinel: the move turns every layer (x / y / z rotations).
pub const LAYER_ALL: i32 = 99;

/// One layer turn, fully integral.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CubeMove {
    /// 0 = X, 1 = Y, 2 = Z.
    pub axis: u8,
    /// Outermost turned layer in doubled coordinates, or [`LAYER_ALL`].
    pub layer: i32,
    /// Layers turned together, from `layer` toward the center (wide moves).
    pub depth: u8,
    /// Quarter turns around +axis (right-hand rule); in -2..=2, never 0.
    pub quarters: i8,
}

impl CubeMove {
    pub const fn new(axis: u8, layer: i32, depth: u8, quarters: i8) -> Self {
        Self { axis, layer, depth, quarters }
    }

    /// Whole-cube rotation around `axis` (x / y / z in notation).
    pub const fn rotation(axis: u8, quarters: i8) -> Self {
        Self::new(axis, LAYER_ALL, 1, quarters)
    }

    pub fn rotation_axis(&self) -> Vec3 {
        AXES[self.axis as usize]
    }

    pub fn angle(&self) -> f32 {
        self.quarters as f32 * FRAC_PI_2
    }

    /// Does a cubie whose coordinate on the move's axis is `v` take part?
    pub fn affects(&self, v: i32) -> bool {
        if self.layer == LAYER_ALL {
            return true;
        }
        if self.depth <= 1 || self.layer == 0 {
            return v == self.layer;
        }
        // Distance inward from the outer turned layer, in doubled units.
        let inward = -self.layer.signum();
        let d = (v - self.layer) * inward;
        d >= 0 && d % 2 == 0 && d / 2 < self.depth as i32
    }

    pub fn inverse(self) -> Self {
        Self { quarters: -self.quarters, ..self }
    }

    /// Is the move legal on an n x n cube? Used to validate anything that
    /// comes from storage or from the page.
    pub fn valid_for(&self, n: i32) -> bool {
        let max = max_coord(n);
        if self.axis > 2 || self.quarters == 0 || !(-2..=2).contains(&self.quarters) {
            return false;
        }
        if self.layer == LAYER_ALL {
            return true;
        }
        if self.layer.abs() > max || (self.layer - max).rem_euclid(2) != 0 {
            return false;
        }
        if self.depth == 0 || self.depth as i32 > n {
            return false;
        }
        // A wide move can't run past the opposite face.
        let inner = self.layer.abs() - 2 * (self.depth as i32 - 1);
        self.depth == 1 || (self.layer != 0 && inner >= -max)
    }
}

/// Integer cubie position (doubled coordinates).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GridPos(pub IVec3);

/// Exact rotation of a lattice point by quarter turns around a world axis.
pub fn rotate_grid(p: IVec3, axis: u8, quarters: i8) -> IVec3 {
    let mut p = p;
    for _ in 0..(quarters as i32).rem_euclid(4) {
        p = match axis {
            0 => IVec3::new(p.x, -p.z, p.y),
            1 => IVec3::new(p.z, p.y, -p.x),
            _ => IVec3::new(-p.y, p.x, p.z),
        };
    }
    p
}

/// Snaps an orientation to the nearest 90°-multiple rotation, removing any
/// float error accumulated by animation.
pub fn snap_rotation(q: Quat) -> Quat {
    let m = Mat3::from_quat(q.normalize());
    let snap = |v: Vec3| Vec3::new(v.x.round(), v.y.round(), v.z.round());
    Quat::from_mat3(&Mat3::from_cols(snap(m.col(0)), snap(m.col(1)), snap(m.col(2)))).normalize()
}

/// Applies a move to one cubie (position + orientation), if it's affected.
pub fn apply_to(pos: &mut IVec3, rot: &mut Quat, mv: &CubeMove) {
    if !mv.affects(pos[mv.axis as usize]) {
        return;
    }
    *pos = rotate_grid(*pos, mv.axis, mv.quarters);
    *rot = snap_rotation(Quat::from_axis_angle(mv.rotation_axis(), mv.angle()) * *rot);
}

/// Applies a move without animation to the live scene.
pub fn apply_move_instant(cubies: &mut Query<(&mut GridPos, &mut Transform)>, mv: &CubeMove) {
    for (mut gp, mut tf) in cubies.iter_mut() {
        let mut p = gp.0;
        let mut r = tf.rotation;
        apply_to(&mut p, &mut r, mv);
        if p != gp.0 || r != tf.rotation {
            gp.0 = p;
            tf.translation = p.as_vec3() * 0.5;
            tf.rotation = r;
        }
    }
}

/// Solved scene for an n x n cube (surface cubies only, identity orientation),
/// as plain data — used by tests and by the solver checks.
#[cfg(test)]
pub fn solved_scene(n: i32) -> Vec<(IVec3, Quat)> {
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

#[cfg(test)]
pub fn apply_sim(scene: &mut [(IVec3, Quat)], moves: &[CubeMove]) {
    for mv in moves {
        for (p, r) in scene.iter_mut() {
            apply_to(p, r, mv);
        }
    }
}

// ── Colors ────────────────────────────────────────────────────────────────────

/// Sticker palettes, indexed by home face: +Y, -Y, +X, -X, +Z, -Z
/// (U, D, R, L, F, B). The default is the standard Western scheme — white
/// top, green front, red right — so the cube's chirality matches a real one.
pub const SCHEMES: [[[f32; 3]; 6]; 5] = [
    // Classic
    [
        [1.00, 1.00, 1.00],
        [1.00, 0.84, 0.00],
        [0.72, 0.07, 0.02],
        [1.00, 0.35, 0.00],
        [0.00, 0.55, 0.22],
        [0.00, 0.27, 0.68],
    ],
    // Vivid
    [
        [0.98, 0.98, 1.00],
        [1.00, 0.90, 0.10],
        [0.92, 0.10, 0.20],
        [1.00, 0.50, 0.05],
        [0.10, 0.80, 0.35],
        [0.10, 0.45, 1.00],
    ],
    // Colorblind-friendly (Okabe–Ito based: no red/green pair)
    [
        [1.00, 1.00, 1.00],
        [0.94, 0.89, 0.26],
        [0.84, 0.37, 0.00],
        [0.90, 0.62, 0.00],
        [0.00, 0.62, 0.45],
        [0.00, 0.45, 0.70],
    ],
    // Pastel
    [
        [0.97, 0.96, 0.94],
        [1.00, 0.91, 0.55],
        [0.95, 0.52, 0.55],
        [1.00, 0.72, 0.50],
        [0.55, 0.86, 0.66],
        [0.56, 0.70, 0.95],
    ],
    // Monochrome (blindfold-style practice: shapes only)
    [
        [0.92, 0.92, 0.92],
        [0.20, 0.20, 0.22],
        [0.70, 0.70, 0.72],
        [0.45, 0.45, 0.47],
        [0.82, 0.82, 0.84],
        [0.32, 0.32, 0.34],
    ],
];

/// Background choices (midnight, graphite, deep teal, daylight). The canvas
/// is transparent: the page paints the background (a CSS gradient) behind
/// it, so the engine only validates the setting.
pub const BACKGROUND_COUNT: usize = 4;

/// Home face index of a sticker (same order as [`SCHEMES`]).
pub fn home_face(axis: usize, sign: i32) -> usize {
    match (axis, sign > 0) {
        (1, true) => 0,
        (1, false) => 1,
        (0, true) => 2,
        (0, false) => 3,
        (2, true) => 4,
        _ => 5,
    }
}

/// Shared mesh and material handles, so a color-scheme change only touches
/// six materials instead of thousands of stickers.
#[derive(Resource)]
pub struct CubeAssets {
    /// Per [`detail_level`].
    pub cubie_meshes: Vec<Handle<Mesh>>,
    pub sticker_meshes: Vec<Handle<Mesh>>,
    pub body: Handle<StandardMaterial>,
    pub stickers: [Handle<StandardMaterial>; 6],
}

impl CubeAssets {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, scheme: usize) -> Self {
        let palette = SCHEMES[scheme.min(SCHEMES.len() - 1)];
        let stickers = std::array::from_fn(|i| {
            let [r, g, b] = palette[i];
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                // Glossy vinyl: a soft highlight that moves as the cube turns.
                perceptual_roughness: 0.3,
                reflectance: 0.42,
                double_sided: true,
                ..default()
            })
        });
        Self {
            cubie_meshes: DETAIL
                .iter()
                .map(|&(seg, _)| meshes.add(crate::mesh::rounded_box(CUBIE_SIZE, BEVEL, seg)))
                .collect(),
            sticker_meshes: DETAIL
                .iter()
                .map(|&(_, seg)| meshes.add(crate::mesh::rounded_square(STICKER_SIZE, STICKER_RADIUS, seg)))
                .collect(),
            body: materials.add(StandardMaterial {
                // Satin black plastic: dark, but the rounded edges catch light.
                base_color: Color::srgb(0.03, 0.03, 0.036),
                perceptual_roughness: 0.62,
                reflectance: 0.3,
                ..default()
            }),
            stickers,
        }
    }

    pub fn set_scheme(&self, materials: &mut Assets<StandardMaterial>, scheme: usize) {
        let palette = SCHEMES[scheme.min(SCHEMES.len() - 1)];
        for (i, h) in self.stickers.iter().enumerate() {
            if let Some(m) = materials.get_mut(h) {
                let [r, g, b] = palette[i];
                m.base_color = Color::srgb(r, g, b);
            }
        }
    }
}

/// Builds the n x n cube: only the surface cubies, with stickers on the
/// outer faces.
pub fn spawn_cube(commands: &mut Commands, assets: &CubeAssets, n: i32) {
    let max = max_coord(n);
    let level = detail_level(n);
    let (cubie_mesh, sticker_mesh) = (&assets.cubie_meshes[level], &assets.sticker_meshes[level]);
    // (axis, sign, local offset, local rotation) per face, in SCHEMES order.
    let face_defs = [
        (1, 1, Vec3::new(0.0, FACE_OFFSET, 0.0), Quat::from_rotation_x(-FRAC_PI_2)),
        (1, -1, Vec3::new(0.0, -FACE_OFFSET, 0.0), Quat::from_rotation_x(FRAC_PI_2)),
        (0, 1, Vec3::new(FACE_OFFSET, 0.0, 0.0), Quat::from_rotation_y(FRAC_PI_2)),
        (0, -1, Vec3::new(-FACE_OFFSET, 0.0, 0.0), Quat::from_rotation_y(-FRAC_PI_2)),
        (2, 1, Vec3::new(0.0, 0.0, FACE_OFFSET), Quat::IDENTITY),
        (2, -1, Vec3::new(0.0, 0.0, -FACE_OFFSET), Quat::from_rotation_y(PI)),
    ];

    for x in (-max..=max).step_by(2) {
        for y in (-max..=max).step_by(2) {
            for z in (-max..=max).step_by(2) {
                // The interior is never visible.
                if x.abs() != max && y.abs() != max && z.abs() != max {
                    continue;
                }
                let p = IVec3::new(x, y, z);
                let id = commands
                    .spawn((
                        PbrBundle {
                            mesh: cubie_mesh.clone(),
                            material: assets.body.clone(),
                            transform: Transform::from_translation(p.as_vec3() * 0.5),
                            ..default()
                        },
                        GridPos(p),
                    ))
                    .id();
                for (face, &(axis, sign, offset, rotation)) in face_defs.iter().enumerate() {
                    if p[axis] != sign * max {
                        continue;
                    }
                    commands
                        .spawn((
                            PbrBundle {
                                mesh: sticker_mesh.clone(),
                                material: assets.stickers[face].clone(),
                                transform: Transform { translation: offset, rotation, ..default() },
                                ..default()
                            },
                            // Flush with the body: its shadow is the body's.
                            NotShadowCaster,
                        ))
                        .set_parent(id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangles(mesh: &Mesh) -> usize {
        mesh.indices().map(|i| i.len() / 3).unwrap_or(0)
    }

    #[test]
    fn every_size_stays_within_the_triangle_budget() {
        for n in MIN_SIZE..=MAX_SIZE {
            let (cubie_seg, sticker_seg) = DETAIL[detail_level(n)];
            let cubie = triangles(&crate::mesh::rounded_box(CUBIE_SIZE, BEVEL, cubie_seg));
            let sticker = triangles(&crate::mesh::rounded_square(STICKER_SIZE, STICKER_RADIUS, sticker_seg));
            let cubies = (n.pow(3) - (n - 2).max(0).pow(3)) as usize;
            let stickers = 6 * (n * n) as usize;
            let total = cubies * cubie + stickers * sticker;
            assert!(total <= TRIANGLE_BUDGET, "{n}x{n}: {total} triangles");
        }
    }

    fn sample_moves() -> Vec<CubeMove> {
        let mut v = Vec::new();
        for axis in 0..3u8 {
            for layer in [-2, 0, 2, LAYER_ALL] {
                for q in [-2i8, -1, 1, 2] {
                    v.push(CubeMove::new(axis, layer, 1, q));
                }
            }
        }
        v
    }

    #[test]
    fn four_quarter_turns_are_identity() {
        for mv in sample_moves() {
            let start = IVec3::new(2, 2, -2);
            let mut p = start;
            for _ in 0..4 {
                p = rotate_grid(p, mv.axis, mv.quarters);
            }
            assert_eq!(p, start, "{mv:?} doesn't have order 4");
        }
    }

    #[test]
    fn integer_rotation_matches_quaternion() {
        for mv in sample_moves() {
            for p in [IVec3::new(2, 2, 2), IVec3::new(3, -1, 1), IVec3::new(0, 2, -2)] {
                let q = Quat::from_axis_angle(mv.rotation_axis(), mv.angle());
                let f = (q * p.as_vec3()).round().as_ivec3();
                assert_eq!(rotate_grid(p, mv.axis, mv.quarters), f, "{mv:?} at {p}");
            }
        }
    }

    #[test]
    fn move_then_inverse_is_identity() {
        for mv in sample_moves() {
            let mut scene = solved_scene(3);
            let before = scene.clone();
            apply_sim(&mut scene, &[mv, mv.inverse()]);
            for (a, b) in scene.iter().zip(&before) {
                assert_eq!(a.0, b.0);
                assert!(a.1.angle_between(b.1) < 1e-4);
            }
        }
    }

    #[test]
    fn wide_moves_cover_the_right_layers() {
        // 5x5: max = 4; Rw (depth 2) turns layers 4 and 2, not 0.
        let rw = CubeMove::new(0, 4, 2, -1);
        assert!(rw.affects(4) && rw.affects(2) && !rw.affects(0) && !rw.affects(-2));
        // 3Lw on a 6x6: layers -5, -3, -1.
        let lw = CubeMove::new(0, -5, 3, 1);
        assert!(lw.affects(-5) && lw.affects(-3) && lw.affects(-1) && !lw.affects(1));
        assert!(lw.valid_for(6));
        // A 4-deep wide move on a 3x3 runs off the cube.
        assert!(!CubeMove::new(0, 2, 4, 1).valid_for(3));
        assert!(!CubeMove::new(0, 1, 1, 1).valid_for(3), "wrong parity");
        assert!(!CubeMove::new(0, 2, 1, 0).valid_for(3), "zero turn");
    }

    #[test]
    fn default_scheme_has_real_chirality() {
        // On a real cube, looking at the U-F-R corner from outside, the
        // colors white → red → green go clockwise. In a right-handed frame the
        // corner (+X,+Y,+Z) has normals ordered x → y → z counter-clockwise,
        // so U(y) → R(x) → F(z) must be clockwise: white, red, green.
        let s = SCHEMES[0];
        assert_eq!(s[0], [1.0, 1.0, 1.0], "U white");
        assert!(s[2][0] > 0.6 && s[2][1] < 0.2, "R red");
        assert!(s[4][1] > s[4][2] && s[4][1] > s[4][0], "F green");
    }
}
