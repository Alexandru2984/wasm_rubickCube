//! Pointer input (mouse + touch), camera orbit / zoom and canvas sizing.
//! Keyboard input lives in the page, which forwards notation to the engine.

use crate::cube::{CubeMove, GridPos, CUBIE_SIZE};
use crate::game::{CubeSize, RotationState, Session, Settings};
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

const ROTATION_DURATION: f32 = 0.18;
const ZOOM_MIN: f32 = 0.3;
const ZOOM_MAX: f32 = 3.0;
/// Live drag: distance at which the layer is picked, and angle sensitivity.
const DRAG_LOCK_THRESHOLD: f32 = 10.0;
const DRAG_ANGLE_PER_PIXEL: f32 = FRAC_PI_2 / 130.0;

// ── Camera ────────────────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct OrbitCamera {
    pub rotation: Quat,
    /// Multiplier over the distance that fits the cube in the viewport.
    pub zoom: f32,
}

impl OrbitCamera {
    fn default_rotation() -> Quat {
        Quat::from_euler(EulerRot::YXZ, 0.5, -0.45, 0.0)
    }

    pub fn reset(&mut self) {
        self.rotation = Self::default_rotation();
        self.zoom = 1.0;
    }
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self { rotation: Self::default_rotation(), zoom: 1.0 }
    }
}

fn apply_rotation_delta(state: &mut OrbitCamera, delta: Vec2) {
    if delta == Vec2::ZERO {
        return;
    }
    let sensitivity = 0.006;
    let up = state.rotation * Vec3::Y;
    let yaw_sign = if up.y >= 0.0 { -1.0 } else { 1.0 };
    let yaw = Quat::from_rotation_y(yaw_sign * delta.x * sensitivity);
    state.rotation = yaw * state.rotation;
    let right = state.rotation * Vec3::X;
    let pitch = Quat::from_axis_angle(right, -delta.y * sensitivity);
    state.rotation = (pitch * state.rotation).normalize();
}

/// Distance at which an n-cube fits the viewport, whatever its aspect: on a
/// portrait phone the horizontal field of view is the narrow one.
fn fit_distance(n: i32, aspect: f32, fov: f32) -> f32 {
    let half_v = fov / 2.0;
    let half_h = (half_v.tan() * aspect.max(0.1)).atan();
    let half = half_v.min(half_h);
    n as f32 * 1.02 / half.sin()
}

pub fn update_camera_transform(
    state: Res<OrbitCamera>,
    size: Res<CubeSize>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut query: Query<(&mut Transform, &Projection), With<Camera3d>>,
) {
    let Ok((mut transform, projection)) = query.get_single_mut() else { return };
    let aspect = windows.get_single().map(|w| w.width() / w.height().max(1.0)).unwrap_or(1.0);
    let fov = match projection {
        Projection::Perspective(p) => p.fov,
        _ => std::f32::consts::FRAC_PI_4,
    };
    let radius = fit_distance(size.0, aspect, fov) * state.zoom;
    transform.translation = state.rotation * Vec3::new(0.0, 0.0, radius);
    transform.look_at(Vec3::ZERO, state.rotation * Vec3::Y);
}

pub fn camera_zoom(mut state: ResMut<OrbitCamera>, mut scroll: EventReader<MouseWheel>) {
    for ev in scroll.read() {
        let steps = match ev.unit {
            MouseScrollUnit::Line => ev.y * 0.12,
            MouseScrollUnit::Pixel => ev.y * 0.0015,
        };
        state.zoom = (state.zoom * (-steps).exp()).clamp(ZOOM_MIN, ZOOM_MAX);
    }
}

// ── Canvas resolution ─────────────────────────────────────────────────────────

/// winit-web sets the physical resolution to the canvas's CSS size (without
/// devicePixelRatio), so high-DPI screens render blurry and pointer positions
/// drift. Physical = CSS × dpr (capped by settings: a 3× phone doesn't need
/// 9× the pixels of a 1× screen), scale_factor = dpr.
#[cfg(target_arch = "wasm32")]
pub fn sync_canvas_resolution(
    settings: Res<Settings>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    let Some(win) = web_sys::window() else { return };
    let dpr = (win.device_pixel_ratio() as f32).min(settings.dpr_cap);
    if dpr <= 0.0 {
        return;
    }
    let Some(rect) = win.document().and_then(|d| d.get_element_by_id("bevy")).map(|c| c.get_bounding_client_rect())
    else {
        return;
    };
    let (css_w, css_h) = (rect.width() as f32, rect.height() as f32);
    if css_w < 1.0 || css_h < 1.0 {
        return;
    }
    let (phys_w, phys_h) = ((css_w * dpr).round() as u32, (css_h * dpr).round() as u32);
    let Ok(mut window) = windows.get_single_mut() else { return };
    let r = &window.resolution;
    if r.physical_width() != phys_w || r.physical_height() != phys_h || (r.scale_factor() - dpr).abs() > 1e-3 {
        window.resolution.set_scale_factor_override(Some(dpr));
        window.resolution.set_physical_resolution(phys_w, phys_h);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn sync_canvas_resolution(_settings: Res<Settings>) {}

// ── Pointer ───────────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct PointerState {
    pub drag: Option<DragData>,
    pub manual: Option<ManualRotation>,
    prev_pinch_distance: Option<f32>,
}

pub struct DragData {
    start_screen: Vec2,
    kind: DragKind,
}

#[derive(Clone, Copy)]
enum DragKind {
    Camera,
    Face,
}

/// Layer rotation driven by finger/mouse: the layer follows the drag in real
/// time and snaps to a multiple of 90° on release.
pub struct ManualRotation {
    axis: u8,
    layer: i32,
    entities: Vec<Entity>,
    initial_transforms: Vec<Transform>,
    /// Screen direction that corresponds to a positive angle around the axis.
    tangent_screen: Vec2,
    angle: f32,
}

type CubieQuery<'w, 's> = Query<'w, 's, (Entity, &'static GridPos, &'static mut Transform)>;

#[allow(clippy::too_many_arguments)]
pub fn pointer_input(
    mut s: Session,
    mut cam_state: ResMut<OrbitCamera>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    touches: Res<Touches>,
    mut cubie_q: CubieQuery,
) {
    let Ok((camera, cam_transform)) = camera_q.get_single() else { return };
    let Ok(window) = windows.get_single() else { return };

    // 2+ fingers → pinch zoom; an ongoing manual rotation snaps and ends.
    let touch_positions: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
    if touch_positions.len() >= 2 {
        finish_manual(&mut s);
        let d = touch_positions[0].distance(touch_positions[1]);
        if let Some(prev) = s.pointer.prev_pinch_distance {
            if d > 1.0 {
                cam_state.zoom = (cam_state.zoom * prev / d).clamp(ZOOM_MIN, ZOOM_MAX);
            }
        }
        s.pointer.prev_pinch_distance = Some(d);
        s.pointer.drag = None;
        mouse_motion.clear();
        return;
    }

    let touch_pos = touch_positions.first().copied();

    // Pinch just ended: the remaining finger takes over the camera.
    if s.pointer.prev_pinch_distance.take().is_some() {
        if let Some(p) = touch_pos {
            s.pointer.drag = Some(DragData { start_screen: p, kind: DragKind::Camera });
        }
    }

    let touch_delta: Vec2 = touches.iter().next().map(|t| t.delta()).unwrap_or(Vec2::ZERO);
    let touch_just_pressed = touches.iter_just_pressed().next().map(|t| t.position());
    let touch_ended = touches.iter_just_released().next().is_some() || touches.iter_just_canceled().next().is_some();

    let mouse_down = mouse_button.pressed(MouseButton::Left);
    let mouse_just_pressed = mouse_button.just_pressed(MouseButton::Left).then(|| window.cursor_position()).flatten();
    let mouse_just_released = mouse_button.just_released(MouseButton::Left);

    let pressed_now = touch_just_pressed.or(mouse_just_pressed);
    let pos_now: Option<Vec2> = touch_pos.or_else(|| if mouse_down { window.cursor_position() } else { None });
    let delta_now: Vec2 = if touch_pos.is_some() {
        touch_delta
    } else if mouse_down {
        mouse_motion.read().fold(Vec2::ZERO, |acc, ev| acc + ev.delta)
    } else {
        mouse_motion.clear();
        Vec2::ZERO
    };
    let released = touch_ended || mouse_just_released;

    // New drag: on a sticker → layer turn, elsewhere → camera.
    if let Some(start_pos) = pressed_now {
        finish_manual(&mut s);
        let kind = match raycast_cubie(camera, cam_transform, start_pos, &cubie_q) {
            Some(_) => DragKind::Face,
            None => DragKind::Camera,
        };
        s.pointer.drag = Some(DragData { start_screen: start_pos, kind });
    }

    if let Some((start, kind)) = s.pointer.drag.as_ref().map(|d| (d.start_screen, d.kind)) {
        match kind {
            DragKind::Camera => apply_rotation_delta(&mut cam_state, delta_now),
            DragKind::Face => {
                // Pick the layer after a small threshold, only when nothing is
                // animating or queued (the layer must be settled to capture it).
                let free = s.pointer.manual.is_none() && s.active.0.is_none() && s.queue.0.is_empty();
                if free {
                    if let Some(now) = pos_now {
                        let total = now - start;
                        if total.length() > DRAG_LOCK_THRESHOLD {
                            s.pointer.manual = begin_manual(camera, cam_transform, start, total, &cubie_q);
                        }
                    }
                }
                if let Some(m) = s.pointer.manual.as_mut() {
                    if let Some(now) = pos_now {
                        m.angle = ((now - start).dot(m.tangent_screen) * DRAG_ANGLE_PER_PIXEL).clamp(-PI, PI);
                        let q = Quat::from_axis_angle(crate::cube::AXES[m.axis as usize], m.angle);
                        for (i, &entity) in m.entities.iter().enumerate() {
                            if let Ok((_, _, mut tf)) = cubie_q.get_mut(entity) {
                                tf.translation = q * m.initial_transforms[i].translation;
                                tf.rotation = q * m.initial_transforms[i].rotation;
                            }
                        }
                    }
                }
            }
        }
    }

    if released {
        finish_manual(&mut s);
        s.pointer.drag = None;
    }
}

/// Starts a manual rotation: of the two axes lying in the hit face's plane,
/// picks the one whose screen tangent best matches the drag direction.
fn begin_manual(
    camera: &Camera,
    cam_transform: &GlobalTransform,
    start_screen: Vec2,
    drag_screen: Vec2,
    cubie_q: &CubieQuery,
) -> Option<ManualRotation> {
    let (grid_pos, hit_world, normal) = raycast_cubie(camera, cam_transform, start_screen, cubie_q)?;
    let hit_screen = camera.world_to_viewport(cam_transform, hit_world)?;

    let mut best: Option<(f32, usize, Vec2)> = None;
    for (i, axis) in crate::cube::AXES.iter().enumerate() {
        // Like a physical cube: a face only drives the layers in its plane.
        if axis.dot(normal).abs() > 0.5 {
            continue;
        }
        let tangent_world = axis.cross(hit_world);
        if tangent_world.length_squared() < 1e-4 {
            continue;
        }
        let probe = hit_world + tangent_world.normalize() * 0.2;
        let Some(probe_screen) = camera.world_to_viewport(cam_transform, probe) else { continue };
        let tangent_screen = probe_screen - hit_screen;
        if tangent_screen.length_squared() < 1.0 {
            continue;
        }
        let tangent_unit = tangent_screen.normalize();
        let score = drag_screen.dot(tangent_unit).abs();
        if best.as_ref().is_none_or(|(b, _, _)| score > *b) {
            best = Some((score, i, tangent_unit));
        }
    }
    let (_, axis_idx, tangent_screen) = best?;
    let layer = grid_pos.0[axis_idx];

    let mut entities = Vec::new();
    let mut initial_transforms = Vec::new();
    for (entity, gp, tf) in cubie_q.iter() {
        if gp.0[axis_idx] == layer {
            entities.push(entity);
            initial_transforms.push(*tf);
        }
    }
    Some(ManualRotation { axis: axis_idx as u8, layer, entities, initial_transforms, tangent_screen, angle: 0.0 })
}

/// Ends the manual rotation: snaps to the nearest quarter turn, animated from
/// the current angle; a real turn goes into history.
fn finish_manual(s: &mut Session) {
    let Some(m) = s.pointer.manual.take() else { return };
    let quarters = ((m.angle / FRAC_PI_2).round() as i32).clamp(-2, 2) as i8;
    let mv = CubeMove::new(m.axis, m.layer, 1, quarters);
    if quarters != 0 {
        s.record_user_move(mv);
    }
    let target = mv.angle();
    let remaining = (target - m.angle).abs() / FRAC_PI_2;
    s.active.0 = Some(RotationState {
        mv,
        elapsed: 0.0,
        duration: (ROTATION_DURATION * remaining).clamp(0.04, ROTATION_DURATION),
        entities: m.entities,
        initial_transforms: m.initial_transforms,
        start_angle: m.angle,
    });
}

fn raycast_cubie(
    camera: &Camera,
    cam_transform: &GlobalTransform,
    screen_pos: Vec2,
    cubie_q: &CubieQuery,
) -> Option<(GridPos, Vec3, Vec3)> {
    let ray = camera.viewport_to_world(cam_transform, screen_pos)?;
    let origin = ray.origin;
    let dir: Vec3 = ray.direction.as_vec3();
    let half = CUBIE_SIZE / 2.0;

    let mut best: Option<(f32, GridPos, Vec3, Vec3)> = None;
    for (_entity, grid_pos, transform) in cubie_q.iter() {
        let inv = transform.compute_matrix().inverse();
        let local_origin = inv.transform_point3(origin);
        let local_dir = inv.transform_vector3(dir);
        if let Some((t, entry_axis)) =
            ray_aabb_intersect(local_origin, local_dir, Vec3::splat(-half), Vec3::splat(half))
        {
            if t > 0.0 && best.as_ref().is_none_or(|(bt, ..)| t < *bt) {
                let mut local_normal = Vec3::ZERO;
                local_normal[entry_axis] = if local_dir[entry_axis] > 0.0 { -1.0 } else { 1.0 };
                let world_normal = (transform.rotation * local_normal).normalize();
                best = Some((t, *grid_pos, origin + dir * t, world_normal));
            }
        }
    }
    best.map(|(_, gp, hw, n)| (gp, hw, n))
}

fn ray_aabb_intersect(origin: Vec3, dir: Vec3, aabb_min: Vec3, aabb_max: Vec3) -> Option<(f32, usize)> {
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    let mut entry_axis = 0usize;
    for i in 0..3 {
        let o = origin[i];
        let d = dir[i];
        if d.abs() < 1e-8 {
            if o < aabb_min[i] || o > aabb_max[i] {
                return None;
            }
        } else {
            let t1 = (aabb_min[i] - o) / d;
            let t2 = (aabb_max[i] - o) / d;
            let (tn, tf) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            if tn > tmin {
                tmin = tn;
                entry_axis = i;
            }
            if tf < tmax {
                tmax = tf;
            }
            if tmin > tmax {
                return None;
            }
        }
    }
    if tmax < 0.0 {
        return None;
    }
    Some(if tmin > 0.0 { (tmin, entry_axis) } else { (tmax, entry_axis) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_distance_grows_on_portrait() {
        let fov = std::f32::consts::FRAC_PI_4;
        let landscape = fit_distance(3, 16.0 / 9.0, fov);
        let portrait = fit_distance(3, 9.0 / 19.5, fov);
        assert!(portrait > landscape * 1.8, "portrait {portrait} vs landscape {landscape}");
    }

    #[test]
    fn ray_hits_box_from_outside() {
        let hit = ray_aabb_intersect(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z, Vec3::splat(-0.5), Vec3::splat(0.5));
        assert_eq!(hit, Some((4.5, 2)));
        assert!(
            ray_aabb_intersect(Vec3::new(2.0, 0.0, 5.0), Vec3::NEG_Z, Vec3::splat(-0.5), Vec3::splat(0.5)).is_none()
        );
    }
}
